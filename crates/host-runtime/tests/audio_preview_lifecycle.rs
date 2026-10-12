//! Actual scoped host -> CAS -> long-lived Node/Chromium lifecycle acceptance.
//! Run explicitly with the installed OPENCUT_CLASSIC_AUDIO_* runtime and
//! --ignored --test-threads=1. No FFmpeg, user media or injected clock is used.
#![cfg(unix)]

use opencut_asset_store::{AssetStore, gate::Gate};
use opencut_contracts::{
    Asset, AssetKind, CreateProject, EditorError, FrameRate, Time, TimeRange, project_v4::*,
    project_v4_ops::*,
};
use opencut_editor_application::access::{PrincipalPolicy, ProjectAction, ProjectGrant};
use opencut_host_runtime::{
    Automation, CallSource, audio_preview_contracts::*, audio_preview_v4::AudioPreviewStream,
    transport_io::StreamRead,
};
use opencut_sqlite_store::EditorApplication;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    process::Command,
    sync::{Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};

static SERIAL: Mutex<()> = Mutex::new(());
const WAIT: Duration = Duration::from_secs(25);

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}
fn time(ticks: u32) -> Time {
    Time {
        ticks: ticks.to_string(),
        timebase: 1,
    }
}
fn call(host: &mut Automation, tool: &str, input: Value) -> Result<Value, EditorError> {
    let response = host.call(tool, input);
    if let Some(error) = response.error {
        return Err(EditorError::new(error.code, error.message));
    }
    Ok(response.data.expect("Successful typed tool response"))
}
fn float_wav() -> Vec<u8> {
    // Four seconds of mathematically exact .25 mono, independently hand encoded.
    let data_bytes = 4 * 48_000 * 4u32;
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((data_bytes + 36).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(3u16.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(48_000u32.to_le_bytes());
    bytes.extend(192_000u32.to_le_bytes());
    bytes.extend(4u16.to_le_bytes());
    bytes.extend(32u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(data_bytes.to_le_bytes());
    for _ in 0..4 * 48_000 {
        bytes.extend(0.25f32.to_le_bytes());
    }
    bytes
}

#[derive(Clone, Debug)]
struct Process {
    pid: u32,
    parent: u32,
    started: String,
    command: String,
}
fn processes() -> Vec<Process> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,lstart=,command="])
        .output()
        .expect("Read-only process observation must be permitted");
    assert!(
        output.status.success(),
        "ps: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| {
            let parts: Vec<_> = line.split_whitespace().collect();
            if parts.len() < 8 {
                return None;
            }
            Some(Process {
                pid: parts[0].parse().unwrap(),
                parent: parts[1].parse().unwrap(),
                started: parts[2..7].join(" "),
                command: parts[7..].join(" "),
            })
        })
        .collect()
}
fn same_process(a: &Process, b: &Process) -> bool {
    a.pid == b.pid && a.started == b.started && a.command == b.command
}
fn source_processes() -> Vec<Process> {
    let package = std::fs::canonicalize(
        std::env::var_os("OPENCUT_CLASSIC_AUDIO_PACKAGE")
            .expect("Set OPENCUT_CLASSIC_AUDIO_PACKAGE to the installed pinned package"),
    )
    .unwrap();
    let runner = package
        .join("host/preview-runner.mjs")
        .to_string_lossy()
        .into_owned();
    let observed = processes();
    let nodes: Vec<_> = observed
        .iter()
        .filter(|p| p.parent == std::process::id() && p.command.contains(&runner))
        .cloned()
        .collect();
    assert_eq!(
        nodes.len(),
        1,
        "Expected this test's one actual producer Node: {nodes:?}"
    );
    let mut pids: BTreeSet<_> = nodes.iter().map(|p| p.pid).collect();
    loop {
        let before = pids.len();
        for p in &observed {
            if pids.contains(&p.parent) {
                pids.insert(p.pid);
            }
        }
        if pids.len() == before {
            break;
        }
    }
    let tree: Vec<_> = observed
        .into_iter()
        .filter(|p| pids.contains(&p.pid))
        .collect();
    assert!(
        tree.iter()
            .any(|p| p.parent == nodes[0].pid && p.command.contains("--remote-debugging-pipe")),
        "Actual Chromium was not observed: {tree:?}"
    );
    tree
}

struct Fixture {
    directory: tempfile::TempDir,
    database: PathBuf,
    cas: PathBuf,
    owner: EditorApplication,
    host: Automation,
    credential: String,
    session: AudioPreviewSessionV4,
    stream: Option<AudioPreviewStream>,
    processes: Vec<Process>,
    created_at: Instant,
}
impl Fixture {
    fn new() -> Self {
        // Do not mutate process-global environment or silently skip real execution.
        assert!(std::env::var_os("OPENCUT_CLASSIC_AUDIO_CHROMIUM").is_some());
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("workspace.sqlite");
        let mut owner = EditorApplication::open(&database).unwrap();
        let initial = owner
            .create_v4_project(&CreateV4Project {
                project: CreateProject {
                    id: "lifecycle".into(),
                    name: "Synthetic lifecycle".into(),
                    sequence_id: "scene".into(),
                    width: 16,
                    height: 16,
                    fps: FrameRate {
                        numerator: 30,
                        denominator: 1,
                    },
                },
                idempotency_key: "create".into(),
            })
            .unwrap();
        let cas = owner.asset_store_root().to_path_buf();
        let source = directory.path().join("synthetic.wav");
        std::fs::write(&source, float_wav()).unwrap();
        let pinned = AssetStore::open(&cas)
            .unwrap()
            .pin_asset(&Asset {
                id: "source".into(),
                name: "Synthetic Float WAV".into(),
                kind: AssetKind::Audio,
                uri: source.to_string_lossy().into_owned(),
                duration: Some(time(4)),
                version: None,
                metadata: BTreeMap::new(),
            })
            .unwrap();
        let asset = AssetV4 {
            id: "source".into(),
            name: "Synthetic Float WAV".into(),
            kind: AssetKind::Audio,
            version: pinned.version.unwrap(),
            duration: Some(time(4)),
            origin: ResourceOriginV4::LegacyLocator {
                uri: "synthetic.wav".into(),
            },
            metadata: MediaMetadataV4 {
                mime_type: Some("audio/wav".into()),
                intrinsic_size: None,
                frame_rate: None,
                rotation_degrees: None,
                audio: AudioPresenceV4::Present {
                    metadata: Some(AudioMetadataV4 {
                        sample_rate: 48_000,
                        channels: 1,
                        channel_layout: None,
                    }),
                },
            },
            license: None,
        };
        let track = TrackV4 {
            id: "sound".into(),
            name: "Sound".into(),
            kind: TrackKindV4::Audio,
            enabled: true,
            muted: false,
            locked: false,
            opacity: 1.0,
            gain: GainV4::Decibels { value: 0.0 },
            channels: vec![],
            elements: vec![TimelineElementV4 {
                id: "voice".into(),
                name: "Voice".into(),
                enabled: true,
                timeline_range: TimeRange {
                    start: time(0),
                    duration: time(4),
                },
                source_span: ElementSourceSpanV4::ClassicTrimmed {
                    trim_start: time(0),
                    trim_end: time(0),
                    source_duration: Some(time(4)),
                },
                content: ElementContentV4::Audio {
                    asset_id: "source".into(),
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
                        mapping: RetimeMappingV4::ClassicRoundedTicks { timebase: 120_000 },
                    },
                },
                visual: None,
                channels: vec![],
            }],
        };
        let fixed = owner
            .apply_v4_project(&ApplyV4Project {
                project_id: "lifecycle".into(),
                expected_revision: initial.revision,
                idempotency_key: "bind".into(),
                label: "Bind synthetic CAS".into(),
                operations: vec![
                    ProjectOperationV4::AssetPut { asset },
                    ProjectOperationV4::TrackPut {
                        sequence_id: "scene".into(),
                        track,
                    },
                ],
            })
            .unwrap();
        std::fs::remove_file(source).unwrap(); // Only CAS can supply the decoder.
        owner
            .set_principal_policy(PrincipalPolicy {
                principal_id: "listener".into(),
                expected_revision: "0".into(),
                enabled: true,
                grants: vec![ProjectGrant {
                    project_id: "lifecycle".into(),
                    actions: vec![ProjectAction::Read, ProjectAction::RunJobs],
                }],
            })
            .unwrap();
        let issued = owner.issue_credential("listener").unwrap();
        let caller = owner.authenticate(&issued.secret).unwrap();
        let mut host = Automation::open(&database)
            .unwrap()
            .with_source(CallSource::Http)
            .with_caller(caller);
        drop(Gate::exclusive(&cas).unwrap());
        let created_at = Instant::now();
        let session = serde_json::from_value(call(&mut host, "opencut_audio_preview_v4_create", json!({
            "project_id":"lifecycle", "revision":fixed.revision, "sequence_id":"scene", "sample_rate":48000,
            "start_time":{"ticks":"0","timebase":1}, "idempotency_key":"listen" })).unwrap()).unwrap();
        let mut fixture = Self {
            directory,
            database,
            cas,
            owner,
            host,
            credential: issued.credential_id,
            session,
            stream: None,
            processes: vec![],
            created_at,
        };
        fixture.control(AudioPreviewCommandV4::Play);
        let input = fixture.stream_input();
        fixture.stream = Some(fixture.host.audio_preview_stream(input).unwrap());
        fixture.chunk(0);
        fixture.processes = source_processes();
        assert_eq!(
            Gate::exclusive(&fixture.cas).unwrap_err().code,
            "STORAGE_BUSY",
            "A live prepared realm must hold the actual CAS collection gate"
        );
        fixture
    }
    fn stream_input(&self) -> AudioPreviewStreamV4 {
        AudioPreviewStreamV4 {
            session_id: self.session.session_id.clone(),
            generation: self.session.generation.clone(),
            overlay_epoch: self.session.overlay_epoch.clone(),
            owner_id: self.session.owner_id.clone(),
            lease_id: self.session.lease_id.clone(),
        }
    }
    fn control(&mut self, command: AudioPreviewCommandV4) {
        self.session = serde_json::from_value(
            call(
                &mut self.host,
                "opencut_audio_preview_v4_control",
                json!({
            "session_id":self.session.session_id, "generation":self.session.generation,
            "owner_id":self.session.owner_id, "lease_id":self.session.lease_id,
            "control_id":uuid::Uuid::new_v4().to_string(), "command":command }),
            )
            .unwrap(),
        )
        .unwrap();
    }
    fn chunk(&mut self, start: u64) {
        let until = Instant::now() + Duration::from_secs(120);
        let bytes = loop {
            match self
                .stream
                .as_mut()
                .unwrap()
                .read(Duration::from_millis(200))
            {
                Ok(StreamRead::Chunk(bytes)) => break bytes,
                Ok(StreamRead::Timeout) => {
                    assert!(Instant::now() < until, "Producer never became ready")
                }
                Ok(StreamRead::End) => panic!("Premature stream end"),
                Err(error) => self.start_failure(error),
            }
        };
        assert_eq!(&bytes[..4], b"OCA4");
        let header_len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let payload_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let header: AudioPreviewPcmHeaderV4 =
            serde_json::from_slice(&bytes[12..12 + header_len]).unwrap();
        let pcm = &bytes[12 + header_len..];
        assert_eq!(pcm.len(), payload_len);
        assert_eq!(header.start_sample, start.to_string());
        assert_eq!(header.frame_count, 4096);
        assert_eq!(header.content_hash, format!("{:x}", Sha256::digest(pcm)));
        for bytes in pcm.chunks_exact(4) {
            assert_eq!(f32::from_le_bytes(bytes.try_into().unwrap()), 0.25);
        }
    }
    fn start_failure(&self, error: EditorError) -> ! {
        // Receiver disconnection may precede run's machine.fail by one thread
        // scheduling turn. Observe the real public terminal cause; do not retry
        // the producer, substitute identities or obscure the initial read error.
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut owner = Automation::open(&self.database).unwrap();
        let view = loop {
            let view = call(
                &mut owner,
                "opencut_audio_preview_v4_inspect",
                json!({"session_id":self.session.session_id}),
            );
            if view
                .as_ref()
                .is_ok_and(|value| value["failure_code"].is_string())
                || Instant::now() >= deadline
            {
                break view;
            }
            thread::sleep(Duration::from_millis(50));
        };
        let events = call(
            &mut owner,
            "opencut_audio_preview_v4_events",
            json!({"session_id":self.session.session_id,"cursor":null}),
        );
        panic!(
            "Native preview read failed: {error:?}; Play identity={:?}; current={view:?}; events={events:?}",
            self.stream_input()
        );
    }
    fn backpressure(&mut self) {
        self.chunk(4096);
        assert!(matches!(
            self.stream
                .as_mut()
                .unwrap()
                .read(Duration::from_millis(200))
                .unwrap(),
            StreamRead::Timeout
        ));
        let value = self.inspect();
        assert_eq!(value.buffered_frames, "8192");
        assert_eq!(value.consumed_frames, "0");
    }
    fn inspect(&self) -> AudioPreviewSessionV4 {
        let mut owner = Automation::open(&self.database).unwrap();
        serde_json::from_value(
            call(
                &mut owner,
                "opencut_audio_preview_v4_inspect",
                json!({"session_id":self.session.session_id}),
            )
            .unwrap(),
        )
        .unwrap()
    }
    fn assert_terminated(&mut self, expected: AudioPreviewStateV4) {
        let until = Instant::now() + WAIT;
        loop {
            let observed = processes();
            let survivors: Vec<_> = self
                .processes
                .iter()
                .filter(|old| observed.iter().any(|current| same_process(old, current)))
                .collect();
            let gate = Gate::exclusive(&self.cas).map(drop);
            let view = self.inspect();
            if survivors.is_empty() && gate.is_ok() && view.state == expected {
                break;
            }
            assert!(
                Instant::now() < until,
                "Native realm not terminated: survivors={survivors:?}, gate={gate:?}, state={view:?}"
            );
            thread::sleep(Duration::from_millis(100));
        }
        // Check the old data-plane handle and a fresh owner transport, not only
        // machine state. A credential denial alone must not hide a live producer.
        if let Some(stream) = self.stream.as_mut() {
            assert!(
                stream.read(Duration::from_millis(200)).is_err(),
                "Terminal session emitted PCM"
            );
        }
        let input = self.stream_input();
        let mut owner = Automation::open(&self.database).unwrap();
        assert!(
            owner.audio_preview_stream(input).is_err(),
            "Terminal session accepted a new PCM stream"
        );
        let still_empty = processes();
        assert!(
            !still_empty
                .iter()
                .any(|p| self.processes.iter().any(|old| same_process(old, p)))
        );
        drop(Gate::exclusive(&self.cas).unwrap());
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stream.take();
        let _ = opencut_host_runtime::audio_preview_v4::shutdown(&self.database);
        // Keep the TempDir alive until shutdown joins the actual native actor.
        let _ = self.directory.path();
    }
}

#[test]
#[ignore = "Requires pinned OPENCUT_CLASSIC_AUDIO_* runtime and permitted ps; run serially"]
fn credential_revoke_idle_and_backpressure_reaps_actual_realm() {
    let _serial = serial();
    for idle in [true, false] {
        let mut fixture = Fixture::new();
        if idle {
            fixture.stream.take();
        } else {
            fixture.backpressure();
        }
        fixture
            .owner
            .revoke_credential(&fixture.credential)
            .unwrap();
        fixture.assert_terminated(AudioPreviewStateV4::Failed);
    }
}

#[test]
#[ignore = "Requires pinned OPENCUT_CLASSIC_AUDIO_* runtime and permitted ps; run serially"]
fn read_or_run_grant_revoke_reaps_backpressured_realm() {
    let _serial = serial();
    // Project grants require Read: revoke RunJobs by retaining Read-only,
    // then revoke Read and RunJobs together by removing the project grant.
    for remove_project_grant in [false, true] {
        let mut fixture = Fixture::new();
        fixture.backpressure();
        let grants = if remove_project_grant {
            vec![]
        } else {
            vec![ProjectGrant {
                project_id: "lifecycle".into(),
                actions: vec![ProjectAction::Read],
            }]
        };
        fixture
            .owner
            .set_principal_policy(PrincipalPolicy {
                principal_id: "listener".into(),
                expected_revision: "1".into(),
                enabled: true,
                grants,
            })
            .unwrap();
        fixture.assert_terminated(AudioPreviewStateV4::Failed);
    }
}

#[test]
#[ignore = "Uses the real 30-second lease and pinned OPENCUT_CLASSIC_AUDIO_* runtime; run serially"]
fn owner_lease_expires_after_real_thirty_seconds_and_releases_cas() {
    let _serial = serial();
    let mut fixture = Fixture::new();
    fixture.backpressure();
    // No heartbeat, clock substitution or downloaded-PCM consumption report.
    while fixture.created_at.elapsed() < Duration::from_secs(29) {
        thread::sleep(Duration::from_millis(100));
    }
    let before_expiry = fixture.inspect();
    assert_eq!(before_expiry.state, AudioPreviewStateV4::Playing);
    assert_ne!(before_expiry.lease_remaining_ms, "0");
    assert_eq!(
        Gate::exclusive(&fixture.cas).unwrap_err().code,
        "STORAGE_BUSY"
    );
    let still_live = processes();
    assert!(
        fixture
            .processes
            .iter()
            .filter(|p| p.command.contains("preview-runner.mjs")
                || p.command.contains("--remote-debugging-pipe"))
            .all(|old| still_live.iter().any(|current| same_process(old, current)))
    );
    while fixture.created_at.elapsed() < Duration::from_secs(30) {
        thread::sleep(Duration::from_millis(100));
    }
    fixture.assert_terminated(AudioPreviewStateV4::Stopped);
    let expired = fixture.inspect();
    assert_eq!(expired.lease_remaining_ms, "0");
    assert_eq!(expired.failure_code.as_deref(), Some("PREVIEW_EXPIRED"));
}

#[test]
#[ignore = "Requires pinned OPENCUT_CLASSIC_AUDIO_* runtime and permitted ps; run serially"]
fn explicit_stop_reaps_backpressured_source_realm_before_returning() {
    let _serial = serial();
    let mut fixture = Fixture::new();
    fixture.backpressure();
    fixture.control(AudioPreviewCommandV4::Stop);
    assert_eq!(fixture.session.state, AudioPreviewStateV4::Stopped);
    let observed = processes();
    assert!(
        !observed
            .iter()
            .any(|p| fixture.processes.iter().any(|old| same_process(old, p))),
        "Stop returned before the actual source process tree exited"
    );
    drop(Gate::exclusive(&fixture.cas).unwrap());
    fixture.assert_terminated(AudioPreviewStateV4::Stopped);
}
