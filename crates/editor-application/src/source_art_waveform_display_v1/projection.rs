use super::*;
use opencut_contracts::Time;
use opencut_editor_core::project_v4::{
    SampledChannelV4, apply_sample, gain_amplitude, sample_animation_channel_v4,
};

pub(super) fn layout(
    viewport: &SourceArtWaveformViewportV1,
) -> Result<(SourceArtWaveformCanvasV1, usize), EditorError> {
    let values = [
        viewport.clip_left_px,
        viewport.clip_right_px,
        viewport.pixels_per_second,
        viewport.css_height,
        viewport.device_pixel_ratio,
    ];
    if values.iter().any(|value| !value.is_finite())
        || viewport.clip_left_px < 0.0
        || viewport.clip_right_px <= viewport.clip_left_px
        || viewport.pixels_per_second <= 0.0
        || viewport.css_height <= 0.0
        || viewport.device_pixel_ratio <= 0.0
    {
        return Err(invalid(
            "Waveform display needs a finite positive visible viewport",
        ));
    }
    let width = viewport.clip_right_px - viewport.clip_left_px;
    let count = (width / 2.0).floor().max(1.0);
    if count > MAX_WAVEFORM_DISPLAY_BARS_V1 as f64 {
        return Err(budget("Waveform display exceeds 4096 bars"));
    }
    let canvas_width = device_u32((width * viewport.device_pixel_ratio).ceil().max(1.0))?;
    let canvas_height = device_u32(
        (viewport.css_height * viewport.device_pixel_ratio)
            .round()
            .max(1.0),
    )?;
    if u64::from(canvas_width) * u64::from(canvas_height) > MAX_WAVEFORM_CANVAS_PIXELS_V1 {
        return Err(budget(
            "Waveform display canvas exceeds 32 MiB RGBA-equivalent allocation",
        ));
    }
    Ok((
        SourceArtWaveformCanvasV1 {
            width: canvas_width,
            height: canvas_height,
            css_left: viewport.clip_left_px,
            css_width: width,
            css_height: viewport.css_height,
        },
        count as usize,
    ))
}

fn device_u32(value: f64) -> Result<u32, EditorError> {
    if !value.is_finite() || value < 0.0 || value > f64::from(u32::MAX) {
        return Err(budget(
            "Waveform device geometry exceeds its integer budget",
        ));
    }
    Ok(value as u32)
}

fn js_round(value: f64) -> f64 {
    let rounded = value.round();
    // Rust rounds a negative tie away from zero; Math.round ties toward +inf.
    if value < 0.0 && value - rounded == 0.5 {
        rounded + 1.0
    } else {
        rounded
    }
}

fn clock_ticks(seconds: f64) -> Result<Time, EditorError> {
    // Match audio-state.ts: Math.round(localTime * TICKS_PER_SECOND).
    // This is an explicit IEEE observation profile, not authored time editing.
    let ticks = (seconds * 120_000.0).round();
    if !ticks.is_finite() || !(0.0..9_223_372_036_854_775_808.0).contains(&ticks) {
        return Err(overflow());
    }
    Ok(Time {
        ticks: (ticks as i64).to_string(),
        timebase: 120_000,
    })
}

pub(super) fn gain_samples(
    prepared: &PreparedSourceArtWaveformDisplayV1,
) -> Result<Vec<f64>, EditorError> {
    let ElementContentV4::Audio { audio, .. } = &prepared.element.content else {
        return Err(unsupported("Waveform display requires an audio element"));
    };
    if audio.muted {
        return Ok(vec![0.0; CLASSIC_WAVEFORM_GAIN_SAMPLES_V1]);
    }
    let GainV4::Decibels { value: stored } = audio.gain else {
        return Err(unsupported(
            "Waveform display cannot reinterpret legacy linear gain",
        ));
    };
    let binding = ScalarBindingV4::Property {
        parameter: ScalarParameterV4::GainDecibels,
    };
    let mut gains = Vec::with_capacity(CLASSIC_WAVEFORM_GAIN_SAMPLES_V1);
    for index in 0..CLASSIC_WAVEFORM_GAIN_SAMPLES_V1 {
        // Deliberately preserve the Classic expression order and midpoint grid.
        let seconds = ((index as f64 + 0.5) / CLASSIC_WAVEFORM_GAIN_SAMPLES_V1 as f64)
            * prepared.duration_seconds;
        let at = clock_ticks(seconds)?;
        let mut db = prepared
            .request
            .temporary_gain
            .map_or(stored, |tenths| f64::from(tenths) / 10.0);
        for channel in &prepared.element.channels {
            if !matches!(
                channel,
                AnimationChannelV4::Scalar {
                    binding: ScalarBindingV4::Property {
                        parameter: ScalarParameterV4::GainDecibels
                    },
                    ..
                }
            ) {
                continue;
            }
            let SampledChannelV4::Scalar { physical, .. } =
                sample_animation_channel_v4(channel, &at)?
            else {
                return Err(invalid("Waveform gain channel has an incompatible value"));
            };
            db = physical;
        }
        gains.push(gain_amplitude(&GainV4::Decibels {
            value: apply_sample(&binding, db)?,
        })?);
    }
    Ok(gains)
}

fn interpolated_gain(samples: &[f64], local: f64, duration: f64) -> f64 {
    let progress = (local / duration).clamp(0.0, 1.0);
    let index = progress * (samples.len() - 1) as f64;
    let lo = index.floor() as usize;
    let hi = (lo + 1).min(samples.len() - 1);
    samples[lo] + (samples[hi] - samples[lo]) * (index - lo as f64)
}

fn sample_index(seconds: f64, rate: u32, ceil: bool) -> Result<u64, EditorError> {
    let index = (seconds * f64::from(rate)).max(0.0);
    let index = if ceil { index.ceil() } else { index.floor() };
    if !index.is_finite() || index >= 18_446_744_073_709_551_616.0 {
        return Err(overflow());
    }
    Ok(index as u64)
}

fn peak(summary: &[f32], start: u64, end: u64) -> f32 {
    if end <= start {
        return 0.0;
    }
    let first = (start / 128).min(summary.len() as u64) as usize;
    let last = end.div_ceil(128).min(summary.len() as u64) as usize;
    summary[first..last].iter().copied().fold(0.0, f32::max)
}

fn fraction(amplitude: f64) -> f64 {
    if amplitude <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * amplitude.log10();
    if db <= -40.0 {
        0.0
    } else {
        ((db + 40.0) / 40.0).powf(1.5).min(1.0)
    }
}

pub(super) fn project(
    prepared: &PreparedSourceArtWaveformDisplayV1,
    sample_rate: u32,
    total_samples: u64,
    summary: &[f32],
) -> Result<(SourceArtWaveformCanvasV1, Vec<SourceArtWaveformBarV1>), EditorError> {
    let viewport = &prepared.request.viewport;
    let (canvas, count) = layout(viewport)?;
    let gains = gain_samples(prepared)?;
    let scale_y = f64::from(canvas.height) / viewport.css_height;
    let mut bars = Vec::with_capacity(count);
    for index in 0..count {
        let left = viewport.clip_left_px + index as f64 * 2.0;
        let right = viewport.clip_right_px.min(left + 2.0);
        let local_start = (left / viewport.pixels_per_second).clamp(0.0, prepared.duration_seconds);
        let local_end =
            (right / viewport.pixels_per_second).clamp(local_start, prepared.duration_seconds);
        let source_start = sample_index(
            prepared.source_start_seconds + local_start * prepared.effective_rate,
            sample_rate,
            false,
        )?;
        let source_end = sample_index(
            prepared.source_start_seconds + local_end * prepared.effective_rate,
            sample_rate,
            true,
        )?
        .min(total_samples);
        let source_peak = peak(summary, source_start, source_end);
        let center = ((viewport.clip_left_px + index as f64 * 2.0 + 0.5)
            / viewport.pixels_per_second)
            .clamp(0.0, prepared.duration_seconds);
        let gain = interpolated_gain(&gains, center, prepared.duration_seconds).max(0.0);
        let output_amplitude = f64::from(source_peak.max(0.0)) * gain;
        if !output_amplitude.is_finite() {
            return Err(invalid("Waveform output amplitude is nonfinite"));
        }
        let height_fraction = fraction(output_amplitude);
        let device_left = device_u32((index as f64 * 2.0 * viewport.device_pixel_ratio).round())?;
        let device_right =
            device_u32(((index as f64 * 2.0 + 1.0) * viewport.device_pixel_ratio).round())?.max(
                device_left
                    .checked_add(1)
                    .ok_or_else(|| budget("Waveform bar coordinate overflow"))?,
            );
        let (device_top, device_height) = if height_fraction > 0.0 {
            let bar_height = (height_fraction * viewport.css_height).max(1.0);
            let top = js_round((viewport.css_height - bar_height) * scale_y);
            if !top.is_finite() || top < i64::MIN as f64 || top >= 9_223_372_036_854_775_808.0 {
                return Err(budget("Waveform bar height exceeds its integer budget"));
            }
            (
                top as i64,
                device_u32((f64::from(canvas.height) - top).max(1.0))?,
            )
        } else {
            (i64::from(canvas.height), 0)
        };
        let burn = output_amplitude > 1.0;
        bars.push(SourceArtWaveformBarV1 {
            source_start_sample: source_start.to_string(),
            source_end_sample: source_end.to_string(),
            peak: source_peak,
            gain,
            output_amplitude,
            height_fraction,
            burn,
            device_left,
            device_right,
            device_top,
            device_height,
            burn_height: if burn {
                device_u32(scale_y.round().max(1.0))?
            } else {
                0
            },
        });
    }
    Ok((canvas, bars))
}
