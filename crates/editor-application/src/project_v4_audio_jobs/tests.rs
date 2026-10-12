use super::*;
use crate::{JobState, project_v4::new_document};
use opencut_contracts::{
    AssetVersion, CreateProject, FrameRate, LockedAsset, Time, TimeRange, project_v4::*,
};
use std::cell::{Cell, RefCell};

fn time(ticks: i64, timebase: u32) -> Time {
    Time {
        ticks: ticks.to_string(),
        timebase,
    }
}
fn snapshot() -> ProjectSnapshotV4 {
    let mut project = new_document(&CreateProject {
        id: "project".into(),
        name: "Sound".into(),
        sequence_id: "scene".into(),
        width: 160,
        height: 90,
        fps: FrameRate {
            numerator: 30,
            denominator: 1,
        },
    })
    .unwrap();
    project.sequences[0].main_track_id = "main".into();
    project.sequences[0].tracks[0].id = "main".into();
    let resource = |id: &str, hash: &str| AssetV4 {
        id: id.into(),
        name: id.into(),
        kind: AssetKind::Audio,
        version: AssetVersion {
            id: format!("av_{hash}"),
            content_hash: hash.into(),
            size: 16,
        },
        duration: Some(time(2, 1)),
        origin: ResourceOriginV4::LegacyLocator {
            uri: "/private/source-do-not-publish.wav".into(),
        },
        metadata: MediaMetadataV4 {
            mime_type: Some("audio/wav".into()),
            intrinsic_size: None,
            frame_rate: None,
            rotation_degrees: None,
            audio: AudioPresenceV4::Present {
                metadata: Some(AudioMetadataV4 {
                    sample_rate: 48000,
                    channels: 1,
                    channel_layout: None,
                }),
            },
        },
        license: None,
    };
    project.assets = vec![
        resource("audible", &"a".repeat(64)),
        resource("unused", &"b".repeat(64)),
    ];
    let element = |id: &str, start: i64, asset: &str, enabled: bool| TimelineElementV4 {
        id: id.into(),
        name: id.into(),
        timeline_range: TimeRange {
            start: time(start, 1),
            duration: time(1, 7),
        },
        source_span: ElementSourceSpanV4::ClassicTrimmed {
            trim_start: time(1, 4),
            trim_end: time(0, 1),
            source_duration: None,
        },
        enabled,
        content: ElementContentV4::Audio {
            asset_id: asset.into(),
            audio: AudioStyleV4 {
                stream: None,
                gain: GainV4::Decibels { value: 0.0 },
                muted: false,
            },
            retime: RetimeV4 {
                rate: RationalRateV4 {
                    numerator: 1,
                    denominator: 1,
                },
                maintain_pitch: false,
                mapping: RetimeMappingV4::ClassicRoundedTicks { timebase: 120000 },
            },
        },
        visual: None,
        channels: vec![],
    };
    let mut track = project.sequences[0].tracks[0].clone();
    track.id = "sound".into();
    track.kind = TrackKindV4::Audio;
    track.elements = vec![
        element("voice-a", 0, "audible", true),
        element("voice-b", 1, "audible", true),
        element("disabled", 2, "unused", false),
    ];
    project.sequences[0].tracks.push(track);
    ProjectSnapshotV4 {
        project,
        revision: "fixed-r1".into(),
    }
}
fn policy() -> LocalAudioExecutionPolicyV1 {
    LocalAudioExecutionPolicyV1 {
        profile: LocalAudioProcessingProfileV1::LocalClassicAudioV1,
        max_pcm_bytes: 16 * 1024 * 1024,
        max_encoded_bytes: 1024 * 1024,
        max_rss_bytes: 64 * 1024 * 1024,
        deadline_ms: 20000,
    }
}
fn request() -> SubmitV4AudioWav {
    SubmitV4AudioWav {
        idempotency_key: "audio-key".into(),
        project_id: "project".into(),
        revision: "fixed-r1".into(),
        sequence_id: "scene".into(),
        sample_rate: 48000,
    }
}
struct Store {
    snapshot: ProjectSnapshotV4,
    receipt: Option<JobRecord>,
    commits: usize,
    denied: bool,
}
impl Store {
    fn new() -> Self {
        Self {
            snapshot: snapshot(),
            receipt: None,
            commits: 0,
            denied: false,
        }
    }
}
impl V4AudioJobSubmissionStore for Store {
    fn fixed_v4_audio_snapshot(
        &self,
        _: &str,
        _: &str,
    ) -> Result<AuthorizedV4AudioSnapshot, EditorError> {
        if self.denied {
            return Err(crate::access::denied());
        }
        Ok(AuthorizedV4AudioSnapshot {
            snapshot: self.snapshot.clone(),
            authority: TrustedJobAuthorityV1::Credential {
                principal_id: "agent".into(),
                credential_id: "credential".into(),
            },
            processing_policy: policy(),
        })
    }
    fn find_accepted_v4_audio_job(&self, _: &JobRequest) -> Result<Option<JobRecord>, EditorError> {
        Ok(self.receipt.clone())
    }
    fn accept_v4_audio_job(
        &mut self,
        request: JobRequest,
        prepared: PreparedV4AudioWav,
    ) -> Result<JobRecord, EditorError> {
        validate_prepared_request(&request, &prepared)?;
        self.commits += 1;
        let record = JobRecord {
            job_id: format!("job_{}", "c".repeat(32)),
            state: JobState::Queued,
            attempt: 0,
            kind: request.kind,
            input: serde_json::to_value(prepared).unwrap(),
            result: None,
            error: None,
            lease_owner: None,
            lease_until: None,
            cancel_requested: false,
            progress: None,
        };
        self.receipt = Some(record.clone());
        Ok(record)
    }
}
#[derive(Default)]
struct Capture {
    calls: Cell<usize>,
    ids: RefCell<Vec<String>>,
    corrupt: bool,
}
impl V4AudioAssetCapture for Capture {
    fn capture(
        &self,
        snapshot: &ProjectSnapshotV4,
        sequence: &str,
        ids: &[String],
    ) -> Result<AssetLockManifest, EditorError> {
        self.calls.set(self.calls.get() + 1);
        *self.ids.borrow_mut() = ids.to_vec();
        Ok(AssetLockManifest {
            version: 1,
            revision: snapshot.revision.clone(),
            sequence_id: sequence.into(),
            assets: ids
                .iter()
                .map(|id| {
                    let asset = snapshot
                        .project
                        .assets
                        .iter()
                        .find(|a| a.id == *id)
                        .unwrap();
                    LockedAsset {
                        asset_id: id.clone(),
                        content_hash: asset.version.content_hash.clone(),
                        size: if self.corrupt { 99 } else { asset.version.size },
                    }
                })
                .collect(),
        })
    }
}

#[test]
fn explicit_stream_zero_is_not_implicitly_rebound_to_the_primary_audio_track() {
    for stream in [0, 1] {
        let mut store = Store::new();
        let ElementContentV4::Audio { audio, .. } =
            &mut store.snapshot.project.sequences[0].tracks[1].elements[0].content
        else {
            panic!("Audio fixture changed");
        };
        audio.stream = Some(stream);
        let error = submit_audio_wav(&mut store, &Capture::default(), request()).unwrap_err();
        assert_eq!(error.code, "UNSUPPORTED_AUDIO_CAPABILITY");
        assert_eq!(store.commits, 0);
    }
    let mut store = Store::new();
    submit_audio_wav(&mut store, &Capture::default(), request()).unwrap();
    assert_eq!(store.commits, 1, "None retains the primary-track profile");
}

#[test]
fn submission_locks_unique_participating_resources_preserves_fixed_aggregate_and_replay_reauthorizes()
 {
    let mut store = Store::new();
    let capture = Capture::default();
    let original = request();
    let accepted = submit_audio_wav(&mut store, &capture, original.clone()).unwrap();
    let prepared: PreparedV4AudioWav = serde_json::from_value(accepted.input.clone()).unwrap();
    assert!(prepared.output_range.is_none());
    assert!(
        !accepted
            .input
            .as_object()
            .unwrap()
            .contains_key("output_range")
    );
    assert_eq!(prepared.snapshot, store.snapshot);
    assert_eq!(*capture.ids.borrow(), vec!["audible"]);
    assert_eq!(prepared.asset_lock.assets.len(), 1);
    let audio = validate_prepared(&prepared).unwrap();
    assert_eq!(audio.voices().count(), 2);
    // The disabled tail still contributes to authored scene duration, while its
    // resource is not captured or decoded. 15/7 seconds => ceil(48000*15/7).
    assert_eq!(audio.duration_samples(), 102858);
    assert_eq!(
        audio
            .voices()
            .next()
            .unwrap()
            .source
            .clock
            .nominal_request
            .start,
        time(1, 4)
    );
    assert_eq!(
        submit_audio_wav(&mut store, &capture, original.clone()).unwrap(),
        accepted
    );
    assert_eq!(capture.calls.get(), 1);
    assert_eq!(store.commits, 1);
    store.denied = true;
    assert_eq!(
        submit_audio_wav(&mut store, &capture, original)
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
}

mod range;

#[test]
fn malformed_lock_rate_and_request_preparation_binding_never_accept_a_job() {
    let mut store = Store::new();
    let capture = Capture {
        corrupt: true,
        ..Capture::default()
    };
    assert_eq!(
        submit_audio_wav(&mut store, &capture, request())
            .unwrap_err()
            .code,
        "ASSET_LOCK_INVALID"
    );
    assert_eq!(store.commits, 0);
    let mut wrong_rate = request();
    wrong_rate.sample_rate = 96000;
    assert_eq!(
        submit_audio_wav(&mut store, &capture, wrong_rate)
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
    assert_eq!(capture.calls.get(), 1);
    let capture = Capture::default();
    let record = submit_audio_wav(&mut store, &capture, request()).unwrap();
    let prepared: PreparedV4AudioWav = serde_json::from_value(record.input).unwrap();
    let mut changed = request();
    changed.revision = "different-fixed-revision".into();
    let intent = JobRequest {
        kind: V4_AUDIO_WAV_KIND.into(),
        idempotency_key: changed.idempotency_key.clone(),
        input: serde_json::to_value(changed).unwrap(),
    };
    assert_eq!(
        validate_prepared_request(&intent, &prepared)
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
}

fn result(prepared: &PreparedV4AudioWav) -> StoredV4AudioWavResultV1 {
    let audio = validate_prepared(prepared).unwrap();
    let hash = "d".repeat(64);
    StoredV4AudioWavResultV1 {
        public: V4AudioWavResultV1 {
            schema_version: 1,
            identity: V4AudioWavIdentityV1 {
                project_id: prepared.snapshot.project.id.clone(),
                revision: prepared.snapshot.revision.clone(),
                sequence_id: prepared.sequence_id.clone(),
                profile: V4AudioProfileV1::ClassicExportV31WebAudio,
                sample_rate: prepared.sample_rate,
                channels: 2,
                frame_count: audio.duration_samples().to_string(),
                sample_format: V4AudioSampleFormatV1::IeeeFloat32Le,
            },
            asset_lock: prepared.asset_lock.clone(),
            render_input_sha256: render_input_sha256(
                &prepared.snapshot,
                &prepared.sequence_id,
                prepared.sample_rate,
            )
            .unwrap(),
            execution: V4AudioExecutionSummaryV1 {
                host_sha256: "e".repeat(64),
                browser_version: "Chromium pinned".into(),
                browser_sha256: "f".repeat(64),
                module_sha256: "a".repeat(64),
                playwright_version: "1.63.0".into(),
                mediabunny_version: "1.41.0".into(),
                soundtouch_version: "0.3.0".into(),
                decoder_profiles: vec![V4AudioDecoderProfileV1::WebAudioDecodeAtTargetRate],
                pitch_profiles: vec![V4AudioPitchProfileV1::UnpreparedLinear],
                mastering_profile: V4AudioMasteringProfileV1::ClassicExportWholeSceneWebAudioV31,
                mastering_applied: false,
            },
            artifact: V4AudioArtifactMetadataV1 {
                mime_type: "audio/wav".into(),
                size_bytes: "822922".into(),
                content_hash: hash.clone(),
            },
        },
        artifact: ArtifactDescriptor {
            path: "/private/attempt/render.wav".into(),
            mime_type: "audio/wav".into(),
            size: 822922,
            content_hash: hash,
        },
    }
}

#[test]
fn public_result_is_path_free_and_full_snapshot_digest_rejects_non_audio_changes() {
    let mut store = Store::new();
    let mut record = submit_audio_wav(&mut store, &Capture::default(), request()).unwrap();
    let mut prepared: PreparedV4AudioWav = serde_json::from_value(record.input.clone()).unwrap();
    let stored = result(&prepared);
    record.state = JobState::Succeeded;
    record.attempt = 1;
    record.result = Some(serde_json::to_value(&stored).unwrap());
    let view = crate::job_views::project(record.clone());
    assert_eq!(view.target.unwrap().revision, "fixed-r1");
    assert_eq!(view.artifacts.len(), 1);
    let json = serde_json::to_string(&view.extended_result).unwrap();
    assert!(
        !json.contains("/private/")
            && !json.contains("credential")
            && !json.contains("\"path\"")
            && !json.contains("\"uri\"")
    );
    let data: V4AudioWavResultV1 =
        serde_json::from_value(view.extended_result.unwrap().data).unwrap();
    assert_eq!(data, stored.public);
    // Matching public/private descriptors cannot authorize a short or padded
    // file: its exact byte count comes independently from the fixed scene.
    for size in [stored.artifact.size - 8, stored.artifact.size + 8] {
        let mut wrong_size = stored.clone();
        wrong_size.artifact.size = size;
        wrong_size.public.artifact.size_bytes = size.to_string();
        assert_eq!(
            validate_stored_result(&prepared, &wrong_size)
                .unwrap_err()
                .code,
            "INVALID_STORED_RESULT"
        );
        let mut invalid_record = record.clone();
        invalid_record.result = Some(serde_json::to_value(wrong_size).unwrap());
        let unavailable = crate::job_views::project(invalid_record);
        assert!(
            unavailable.extended_result.is_none()
                && unavailable.artifacts.is_empty()
                && unavailable.result_unavailable.is_some()
        );
    }
    prepared.snapshot.project.sequences[0].bookmarks.push(
        opencut_contracts::scene_bookmarks::SceneBookmarkV1 {
            id: "note".into(),
            sequence_id: "scene".into(),
            time: time(0, 1),
            duration: None,
            color: None,
            note: Some("Non-audio edit".into()),
        },
    );
    assert_ne!(
        render_input_sha256(
            &prepared.snapshot,
            &prepared.sequence_id,
            prepared.sample_rate
        )
        .unwrap(),
        stored.public.render_input_sha256
    );
    assert_eq!(
        validate_stored_result(&prepared, &stored).unwrap_err().code,
        "INVALID_STORED_RESULT"
    );
    record.input = serde_json::to_value(prepared).unwrap();
    let denied = crate::job_views::project(record);
    assert!(
        denied.extended_result.is_none()
            && denied.artifacts.is_empty()
            && denied.result_unavailable.is_some()
    );
}
