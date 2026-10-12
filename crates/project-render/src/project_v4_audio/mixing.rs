use super::*;

pub(super) fn mix(
    voice: &AudioVoiceBlockV4,
    rate: u32,
    cursor: &mut cursor::Cursor,
    output: &mut [f32],
    cancel: &CancellationToken,
) -> Result<(), ProjectRenderError> {
    for i in 0..voice.frame_count {
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled.into());
        }
        let local = voice.local_start_sample + u64::from(i);
        let position = voice.source.position_at(local, rate)?;
        let Some(lower) = cursor.frame(position.lower, cancel)? else {
            continue;
        };
        // Last sample is repeated only as interpolation's upper endpoint.
        // A lower index at or beyond actual EOF remains exact zero tail.
        let upper = if position.fraction_numerator == 0 {
            lower
        } else {
            cursor
                .frame(
                    position
                        .lower
                        .checked_add(1)
                        .ok_or_else(|| error("INVALID_PROJECT", "Audio sample index overflow"))?,
                    cancel,
                )?
                .unwrap_or(lower)
        };
        let fraction = position.fraction_numerator as f64 / position.fraction_denominator as f64;
        let gain = match &voice.gain {
            SampleGainV4::Constant { amplitude } => *amplitude,
            SampleGainV4::Dense { amplitudes } => amplitudes[i as usize],
        };
        let start = (voice.block_offset + i) as usize * 2;
        for channel in 0..2 {
            let sample =
                f64::from(lower[channel]) * (1.0 - fraction) + f64::from(upper[channel]) * fraction;
            let mixed = (f64::from(output[start + channel]) + sample * gain) as f32;
            if !mixed.is_finite() {
                return Err(error(
                    "INVALID_PROJECT",
                    "Unmastered PCM mix overflowed finite float32",
                ));
            }
            output[start + channel] = mixed;
        }
    }
    Ok(())
}
