//! Reuses the bounded FFmpeg process runner and private metadata admission.
use super::*;
use opencut_media_contracts::quality::*;
use serde_json::Value;
fn invalid(message: &str) -> MediaError {
    MediaError::InvalidRequest(message.into())
}
fn finite(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
        .filter(|v| v.is_finite())
}
fn stream(value: &Value) -> MediaResult<QualityStream> {
    let rate = value
        .get("avg_frame_rate")
        .and_then(Value::as_str)
        .and_then(|r| r.split_once('/'))
        .and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))
        .filter(|(a, b)| *a > 0 && *b > 0);
    Ok(QualityStream {
        codec: value
            .get("codec_name")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("Output codec unavailable"))?
            .into(),
        start_seconds: finite(value.get("start_time")),
        duration_seconds: finite(value.get("duration")),
        width: value
            .get("width")
            .and_then(Value::as_u64)
            .and_then(|n| n.try_into().ok()),
        height: value
            .get("height")
            .and_then(Value::as_u64)
            .and_then(|n| n.try_into().ok()),
        average_rate: rate,
    })
}
fn intervals(
    metadata: &str,
    start_key: &str,
    end_key: &str,
    duration: f64,
) -> MediaResult<Vec<QualityInterval>> {
    let mut current = None;
    let mut intervals = vec![];
    for line in metadata.lines() {
        if let Some((key, value)) = line.split_once('=') {
            if key != start_key && key != end_key {
                continue;
            }
            let Ok(at) = value.trim().parse::<f64>() else {
                continue;
            };
            if !at.is_finite() || at < 0.0 || at > duration + 0.2 {
                return Err(invalid("Detector time outside decoded output"));
            }
            if key == start_key {
                current = Some(at);
            } else if key == end_key {
                if let Some(start) = current.take() {
                    intervals.push(QualityInterval {
                        start_seconds: start,
                        end_seconds: at.max(start),
                    });
                }
            }
        }
    }
    if let Some(start) = current {
        intervals.push(QualityInterval {
            start_seconds: start,
            end_seconds: duration.max(start),
        });
    }
    Ok(intervals)
}
impl FfmpegAnalysisBackend {
    pub fn output_quality(
        &self,
        input: &Path,
        cancel: &CancellationToken,
    ) -> MediaResult<OutputMeasurements> {
        let bytes=crate::process::checked_output_limited(Command::new(&self.ffprobe).args(["-v","error","-protocol_whitelist","file,pipe","-show_entries","format=duration:stream=codec_type,codec_name,start_time,duration,width,height,avg_frame_rate","-of","json"]).arg(input),"ffprobe",cancel,Duration::from_secs(30),self.admission.as_deref())?;
        let probe: Value =
            serde_json::from_slice(&bytes).map_err(|_| invalid("Invalid quality probe JSON"))?;
        let duration = finite(probe.pointer("/format/duration"))
            .ok_or_else(|| invalid("Output duration unavailable"))?;
        if duration <= 0.0 || duration > 900.0 {
            return Err(invalid(
                "Full quality analysis supports output duration in (0,900] seconds",
            ));
        }
        let streams = probe
            .get("streams")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("Output streams unavailable"))?;
        let video = stream(
            streams
                .iter()
                .find(|s| s["codec_type"] == "video")
                .ok_or_else(|| invalid("Output video stream missing"))?,
        )?;
        let audio = streams
            .iter()
            .find(|s| s["codec_type"] == "audio")
            .map(stream)
            .transpose()?;
        let (visual,_)=self.run_metadata(input,"blackdetect=d=0.1:pix_th=0.10,freezedetect=n=-60dB:d=0.5,metadata=mode=add:key=opencut.quality.frame:value=1,metadata=print:file='{OUTPUT}'",true,cancel)?;
        let black = intervals(&visual, "lavfi.black_start", "lavfi.black_end", duration)?;
        let frozen = intervals(
            &visual,
            "lavfi.freezedetect.freeze_start",
            "lavfi.freezedetect.freeze_end",
            duration,
        )?;
        let frames = visual.lines().filter(|l| l.starts_with("frame:")).count();
        let (mut integrated, mut range, mut peak, mut audio_frames) = (None, None, None, None);
        if audio.is_some() {
            let (metadata, _) = self.run_metadata(
                input,
                "ebur128=metadata=1:peak=true,ametadata=print:file='{OUTPUT}'",
                false,
                cancel,
            )?;
            audio_frames = Some(
                metadata
                    .lines()
                    .filter(|l| l.starts_with("frame:"))
                    .count()
                    .to_string(),
            );
            for line in metadata.lines() {
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                let Ok(value) = value.trim().parse::<f64>() else {
                    continue;
                };
                if !value.is_finite() {
                    continue;
                }
                match key {
                    "lavfi.r128.I" => integrated = Some(value),
                    "lavfi.r128.LRA" => range = Some(value),
                    key if key.starts_with("lavfi.r128.true_peaks_ch") => {
                        if value > 0.0 {
                            let db = 20.0 * value.log10();
                            peak = Some(peak.map_or(db, |old: f64| old.max(db)));
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(OutputMeasurements{duration_seconds:duration,video,audio,black_intervals:black,frozen_intervals:frozen,integrated_lufs:integrated,loudness_range_lu:range,true_peak_dbfs:peak,video_frames_decoded:frames.to_string(),audio_blocks_analyzed:audio_frames,coverage:"full_decoded_output; black>=0.1s; freeze>=0.5s; EBU R128; no perceptual sync or aesthetic score".into()})
    }
}
