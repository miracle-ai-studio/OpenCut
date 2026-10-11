//! Native worker regression tests; compiled only for the test target.
use super::*;
use opencut_contracts::Time;
use opencut_editor_application::JobRequest;
use opencut_job_runtime::Publication;

#[test]
fn publication_retries_a_real_sqlite_writer_lock_without_duplicating_the_result() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("workspace.sqlite");
    let mut app = EditorApplication::open_managed(&database).unwrap();
    let job = app
        .create_job(JobRequest {
            kind: "render.project".into(),
            input: json!({}),
            idempotency_key: "publish-busy".into(),
        })
        .unwrap();
    let claimed = app.live_jobs().claim("worker", LEASE_MS).unwrap().unwrap();
    assert_eq!(claimed.job_id, job.job_id);
    drop(app);

    let blocker = rusqlite::Connection::open(&database).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let result = Ok(json!({"published": true}));
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let publish =
            scope.spawn(|| publish_with_busy_retry(&database, "worker", &claimed, &result, &stop));
        // Outlast one full SQLite busy timeout so the first publication
        // attempt observes the lock, then allow the fresh connection to
        // inspect and commit the same attempt.
        std::thread::sleep(Duration::from_millis(5_500));
        blocker.execute_batch("COMMIT").unwrap();
        assert!(matches!(
            publish.join().unwrap().unwrap(),
            Publication::Published
        ));
    });
    assert!(matches!(
        publish_with_busy_retry(&database, "worker", &claimed, &result, &stop).unwrap(),
        Publication::AlreadyTerminal
    ));
    let app = EditorApplication::open_managed(&database).unwrap();
    let finished = app.get_job(&job.job_id).unwrap();
    assert_eq!(finished.state, JobState::Succeeded);
    assert_eq!(finished.result, Some(json!({"published": true})));
    assert_eq!(
        app.job_events_after(0, 100, Some(&job.job_id))
            .unwrap()
            .into_iter()
            .filter(|event| event.event_type == "succeeded")
            .count(),
        1
    );
}

#[test]
fn versioned_inputs_require_lock_but_unused_versions_do_not_block_legacy_jobs() {
    let hash = "a".repeat(64);
    let mut snapshot: ProjectSnapshot = serde_json::from_value(json!({
        "revision": "revision-1",
        "project": {
            "id": "project", "name": "Project", "schema_version": 2,
            "default_sequence_id": "main",
            "assets": [{
                "id": "source", "name": "Source", "kind": "video",
                "uri": "/never-read-provenance.mov", "duration": null,
                "version": {"id": format!("av_{hash}"), "content_hash": hash, "size": 10}
            }],
            "sequences": [{
                "id": "main", "name": "Main", "width": 64, "height": 48,
                "fps": {"numerator": 10, "denominator": 1},
                "tracks": [{
                    "id": "video", "name": "Video", "kind": "video",
                    "enabled": true, "muted": false, "locked": false,
                    "clips": [{
                        "id": "clip", "asset_id": "source",
                        "timeline_range": {"start": {"ticks": "0", "timebase": 1},
                            "duration": {"ticks": "1", "timebase": 1}},
                        "source_range": {"start": {"ticks": "0", "timebase": 1},
                            "duration": {"ticks": "1", "timebase": 1}},
                        "params": {}, "channels": []
                    }]
                }]
            }]
        }
    }))
    .unwrap();
    let root = std::env::temp_dir().join(format!("opencut-no-lock-{}", uuid::Uuid::new_v4()));
    let cancel = CancellationToken::new();
    let mut progress = |_| panic!("Unlocked input must not start resolving assets");
    assert_eq!(
        resolve_inputs(
            &root,
            &root.join("inputs"),
            &snapshot,
            "main",
            None,
            &cancel,
            &mut progress
        )
        .unwrap_err()
        .code,
        "ASSET_LOCK_REQUIRED"
    );
    assert_eq!(
        resolve_visual_inputs(
            &root,
            &root.join("inputs"),
            &snapshot,
            "main",
            None,
            &cancel,
            &mut progress
        )
        .unwrap_err()
        .code,
        "ASSET_LOCK_REQUIRED"
    );
    let mut audio_only = snapshot.clone();
    audio_only.project.sequences[0].tracks[0].kind = opencut_contracts::TrackKind::Audio;
    audio_only.project.assets[0].kind = opencut_contracts::AssetKind::Audio;
    assert!(
        resolve_visual_inputs(
            &root,
            &root.join("inputs"),
            &audio_only,
            "main",
            None,
            &cancel,
            &mut progress
        )
        .is_ok()
    );
    assert_eq!(
        resolve_inputs(
            &root,
            &root.join("inputs"),
            &audio_only,
            "main",
            None,
            &cancel,
            &mut progress
        )
        .unwrap_err()
        .code,
        "ASSET_LOCK_REQUIRED"
    );
    snapshot.project.sequences[0].tracks[0].enabled = false;
    assert!(
        resolve_inputs(
            &root,
            &root.join("inputs"),
            &snapshot,
            "main",
            None,
            &cancel,
            &mut progress
        )
        .is_ok()
    );
    snapshot.project.sequences[0].tracks[0].enabled = true;
    snapshot.project.assets[0].version = None;
    assert!(
        resolve_inputs(
            &root,
            &root.join("inputs"),
            &snapshot,
            "main",
            None,
            &cancel,
            &mut progress
        )
        .is_ok()
    );
    assert!(!root.exists());
}

#[test]
fn project_frame_selection_rejects_end_and_does_not_clamp_alignment() {
    let fps = opencut_contracts::FrameRate {
        numerator: 10,
        denominator: 1,
    };
    let duration = MediaTime {
        ticks: 1,
        timebase: 1,
    };
    let requested = Time {
        ticks: "99".into(),
        timebase: 100,
    };
    let floor =
        select_project_frame_time(&requested, &fps, FrameAlignment::Floor, duration).unwrap();
    assert_eq!(
        floor,
        Time {
            ticks: "9".into(),
            timebase: 10
        }
    );
    assert!(
        select_project_frame_time(&requested, &fps, FrameAlignment::Nearest, duration).is_err()
    );
    assert!(select_project_frame_time(&requested, &fps, FrameAlignment::Ceil, duration).is_err());
    assert!(select_project_frame_time(&requested, &fps, FrameAlignment::Exact, duration).is_err());
    let end = Time {
        ticks: "1".into(),
        timebase: 1,
    };
    assert!(select_project_frame_time(&end, &fps, FrameAlignment::Floor, duration).is_err());
}

#[test]
fn preview_private_files_are_removed_when_backend_unwinds() {
    let directory =
        std::env::temp_dir().join(format!("opencut-preview-cleanup-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let intermediate = directory.join("preview-source.mp4");
    let frame = directory.join("frame.png");
    let inputs = directory.join("inputs");
    let result = std::panic::catch_unwind(|| {
        let _inputs = PrivateInputs(inputs.clone());
        fs::create_dir(&inputs).unwrap();
        fs::write(inputs.join("locked-input"), b"private verified copy").unwrap();
        let _private_files = PreviewFiles {
            intermediate: Some(intermediate.clone()),
            output: frame.clone(),
            keep_output: false,
        };
        fs::write(&intermediate, b"partial encoded movie").unwrap();
        fs::write(&frame, b"partial image").unwrap();
        panic!("injected media backend failure");
    });
    assert!(!inputs.exists());
    assert!(result.is_err());
    assert!(!intermediate.exists());
    assert!(!frame.exists());
    fs::remove_dir(directory).unwrap();
}

fn claimed_job(app: &mut EditorApplication) -> JobRecord {
    app.create_job(JobRequest {
        kind: "render.project".into(),
        input: json!({}),
        idempotency_key: "progress-test".into(),
    })
    .unwrap();
    app.claim_job("worker", now(), LEASE_MS).unwrap().unwrap()
}

fn encoding(fraction: f32) -> ProgressUpdate {
    ProgressUpdate {
        stage: ProgressStage::Encoding,
        fraction: Some(fraction),
        media_time: Some(opencut_media::MediaTime {
            ticks: 123,
            timebase: 1000,
        }),
    }
}

#[test]
fn recovery_cleanup_removes_abandoned_attempts_and_preserves_live_and_future_attempts() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let job = claimed_job(&mut app);
    let root =
        std::env::temp_dir().join(format!("opencut-input-recovery-{}", uuid::Uuid::new_v4()));
    for attempt in 0..=2 {
        let directory = root.join(&job.job_id).join(format!("attempt-{attempt}"));
        fs::create_dir_all(directory.join("inputs")).unwrap();
        fs::write(directory.join("inputs/source"), b"private media").unwrap();
        fs::write(directory.join("render.mp4"), b"artifact").unwrap();
    }
    clean_abandoned_inputs(&app, &root).unwrap();
    assert!(!root.join(&job.job_id).join("attempt-0").exists());
    assert!(
        root.join(&job.job_id)
            .join("attempt-1/inputs/source")
            .exists()
    );
    assert!(
        root.join(&job.job_id)
            .join("attempt-2/inputs/source")
            .exists()
    );
    assert_eq!(app.recover_expired(now() + LEASE_MS + 1).unwrap(), 1);
    clean_abandoned_inputs(&app, &root).unwrap();
    assert!(!root.join(&job.job_id).join("attempt-0").exists());
    assert!(!root.join(&job.job_id).join("attempt-1").exists());
    assert!(
        root.join(&job.job_id).join("attempt-2/render.mp4").exists(),
        "a directory newer than the database attempt may belong to a concurrent claimant"
    );
    let next = app.claim_job("next", now(), LEASE_MS).unwrap().unwrap();
    assert_eq!(next.attempt, 2);
    app.cancel_job(&next.job_id).unwrap();
    app.recover_expired(now() + LEASE_MS + 1).unwrap();
    assert_eq!(
        app.get_job(&next.job_id).unwrap().state,
        JobState::Cancelled
    );
    clean_abandoned_inputs(&app, &root).unwrap();
    assert!(!root.join(&job.job_id).join("attempt-2").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn recovery_cleans_unpublished_attempts_and_preserves_only_published_artifacts() {
    for (kind, output_name) in [
        ("preview.composited_frame", "frame.png"),
        ("preview.project_range", "preview.mp4"),
        ("render.project", "render.mp4"),
        ("preview.project_frame", "frame.png"),
    ] {
        let mut app = EditorApplication::open(":memory:").unwrap();
        app.create_job(JobRequest {
            kind: kind.into(),
            input: json!({}),
            idempotency_key: "direct-cleanup".into(),
        })
        .unwrap();
        let job = app.claim_job("first", now(), LEASE_MS).unwrap().unwrap();
        let root =
            std::env::temp_dir().join(format!("opencut-direct-cleanup-{}", uuid::Uuid::new_v4()));
        let first = root.join(&job.job_id).join("attempt-1");
        fs::create_dir_all(first.join("inputs")).unwrap();
        fs::write(first.join(output_name), b"unpublished preview").unwrap();
        fs::write(
            first.join(format!(".{output_name}.partial.caption-0.png")),
            b"private caption",
        )
        .unwrap();
        clean_abandoned_inputs(&app, &root).unwrap();
        assert!(first.join(output_name).exists());
        app.recover_expired(now() + LEASE_MS + 1).unwrap();
        clean_abandoned_inputs(&app, &root).unwrap();
        assert!(!first.exists());

        let next = app.claim_job("second", now(), LEASE_MS).unwrap().unwrap();
        let second = root.join(&job.job_id).join("attempt-2");
        fs::create_dir_all(second.join("inputs")).unwrap();
        let published = second.join(output_name);
        fs::write(&published, b"published preview").unwrap();
        app.complete_job(
            &job.job_id,
            "second",
            next.attempt,
            now(),
            json!({
                "artifact": { "path": published }
            }),
        )
        .unwrap();
        clean_abandoned_inputs(&app, &root).unwrap();
        assert!(published.exists());
        assert!(!second.join("inputs").exists());
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn progress_throttles_samples_but_persists_stage_boundaries_and_exact_time() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let job = claimed_job(&mut app);
    let cancel = CancellationToken::new();
    let start = Instant::now();
    {
        let mut reporter = ProgressReporter::new(app, None, "worker", &job, &cancel);
        reporter.report_at(encoding(0.1), start);
        reporter.report_at(encoding(0.2), start + Duration::from_millis(1));
        reporter.report_at(encoding(0.5), start + Duration::from_millis(300));
        reporter.report_at(encoding(1.0), start + Duration::from_millis(301));
        reporter.report_at(encoding(1.0), start + Duration::from_millis(302));
        reporter.report_at(
            ProgressUpdate {
                stage: ProgressStage::Verifying,
                fraction: None,
                media_time: None,
            },
            start + Duration::from_millis(303),
        );
        assert!(reporter.error.is_none());
        app = reporter.app.take().unwrap();
    }
    let events = app.job_events_after(0, 100, Some(&job.job_id)).unwrap();
    let progress: Vec<_> = events
        .iter()
        .filter(|event| event.event_type == "progress")
        .collect();
    assert_eq!(progress.len(), 4);
    assert_eq!(
        progress[0].payload["media_time"],
        json!({"ticks":"123","timebase":1000})
    );
    assert_eq!(progress[1].payload["fraction"], 0.5);
    assert_eq!(progress[2].payload["fraction"], 1.0);
    assert_eq!(progress[3].payload["stage"], "verifying");
    assert_eq!(progress[3].payload["fraction"], Value::Null);
    assert_eq!(app.get_job(&job.job_id).unwrap().state, JobState::Running);
    assert!(!cancel.is_cancelled());
}

#[test]
fn progress_yields_to_a_pending_heartbeat_then_resumes() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let job = claimed_job(&mut app);
    let cancel = CancellationToken::new();
    let pending = Arc::new(AtomicBool::new(true));
    let start = Instant::now();
    let mut reporter = ProgressReporter::new(app, None, "worker", &job, &cancel)
        .with_heartbeat_priority(pending.clone());
    reporter.report_at(encoding(0.2), start);
    assert!(reporter.last.is_none());
    pending.store(false, Ordering::SeqCst);
    reporter.report_at(encoding(0.5), start + Duration::from_millis(1));
    assert!(reporter.error.is_none());
    app = reporter.app.take().unwrap();
    let progress: Vec<_> = app
        .job_events_after(0, 100, Some(&job.job_id))
        .unwrap()
        .into_iter()
        .filter(|event| event.event_type == "progress")
        .collect();
    assert_eq!(progress.len(), 1);
    assert_eq!(progress[0].payload["fraction"], 0.5);
}

#[test]
fn stale_attempt_cancels_media_without_publishing_progress() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let job = claimed_job(&mut app);
    app.recover_expired(now() + LEASE_MS + 1).unwrap();
    let replacement = app
        .claim_job("replacement", now(), LEASE_MS)
        .unwrap()
        .unwrap();
    let before = app.job_events_after(0, 100, None).unwrap().len();
    let cancel = CancellationToken::new();
    {
        let mut reporter = ProgressReporter::new(app, None, "worker", &job, &cancel);
        reporter.report(encoding(0.5));
        assert_eq!(reporter.error.as_ref().unwrap().code, "JOB_LEASE_LOST");
        reporter.report(encoding(1.0));
        app = reporter.app.take().unwrap();
    }
    assert!(cancel.is_cancelled());
    assert_eq!(
        app.get_job(&job.job_id).unwrap().attempt,
        replacement.attempt
    );
    assert!(app.get_job(&job.job_id).unwrap().progress.is_none());
    assert_eq!(app.job_events_after(0, 100, None).unwrap().len(), before);
}

fn publish_with_busy_retry(
    database: &Path,
    worker: &str,
    job: &JobRecord,
    result: &Result<Value, EditorError>,
    stop: &AtomicBool,
) -> Result<Publication, EditorError> {
    opencut_job_runtime::publish_with_busy_retry(
        &SqliteFactory(database),
        worker,
        job,
        result,
        stop,
    )
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}
