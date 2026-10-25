//! Independent sample/packet oracles through the production CAS and browser port.
use super::*;

fn float_wav(path: &Path, channels: &[Vec<f32>]) {
    let frames = channels[0].len();
    assert!(channels.iter().all(|channel| channel.len() == frames));
    let width = channels.len() as u16 * 4;
    let data_bytes = frames as u32 * u32::from(width);
    let mut bytes = Vec::new();
    bytes.extend(b"RIFF");
    bytes.extend((data_bytes + 36).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(3u16.to_le_bytes());
    bytes.extend((channels.len() as u16).to_le_bytes());
    bytes.extend(48_000u32.to_le_bytes());
    bytes.extend((48_000 * u32::from(width)).to_le_bytes());
    bytes.extend(width.to_le_bytes());
    bytes.extend(32u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(data_bytes.to_le_bytes());
    for frame in 0..frames {
        for channel in channels {
            bytes.extend(channel[frame].to_le_bytes());
        }
    }
    std::fs::write(path, bytes).unwrap();
}

fn decoder_snapshot(
    directory: &Path,
    path: &Path,
    kind: AssetKind,
    channels: u32,
) -> (PathBuf, EditorSnapshot) {
    // Reuse the legal aggregate fixture; replace its media with the authored
    // packet/sample oracle. All temporary originals and CAS files die together.
    let (root, mut snapshot) = fixture(directory, false, false);
    let asset = &mut snapshot.project.assets[0];
    let pinned = AssetStore::open(&root)
        .unwrap()
        .pin_asset(&Asset {
            id: asset.id.clone(),
            name: "Independent decoder oracle".into(),
            kind: kind.clone(),
            uri: path.to_string_lossy().into(),
            duration: None,
            version: None,
            metadata: BTreeMap::new(),
        })
        .unwrap();
    asset.kind = kind;
    asset.version = pinned.version.unwrap();
    // Unknown extent is explicit and legal. Actual decoded length, not a false
    // declared asset duration, is the source of the EOF zero-tail oracle.
    asset.duration = None;
    asset.metadata.mime_type = Some(if asset.kind == AssetKind::Audio {
        "audio/wav".into()
    } else {
        "video/x-matroska".into()
    });
    asset.metadata.audio = AudioPresence::Present {
        metadata: Some(AudioMetadata {
            sample_rate: 48_000,
            channels,
            channel_layout: None,
        }),
    };
    let voice = &mut snapshot.project.sequences[0].tracks[1].elements[0];
    voice.timeline_range = TimeRange {
        start: time(0, 1),
        duration: time(1, 2),
    };
    voice.source_span = ElementSourceSpan::ClassicTrimmed {
        trim_start: time(0, 1),
        trim_end: time(0, 1),
        source_duration: None,
    };
    let ElementContent::Audio { audio, retime, .. } = &mut voice.content else {
        panic!("Audio fixture changed");
    };
    audio.gain = Gain::Decibels { value: 0.0 };
    retime.maintain_pitch = false;
    (root, snapshot)
}

fn execute(snapshot: &EditorSnapshot, root: &Path) -> (Pcm, ClassicAudioExecutionReceipt) {
    execute_selection(snapshot, root, None)
}

fn execute_selection(
    snapshot: &EditorSnapshot,
    root: &Path,
    range: Option<ClassicAudioRange>,
) -> (Pcm, ClassicAudioExecutionReceipt) {
    let prepared = prepare_audio_scene(
        snapshot,
        "s",
        AudioExecutionProfile::ClassicV31,
        PcmFormat {
            sample_rate: 48_000,
            channels: 2,
        },
    )
    .unwrap();
    let mut resources = VerifiedClassicAudioResources::new(root, &prepared, || Ok(())).unwrap();
    let handles = resources.handles();
    let expected_frames = range.as_ref().map_or(prepared.duration_samples(), |r| {
        r.frame_count.parse::<u64>().unwrap()
    });
    let mut session = ClassicAudioSession::new_with_range(
        &prepared,
        64 * 1024 * 1024,
        32 * 1024 * 1024,
        range.clone(),
    )
    .unwrap();
    let mut pcm = Pcm::default();
    let receipt = ClassicAudioProcess { config: config() }
        .execute(&mut session, &mut resources, &handles, &mut pcm)
        .unwrap();
    assert!(
        !receipt.receipt.mastering.applied,
        "The pulse oracle must avoid mastering DSP"
    );
    assert_eq!(pcm.channels[0].len() as u64, expected_frames);
    assert_eq!(receipt.receipt.range, range);
    (pcm, receipt)
}

fn pulse_snapshot(directory: &Path) -> (PathBuf, EditorSnapshot) {
    let path = directory.join("pulse.wav");
    let mut left = vec![0.0; 16384];
    let mut right = left.clone();
    left[1024] = 0.25;
    left[4095] = 0.5;
    left[8192] = 0.01;
    right[4096] = -0.25;
    right[16383] = 0.125;
    float_wav(&path, &[left, right]);
    let (root, mut snapshot) = decoder_snapshot(directory, &path, AssetKind::Audio, 2);
    let voice = &mut snapshot.project.sequences[0].tracks[1].elements[0];
    voice.timeline_range.start = time(17, 48_000);
    voice.source_span = ElementSourceSpan::ClassicTrimmed {
        trim_start: time(1024, 48_000),
        trim_end: time(0, 1),
        source_duration: None,
    };
    (root, snapshot)
}

#[test]
#[ignore = "Requires installed pinned browser runtime; run explicitly with OPENCUT_CHROME"]
fn rust_browser_known_pulse_trim_placement_and_actual_eof() {
    let directory = tempfile::tempdir().unwrap();
    let (root, original) = pulse_snapshot(directory.path());
    for (db, muted) in [(0.0, false), (-60.0, false), (0.0, true), (20.0, false)] {
        let mut snapshot = original.clone();
        let voice = &mut snapshot.project.sequences[0].tracks[1].elements[0];
        let high = db == 20.0;
        voice.timeline_range.start = time(if high { 0 } else { 17 }, 48_000);
        if high {
            voice.timeline_range.duration = time(1, 48_000);
        }
        voice.source_span = ElementSourceSpan::ClassicTrimmed {
            trim_start: time(if high { 8192 } else { 1024 }, 48_000),
            trim_end: time(0, 1),
            source_duration: None,
        };
        let ElementContent::Audio { audio, .. } = &mut voice.content else {
            unreachable!()
        };
        audio.gain = Gain::Decibels { value: db };
        audio.muted = muted;
        let (pcm, receipt) = execute(&snapshot, &root);
        if high {
            assert_eq!(pcm.channels[0], vec![(f64::from(0.01f32) * 10.0) as f32]);
            assert_eq!(pcm.channels[1], vec![0.0]);
            continue;
        }
        assert_eq!(pcm.channels[0].len(), 24017);
        let amplitude = if muted {
            0.0
        } else if db == -60.0 {
            0.001
        } else {
            1.0
        };
        for (channel, data) in pcm.channels.iter().enumerate() {
            for (index, &actual) in data.iter().enumerate() {
                let source = match (channel, index) {
                    (0, 17) => 0.25,
                    (0, 3088) => 0.5,
                    (0, 7185) => f64::from(0.01f32),
                    (1, 3089) => -0.25,
                    (1, 15376) => 0.125,
                    _ => 0.0,
                };
                let expected = (source * amplitude) as f32;
                assert!(
                    (actual - expected).abs() <= 1e-10,
                    "db={db}, muted={muted}, channel={channel}, sample={index}: {actual} != {expected}"
                );
            }
        }
        if db == -60.0 {
            assert!(pcm.channels[0][17] > 0.0, "-60dB is not mute");
        }
        if !muted {
            assert_eq!(receipt.receipt.decoders[0].frame_count, "16384");
            assert_eq!(receipt.receipt.decoders[0].channels, 2);
            assert!(
                pcm.channels
                    .iter()
                    .all(|c| c[15377..].iter().all(|&v| v == 0.0))
            );
        } else {
            assert!(receipt.receipt.decoders.is_empty());
        }
    }
}

#[test]
#[ignore = "Requires installed pinned browser runtime; run explicitly with OPENCUT_CHROME"]
fn rust_browser_ranges_preserve_absolute_pulse_clock_in_reverse_and_repeated_order() {
    let directory = tempfile::tempdir().unwrap();
    let (root, snapshot) = pulse_snapshot(directory.path());
    // These are absolute authored scene coordinates, not samples copied from
    // another renderer's full output. Every request shares one fixed CAS/revision.
    for (start, count) in [(7180, 16), (3000, 120), (15370, 32), (3000, 120)] {
        let range = ClassicAudioRange {
            start_sample: start.to_string(),
            frame_count: count.to_string(),
        };
        let (pcm, receipt) = execute_selection(&snapshot, &root, Some(range));
        assert_eq!(receipt.receipt.identity.revision, snapshot.revision);
        assert_eq!(receipt.receipt.mastering.whole_scene_frames, "24017");
        assert_eq!(receipt.receipt.decoders[0].primary, None);
        for (channel, samples) in pcm.channels.iter().enumerate() {
            for (local, &sample) in samples.iter().enumerate() {
                let expected = match (channel, start + local) {
                    (0, 3088) => 0.5,
                    (1, 3089) => -0.25,
                    (0, 7185) => 0.01f32,
                    (1, 15376) => 0.125,
                    _ => 0.0,
                };
                assert_eq!(
                    sample, expected,
                    "range={start}, channel={channel}, sample={local}"
                );
            }
        }
    }
}

fn ffmpeg(arguments: &[&std::ffi::OsStr]) {
    let output = std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn inspect(path: &Path, entries: &str) -> serde_json::Value {
    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a",
            "-show_entries",
            entries,
            "-show_data",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
#[ignore = "Requires ffmpeg/ffprobe and pinned browser runtime; run explicitly with OPENCUT_CHROME"]
fn rust_browser_pcm_container_concatenates_pts_gap() {
    let directory = tempfile::tempdir().unwrap();
    let wav = directory.path().join("gap.wav");
    let container = directory.path().join("gap.mkv");
    let mut source = vec![0.0; 4800];
    source[100] = 0.25;
    source[2450] = -0.5;
    float_wav(&wav, &[source]);
    ffmpeg(&[
        "-i".as_ref(),
        wav.as_os_str(),
        "-af".as_ref(),
        "asetnsamples=n=2400:p=0,asetpts=PTS+0.1/TB+if(gte(N\\,2400)\\,0.15/TB\\,0)".as_ref(),
        "-c:a".as_ref(),
        "pcm_f32le".as_ref(),
        "-f".as_ref(),
        "matroska".as_ref(),
        container.as_os_str(),
    ]);
    let observed = inspect(&container, "packet=pts_time,duration_time,size");
    let packets = observed["packets"].as_array().unwrap();
    assert_eq!(packets.len(), 2);
    for (packet, pts) in packets.iter().zip([0.1, 0.3]) {
        assert!((packet["pts_time"].as_str().unwrap().parse::<f64>().unwrap() - pts).abs() < 1e-12);
        assert_eq!(packet["size"], "9600");
    }
    let (root, snapshot) = decoder_snapshot(directory.path(), &container, AssetKind::Video, 1);
    let (pcm, receipt) = execute(&snapshot, &root);
    assert_eq!(receipt.receipt.decoders[0].frame_count, "4800");
    assert_eq!(
        receipt.receipt.decoders[0].profile,
        "mediabunny-1.41.0-concatenate-offline-resample"
    );
    for channel in &pcm.channels {
        for (index, &value) in channel.iter().enumerate() {
            assert_eq!(
                value,
                match index {
                    100 => 0.25,
                    2450 => -0.5,
                    _ => 0.0,
                },
                "sample {index}"
            );
        }
    }
}

#[test]
#[ignore = "Requires ffmpeg/ffprobe and pinned browser runtime; run explicitly with OPENCUT_CHROME"]
fn rust_browser_primary_audio_track_is_explicitly_default_selected() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.wav");
    let second = directory.path().join("second.wav");
    let container = directory.path().join("two-tracks.mkv");
    let mut a = vec![0.0; 4800];
    a[100] = 0.25;
    let mut b = vec![0.0; 4800];
    let mut right = b.clone();
    b[200] = -0.5;
    right[300] = 0.25;
    float_wav(&first, &[a]);
    float_wav(&second, &[b, right]);
    ffmpeg(&[
        "-i".as_ref(),
        first.as_os_str(),
        "-i".as_ref(),
        second.as_os_str(),
        "-map".as_ref(),
        "0:a:0".as_ref(),
        "-map".as_ref(),
        "1:a:0".as_ref(),
        "-c:a".as_ref(),
        "pcm_f32le".as_ref(),
        "-disposition:a:0".as_ref(),
        "0".as_ref(),
        "-disposition:a:1".as_ref(),
        "default".as_ref(),
        "-f".as_ref(),
        "matroska".as_ref(),
        container.as_os_str(),
    ]);
    let observed = inspect(
        &container,
        "stream=index,channels,sample_rate,codec_name:stream_disposition=default",
    );
    assert_eq!(observed["streams"][0]["disposition"]["default"], 0);
    assert_eq!(observed["streams"][1]["disposition"]["default"], 1);
    assert_eq!(observed["streams"][0]["channels"], 1);
    assert_eq!(observed["streams"][1]["channels"], 2);
    assert_eq!(observed["streams"][1]["sample_rate"], "48000");
    assert_eq!(observed["streams"][1]["codec_name"], "pcm_f32le");
    // Deliberately retain the old first-track mono metadata: production must
    // admit the selected default stereo facts, not allocate against this row.
    let (root, snapshot) = decoder_snapshot(directory.path(), &container, AssetKind::Video, 1);
    let (pcm, receipt) = execute(&snapshot, &root);
    let primary = receipt.receipt.decoders[0].primary.as_ref().unwrap();
    assert_eq!(primary.codec, "pcm-f32");
    assert_eq!(primary.sample_rate, 48000);
    assert_eq!(primary.channels, 2);
    for (channel, samples) in pcm.channels.iter().enumerate() {
        for (index, &sample) in samples.iter().enumerate() {
            assert_eq!(
                sample,
                match (channel, index) {
                    (0, 200) => -0.5,
                    (1, 300) => 0.25,
                    _ => 0.0,
                },
                "primary channel={channel}, sample={index}"
            );
        }
    }
}

fn iso_child<'a>(mut bytes: &'a [u8], wanted: &[u8; 4]) -> &'a [u8] {
    // Independent inspection of this tiny generated MP4 fixture only. The
    // production capability parser remains in the pinned renderer package.
    for _ in 0..64 {
        assert!(bytes.len() >= 8);
        let size = u32::from_be_bytes(bytes[..4].try_into().unwrap());
        let (size, header) = match size {
            0 => (bytes.len(), 8),
            1 => (
                usize::try_from(u64::from_be_bytes(bytes[8..16].try_into().unwrap())).unwrap(),
                16,
            ),
            n => (n as usize, 8),
        };
        assert!(size >= header && size <= bytes.len());
        if &bytes[4..8] == wanted {
            return &bytes[header..size];
        }
        bytes = &bytes[size..];
    }
    panic!("Generated AAC fixture exceeds its box inspection bound");
}

#[test]
#[ignore = "Requires ffmpeg/ffprobe and pinned browser runtime; run explicitly with OPENCUT_CHROME"]
fn rust_browser_aac_priming_preserves_independent_pulse_centers_and_actual_padded_eof() {
    let directory = tempfile::tempdir().unwrap();
    let wav = directory.path().join("aac-source.wav");
    let container = directory.path().join("aac-source.mp4");
    let mut source = vec![0.0; 24000];
    for (center, amplitude) in [(4096i32, 0.25), (16384, -0.25)] {
        for n in center - 128..=center + 128 {
            let offset = f64::from(n - center);
            let x = offset / 8.0;
            let sinc = if x == 0.0 {
                1.0
            } else {
                (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
            };
            source[n as usize] =
                (amplitude * sinc * (0.5 + 0.5 * (std::f64::consts::PI * offset / 128.0).cos()))
                    as f32;
        }
    }
    float_wav(&wav, &[source]);
    ffmpeg(&[
        "-i".as_ref(),
        wav.as_os_str(),
        "-c:a".as_ref(),
        "aac".as_ref(),
        "-profile:a".as_ref(),
        "aac_low".as_ref(),
        "-b:a".as_ref(),
        "128k".as_ref(),
        "-f".as_ref(),
        "mp4".as_ref(),
        container.as_os_str(),
    ]);
    // Root independently froze these FFmpeg8.1.2 packet/edit-list facts before
    // executing the renderer. A different encode must fail this premise, never
    // silently adopt a new lag/tail oracle from its rendered PCM.
    let observed = inspect(
        &container,
        "stream=id,codec_name,profile,sample_rate,channels,time_base,duration_ts,extradata_size:packet=pts,dts,duration,size:packet_side_data=side_data_type,skip_samples,discard_padding",
    );
    let stream = &observed["streams"][0];
    assert_eq!(stream["codec_name"], "aac");
    assert_eq!(stream["profile"], "LC");
    assert_eq!(stream["sample_rate"], "48000");
    assert_eq!(stream["channels"], 1);
    assert_eq!(stream["time_base"], "1/48000");
    assert_eq!(stream["duration_ts"], 24000);
    assert_eq!(stream["extradata_size"], 5);
    let packets = observed["packets"].as_array().unwrap();
    assert_eq!(packets.len(), 25);
    assert_eq!(packets[0]["pts"], -1024);
    assert_eq!(packets[0]["duration"], 1024);
    assert_eq!(packets[0]["side_data_list"][0]["skip_samples"], 1024);
    assert_eq!(packets[0]["side_data_list"][0]["discard_padding"], 0);
    assert_eq!(packets[1]["pts"], 0);
    assert!(packets[1..24].iter().all(|p| p["duration"] == 1024));
    assert_eq!(packets[24]["pts"], 23552);
    assert_eq!(packets[24]["duration"], 448);
    let encoded = std::fs::read(&container).unwrap();
    let elst = iso_child(
        iso_child(iso_child(iso_child(&encoded, b"moov"), b"trak"), b"edts"),
        b"elst",
    );
    assert_eq!(elst.len(), 20);
    assert_eq!(elst[0], 0);
    assert_eq!(u32::from_be_bytes(elst[4..8].try_into().unwrap()), 1);
    assert_eq!(u32::from_be_bytes(elst[8..12].try_into().unwrap()), 500);
    assert_eq!(i32::from_be_bytes(elst[12..16].try_into().unwrap()), 1024);
    assert_eq!(i32::from_be_bytes(elst[16..20].try_into().unwrap()), 65536);
    let (root, original) = decoder_snapshot(directory.path(), &container, AssetKind::Video, 1);
    for trim in [0, 1024] {
        let mut snapshot = original.clone();
        snapshot.project.assets[0].metadata.mime_type = Some("video/mp4".into());
        let placement = if trim == 0 { 0 } else { 17 };
        let voice = &mut snapshot.project.sequences[0].tracks[1].elements[0];
        voice.timeline_range.start = time(placement, 48000);
        voice.source_span = ElementSourceSpan::ClassicTrimmed {
            trim_start: time(trim, 48000),
            trim_end: time(0, 1),
            source_duration: None,
        };
        let (pcm, receipt) = execute(&snapshot, &root);
        // MB starts at key packet(0), skipping the first priming packet. The
        // remaining24 AAC frames are1024 samples each, including the last448-
        // duration container packet. Container duration is not decoded EOF.
        assert_eq!(receipt.receipt.decoders[0].frame_count, "24576");
        let primary = receipt.receipt.decoders[0].primary.as_ref().unwrap();
        assert_eq!(primary.codec, "aac");
        assert_eq!(primary.sample_rate, 48000);
        assert_eq!(primary.channels, 1);
        let ffprobe_id = stream["id"].as_str().unwrap().strip_prefix("0x").unwrap();
        assert_eq!(
            primary.track_id,
            u32::from_str_radix(ffprobe_id, 16).unwrap()
        );
        assert_eq!(pcm.channels[0], pcm.channels[1]);
        for (source_center, positive) in [(4096, true), (16384, false)] {
            let center = (placement + source_center - trim) as usize;
            let (observed_index, &peak) = pcm.channels[0]
                .iter()
                .enumerate()
                .skip(center - 128)
                .take(257)
                .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
                .unwrap();
            assert!(
                observed_index.abs_diff(center) <= 1,
                "AAC trim={trim}: peak index {observed_index} != fixed source center {center}; peak={peak}"
            );
            assert_eq!(peak > 0.0, positive);
            assert!(peak.abs() > 0.1 && peak.abs() < 0.5);
            eprintln!(
                "AAC trim={trim}: expected-center={center}, observed-peak={observed_index}, peak={peak}, decoded=24576"
            );
        }
        if trim == 1024 {
            assert!(
                pcm.channels
                    .iter()
                    .all(|c| c[23569..].iter().all(|&v| v == 0.0)),
                "Actual decoder EOF must produce an exact zero tail; AAC ringing before EOF remains permitted"
            );
        }
    }
}

// One four-second source, not a long-scene PCM allocation. All amplitudes are
// binary fractions so both interpolation and stored Float32 accumulation have
// independently exact expected samples.
fn dense_pulse(sample: u64) -> f32 {
    match sample {
        2047 => 1.0 / 128.0,
        2048 => -1.0 / 128.0,
        4095 => 3.0 / 256.0,
        4096 => -3.0 / 256.0,
        _ => 0.0,
    }
}
fn dense_snapshot(directory: &Path, tracks: usize, clips: usize) -> (PathBuf, EditorSnapshot) {
    let path = directory.join("dense-pulse.wav");
    float_wav(&path, &[(0..192_000).map(dense_pulse).collect()]);
    let (root, mut snapshot) = decoder_snapshot(directory, &path, AssetKind::Audio, 1);
    snapshot.project.assets[0].duration = Some(time(4, 1));
    let sequence = &mut snapshot.project.sequences[0];
    let template_track = sequence.tracks[1].clone();
    let template_element = template_track.elements[0].clone();
    sequence.tracks.truncate(1); // Retain the legal empty main video track.
    for track_index in 0..tracks {
        let mut track = template_track.clone();
        track.id = format!("dense-track-{track_index}");
        track.elements.clear();
        for clip_index in 0..clips {
            let mut element = template_element.clone();
            element.id = format!("dense-{track_index}-{clip_index}");
            element.timeline_range = TimeRange {
                start: if clips == 1 {
                    time(0, 1)
                } else {
                    time(296 * clip_index as i32, 19)
                },
                duration: if clips == 1 {
                    time(8192, 48_000)
                } else {
                    time(4, 1)
                },
            };
            element.source_span = ElementSourceSpan::ClassicTrimmed {
                trim_start: time(0, 1),
                trim_end: if clips == 1 {
                    time(192_000 - 8192, 48_000)
                } else {
                    time(0, 1)
                },
                source_duration: Some(time(4, 1)),
            };
            track.elements.push(element);
        }
        sequence.tracks.push(track);
    }
    (root, snapshot)
}

#[cfg(unix)]
struct CountedDenseResources<R> {
    inner: R,
    resolutions: usize,
    releases: usize,
    bytes: u64,
}
#[cfg(unix)]
impl<R: ClassicAudioResources> ClassicAudioResources for CountedDenseResources<R> {
    fn checkpoint(&mut self) -> Result<(), EditorError> {
        self.inner.checkpoint()
    }
    fn resolve(
        &mut self,
        resource: &AudioResource,
        handle: &str,
    ) -> Result<(String, u64), EditorError> {
        let result = self.inner.resolve(resource, handle)?;
        self.resolutions += 1;
        Ok(result)
    }
    fn read(
        &mut self,
        transfer: &str,
        offset: u64,
        count: u32,
    ) -> Result<(Vec<u8>, bool), EditorError> {
        let result = self.inner.read(transfer, offset, count)?;
        self.bytes += result.0.len() as u64;
        Ok(result)
    }
    fn release(&mut self, transfer: &str) -> Result<(), EditorError> {
        self.inner.release(transfer)?;
        self.releases += 1;
        Ok(())
    }
}

// Same ownership rule as the host lifecycle acceptance: exact runner, direct
// child of this test process, then that observed Node's descendant tree. No
// whole-system RSS or unrelated browser is included in the sampled peak.
#[cfg(unix)]
#[derive(Clone)]
struct DenseProcess {
    pid: u32,
    parent: u32,
    rss_bytes: u64,
    started: String,
    command: String,
}
#[cfg(unix)]
fn dense_processes() -> Vec<DenseProcess> {
    let output = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid=,rss=,lstart=,command="])
        .output()
        .expect("Read-only owned process observation");
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| {
            let parts: Vec<_> = line.split_whitespace().collect();
            (parts.len() >= 9).then(|| DenseProcess {
                pid: parts[0].parse().unwrap(),
                parent: parts[1].parse().unwrap(),
                rss_bytes: parts[2].parse::<u64>().unwrap() * 1024,
                started: parts[3..8].join(" "),
                command: parts[8..].join(" "),
            })
        })
        .collect()
}
#[cfg(unix)]
fn dense_owned_tree(node: &DenseProcess) -> Vec<DenseProcess> {
    let all = dense_processes();
    assert!(
        all.iter().any(|p| p.pid == node.pid
            && p.parent == std::process::id()
            && p.started == node.started
            && p.command == node.command),
        "Owned producer identity changed"
    );
    let mut ids = std::collections::BTreeSet::from([node.pid]);
    loop {
        let before = ids.len();
        for process in &all {
            if ids.contains(&process.parent) {
                ids.insert(process.pid);
            }
        }
        if before == ids.len() {
            break;
        }
    }
    let tree: Vec<_> = all.into_iter().filter(|p| ids.contains(&p.pid)).collect();
    assert!(
        tree.iter()
            .any(|p| p.parent == node.pid && p.command.contains("--remote-debugging-pipe")),
        "Actual owned Chromium was not observed"
    );
    tree
}

#[cfg(unix)]
#[test]
#[ignore = "Requires installed pinned browser runtime and read-only ps; run serially with OPENCUT_CHROME"]
fn rust_browser_dense_ordinary_shared_cas_preview() {
    use opencut_native_services::classic_audio_preview::*;
    let directory = tempfile::tempdir().unwrap();
    let (root, snapshot) = dense_snapshot(directory.path(), 5, 20);
    let prepared = prepare_preview_audio_scene(
        &snapshot,
        "s",
        PcmFormat {
            sample_rate: 48_000,
            channels: 2,
        },
    )
    .unwrap();
    assert_eq!(prepared.audio().voices().count(), 100);
    assert_eq!(prepared.extent(), 300 * 48_000);
    let verified = VerifiedClassicAudioResources::new(&root, prepared.audio(), || Ok(())).unwrap();
    let handles = verified.handles();
    assert_eq!(handles.len(), 1);
    let encoded_bytes = prepared.audio().resources().next().unwrap().version.size;
    let mut resources = CountedDenseResources {
        inner: verified,
        resolutions: 0,
        releases: 0,
        bytes: 0,
    };
    let mut session =
        ClassicAudioSession::new_preview(&prepared, 256 * 1024 * 1024, 32 * 1024 * 1024).unwrap();
    let base = config();
    let settings = ClassicAudioPreviewProcessConfig {
        node: base.node,
        runner: base.runner.with_file_name("preview-runner.mjs"),
        chromium: base.chromium,
        preparation_deadline: Duration::from_secs(120),
        max_pcm_bytes: 256 * 1024 * 1024,
        max_encoded_bytes: 32 * 1024 * 1024,
        max_rss_bytes: 2 * 1024 * 1024 * 1024,
        strict_cgroup: None,
    };
    let mut producer =
        ClassicAudioPreviewProducer::start(&settings, &mut session, &mut resources, &handles)
            .unwrap();
    let mut observed_processes = Vec::new();
    let checks = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let ready = producer.ready_receipt().clone();
        eprintln!(
            "dense preview pinned runtime={}",
            producer.runtime_identity()
        );
        let decoders = ready["decoders"].as_array().unwrap();
        assert_eq!(decoders.len(), 1);
        assert_eq!(decoders[0]["sampleRate"], 48_000);
        assert_eq!(decoders[0]["frameCount"], "192000");
        assert_eq!(decoders[0]["primary"]["channels"], 1);
        let formats = ready["sourceFormats"].as_array().unwrap();
        assert_eq!(formats.len(), 100);
        assert!(formats.iter().all(|f| f["prepared"] == false));
        assert!(ready["preparation"].as_array().unwrap().is_empty());
        assert!(ready["peakAccountedBytes"].as_u64().unwrap() <= settings.max_pcm_bytes);
        assert_eq!(resources.resolutions, 1);
        assert_eq!(resources.releases, 1);
        assert_eq!(resources.bytes, encoded_bytes);
        let runner = settings.runner.to_string_lossy();
        let nodes: Vec<_> = dense_processes()
            .into_iter()
            .filter(|p| p.parent == std::process::id() && p.command.contains(runner.as_ref()))
            .collect();
        assert_eq!(
            nodes.len(),
            1,
            "Only this test's direct producer may be observed"
        );
        let node = &nodes[0];
        let starts: Vec<u64> = (0..20).map(|k| 296 * k * 48_000 / 19).collect();
        let mut sampled_peak = 0;
        let mut application_compile_ms = Vec::new();
        let mut producer_block_ms = Vec::new();
        for i in 0..20 {
            let start = match i % 4 {
                0 => 0,
                1 => 296 * 48_000,
                2 => 5 * 48_000,
                _ => starts[10],
            };
            session.set_preview_state(start, None).unwrap();
            let count = prepared.block_frame_limit(start);
            assert_eq!(count, 4096);
            if i == 0 || i == 19 {
                let began = std::time::Instant::now();
                let compiled = session
                    .callback(
                        AudioCallback::PreviewBlock(PreviewBlockRequest {
                            start_sample: start.to_string(),
                            frame_count: count,
                        }),
                        &mut resources,
                    )
                    .unwrap();
                application_compile_ms.push(began.elapsed().as_secs_f64() * 1000.0);
                assert_eq!(compiled["startSample"], start.to_string());
                assert_eq!(compiled["frameCount"], count);
            }
            let began = std::time::Instant::now();
            // Host production performs the idle authorization/error poll before
            // every next command. Include that path in whole-block throughput.
            producer.checkpoint(&mut session, &mut resources).unwrap();
            let block = producer
                .next(&mut session, &mut resources, i + 1, 0, start, count)
                .unwrap();
            producer_block_ms.push(began.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(block.generation, i + 1);
            assert_eq!(block.start_sample, start);
            assert_eq!(block.frame_count, count);
            assert_eq!(
                block.receipt["sourceBindingSha256"],
                ready["sourceBindingSha256"]
            );
            assert_eq!(block.receipt["engineIdentity"], ready["engineIdentity"]);
            let expected: Vec<f32> = (start..start + u64::from(count))
                .map(|sample| {
                    starts
                        .iter()
                        .filter_map(|&origin| sample.checked_sub(origin))
                        .filter(|&local| local < 192_000)
                        .map(|local| dense_pulse(local) * 5.0)
                        .sum()
                })
                .collect();
            assert_eq!(block.channels[0], expected, "PCM seek {i} start {start}");
            assert_eq!(block.channels[1], expected);
            let tree = dense_owned_tree(node);
            sampled_peak = sampled_peak.max(tree.iter().map(|p| p.rss_bytes).sum::<u64>());
            for process in tree {
                if !observed_processes.iter().any(|p: &DenseProcess| {
                    p.pid == process.pid
                        && p.started == process.started
                        && p.command == process.command
                }) {
                    observed_processes.push(process);
                }
            }
        }
        assert!(sampled_peak > 0 && sampled_peak <= settings.max_rss_bytes);
        eprintln!(
            "dense preview owned-tree sampled peak RSS={sampled_peak}; monitored limit={}",
            settings.max_rss_bytes
        );
        eprintln!(
            "dense preview direct application compile ms={application_compile_ms:?}; checkpoint+whole producer block ms={producer_block_ms:?}"
        );
        assert_eq!(
            resources.resolutions, 1,
            "Seek reuses the same prepared source realm"
        );
    }));
    // A failed oracle still closes the actual realm before TempDir/CAS release.
    let stopped = producer.stop(&mut session, &mut resources);
    assert!(stopped.is_ok(), "Actual producer stop failed: {stopped:?}");
    let remaining = dense_processes();
    for old in observed_processes {
        assert!(
            !remaining
                .iter()
                .any(|p| p.pid == old.pid && p.started == old.started && p.command == old.command),
            "Owned native process remains after Stop"
        );
    }
    if let Err(panic) = checks {
        std::panic::resume_unwind(panic);
    }
}

#[test]
#[ignore = "Requires installed pinned browser runtime; run explicitly with OPENCUT_CHROME"]
fn rust_browser_32_voices_use_contiguous_short_wav_blocks() {
    let directory = tempfile::tempdir().unwrap();
    let (root, snapshot) = dense_snapshot(directory.path(), 32, 1);
    let prepared = prepare_audio_scene(
        &snapshot,
        "s",
        AudioExecutionProfile::ClassicV31,
        PcmFormat {
            sample_rate: 48_000,
            channels: 2,
        },
    )
    .unwrap();
    assert_eq!(prepared.duration_samples(), 8192);
    for start in [0, 2048, 4096, 6144] {
        assert_eq!(prepared.block_frame_limit(start, 4096), 2048);
    }
    let mut resources = VerifiedClassicAudioResources::new(&root, &prepared, || Ok(())).unwrap();
    let handles = resources.handles();
    let mut settings = config();
    settings.max_pcm_bytes = 256 * 1024 * 1024;
    let mut session = ClassicAudioSession::new(
        &prepared,
        settings.max_pcm_bytes,
        settings.max_encoded_bytes,
    )
    .unwrap();
    let mut pcm = Pcm::default();
    let result = ClassicAudioProcess { config: settings }
        .execute(&mut session, &mut resources, &handles, &mut pcm)
        .unwrap();
    assert_eq!(result.receipt.decoders.len(), 1);
    assert_eq!(result.receipt.preparation.len(), 32);
    assert!(!result.receipt.mastering.applied);
    assert_eq!(result.receipt.mastering.input_peak, 0.375);
    let expected: Vec<f32> = (0..8192).map(|sample| dense_pulse(sample) * 32.0).collect();
    assert_eq!(pcm.channels[0], expected);
    assert_eq!(pcm.channels[1], expected);
    let observed: u64 = result.quota.observed_peak_rss_bytes.parse().unwrap();
    assert!(observed > 0 && observed <= 2 * 1024 * 1024 * 1024);
    assert!(!result.quota.hard_memory_enforced);
    #[cfg(unix)]
    unsafe {
        assert_eq!(
            libc::kill(result.runtime_identity.browser_pid as i32, 0),
            -1
        );
    }
}

#[test]
fn dense_33_overlapping_voices_reject_before_native_decode() {
    let directory = tempfile::tempdir().unwrap();
    let (_, snapshot) = dense_snapshot(directory.path(), 33, 1);
    let format = PcmFormat {
        sample_rate: 48_000,
        channels: 2,
    };
    let wav = prepare_audio_scene(&snapshot, "s", AudioExecutionProfile::ClassicV31, format)
        .err()
        .expect("Reject genuine concurrency");
    assert_eq!(wav.code, "UNSUPPORTED_V4");
    let preview = prepare_preview_audio_scene(&snapshot, "s", format)
        .err()
        .expect("Reject genuine concurrency");
    assert_eq!(preview.code, "UNSUPPORTED_V4");
    // No native resources, session or process is constructed after rejection.
}
