//! Real Rust -> isolated Chromium -> PCM acceptance. Uses only synthetic local
//! Synthetic WAV/PCM-container CAS in a TempDir; no public job claim is made here.
use opencut_asset_store::AssetStore;
use opencut_composition::project_v4::*;
use opencut_contracts::{
    Asset, AssetKind, CreateProject, EditorError, FrameRate, Time, TimeRange, project_v4::*,
};
use opencut_editor_application::{classic_audio::*, project_v4::new_document};
use opencut_native_services::classic_audio::*;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

#[path = "classic_audio/decoder.rs"]
mod decoder;

fn time(n: i32, d: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: d,
    }
}
fn wav(path: &Path, tone: bool) {
    let samples: Vec<f32> = (0..if tone { 88_200 } else { 44_100 })
        .map(|n| {
            if tone {
                (2.0 * std::f64::consts::PI * 441.0 * n as f64 / 44_100.0).sin() as f32 * 0.25
            } else if n <= 5512 {
                0.0
            } else {
                0.5
            }
        })
        .collect();
    let bytes = samples.len() as u32 * 4;
    let mut output = Vec::new();
    output.extend(b"RIFF");
    output.extend((bytes + 36).to_le_bytes());
    output.extend(b"WAVEfmt ");
    output.extend(16u32.to_le_bytes());
    output.extend(3u16.to_le_bytes());
    output.extend(1u16.to_le_bytes());
    output.extend(44_100u32.to_le_bytes());
    output.extend(176_400u32.to_le_bytes());
    output.extend(4u16.to_le_bytes());
    output.extend(32u16.to_le_bytes());
    output.extend(b"data");
    output.extend(bytes.to_le_bytes());
    for sample in samples {
        output.extend(sample.to_le_bytes());
    }
    std::fs::write(path, output).unwrap();
}
fn fixture(directory: &Path, tone: bool, pitch: bool) -> (PathBuf, ProjectSnapshotV4) {
    let path = directory.join("synthetic.wav");
    wav(&path, tone);
    let root = directory.join("cas");
    let old = Asset {
        id: "source".into(),
        name: "Synthetic".into(),
        kind: AssetKind::Audio,
        uri: path.to_string_lossy().into(),
        duration: Some(time(if tone { 2 } else { 1 }, 1)),
        version: None,
        metadata: BTreeMap::new(),
    };
    let pinned = AssetStore::open(&root).unwrap().pin_asset(&old).unwrap();
    let mut project = new_document(&CreateProject {
        id: "p".into(),
        name: "Synthetic acceptance".into(),
        sequence_id: "s".into(),
        width: 320,
        height: 180,
        fps: FrameRate {
            numerator: 25,
            denominator: 1,
        },
    })
    .unwrap();
    project.assets.push(AssetV4 {
        id: "source".into(),
        name: "Synthetic".into(),
        kind: AssetKind::Audio,
        version: pinned.version.unwrap(),
        duration: Some(time(if tone { 2 } else { 1 }, 1)),
        origin: ResourceOriginV4::LegacyLocator {
            uri: "unused-provenance.wav".into(),
        },
        metadata: MediaMetadataV4 {
            mime_type: Some("audio/wav".into()),
            intrinsic_size: None,
            frame_rate: None,
            rotation_degrees: None,
            audio: AudioPresenceV4::Present {
                metadata: Some(AudioMetadataV4 {
                    sample_rate: 44_100,
                    channels: 1,
                    channel_layout: None,
                }),
            },
        },
        license: None,
    });
    let sequence = &mut project.sequences[0];
    let mut track = sequence.tracks[0].clone();
    track.id = "sound".into();
    track.name = "Sound".into();
    track.kind = TrackKindV4::Audio;
    track.elements.push(TimelineElementV4 {
        id: "voice".into(),
        name: "Voice".into(),
        enabled: true,
        timeline_range: TimeRange {
            start: if tone { time(0, 1) } else { time(1, 8) },
            duration: if tone { time(1, 1) } else { time(3, 8) },
        },
        source_span: ElementSourceSpanV4::ClassicTrimmed {
            trim_start: if tone { time(0, 1) } else { time(1, 8) },
            trim_end: if tone { time(0, 1) } else { time(1, 2) },
            source_duration: Some(time(if tone { 2 } else { 1 }, 1)),
        },
        content: ElementContentV4::Audio {
            asset_id: "source".into(),
            audio: AudioStyleV4 {
                stream: None,
                gain: GainV4::LinearLegacy { value: 0.5 },
                muted: false,
            },
            retime: RetimeV4 {
                rate: RationalRateV4 {
                    numerator: if tone { 2 } else { 1 },
                    denominator: 1,
                },
                maintain_pitch: pitch,
                mapping: RetimeMappingV4::ExactRational,
            },
        },
        visual: None,
        channels: vec![],
    });
    sequence.tracks.push(track);
    (
        root,
        ProjectSnapshotV4 {
            project,
            revision: "fixed-native-audio".into(),
        },
    )
}
#[derive(Default)]
struct Pcm {
    channels: [Vec<f32>; 2],
}
struct RevokeAfterRead<R> {
    inner: R,
    revoked: bool,
}
impl<R: ClassicAudioResources> ClassicAudioResources for RevokeAfterRead<R> {
    fn checkpoint(&mut self) -> Result<(), EditorError> {
        if self.revoked {
            return Err(EditorError::new(
                "NOT_AUTHORIZED",
                "Synthetic live revocation",
            ));
        }
        self.inner.checkpoint()
    }
    fn resolve(
        &mut self,
        resource: &AudioResource,
        handle: &str,
    ) -> Result<(String, u64), EditorError> {
        self.checkpoint()?;
        self.inner.resolve(resource, handle)
    }
    fn read(
        &mut self,
        transfer: &str,
        offset: u64,
        count: u32,
    ) -> Result<(Vec<u8>, bool), EditorError> {
        self.checkpoint()?;
        let result = self.inner.read(transfer, offset, count)?;
        self.revoked = true;
        Ok(result)
    }
    fn release(&mut self, transfer: &str) -> Result<(), EditorError> {
        self.checkpoint()?;
        self.inner.release(transfer)
    }
}
impl ClassicAudioPcmSink for Pcm {
    fn chunk(&mut self, channel: u32, offset: u64, samples: &[f32]) -> Result<(), EditorError> {
        assert_eq!(self.channels[channel as usize].len() as u64, offset);
        self.channels[channel as usize].extend_from_slice(samples);
        Ok(())
    }
}
fn config() -> ClassicAudioProcessConfig {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    ClassicAudioProcessConfig {
        node: std::env::var_os("OPENCUT_NODE")
            .unwrap_or_else(|| "node".into())
            .into(),
        chromium: std::env::var_os("OPENCUT_CHROME")
            .expect("Set OPENCUT_CHROME to the actual installed browser")
            .into(),
        runner: workspace.join("packages/classic-audio-renderer/host/runner.mjs"),
        deadline: Duration::from_secs(45),
        max_pcm_bytes: 64 * 1024 * 1024,
        max_encoded_bytes: 32 * 1024 * 1024,
        max_rss_bytes: 2 * 1024 * 1024 * 1024,
        strict_cgroup: None,
    }
}
#[test]
fn fixed_callback_admission_rejects_zero_decode_and_changed_final_pcm() {
    let directory = tempfile::tempdir().unwrap();
    let (root, snapshot) = fixture(directory.path(), false, false);
    let prepared = prepare_audio_v4(
        &snapshot,
        "s",
        AudioExecutionProfileV4::ClassicV31,
        PcmFormatV4 {
            sample_rate: 44_100,
            channels: 2,
        },
    )
    .unwrap();
    let mut resources = VerifiedClassicAudioResources::new(&root, &prepared, || Ok(())).unwrap();
    let mut session =
        ClassicAudioSession::new(&prepared, 64 * 1024 * 1024, 32 * 1024 * 1024).unwrap();
    let empty:ClassicAudioRenderReceipt=serde_json::from_value(serde_json::json!({
        "identity":session.plan().unwrap()["identity"],"engineIdentity":"synthetic",
        "decoders":[],"preparation":[],"mastering":{"profile":"classic-export-whole-scene-web-audio-v31","wholeSceneFrames":prepared.duration_samples().to_string(),"inputPeak":0.0,"applied":false},
        "range":null,"peakAccountedBytes":0
    })).unwrap();
    assert!(
        session.validate_receipt(&empty).is_err(),
        "Participating voices cannot be reported as empty execution"
    );
    let pitch = PitchRequest {
        element_id: "voice".into(),
        decoded_sample_rate: 44_100,
        decoded_frame_count: "44100".into(),
        decoded_channels: 1,
    };
    assert!(
        session
            .callback(AudioCallback::Pitch(pitch.clone()), &mut resources)
            .is_err()
    );
    let resource = session.resource("source").unwrap();
    let admission = session
        .callback(
            AudioCallback::Admit(DecodeAdmission {
                resource,
                primary: None,
            }),
            &mut resources,
        )
        .unwrap();
    assert_eq!(admission["maxChannels"], 1);
    assert!(admission["maxFrames"].as_u64().unwrap() >= 44_100);
    let mut zero = pitch.clone();
    zero.decoded_frame_count = "0".into();
    assert!(
        session
            .callback(AudioCallback::Pitch(zero), &mut resources)
            .is_err()
    );
    let mut channels = pitch.clone();
    channels.decoded_channels = 2;
    assert!(
        session
            .callback(AudioCallback::Pitch(channels), &mut resources)
            .is_err()
    );
    session
        .callback(AudioCallback::Pitch(pitch), &mut resources)
        .unwrap();
    for (channels, prepared, frames) in
        [(2, false, "44100"), (1, true, "44100"), (1, false, "44099")]
    {
        let formats = vec![FinalSourceFormat {
            element_id: "voice".into(),
            sample_rate: 44_100,
            frame_count: frames.into(),
            channels,
            prepared,
        }];
        assert!(
            session
                .callback(
                    AudioCallback::Block(BlockRequest {
                        start_sample: "5512".into(),
                        frame_count: 1,
                        source_formats: formats
                    }),
                    &mut resources
                )
                .is_err()
        );
    }
    let formats = vec![FinalSourceFormat {
        element_id: "voice".into(),
        sample_rate: 44_100,
        frame_count: "44100".into(),
        channels: 1,
        prepared: false,
    }];
    let block = session
        .callback(
            AudioCallback::Block(BlockRequest {
                start_sample: "5512".into(),
                frame_count: 1,
                source_formats: formats,
            }),
            &mut resources,
        )
        .unwrap();
    assert_eq!(block["identity"]["revision"], "fixed-native-audio");
    assert_eq!(
        block["voices"][0]["positions"][0],
        serde_json::json!({"lower":"5512","fractionNumerator":"1","fractionDenominator":"2"})
    );
}
#[test]
#[ignore = "Requires installed pinned browser runtime; run explicitly with OPENCUT_CHROME"]
fn rust_browser_fixed_trim_gain_pitch_and_termination() {
    for (tone, pitch, frequency) in [
        (false, false, 0),
        (false, true, 0),
        (true, false, 882),
        (true, true, 441),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (root, snapshot) = fixture(directory.path(), tone, pitch);
        let prepared = prepare_audio_v4(
            &snapshot,
            "s",
            AudioExecutionProfileV4::ClassicV31,
            PcmFormatV4 {
                sample_rate: 44_100,
                channels: 2,
            },
        )
        .unwrap();
        let mut resources =
            VerifiedClassicAudioResources::new(&root, &prepared, || Ok(())).unwrap();
        let handles = resources.handles();
        let mut session =
            ClassicAudioSession::new(&prepared, 64 * 1024 * 1024, 32 * 1024 * 1024).unwrap();
        let mut pcm = Pcm::default();
        let result = ClassicAudioProcess { config: config() }
            .execute(&mut session, &mut resources, &handles, &mut pcm)
            .unwrap();
        assert_eq!(result.receipt.identity.revision, "fixed-native-audio");
        assert!(!result.quota.hard_memory_enforced);
        assert_eq!(pcm.channels[0], pcm.channels[1]);
        assert_eq!(pcm.channels[0].len() as u64, prepared.duration_samples());
        if !tone {
            assert!(pcm.channels[0][..5512].iter().all(|&sample| sample == 0.0));
            assert_eq!(pcm.channels[0][5512], 0.125); // trim is 5512.5 source samples, then gain .5
            assert_eq!(pcm.channels[0][5513], 0.25);
        } else {
            // Independent sign crossings after DSP priming; the source is441Hz,
            // authored2x is882Hz unless the actual SoundTouch pitch path runs.
            let range = &pcm.channels[0][8192..40000];
            let crossings = range
                .windows(2)
                .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
                .count();
            let observed = crossings as f64 * 44_100.0 / (range.len() - 1) as f64;
            assert!(
                (observed - frequency as f64).abs() < 5.0,
                "observed {observed}, expected {frequency}"
            );
        }
        let pid = result.runtime_identity.browser_pid;
        #[cfg(unix)]
        unsafe {
            assert_eq!(
                libc::kill(pid as i32, 0),
                -1,
                "Browser remains alive after completion"
            );
        }
    }
}
#[test]
#[ignore = "Requires installed pinned browser runtime; run explicitly with OPENCUT_CHROME"]
fn rust_browser_policy_revocation_terminates_without_pcm() {
    let directory = tempfile::tempdir().unwrap();
    let (root, snapshot) = fixture(directory.path(), false, false);
    let prepared = prepare_audio_v4(
        &snapshot,
        "s",
        AudioExecutionProfileV4::ClassicV31,
        PcmFormatV4 {
            sample_rate: 44_100,
            channels: 2,
        },
    )
    .unwrap();
    // A live host port revokes authority during encoded reads. No alternate
    // revision/request is started, and no rendered PCM is accepted.
    let resources = VerifiedClassicAudioResources::new(&root, &prepared, || Ok(())).unwrap();
    let handles = resources.handles();
    let mut resources = RevokeAfterRead {
        inner: resources,
        revoked: false,
    };
    let mut session =
        ClassicAudioSession::new(&prepared, 64 * 1024 * 1024, 32 * 1024 * 1024).unwrap();
    let mut pcm = Pcm::default();
    let result = ClassicAudioProcess { config: config() }.execute(
        &mut session,
        &mut resources,
        &handles,
        &mut pcm,
    );
    assert_eq!(result.err().unwrap().code, "NOT_AUTHORIZED");
    assert!(pcm.channels.iter().all(Vec::is_empty));
}
#[test]
#[ignore = "Requires installed pinned browser runtime; run explicitly with OPENCUT_CHROME"]
fn rust_browser_rejects_unsupported_hard_quota_and_enforces_rss_monitor() {
    let directory = tempfile::tempdir().unwrap();
    let (root, snapshot) = fixture(directory.path(), false, false);
    let prepared = prepare_audio_v4(
        &snapshot,
        "s",
        AudioExecutionProfileV4::ClassicV31,
        PcmFormatV4 {
            sample_rate: 44_100,
            channels: 2,
        },
    )
    .unwrap();
    for strict in [false, true] {
        if strict && !cfg!(target_os = "macos") {
            continue;
        }
        let mut settings = config();
        if strict {
            settings.strict_cgroup = Some(PathBuf::from("/sys/fs/cgroup/unavailable"));
        } else {
            settings.max_rss_bytes = 1024 * 1024;
        }
        let mut resources =
            VerifiedClassicAudioResources::new(&root, &prepared, || Ok(())).unwrap();
        let handles = resources.handles();
        let mut session =
            ClassicAudioSession::new(&prepared, 64 * 1024 * 1024, 32 * 1024 * 1024).unwrap();
        let mut pcm = Pcm::default();
        let error = ClassicAudioProcess { config: settings }
            .execute(&mut session, &mut resources, &handles, &mut pcm)
            .err()
            .unwrap();
        assert_eq!(
            error.code,
            if strict {
                "UNSUPPORTED_AUDIO_QUOTA"
            } else {
                "AUDIO_RSS_QUOTA_EXCEEDED"
            }
        );
        assert!(pcm.channels.iter().all(Vec::is_empty));
    }
}

#[test]
fn private_wav_sink_has_independent_float_header_and_merges_unequal_channel_blocks() {
    use std::io::{Read, Seek, SeekFrom};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("private.wav");
    let file = std::fs::OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    assert_eq!(ClassicAudioWavSink::checked_bytes(44_100, 4).unwrap(), 90);
    let mut sink = ClassicAudioWavSink::new(file, 44_100, 4).unwrap();
    sink.chunk(0, 0, &[1.0]).unwrap();
    sink.chunk(0, 1, &[0.25, -0.5, -0.0]).unwrap();
    let incomplete = std::fs::read(&path).unwrap();
    for frame in 0..4 {
        assert_eq!(&incomplete[58 + frame * 8 + 4..58 + frame * 8 + 8], &[0; 4]);
    }
    sink.chunk(1, 0, &[-1.0, 0.5]).unwrap();
    sink.chunk(1, 2, &[0.0, 2.0]).unwrap();
    let mut file = sink.finalize().unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    let u16_at = |at| u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
    let u32_at = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    assert_eq!(bytes.len(), 90);
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(u32_at(4), 82);
    assert_eq!(&bytes[8..16], b"WAVEfmt ");
    assert_eq!(u32_at(16), 18);
    assert_eq!((u16_at(20), u16_at(22)), (3, 2));
    assert_eq!((u32_at(24), u32_at(28)), (44_100, 352_800));
    assert_eq!((u16_at(32), u16_at(34), u16_at(36)), (8, 32, 0));
    assert_eq!(&bytes[38..42], b"fact");
    assert_eq!((u32_at(42), u32_at(46)), (4, 4));
    assert_eq!(&bytes[50..54], b"data");
    assert_eq!(u32_at(54), 32);
    let expected = [1.0f32, -1.0, 0.25, 0.5, -0.5, 0.0, -0.0, 2.0];
    for (sample, expected) in bytes[58..].chunks_exact(4).zip(expected) {
        assert_eq!(sample, expected.to_le_bytes());
    }
}

#[test]
fn private_wav_sink_rejects_incomplete_invalid_and_replayed_pcm() {
    let mut missing = ClassicAudioWavSink::new(tempfile::tempfile().unwrap(), 48_000, 2).unwrap();
    missing.chunk(0, 0, &[0.5, 0.5]).unwrap();
    missing.chunk(1, 0, &[0.25]).unwrap();
    assert_eq!(
        missing.finalize().err().unwrap().code,
        "INCOMPLETE_OUTPUT_PCM"
    );
    for (channel, offset, samples) in [
        (0, 1, vec![0.0]), // skipped frame
        (2, 0, vec![0.0]),
        (1, 0, vec![]),
        (0, 0, vec![f32::NAN]),
        (0, 0, vec![f32::INFINITY]),
        (0, 0, vec![0.0; 3]), // exceeds admitted two frames
        (0, 0, vec![0.0; 4097]),
        (0, u64::MAX, vec![0.0]),
    ] {
        let mut sink = ClassicAudioWavSink::new(tempfile::tempfile().unwrap(), 48_000, 2).unwrap();
        assert_eq!(
            sink.chunk(channel, offset, &samples).err().unwrap().code,
            "INVALID_OUTPUT_PCM"
        );
        assert!(
            sink.chunk(0, 0, &[0.0, 0.0]).is_err(),
            "A rejected execution remains failed"
        );
        assert!(sink.finalize().is_err());
    }
    let mut replay = ClassicAudioWavSink::new(tempfile::tempfile().unwrap(), 48_000, 1).unwrap();
    replay.chunk(0, 0, &[0.5]).unwrap();
    replay.chunk(1, 0, &[0.25]).unwrap();
    assert!(replay.chunk(0, 0, &[0.5]).is_err());
    assert!(replay.finalize().is_err());
}

#[test]
fn private_wav_storage_bounds_precede_allocation_and_final_length_is_fenced() {
    let maximum_frames = (u64::from(u32::MAX) - 50) / 8;
    assert_eq!(
        ClassicAudioWavSink::checked_bytes(48_000, maximum_frames).unwrap(),
        58 + 8 * maximum_frames
    );
    for frames in [0, maximum_frames + 1, u64::MAX] {
        assert!(ClassicAudioWavSink::checked_bytes(44_100, frames).is_err());
    }
    assert!(ClassicAudioWavSink::checked_bytes(96_000, 1).is_err());
    let file = tempfile::tempfile().unwrap();
    let tamper = file.try_clone().unwrap();
    let mut sink = ClassicAudioWavSink::new(file, 48_000, 1).unwrap();
    sink.chunk(0, 0, &[0.0]).unwrap();
    sink.chunk(1, 0, &[0.0]).unwrap();
    tamper.set_len(67).unwrap(); // Correct one-frame container is 66 bytes.
    assert_eq!(sink.finalize().err().unwrap().code, "INVALID_OUTPUT_PCM");
    let nonempty = tempfile::tempfile().unwrap();
    nonempty.set_len(1).unwrap();
    assert!(ClassicAudioWavSink::new(nonempty, 48_000, 1).is_err());
}

#[cfg(unix)]
#[test]
#[ignore = "Requires installed pinned browser runtime; run explicitly with OPENCUT_CHROME"]
fn real_browser_startup_cancellation_consumes_delayed_started_and_leaves_no_group() {
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    };
    // Cleanup is restricted to the exact browser PID recorded by this runner.
    // Chromium launchServer owns a separate group; never signal the test group
    // or any unrelated browser in a failed regression.
    struct RecordedBrowser {
        marker: PathBuf,
        observed: Arc<AtomicU32>,
        group: Arc<AtomicU32>,
    }
    impl Drop for RecordedBrowser {
        fn drop(&mut self) {
            let pid = self.observed.load(Ordering::Acquire);
            let pid = if pid != 0 {
                Some(pid)
            } else {
                std::fs::read_to_string(&self.marker)
                    .ok()
                    .and_then(|value| value.parse::<u32>().ok())
            };
            if let Some(pid) = pid.filter(|pid| *pid > 0 && *pid <= i32::MAX as u32) {
                unsafe {
                    // Positive getpgid matching this recorded PID proves that
                    // the still-live browser is its own group leader.
                    if self.group.load(Ordering::Acquire) == pid
                        || libc::getpgid(pid as i32) == pid as i32
                    {
                        libc::kill(-(pid as i32), libc::SIGKILL);
                    }
                }
            }
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let (root, snapshot) = fixture(directory.path(), false, false);
    let prepared = prepare_audio_v4(
        &snapshot,
        "s",
        AudioExecutionProfileV4::ClassicV31,
        PcmFormatV4 {
            sample_rate: 44_100,
            channels: 2,
        },
    )
    .unwrap();
    let marker = directory.path().join("actual-browser-pid");
    let observed = Arc::new(AtomicU32::new(0));
    let group = Arc::new(AtomicU32::new(0));
    let cleanup = RecordedBrowser {
        marker: marker.clone(),
        observed: Arc::clone(&observed),
        group: Arc::clone(&group),
    };
    let shared = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("packages/classic-audio-renderer/host/shared.mjs");
    let runner = directory.path().join("delayed-real-browser.mjs");
    let shared = serde_json::to_string(&format!("file://{}", shared.to_str().unwrap())).unwrap();
    let marker_json = serde_json::to_string(marker.to_str().unwrap()).unwrap();
    // Public testing hooks wrap the pinned, actual Playwright API. The browser
    // really exists while BrowserRealm.start awaits this delayed return.
    let script = format!(
        r#"
import {{ runHost, playwrightRuntime }} from {shared};
import {{ writeFile, rename }} from "node:fs/promises";
await runHost(process.stdin, process.stdout, {{ browser: {{
  playwrightRuntime: async runtime => {{
    const {{ chromium }} = await playwrightRuntime(runtime);
    return {{ chromium: {{
      launchServer: async options => {{
        const server = await chromium.launchServer(options);
        await writeFile({marker_json} + ".tmp", String(server.process().pid));
        await rename({marker_json} + ".tmp", {marker_json});
        await new Promise(resolve => setTimeout(resolve, 6000));
        return server;
      }},
      connect: (...args) => chromium.connect(...args),
    }} }};
  }},
}} }});
"#
    );
    std::fs::write(&runner, script).unwrap();
    let checkpoint_pid = Arc::clone(&observed);
    let checkpoint_group = Arc::clone(&group);
    let mut resources = VerifiedClassicAudioResources::new(&root, &prepared, || {
        if let Ok(value) = std::fs::read_to_string(&marker) {
            let pid = value.parse::<u32>().map_err(|_| {
                EditorError::new(
                    "INVALID_TEST_MARKER",
                    "Browser PID marker must be an integer",
                )
            })?;
            if pid == 0 || pid > i32::MAX as u32 {
                return Err(EditorError::new(
                    "INVALID_TEST_MARKER",
                    "Browser PID marker is out of range",
                ));
            }
            checkpoint_pid.store(pid, Ordering::Release);
            unsafe {
                if libc::getpgid(pid as i32) != pid as i32 {
                    return Err(EditorError::new(
                        "INVALID_TEST_MARKER",
                        "Real browser must own its isolated process group",
                    ));
                }
            }
            checkpoint_group.store(pid, Ordering::Release);
            return Err(EditorError::new(
                "CANCELLED",
                "Synthetic cancellation before delayed started acknowledgement",
            ));
        }
        Ok(())
    })
    .unwrap();
    let handles = resources.handles();
    let mut session =
        ClassicAudioSession::new(&prepared, 64 * 1024 * 1024, 32 * 1024 * 1024).unwrap();
    let mut pcm = Pcm::default();
    let mut settings = config();
    settings.runner = runner;
    let error = ClassicAudioProcess { config: settings }
        .execute(&mut session, &mut resources, &handles, &mut pcm)
        .err()
        .unwrap();
    let pid = observed.load(Ordering::Acquire);
    assert!(
        pid > 0,
        "Cancellation must observe a real browser before its delayed started packet"
    );
    assert_eq!(error.code, "CANCELLED");
    assert!(pcm.channels.iter().all(Vec::is_empty));
    unsafe {
        assert_eq!(
            libc::kill(pid as i32, 0),
            -1,
            "Recorded browser remains alive after startup cancellation"
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
        assert_eq!(
            libc::kill(-(pid as i32), 0),
            -1,
            "Recorded browser group remains alive after startup cancellation"
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }
    drop(cleanup);
}
