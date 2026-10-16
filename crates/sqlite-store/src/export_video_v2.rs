//! Configurable export uses the existing fixed revision capture and durable
//! queue. Global acceptance/completion/target registries delegate here.
use super::sqlite_project_v4_jobs::context;
use super::sqlite_project_v4_video_jobs::{Assets, policy, require_prepared_authority};
use super::*;
use opencut_editor_application::{audit::ExecutionContext, export_video_v2 as export};
use opencut_job_contracts::export_video_v2::{EXPORT_VIDEO_V2_KIND, SubmitExportVideoV2};
use opencut_job_execution::project_v4_audio::TrustedJobAuthorityV1;

pub fn start_v2(
    app: &mut EditorApplication,
    input: SubmitExportVideoV2,
) -> Result<JobRecord, EditorError> {
    let assets = Assets(opencut_asset_store::AssetStore::read_only(
        app.asset_store_root(),
    )?);
    export::submit(app, &assets, input)
}
impl export::ExportVideoStoreV2 for EditorApplication {
    fn accept_export_video_v2(
        &mut self,
        request: JobRequest,
        prepared: export::PreparedExportVideoV2,
    ) -> Result<JobRecord, EditorError> {
        self.create_prepared_job(
            request,
            serde_json::to_value(prepared).map_err(|_| {
                EditorError::new(
                    "SERIALIZATION_ERROR",
                    "Cannot persist configurable export input",
                )
            })?,
        )
    }
}
pub(super) fn verified_target(
    connection: &Connection,
    input: &Value,
) -> Result<Option<(String, String)>, EditorError> {
    let fixed = if let Ok(public) = serde_json::from_value::<SubmitExportVideoV2>(input.clone()) {
        export::validate_submission(&public)?;
        serde_json::to_value(
            opencut_job_contracts::project_v4_video::SubmitV4VideoMp4V1 {
                idempotency_key: public.idempotency_key,
                project_id: public.project_id,
                revision: public.revision,
                sequence_id: public.sequence_id,
            },
        )
    } else if let Ok(prepared) =
        serde_json::from_value::<export::PreparedExportVideoV2>(input.clone())
    {
        serde_json::to_value(prepared.execution)
    } else {
        return Ok(None);
    }
    .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Invalid export target"))?;
    super::sqlite_project_v4_video_jobs::verified_target(connection, &fixed)
}
pub(super) fn validate_acceptance(
    connection: &Connection,
    caller: &ExecutionContext,
    request: &JobRequest,
    input: &Value,
) -> Result<(), EditorError> {
    let prepared: export::PreparedExportVideoV2 =
        serde_json::from_value(input.clone()).map_err(|_| {
            EditorError::new("INVALID_ARGUMENT", "Invalid prepared configurable export")
        })?;
    export::validate_prepared_request(request, &prepared)?;
    require_prepared_authority(connection, caller, &prepared.execution)
}
pub(super) fn require_completion(
    connection: &Connection,
    job: &JobRecord,
) -> Result<(), EditorError> {
    if job.kind != EXPORT_VIDEO_V2_KIND {
        return Err(EditorError::new(
            "INVALID_STORED_RESULT",
            "Export completion kind differs",
        ));
    }
    let prepared: export::PreparedExportVideoV2 = serde_json::from_value(job.input.clone())
        .map_err(|_| {
            EditorError::new("INVALID_ARGUMENT", "Invalid persisted configurable export")
        })?;
    require_prepared_authority(
        connection,
        &context(&prepared.execution.authority),
        &prepared.execution,
    )?;
    let stored: export::StoredExportVideoV2 =
        serde_json::from_value(job.result.clone().ok_or_else(|| {
            EditorError::new(
                "INVALID_STORED_RESULT",
                "Missing configurable export result",
            )
        })?)
        .map_err(|_| {
            EditorError::new(
                "INVALID_STORED_RESULT",
                "Invalid configurable export result",
            )
        })?;
    export::validate_stored_result(&prepared, &stored)?;
    Ok(())
}
impl EditorApplication {
    pub fn revalidate_export_video_v2(
        &self,
        id: &str,
        worker: &str,
        attempt: u32,
        authority: &TrustedJobAuthorityV1,
    ) -> Result<(), EditorError> {
        // The shared revalidator selects the nested execution authority/policy
        // only for this registered kind; lease/cancel/permission logic is shared.
        super::sqlite_project_v4_jobs::revalidate(
            self,
            EXPORT_VIDEO_V2_KIND,
            id,
            worker,
            attempt,
            authority,
            &policy(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::{
        CreateProject, FrameRate, Time, graphic_edit_v1::*, project_v4_ops::*,
    };
    use opencut_editor_application::access::{Caller, PrincipalPolicy, ProjectGrant};
    use opencut_job_contracts::export_video_v2::{
        ExportFormatV2, ExportQualityV2, ExportVideoOptionsV2,
    };
    fn now() -> i64 {
        lifecycle::JobClock::now_ms(&sqlite_jobs::SystemClock).unwrap()
    }
    fn fixture() -> (tempfile::TempDir, EditorApplication, SubmitExportVideoV2) {
        let directory = tempfile::tempdir().unwrap();
        let mut app = EditorApplication::open(directory.path().join("workspace.sqlite")).unwrap();
        let initial = app
            .create_v4_project(&CreateV4Project {
                idempotency_key: "create".into(),
                project: CreateProject {
                    id: "project".into(),
                    name: "Configurable export".into(),
                    sequence_id: "scene".into(),
                    width: 64,
                    height: 48,
                    fps: FrameRate {
                        numerator: 30,
                        denominator: 1,
                    },
                },
            })
            .unwrap();
        let time = |ticks: &str, timebase| Time {
            ticks: ticks.into(),
            timebase,
        };
        let mut request = GraphicPlanRequestV1 {
            project_id: "project".into(),
            expected_revision: initial.revision,
            sequence_id: "scene".into(),
            idempotency_key: "graphic".into(),
            command: GraphicPlanCommandV1::Defaults {
                geometry: GraphicKindV1::Rectangle,
                start: GraphicTimeInputV1::Exact { time: time("0", 1) },
            },
        };
        let defaults = opencut_editor_application::graphic_edit_v1::plan(&app, &request)
            .unwrap()
            .initial
            .unwrap();
        request.command = GraphicPlanCommandV1::Create {
            element_id: "shape".into(),
            track: GraphicTrackTargetV1::New {
                track_id: "shapes".into(),
            },
            values: GraphicValuesV1 {
                name: defaults.name,
                graphic: defaults.graphic,
                transform: defaults.transform,
                opacity: defaults.opacity,
                start: GraphicTimeInputV1::Exact { time: time("0", 1) },
                duration: GraphicTimeInputV1::Exact {
                    time: time("1", 10),
                },
            },
        };
        let transaction = opencut_editor_application::graphic_edit_v1::plan(&app, &request)
            .unwrap()
            .transaction
            .unwrap();
        let revision = app.apply_public_v4_project(&transaction).unwrap().revision;
        (
            directory,
            app,
            SubmitExportVideoV2 {
                idempotency_key: "export".into(),
                project_id: "project".into(),
                revision,
                sequence_id: "scene".into(),
                options: ExportVideoOptionsV2 {
                    format: ExportFormatV2::Webm,
                    quality: ExportQualityV2::VeryHigh,
                    include_audio: false,
                },
            },
        )
    }
    #[test]
    fn durable_options_and_fixed_revision_survive_head_change_and_reopen() {
        let (directory, mut app, request) = fixture();
        let job = start_v2(&mut app, request.clone()).unwrap();
        let prepared: export::PreparedExportVideoV2 =
            serde_json::from_value(job.input.clone()).unwrap();
        assert_eq!(prepared.options, request.options);
        assert!(prepared.execution.asset_lock.assets.is_empty());
        assert!(export::prepare(&prepared).unwrap().audio.is_none());
        app.apply_v4_project(&ApplyV4Project {
            project_id: "project".into(),
            expected_revision: request.revision.clone(),
            idempotency_key: "rename".into(),
            label: "New head".into(),
            operations: vec![ProjectOperationV4::Rename {
                name: "Changed".into(),
            }],
        })
        .unwrap();
        assert_eq!(start_v2(&mut app, request.clone()).unwrap(), job);
        let mut changed = request.clone();
        changed.options.include_audio = true;
        assert!(start_v2(&mut app, changed).is_err());
        let mut reopened =
            EditorApplication::open(directory.path().join("workspace.sqlite")).unwrap();
        assert_eq!(start_v2(&mut reopened, request).unwrap(), job);
        assert_eq!(
            reopened
                .job_view(&job.job_id)
                .unwrap()
                .target
                .unwrap()
                .revision,
            prepared.execution.snapshot.revision
        );
    }
    #[test]
    fn nested_authority_keeps_credential_kind_lease_and_cancellation_fences() {
        for revoke in [false, true] {
            let (_directory, mut app, request) = fixture();
            app.set_principal_policy(PrincipalPolicy {
                principal_id: "agent".into(),
                expected_revision: "0".into(),
                enabled: true,
                grants: vec![ProjectGrant {
                    project_id: "project".into(),
                    actions: vec![ProjectAction::Read, ProjectAction::RunJobs],
                }],
            })
            .unwrap();
            let credential = app.issue_credential("agent").unwrap();
            app.set_caller(app.authenticate(&credential.secret).unwrap());
            let job = start_v2(&mut app, request).unwrap();
            app.set_caller(Caller::LocalOwner);
            let running = app
                .claim_job("export-worker", now(), 30_000)
                .unwrap()
                .unwrap();
            let input: export::PreparedExportVideoV2 =
                serde_json::from_value(running.input.clone()).unwrap();
            app.revalidate_export_video_v2(
                &job.job_id,
                "export-worker",
                running.attempt,
                &input.execution.authority,
            )
            .unwrap();
            assert!(
                app.revalidate_export_video_v2(
                    &job.job_id,
                    "wrong-worker",
                    running.attempt,
                    &input.execution.authority
                )
                .is_err()
            );
            assert!(
                app.revalidate_v4_video_job(
                    &job.job_id,
                    "export-worker",
                    running.attempt,
                    &input.execution.authority
                )
                .is_err()
            );
            assert!(
                app.revalidate_export_video_v2(
                    &job.job_id,
                    "export-worker",
                    running.attempt,
                    &TrustedJobAuthorityV1::LocalOwner
                )
                .is_err()
            );
            if revoke {
                app.revoke_credential(&credential.credential_id).unwrap();
                assert_eq!(
                    app.complete_job(
                        &job.job_id,
                        "export-worker",
                        running.attempt,
                        now(),
                        Value::Null
                    )
                    .unwrap_err()
                    .code,
                    "NOT_AUTHORIZED"
                );
                assert_eq!(app.get_job(&job.job_id).unwrap(), running);
            } else {
                app.cancel_job(&job.job_id).unwrap();
                let done = app
                    .complete_job(
                        &job.job_id,
                        "export-worker",
                        running.attempt,
                        now(),
                        Value::Null,
                    )
                    .unwrap();
                assert_eq!(done.state, JobState::Cancelled);
                assert!(done.result.is_none());
            }
            assert!(app.job_view(&job.job_id).unwrap().artifacts.is_empty());
        }
    }
}
