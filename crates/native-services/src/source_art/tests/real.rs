//! Explicit local runtime acceptance; only small synthetic owned TempDir media.
use super::super::*;
use opencut_asset_store::{AssetStore, gate::Gate};
use opencut_contracts::{Asset, AssetKind, CreateProject, FrameRate, Time, scene::*};
use opencut_editor_application::{scene::new_document, scene_audio_jobs::TrustedJobAuthority};
use std::collections::BTreeMap;
struct Store(AuthorizedSourceArt);
impl SourceArtRead for Store {
    fn fixed_source_art_snapshot(
        &self,
        _: &str,
        _: &str,
    ) -> Result<AuthorizedSourceArt, EditorError> {
        Ok(self.0.clone())
    }
    fn revalidate_source_art(
        &self,
        _: &str,
        _: &str,
        _: &TrustedJobAuthority,
    ) -> Result<(), EditorError> {
        Ok(())
    }
}
struct Live;
impl SourceArtCheckpoint for Live {
    fn checkpoint(&mut self) -> Result<(), EditorError> {
        Ok(())
    }
}
fn prepare(
    directory: &Path,
    path: &Path,
    kind: AssetKind,
    query: SourceArtQuery,
) -> (PathBuf, PreparedSourceArt) {
    let root = directory.join("cas");
    let old = Asset {
        id: "source".into(),
        name: "Synthetic source".into(),
        kind: kind.clone(),
        uri: path.to_string_lossy().into(),
        duration: Some(Time {
            ticks: "4".into(),
            timebase: 1,
        }),
        version: None,
        metadata: BTreeMap::new(),
    };
    let pinned = AssetStore::open(&root).unwrap().pin_asset(&old).unwrap();
    let mut project = new_document(&CreateProject {
        id: "project".into(),
        name: "Source art acceptance".into(),
        sequence_id: "scene".into(),
        width: 16,
        height: 16,
        fps: FrameRate {
            numerator: 30,
            denominator: 1,
        },
    })
    .unwrap();
    project.assets.push(ProjectAsset {
        id: "source".into(),
        name: "Synthetic source".into(),
        kind: kind.clone(),
        version: pinned.version.unwrap(),
        duration: old.duration,
        origin: ResourceOrigin::LocalImport {
            original_name: "synthetic".into(),
        },
        metadata: ProjectMediaMetadata {
            mime_type: None,
            intrinsic_size: None,
            frame_rate: None,
            rotation_degrees: None,
            audio: if kind == AssetKind::Audio {
                AudioPresence::Present { metadata: None }
            } else {
                AudioPresence::Unknown {}
            },
        },
        license: None,
    });
    let store = Store(AuthorizedSourceArt {
        snapshot: EditorSnapshot {
            project,
            revision: "fixed".into(),
        },
        authority: TrustedJobAuthority::LocalOwner,
        scope: "source-art-acceptance".into(),
        policy: SourceArtPolicy {
            max_encoded_bytes: 32 * 1024 * 1024,
            max_pcm_bytes: 256 * 1024 * 1024,
            max_rss_bytes: 2 * 1024 * 1024 * 1024,
            deadline_ms: 60000,
        },
    });
    let prepared = prepare_source_art(
        &store,
        &SourceArtRequest {
            project_id: "project".into(),
            revision: "fixed".into(),
            asset_id: "source".into(),
            request_id: "waiter".into(),
            query,
        },
    )
    .unwrap();
    (root, prepared)
}
fn config(thumbnail: bool) -> SourceArtProcessConfig {
    let node = PathBuf::from(std::env::var_os("OPENCUT_NODE").expect("Set absolute OPENCUT_NODE"));
    let chromium =
        PathBuf::from(std::env::var_os("OPENCUT_CHROME").expect("Set absolute OPENCUT_CHROME"));
    let runner = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/classic-source-art-renderer/host/runner.mjs");
    let mut identity = resolve_identity(&node, &runner, &chromium).unwrap();
    if thumbnail {
        identity.algorithm = THUMBNAIL_ALGORITHM.into();
    }
    SourceArtProcessConfig {
        node,
        runner,
        chromium,
        deadline: Duration::from_secs(60),
        max_encoded_bytes: 32 * 1024 * 1024,
        max_pcm_bytes: 256 * 1024 * 1024,
        max_rss_bytes: 2 * 1024 * 1024 * 1024,
        identity,
    }
}
fn wav(path: &Path) {
    let samples = 257u32;
    let channels = 4u16;
    let length = samples * u32::from(channels) * 4;
    let mut out = Vec::new();
    out.extend(b"RIFF");
    out.extend((36 + length).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(3u16.to_le_bytes());
    out.extend(channels.to_le_bytes());
    out.extend(48000u32.to_le_bytes());
    out.extend((48000 * u32::from(channels) * 4).to_le_bytes());
    out.extend((channels * 4).to_le_bytes());
    out.extend(32u16.to_le_bytes());
    out.extend(b"data");
    out.extend(length.to_le_bytes());
    for frame in 0..257 {
        for channel in 0..4 {
            let value: f32 = match (frame, channel) {
                (0, 0) => 0.25,
                (127, 1) => -0.75,
                (128, 2) => 0.875,
                (256, 3) => 0.125,
                _ => 0.0,
            };
            out.extend(value.to_le_bytes());
        }
    }
    std::fs::write(path, out).unwrap();
}
#[test]
#[ignore = "Requires absolute OPENCUT_NODE/OPENCUT_CHROME and pinned local runtime"]
fn actual_multichannel_streamed_peaks_use_decoded_extent() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("four-channel.wav");
    wav(&path);
    let (root, prepared) = prepare(
        temp.path(),
        &path,
        AssetKind::Audio,
        SourceArtQuery::Waveform {
            sample_rate: 48000,
            projection: WaveformProjection::Source {
                range: None,
                bins: 3,
            },
        },
    );
    let executor = SourceArtProcessExecutor::new(config(false), root.clone()).unwrap();
    let result = executor.execute(&prepared, &mut Live).unwrap();
    let DecodedSourceArt::Waveform {
        version,
        sample_rate,
        channels,
        total_samples,
        amplitudes,
        ..
    } = result
    else {
        panic!()
    };
    assert_eq!(version, prepared.asset().version);
    assert_eq!(sample_rate, 48000);
    assert_eq!(channels, 4);
    assert_eq!(total_samples, 257);
    assert_eq!(amplitudes.len(), 3);
    for (actual, expected) in amplitudes.iter().zip([0.75, 0.875, 0.125]) {
        assert!((actual - expected).abs() < 1e-6, "{actual} vs {expected}");
    }
    drop(Gate::exclusive(&root).unwrap());
}
#[test]
#[ignore = "Requires absolute OPENCUT_NODE/OPENCUT_CHROME and pinned local runtime"]
fn actual_five_minute_source_preserves_every_bucket_and_partial_tail() {
    use std::io::Write;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("five-minute.wav");
    let samples = 14_400_017u32;
    let bytes = samples * 4;
    let mut out = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
    out.write_all(b"RIFF").unwrap();
    out.write_all(&(36 + bytes).to_le_bytes()).unwrap();
    out.write_all(b"WAVEfmt ").unwrap();
    out.write_all(&16u32.to_le_bytes()).unwrap();
    out.write_all(&1u16.to_le_bytes()).unwrap(); // PCM16
    out.write_all(&2u16.to_le_bytes()).unwrap(); // stereo, >32 MiB source
    out.write_all(&48000u32.to_le_bytes()).unwrap();
    out.write_all(&192000u32.to_le_bytes()).unwrap();
    out.write_all(&4u16.to_le_bytes()).unwrap();
    out.write_all(&16u16.to_le_bytes()).unwrap();
    out.write_all(b"data").unwrap();
    out.write_all(&bytes.to_le_bytes()).unwrap();
    let mut bucket = [0u8; 512];
    bucket[..2].copy_from_slice(&16384i16.to_le_bytes());
    for _ in 0..112_500 {
        out.write_all(&bucket).unwrap();
    }
    let mut tail = [0u8; 68];
    tail[..2].copy_from_slice(&8192i16.to_le_bytes());
    out.write_all(&tail).unwrap();
    out.flush().unwrap();
    drop(out);
    let (root, prepared) = prepare(
        temp.path(),
        &path,
        AssetKind::Audio,
        SourceArtQuery::Waveform {
            sample_rate: 48000,
            projection: WaveformProjection::Source {
                range: None,
                bins: 100,
            },
        },
    );
    // Authored metadata is deliberately four seconds; the decoder must observe
    // the entire real source. The >32 MiB encoded file is not a resident-memory
    // allocation; bounded range/PCM shares and the 60 s policy remain enforced.
    let result = SourceArtProcessExecutor::new(config(false), root.clone())
        .unwrap()
        .execute(&prepared, &mut Live)
        .unwrap();
    let DecodedSourceArt::Waveform {
        version,
        sample_rate,
        channels,
        total_samples,
        amplitudes,
        ..
    } = result
    else {
        panic!()
    };
    assert_eq!(version, prepared.asset().version);
    assert_eq!(sample_rate, 48000);
    assert_eq!(channels, 2);
    assert_eq!(total_samples, u64::from(samples));
    assert_eq!(amplitudes.len(), 112_501);
    // The captured streamed decoder has signed PCM16 normalization by 2^15.
    // These exact original-byte constants do not depend on measured peaks.
    // This differs from the former decodeAudioData positive /32767 profile;
    // the new decode_profile and module identity must distinguish the paths.
    let positive_scale = 1.0f32 / 32768.0;
    let expected_peak = 16384.0f32 * positive_scale;
    let mismatches: Vec<_> = amplitudes[..112_500]
        .iter()
        .enumerate()
        .filter(|(_, peak)| **peak != expected_peak)
        .take(8)
        .collect();
    assert!(
        mismatches.is_empty(),
        "Unexpected source buckets: {mismatches:?}"
    );
    assert_eq!(amplitudes[112_500], 8192.0f32 * positive_scale);
    drop(Gate::exclusive(&root).unwrap());
}
#[test]
#[ignore = "Requires absolute OPENCUT_NODE/OPENCUT_CHROME and pinned local runtime"]
fn actual_image_jpeg_has_observed_dimensions_and_independent_color() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("red.png");
    let mut encoder = png::Encoder::new(std::fs::File::create(&path).unwrap(), 16, 9);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&[240u8, 16, 8].repeat(16 * 9))
        .unwrap();
    let (root, prepared) = prepare(
        temp.path(),
        &path,
        AssetKind::Image,
        SourceArtQuery::Thumbnail {},
    );
    let result = SourceArtProcessExecutor::new(config(true), root)
        .unwrap()
        .execute(&prepared, &mut Live)
        .unwrap();
    let DecodedSourceArt::ThumbnailReady {
        width,
        height,
        jpeg_bytes,
        observation,
        ..
    } = result
    else {
        panic!()
    };
    assert_eq!((width, height), (16, 9));
    assert_eq!(
        observation,
        SourceArtThumbnailObservation::StaticImage {
            source_width: 16,
            source_height: 9
        }
    );
    assert_eq!(&jpeg_bytes[..2], &[255, 216]);
    assert_eq!(&jpeg_bytes[jpeg_bytes.len() - 2..], &[255, 217]);
    // Independent actual JPEG decoder: no renderer-produced pixels as oracle.
    let jpeg = temp.path().join("thumbnail.jpg");
    std::fs::write(&jpeg, jpeg_bytes).unwrap();
    let decoded = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(jpeg)
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"])
        .output()
        .unwrap();
    assert!(decoded.status.success());
    assert_eq!(decoded.stdout.len(), 16 * 9 * 3);
    for pixel in decoded.stdout.chunks_exact(3) {
        assert!(
            pixel[0] > 220 && pixel[1] < 35 && pixel[2] < 30,
            "{pixel:?}"
        );
    }
}
#[test]
#[ignore = "Requires OPENCUT_NODE/OPENCUT_CHROME, pinned runtime and FFmpeg"]
fn actual_primary_video_selects_one_second_and_no_sample_is_explicit() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("color.mp4");
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=red:s=32x18:r=10:d=1",
            "-f",
            "lavfi",
            "-i",
            "color=blue:s=32x18:r=10:d=1",
            "-filter_complex",
            "[0:v][1:v]concat=n=2:v=1:a=0[v]",
            "-map",
            "[v]",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let (root, prepared) = prepare(
        temp.path(),
        &path,
        AssetKind::Video,
        SourceArtQuery::Thumbnail {},
    );
    let executor = SourceArtProcessExecutor::new(config(true), root).unwrap();
    let result = executor.execute(&prepared, &mut Live).unwrap();
    let DecodedSourceArt::ThumbnailReady {
        observation,
        jpeg_bytes,
        ..
    } = result
    else {
        panic!()
    };
    let SourceArtThumbnailObservation::Video {
        requested_seconds,
        actual_timestamp_seconds,
        frame_duration_seconds,
        ..
    } = observation
    else {
        panic!()
    };
    assert_eq!(requested_seconds, 1.0);
    assert!((actual_timestamp_seconds.unwrap() - 1.0).abs() < 1e-6);
    assert!((frame_duration_seconds.unwrap() - 0.1).abs() < 1e-6);
    let jpeg = temp.path().join("frame.jpg");
    std::fs::write(&jpeg, jpeg_bytes).unwrap();
    let decoded = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(jpeg)
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"])
        .output()
        .unwrap();
    assert!(decoded.status.success());
    for px in decoded.stdout.chunks_exact(3) {
        assert!(px[2] > 220 && px[0] < 35 && px[1] < 35);
    }
    // getSample(1) is null when the real primary track starts at 2s.
    // This tests absence at the requested timestamp, without redefining short
    // nonempty tracks as half-open (Mediabunny retains their last sample).
    let late = temp.path().join("late.mp4");
    let generated = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=green:s=32x18:r=10:d=1",
            "-vf",
            "setpts=PTS+2/TB",
            "-fps_mode",
            "passthrough",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&late)
        .output()
        .unwrap();
    assert!(generated.status.success());
    let (root, prepared) = prepare(
        temp.path(),
        &late,
        AssetKind::Video,
        SourceArtQuery::Thumbnail {},
    );
    let result = SourceArtProcessExecutor::new(config(true), root)
        .unwrap()
        .execute(&prepared, &mut Live)
        .unwrap();
    let DecodedSourceArt::ThumbnailNoFrame {
        observation,
        reason,
        ..
    } = result
    else {
        panic!("Expected actual no sample before primary track's first PTS")
    };
    assert_eq!(reason, SourceArtNoFrameReason::NoSampleAtOneSecond);
    let SourceArtThumbnailObservation::Video {
        requested_seconds,
        actual_timestamp_seconds,
        frame_duration_seconds,
        ..
    } = observation
    else {
        panic!()
    };
    assert_eq!(requested_seconds, 1.0);
    assert_eq!(actual_timestamp_seconds, None);
    assert_eq!(frame_duration_seconds, None);
}
#[cfg(unix)]
#[test]
#[ignore = "Requires absolute OPENCUT_NODE; real isolated child teardown test"]
fn cancellation_reaps_actual_detached_child_before_releasing_cas() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("small.wav");
    wav(&path);
    let (root, prepared) = prepare(
        temp.path(),
        &path,
        AssetKind::Audio,
        SourceArtQuery::Waveform {
            sample_rate: 48000,
            projection: WaveformProjection::Source {
                range: None,
                bins: 3,
            },
        },
    );
    let pid_path = temp.path().join("child.pid");
    let runner = temp.path().join("runner.mjs");
    let script = format!(
        "import{{spawn}}from'node:child_process';import fs from'node:fs';import readline from'node:readline';let child,execution;const r=readline.createInterface({{input:process.stdin}});r.on('line',line=>{{const p=JSON.parse(line);if(p.type==='begin'){{execution=p.executionId;child=spawn('/bin/sleep',['60'],{{detached:true,stdio:'ignore'}});fs.writeFileSync({},String(child.pid));process.stdout.write(JSON.stringify({{executionId:execution,type:'started',browserPid:child.pid}})+'\\n');}}else if(p.type==='cancel'){{child.kill('SIGKILL');child.once('exit',()=>{{r.close();process.exit(0)}});}}}});",
        serde_json::to_string(&pid_path.to_string_lossy()).unwrap()
    );
    std::fs::write(&runner, script).unwrap();
    let node: PathBuf = std::env::var_os("OPENCUT_NODE")
        .expect("Set absolute OPENCUT_NODE")
        .into();
    let mut id = super::identity();
    id.algorithm = SOURCE_PEAKS_ALGORITHM.into();
    let config = SourceArtProcessConfig {
        node: node.clone(),
        runner,
        chromium: node,
        deadline: Duration::from_secs(60),
        max_encoded_bytes: 32 * 1024 * 1024,
        max_pcm_bytes: 256 * 1024 * 1024,
        max_rss_bytes: 2 * 1024 * 1024 * 1024,
        identity: id,
    };
    struct Cancel {
        root: PathBuf,
        pid: PathBuf,
        observed: bool,
    }
    impl SourceArtCheckpoint for Cancel {
        fn checkpoint(&mut self) -> Result<(), EditorError> {
            if self.pid.exists() {
                assert_eq!(
                    Gate::exclusive(&self.root).err().unwrap().code,
                    "STORAGE_BUSY"
                );
                self.observed = true;
                return Err(error("CANCELLED", "Synthetic actual cancellation"));
            }
            Ok(())
        }
    }
    let mut cancel = Cancel {
        root: root.clone(),
        pid: pid_path.clone(),
        observed: false,
    };
    let result = SourceArtProcessExecutor::new(config, root.clone())
        .unwrap()
        .execute(&prepared, &mut cancel);
    assert_eq!(result.unwrap_err().code, "CANCELLED");
    assert!(cancel.observed);
    let pid = std::fs::read_to_string(pid_path).unwrap().parse().unwrap();
    assert!(!crate::classic_audio::lifecycle::group_exists(pid));
    drop(Gate::exclusive(&root).unwrap());
}
