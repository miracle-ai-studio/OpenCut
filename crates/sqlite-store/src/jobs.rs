//! Native composition of the shared job submission use case.
use crate::{EditorApplication, JobRecord, JobRequest};
use opencut_contracts::{AssetLockManifest, EditorError, ProjectSnapshot};
pub use opencut_editor_application::jobs::*;
use rusqlite::{OptionalExtension, params};
use std::path::Path;
/// Application entry used by native hosts. The pure orchestration below depends
/// on a transaction-level store and asset capture ports, not on a transport.
pub fn start(
    app: &mut EditorApplication,
    asset_store_root: &Path,
    request: JobStartInput,
) -> Result<JobRecord, EditorError> {
    submit(
        app,
        &NativeJobAssets {
            root: asset_store_root,
            store: std::cell::RefCell::new(None),
        },
        request,
    )
}

pub fn start_transcribe(
    app: &mut EditorApplication,
    asset_store_root: &Path,
    request: opencut_job_contracts::TranscribeJobInput,
) -> Result<JobRecord, EditorError> {
    submit_transcribe(
        app,
        &NativeJobAssets {
            root: asset_store_root,
            store: std::cell::RefCell::new(None),
        },
        request,
    )
}

struct NativeJobAssets<'a> {
    root: &'a Path,
    store: std::cell::RefCell<Option<opencut_asset_store::AssetStore>>,
}
pub(super) fn native_assets(root: &Path) -> impl JobAssetCapture + '_ {
    NativeJobAssets {
        root,
        store: std::cell::RefCell::new(None),
    }
}
impl JobAssetCapture for NativeJobAssets<'_> {
    fn capture(
        &self,
        snapshot: &ProjectSnapshot,
        sequence_id: &str,
        asset_ids: &[String],
    ) -> Result<AssetLockManifest, EditorError> {
        let mut store = self.store.borrow_mut();
        if store.is_none() {
            *store = Some(opencut_asset_store::AssetStore::open_managed(self.root)?);
        }
        store
            .as_ref()
            .expect("initialized CAS capture")
            .capture(snapshot, sequence_id, asset_ids)
    }
}
impl JobSubmissionStore for EditorApplication {
    fn fixed_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<ProjectSnapshot, EditorError> {
        crate::sqlite_access::require(
            &self.connection,
            &self.connection.1,
            project_id,
            crate::ProjectAction::RunJobs,
        )?;
        self.snapshot_at(project_id, revision)
    }
    fn fixed_analysis_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<ProjectSnapshot, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(crate::storage_error)?;
        crate::sqlite_access::require(
            &tx,
            &self.connection.1,
            project_id,
            crate::ProjectAction::RunJobs,
        )?;
        let raw: String = tx
            .query_row(
                "SELECT document_json FROM revisions WHERE project_id=?1 AND revision=?2",
                params![project_id, revision],
                |row| row.get(0),
            )
            .optional()
            .map_err(crate::storage_error)?
            .ok_or_else(|| EditorError::new("NOT_FOUND", "Project revision does not exist"))?;
        let project = match crate::sqlite_project_document::decode_stored_project(&raw)? {
            crate::sqlite_project_document::ProjectStorageDocument::Legacy(project) => project,
            crate::sqlite_project_document::ProjectStorageDocument::HistoricalVisual(project) => {
                if !self.connection.1.caller().is_owner()
                    && project
                        .base
                        .assets
                        .iter()
                        .any(|asset| asset.version.is_none())
                {
                    return Err(EditorError::new(
                        "NOT_AUTHORIZED",
                        "Schema 3 analysis requires pinned project content",
                    ));
                }
                project.base
            }
            crate::sqlite_project_document::ProjectStorageDocument::ReadOnly(_) => {
                return Err(opencut_editor_application::project_readonly::readonly_error());
            }
            crate::sqlite_project_document::ProjectStorageDocument::Editable(_) => {
                return Err(EditorError::new(
                    "UNSUPPORTED_PROJECT_VERSION",
                    "Schema 4 analysis requires a schema 4 execution snapshot",
                ));
            }
        };
        if project.id != project_id {
            return Err(EditorError::new(
                "STORAGE_ERROR",
                "Project identity differs from revision scope",
            ));
        }
        tx.commit().map_err(crate::storage_error)?;
        Ok(ProjectSnapshot {
            project,
            revision: revision.into(),
        })
    }
    fn find_accepted_job(&self, request: &JobRequest) -> Result<Option<JobRecord>, EditorError> {
        self.find_existing_job(request)
    }
    fn accept_job(
        &mut self,
        request: JobRequest,
        execution_input: serde_json::Value,
    ) -> Result<JobRecord, EditorError> {
        self.create_prepared_job(request, execution_input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::Time;
    use opencut_job_execution::PreparedProjectFrameInput;
    use serde_json::json;
    fn serialize_prepared(value: &impl serde::Serialize) -> Result<serde_json::Value, EditorError> {
        serde_json::to_value(value)
            .map_err(|e| EditorError::new("SERIALIZATION_ERROR", e.to_string()))
    }
    use crate::RestoreRevisionRequest;
    use opencut_contracts::{CreateProject, FrameRate, TransactionRequest};
    use opencut_media_contracts::FrameAlignment;
    use std::sync::{Arc, Barrier};

    fn project(app: &mut EditorApplication) -> opencut_contracts::ProjectSnapshot {
        app.create_project(
            CreateProject {
                id: "project-1".into(),
                name: "Frame preview".into(),
                sequence_id: "sequence-1".into(),
                width: 1920,
                height: 1080,
                fps: FrameRate {
                    numerator: 30,
                    denominator: 1,
                },
            },
            "create-project",
        )
        .unwrap()
    }

    fn request(revision: &str, time: Time, sequence_id: &str) -> JobStartInput {
        JobStartInput::ProjectFrame {
            idempotency_key: "frame-1".into(),
            input: ProjectFrameInput {
                project_id: "project-1".into(),
                revision: revision.into(),
                sequence_id: sequence_id.into(),
                requested_time: time,
                alignment: FrameAlignment::Nearest,
            },
        }
    }

    fn composited_request(revision: &str, key: &str) -> JobStartInput {
        JobStartInput::CompositedFrame {
            idempotency_key: key.into(),
            input: ProjectFrameInput {
                project_id: "project-1".into(),
                revision: revision.into(),
                sequence_id: "sequence-1".into(),
                requested_time: Time {
                    ticks: "0".into(),
                    timebase: 30,
                },
                alignment: FrameAlignment::Exact,
            },
        }
    }

    fn range_request(revision: &str, duration_ticks: &str, key: &str) -> JobStartInput {
        JobStartInput::ProjectRange {
            idempotency_key: key.into(),
            input: ProjectRangeInput {
                project_id: "project-1".into(),
                revision: revision.into(),
                sequence_id: "sequence-1".into(),
                requested_range: opencut_contracts::TimeRange {
                    start: Time {
                        ticks: "30".into(),
                        timebase: 30,
                    },
                    duration: Time {
                        ticks: duration_ticks.into(),
                        timebase: 30,
                    },
                },
                alignment: FrameAlignment::Exact,
            },
        }
    }

    #[test]
    fn project_frame_job_persists_the_requested_revision_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = EditorApplication::open(directory.path().join("workspace.sqlite")).unwrap();
        let original = project(&mut app);
        let asset_store = directory.path().join("asset-store");

        let legacy_identity = json!({
            "snapshot": original.clone(),
            "sequence_id": "sequence-1",
            "requested_time": {"ticks":"15","timebase":30},
            "alignment": "nearest"
        });
        let typed_identity = serialize_prepared(&PreparedProjectFrameInput {
            snapshot: original.clone(),
            sequence_id: "sequence-1".into(),
            requested_time: Time {
                ticks: "15".into(),
                timebase: 30,
            },
            alignment: FrameAlignment::Nearest,
            asset_lock: None,
        })
        .unwrap();
        assert_eq!(typed_identity, legacy_identity);

        let job = start(
            &mut app,
            &asset_store,
            request(
                &original.revision,
                Time {
                    ticks: "15".into(),
                    timebase: 30,
                },
                "sequence-1",
            ),
        )
        .unwrap();
        std::fs::remove_dir_all(&asset_store).unwrap();
        std::fs::write(&asset_store, b"capture must not run on retry").unwrap();
        let retry = start(
            &mut app,
            &asset_store,
            request(
                &original.revision,
                Time {
                    ticks: "15".into(),
                    timebase: 30,
                },
                "sequence-1",
            ),
        )
        .unwrap();
        assert_eq!(retry.job_id, job.job_id);
        let newer = app
            .restore_revision(
                "project-1",
                &RestoreRevisionRequest {
                    expected_revision: original.revision.clone(),
                    target_revision: original.revision.clone(),
                    idempotency_key: "new-head".into(),
                    label: "new head".into(),
                },
            )
            .unwrap();

        assert_eq!(job.kind, "preview.project_frame");
        assert_eq!(job.input["snapshot"]["revision"], original.revision);
        assert_eq!(job.input["sequence_id"], "sequence-1");
        assert_eq!(
            job.input["requested_time"],
            json!({"ticks":"15","timebase":30})
        );
        assert_eq!(job.input["alignment"], "nearest");
        assert_eq!(job.input["asset_lock"]["version"], 1);
        assert_eq!(job.input["asset_lock"]["assets"], json!([]));
        assert_ne!(newer.revision, job.input["snapshot"]["revision"]);
        assert_eq!(app.get_job(&job.job_id).unwrap().input, job.input);
    }

    #[test]
    fn project_frame_job_rejects_invalid_time_and_sequence_before_queueing() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = EditorApplication::open(directory.path().join("workspace.sqlite")).unwrap();
        let snapshot = project(&mut app);

        for time in [
            Time {
                ticks: "-1".into(),
                timebase: 30,
            },
            Time {
                ticks: "0".into(),
                timebase: 0,
            },
            Time {
                ticks: "not-an-integer".into(),
                timebase: 30,
            },
        ] {
            let error = start(
                &mut app,
                &directory.path().join("asset-store"),
                request(&snapshot.revision, time, "sequence-1"),
            )
            .unwrap_err();
            assert_eq!(error.code, "INVALID_ARGUMENT");
        }
        let error = start(
            &mut app,
            &directory.path().join("asset-store"),
            request(
                &snapshot.revision,
                Time {
                    ticks: "0".into(),
                    timebase: 30,
                },
                "missing-sequence",
            ),
        )
        .unwrap_err();
        assert_eq!(error.code, "NOT_FOUND");
        assert!(app.list_jobs().unwrap().is_empty());
    }

    #[test]
    fn composited_frame_queues_a_fixed_revision_with_its_own_kind() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = EditorApplication::open(directory.path().join("workspace.sqlite")).unwrap();
        let snapshot = project(&mut app);
        let asset_store = directory.path().join("asset-store");

        let job = start(
            &mut app,
            &asset_store,
            composited_request(&snapshot.revision, "composited-1"),
        )
        .unwrap();
        assert_eq!(job.kind, "preview.composited_frame");
        assert_eq!(job.input["snapshot"], json!(snapshot.clone()));
        assert_eq!(job.input["asset_lock"]["assets"], json!([]));

        let retry = start(
            &mut app,
            &asset_store,
            composited_request(&snapshot.revision, "composited-1"),
        )
        .unwrap();
        assert_eq!(retry.job_id, job.job_id);
    }

    #[test]
    fn project_range_queues_typed_fixed_revision_and_rejects_empty_duration() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = EditorApplication::open(directory.path().join("workspace.sqlite")).unwrap();
        let snapshot = project(&mut app);
        let asset_store = directory.path().join("asset-store");

        let job = start(
            &mut app,
            &asset_store,
            range_request(&snapshot.revision, "60", "range-1"),
        )
        .unwrap();
        assert_eq!(job.kind, "preview.project_range");
        assert_eq!(job.input["snapshot"], json!(snapshot.clone()));
        assert_eq!(job.input["requested_range"]["start"]["ticks"], "30");
        assert_eq!(job.input["requested_range"]["duration"]["ticks"], "60");

        let error = start(
            &mut app,
            &asset_store,
            range_request(&snapshot.revision, "0", "range-empty"),
        )
        .unwrap_err();
        assert_eq!(error.code, "INVALID_ARGUMENT");
    }

    #[test]
    fn job_start_schema_exposes_strict_project_frame_input() {
        let schema = serde_json::to_string(&schemars::schema_for!(JobStartInput)).unwrap();
        assert!(schema.contains("preview.project_frame"));
        assert!(schema.contains("preview.composited_frame"));
        assert!(schema.contains("preview.project_range"));
        assert!(schema.contains("requested_time"));
        assert!(schema.contains("alignment"));
        assert!(schema.contains("additionalProperties"));
    }

    #[test]
    fn concurrent_project_job_preparation_returns_the_first_committed_job() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("workspace.sqlite");
        let asset_store = directory.path().join("asset-store");
        let revision = {
            let mut app = EditorApplication::open(&database).unwrap();
            project(&mut app).revision
        };
        let barrier = Arc::new(Barrier::new(3));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let database = database.clone();
                let asset_store = asset_store.clone();
                let barrier = Arc::clone(&barrier);
                let request = request(
                    &revision,
                    Time {
                        ticks: "0".into(),
                        timebase: 30,
                    },
                    "sequence-1",
                );
                std::thread::spawn(move || {
                    let mut app = EditorApplication::open(database).unwrap();
                    barrier.wait();
                    start(&mut app, &asset_store, request).unwrap()
                })
            })
            .collect();
        barrier.wait();
        let jobs: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();

        assert_eq!(jobs[0].job_id, jobs[1].job_id);
        let app = EditorApplication::open(&database).unwrap();
        assert_eq!(app.list_jobs().unwrap().len(), 1);
        assert_eq!(app.job_events_after(0, 10, None).unwrap().len(), 1);
    }

    #[test]
    fn project_job_locks_required_media_without_rewriting_snapshot_uri() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("workspace.sqlite");
        let asset_store = directory.path().join("asset-store");
        let source = directory.path().join("source.bin");
        std::fs::write(&source, b"immutable media bytes").unwrap();
        let mut app = EditorApplication::open(&database).unwrap();
        let original = project(&mut app);
        let edit: TransactionRequest = serde_json::from_value(json!({
            "expected_revision":original.revision,
            "idempotency_key":"bind-media",
            "label":"bind media",
            "operations":[
                {"type":"asset.bind","asset":{"id":"asset-1","name":"Source","kind":"video","uri":source,"duration":{"ticks":"1","timebase":1},"metadata":{}}},
                {"type":"track.create","sequence_id":"sequence-1","track":{"id":"video","name":"Video","kind":"video","enabled":true,"muted":false,"locked":false,"clips":[],"params":{}}},
                {"type":"clip.insert","sequence_id":"sequence-1","track_id":"video","clip":{"id":"clip-1","asset_id":"asset-1","timeline_range":{"start":{"ticks":"0","timebase":1},"duration":{"ticks":"1","timebase":1}},"source_range":{"start":{"ticks":"0","timebase":1},"duration":{"ticks":"1","timebase":1}},"params":{},"channels":[]}}
            ]
        }))
        .unwrap();
        let revision = app.apply("project-1", &edit).unwrap().revision;
        let request = request(
            &revision,
            Time {
                ticks: "0".into(),
                timebase: 30,
            },
            "sequence-1",
        );

        let job = start(&mut app, &asset_store, request.clone()).unwrap();

        assert_eq!(
            job.input["snapshot"]["project"]["assets"][0]["uri"],
            json!(source)
        );
        let locked = &job.input["asset_lock"]["assets"][0];
        assert_eq!(locked["asset_id"], "asset-1");
        assert_eq!(locked["size"], 21);
        let hash = locked["content_hash"].as_str().unwrap();
        assert_eq!(hash.len(), 64);
        assert!(asset_store.join(&hash[..2]).join(hash).is_file());

        std::fs::remove_file(&source).unwrap();
        let retry = start(&mut app, &asset_store, request).unwrap();
        assert_eq!(retry.job_id, job.job_id);
        assert_eq!(retry.input, job.input);
    }

    #[test]
    fn transcription_submission_pins_one_asset_and_reuses_the_durable_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let asset_store = directory.path().join("asset-store");
        let source = directory.path().join("source.wav");
        std::fs::write(&source, b"fixed audio bytes").unwrap();
        let mut app = EditorApplication::open(directory.path().join("workspace.sqlite")).unwrap();
        let original = project(&mut app);
        let edit: TransactionRequest = serde_json::from_value(json!({
            "expected_revision": original.revision,
            "idempotency_key": "bind-audio",
            "label": "bind audio",
            "operations": [{"type":"asset.bind","asset":{
                "id":"asset-1","name":"Voice","kind":"audio","uri":source,
                "duration":{"ticks":"1","timebase":1},"metadata":{}
            }}]
        }))
        .unwrap();
        let revision = app.apply("project-1", &edit).unwrap().revision;
        let request = opencut_job_contracts::TranscribeJobInput {
            idempotency_key: "transcribe-1".into(),
            project_id: "project-1".into(),
            revision,
            asset_id: "asset-1".into(),
            language: Some("en".into()),
            word_timestamps: false,
        };
        let job = start_transcribe(&mut app, &asset_store, request.clone()).unwrap();
        assert_eq!(job.kind, "analysis.transcribe");
        assert_eq!(job.input["asset_lock"]["assets"][0]["asset_id"], "asset-1");
        assert_eq!(job.input["language"], "en");
        // JobRecord.input is the prepared execution payload. Only the public
        // request identity omits false to preserve pre-word-timing replay.
        assert_eq!(job.input["word_timestamps"], false);
        assert!(job.input.get("executable").is_none());
        std::fs::remove_file(&source).unwrap();
        let retry = start_transcribe(&mut app, &asset_store, request.clone()).unwrap();
        assert_eq!(retry.job_id, job.job_id);
        assert_eq!(app.list_jobs().unwrap().len(), 1);
        let mut invalid = request;
        invalid.language = Some("en-US".into());
        assert_eq!(
            start_transcribe(&mut app, &asset_store, invalid)
                .unwrap_err()
                .code,
            "INVALID_ARGUMENT"
        );
    }
    #[test]
    fn submission_port_rolls_back_all_queue_writes_when_event_publication_fails() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite");
        let mut app = EditorApplication::open(&path).unwrap();
        app.connection
            .execute_batch(
                "CREATE TRIGGER fail_queued_event BEFORE INSERT ON job_events
             BEGIN SELECT RAISE(ABORT, 'injected event failure'); END;",
            )
            .unwrap();
        let request = JobRequest {
            idempotency_key: "atomic-port".into(),
            kind: "media.probe".into(),
            input: json!({"input":"source.wav"}),
        };
        assert_eq!(
            app.accept_job(request.clone(), json!({"input":"prepared.wav"}))
                .unwrap_err()
                .code,
            "STORAGE_ERROR"
        );
        let independent = EditorApplication::open(&path).unwrap();
        assert!(independent.list_jobs().unwrap().is_empty());
        assert!(independent.find_accepted_job(&request).unwrap().is_none());
        assert!(
            independent
                .job_events_after(0, 100, None)
                .unwrap()
                .is_empty()
        );
        app.connection
            .execute_batch("DROP TRIGGER fail_queued_event")
            .unwrap();
        let accepted = app
            .accept_job(request.clone(), json!({"input":"prepared.wav"}))
            .unwrap();
        drop(app);
        let reopened = EditorApplication::open(&path).unwrap();
        assert_eq!(
            reopened
                .find_accepted_job(&request)
                .unwrap()
                .unwrap()
                .job_id,
            accepted.job_id
        );
        assert_eq!(reopened.job_events_after(0, 100, None).unwrap().len(), 1);
    }

    #[test]
    fn submission_port_keeps_first_execution_input_under_racing_acceptance() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite");
        drop(EditorApplication::open(&path).unwrap());
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = ["first.wav", "second.wav"]
            .into_iter()
            .map(|input| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut app = EditorApplication::open(path).unwrap();
                    let request = JobRequest {
                        idempotency_key: "racing-port".into(),
                        kind: "media.probe".into(),
                        input: json!({"input":"public.wav"}),
                    };
                    assert!(app.find_accepted_job(&request).unwrap().is_none());
                    barrier.wait();
                    app.accept_job(request, json!({"input":input})).unwrap()
                })
            })
            .collect();
        let accepted: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(accepted[0].job_id, accepted[1].job_id);
        assert_eq!(accepted[0].input, accepted[1].input);
        let app = EditorApplication::open(path).unwrap();
        assert_eq!(app.list_jobs().unwrap().len(), 1);
        assert_eq!(app.job_events_after(0, 100, None).unwrap().len(), 1);
    }
}
