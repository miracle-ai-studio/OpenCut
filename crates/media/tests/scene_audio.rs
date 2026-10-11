//! Independent sample oracle for the audio-only path used by scene transcription.
use opencut_media::{CancellationToken, FfmpegBackend, TimelineExportRequest};
use serde_json::json;
use std::process::Command;

#[test]
fn audio_only_mix_preserves_source_trim_rate_delay_and_gain_without_video_work() {
    let backend = FfmpegBackend::discover().expect("native media acceptance requires FFmpeg");
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("tones.wav");
    let output = root.path().join("mix.wav");
    let mut samples = Vec::new();
    for n in 0..48000 * 3 {
        let frequency = if n < 48000 {
            330.0
        } else if n < 96000 {
            440.0
        } else {
            660.0
        };
        let value = (8192.0 * (2.0 * std::f64::consts::PI * frequency * n as f64 / 48000.0).sin())
            .round() as i16;
        samples.extend_from_slice(&value.to_le_bytes());
        samples.extend_from_slice(&value.to_le_bytes());
    }
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + samples.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&48000u32.to_le_bytes());
    wav.extend_from_slice(&192000u32.to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    wav.extend(samples);
    std::fs::write(&source, wav).unwrap();
    let time = |ticks: i64| json!({"ticks":ticks.to_string(),"timebase":48000});
    let request:TimelineExportRequest=serde_json::from_value(json!({
        "output":output,"overwrite":false,"container":"mp4",
        "video":{"width":160,"height":90,"frame_rate":{"numerator":25,"denominator":1},
            "codec":"h264","aspect_mode":"fit","crf":23},
        "duration":time(120000),"video_clips":[],"image_clips":[],"captions":[],
        "audio_clips":[
            {"input":source,"source_start":time(48000),"source_duration":time(48000),
                "timeline_start":time(24000),"timeline_duration":time(24000),"gain":0.5,"optional":false},
            {"input":source,"source_start":time(0),"source_duration":time(48000),
                "timeline_start":time(48000),"timeline_duration":time(48000),"gain":0.25,"optional":false}
        ]})).unwrap();
    backend
        .render_timeline_audio(&request, &CancellationToken::new(), &mut |_| {})
        .unwrap();
    let bytes = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&output)
        .args(["-f", "s16le", "-ar", "48000", "-ac", "2", "-"])
        .output()
        .unwrap();
    assert!(bytes.status.success());
    assert_eq!(bytes.stdout.len(), 120000 * 4);
    let decoded: Vec<_> = bytes
        .stdout
        .chunks_exact(4)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]) as f64)
        .collect();
    let rms = |start: usize, end: usize, frequency: f64, gain: f64| {
        let mut sum = 0.0;
        for (n, sample) in decoded.iter().enumerate().take(end).skip(start) {
            let expected =
                8192.0 * gain * (2.0 * std::f64::consts::PI * frequency * n as f64 / 48000.0).sin();
            sum += (sample - expected).powi(2);
        }
        (sum / (end - start) as f64).sqrt()
    };
    assert!(decoded[..24000].iter().all(|sample| *sample == 0.0));
    assert!(rms(25000, 47000, 880.0, 0.5) < 5.0);
    assert!(rms(49000, 95000, 330.0, 0.25) < 2.0);
    assert!(decoded[100000..].iter().all(|sample| *sample == 0.0));
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let mut another = request.clone();
    another.output = root.path().join("cancelled.wav");
    assert!(
        backend
            .render_timeline_audio(&another, &cancelled, &mut |_| {})
            .is_err()
    );
    assert!(!another.output.exists());
}
