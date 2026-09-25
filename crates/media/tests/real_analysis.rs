use opencut_media::{
    CancellationToken, FfmpegAnalysisBackend, ShotsRequest, SilenceRequest, WaveformRequest,
};
use std::{fs, io::Write, path::Path, process::Command};

fn wav(path: &Path) {
    let rate = 16_000u32;
    let count = rate * 2;
    let mut file = fs::File::create(path).unwrap();
    let data_size = count * 2;
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36 + data_size).to_le_bytes()).unwrap();
    file.write_all(b"WAVEfmt ").unwrap();
    file.write_all(&16u32.to_le_bytes()).unwrap();
    file.write_all(&1u16.to_le_bytes()).unwrap();
    file.write_all(&1u16.to_le_bytes()).unwrap();
    file.write_all(&rate.to_le_bytes()).unwrap();
    file.write_all(&(rate * 2).to_le_bytes()).unwrap();
    file.write_all(&2u16.to_le_bytes()).unwrap();
    file.write_all(&16u16.to_le_bytes()).unwrap();
    file.write_all(b"data").unwrap();
    file.write_all(&data_size.to_le_bytes()).unwrap();
    for index in 0..count {
        let time = index as f64 / rate as f64;
        let value = if (0.5..1.5).contains(&time) {
            (time * std::f64::consts::TAU * 440.0).sin() * 12_000.0
        } else {
            0.0
        };
        file.write_all(&(value as i16).to_le_bytes()).unwrap();
    }
}

#[test]
fn measured_silence_waveform_and_scene_boundaries() {
    let ffmpeg = if Path::new("/usr/local/bin/ffmpeg").is_file() {
        "/usr/local/bin/ffmpeg"
    } else {
        "ffmpeg"
    };
    let ffprobe = if Path::new("/usr/local/bin/ffprobe").is_file() {
        "/usr/local/bin/ffprobe"
    } else {
        "ffprobe"
    };
    if Command::new(ffmpeg).arg("-version").output().is_err()
        || Command::new(ffprobe).arg("-version").output().is_err()
    {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let audio = directory.path().join("audio.wav");
    wav(&audio);
    let engine = FfmpegAnalysisBackend::new(ffmpeg, ffprobe);
    let cancel = CancellationToken::new();
    let silence = engine
        .silence(
            &SilenceRequest {
                input: audio.clone(),
                noise_db: -35,
                min_duration_ms: 200,
            },
            &cancel,
        )
        .unwrap();
    assert_eq!(silence.provider, "ffmpeg");
    assert!(!silence.confidence_known);
    assert!(silence.intervals.iter().any(|interval| interval.start.ticks <= 30 && (400..=600).contains(&interval.end.ticks)));
    assert!(silence.intervals.iter().any(
        |interval| (1400..=1600).contains(&interval.start.ticks) && interval.end.ticks >= 1900
    ));
    let waveform = engine
        .waveform(
            &WaveformRequest {
                input: audio,
                buckets: 20,
            },
            &cancel,
        )
        .unwrap();
    assert!(waveform.buckets.len() <= 20);
    assert!(waveform.buckets.first().unwrap().peak < 0.001);
    assert!(
        waveform
            .buckets
            .iter()
            .any(|bucket| bucket.peak > 0.2 && bucket.rms > 0.1)
    );

    let video = directory.path().join("cut.mp4");
    let status = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=64x64:r=10:d=1",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=64x64:r=10:d=1",
            "-filter_complex",
            "[0:v][1:v]concat=n=2:v=1:a=0",
            "-c:v",
            "mpeg4",
            "-y",
        ])
        .arg(&video)
        .status()
        .unwrap();
    assert!(status.success());
    let shots = engine
        .shots(
            &ShotsRequest {
                input: video,
                threshold: 0.05,
            },
            &cancel,
        )
        .unwrap();
    assert!(
        shots
            .boundaries
            .iter()
            .any(|boundary| (900..=1100).contains(&boundary.at.ticks))
    );
    assert!(!shots.confidence_known);

    let still = directory.path().join("still.mp4");
    let status = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=64x64:r=10:d=1",
            "-c:v",
            "mpeg4",
            "-y",
        ])
        .arg(&still)
        .status()
        .unwrap();
    assert!(status.success());
    let empty = engine
        .shots(
            &ShotsRequest {
                input: still,
                threshold: 0.5,
            },
            &cancel,
        )
        .unwrap();
    assert!(empty.boundaries.is_empty());
}

#[test]
fn waveform_clips_aac_encoder_padding_to_container_duration() {
    let ffmpeg = if Path::new("/usr/local/bin/ffmpeg").is_file() {
        "/usr/local/bin/ffmpeg"
    } else {
        "ffmpeg"
    };
    let ffprobe = if Path::new("/usr/local/bin/ffprobe").is_file() {
        "/usr/local/bin/ffprobe"
    } else {
        "ffprobe"
    };
    if Command::new(ffmpeg).arg("-version").output().is_err()
        || Command::new(ffprobe).arg("-version").output().is_err()
    {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("short-aac.mp4");
    let status = Command::new(ffmpeg)
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=red:s=320x180:r=10:d=2",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
            "-y",
        ])
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success());
    let buckets = FfmpegAnalysisBackend::new(ffmpeg, ffprobe)
        .waveform(
            &WaveformRequest {
                input: source,
                buckets: 32,
            },
            &CancellationToken::new(),
        )
        .unwrap()
        .buckets;
    assert_eq!(buckets.len(), 32);
    assert_eq!(buckets.last().unwrap().end.ticks, 32_000);
    assert!(
        buckets
            .iter()
            .all(|bucket| bucket.end.ticks > bucket.start.ticks)
    );
}
