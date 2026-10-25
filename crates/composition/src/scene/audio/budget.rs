//! Pure integer admission. Placement samples are authoritative half-open spans;
//! no media probe, decode, gain evaluation or allocation of dense PCM facts.
use super::*;

pub(super) fn admit(voices: &[PreparedVoice]) -> Result<(), CompositionError> {
    if voices.len() > MAX_PREPARED_AUDIO_VOICES {
        return Err(unsupported(
            "Classic audio admits at most 256 participating voice records",
        ));
    }
    let mut events = Vec::with_capacity(voices.len() * 2);
    for voice in voices {
        if voice.start > voice.end {
            return Err(invalid("Audio placement has an inverted sample interval"));
        }
        if voice.start < voice.end {
            // false (end) sorts before true (start) at the same sample.
            events.push((voice.start, true));
            events.push((voice.end, false));
        }
    }
    events.sort_unstable();
    let mut active = 0usize;
    for (_, starts) in events {
        if starts {
            active += 1;
            if active > MAX_RUNTIME_AUDIO_VOICES {
                return Err(unsupported(
                    "Classic audio admits at most 32 simultaneous voices",
                ));
            }
        } else {
            active = active
                .checked_sub(1)
                .ok_or_else(|| invalid("Audio admission event underflow"))?;
        }
    }
    Ok(())
}

fn costs(prepared: &PreparedAudio, start: u64, end: u64) -> (usize, u64) {
    let mut union = 0;
    let mut samples = 0;
    for voice in &prepared.voices {
        let frames = end.min(voice.end).saturating_sub(start.max(voice.start));
        if frames > 0 {
            union += 1;
            // End is bounded to a 4096-frame block and metadata to 256
            // voices for Classic. Saturation is still a rejecting cost if a
            // frozen legacy plan ever exceeds those metadata constraints.
            samples = u64::saturating_add(samples, frames);
        }
    }
    (union, samples)
}

fn fits(prepared: &PreparedAudio, start: u64, end: u64) -> bool {
    let (union, samples) = costs(prepared, start, end);
    (prepared.identity.profile != AudioExecutionProfile::ClassicV31
        || union <= MAX_RUNTIME_AUDIO_VOICES)
        && samples <= MAX_AUDIO_BLOCK_GAIN_SAMPLES as u64
}

pub(super) fn check_block(
    prepared: &PreparedAudio,
    start: u64,
    end: u64,
) -> Result<(), CompositionError> {
    let (union, samples) = costs(prepared, start, end);
    if prepared.identity.profile == AudioExecutionProfile::ClassicV31
        && union > MAX_RUNTIME_AUDIO_VOICES
    {
        return Err(invalid("Audio block exceeds the 32-voice union budget"));
    }
    if samples > MAX_AUDIO_BLOCK_GAIN_SAMPLES as u64 {
        return Err(invalid("Audio block exceeds total voice-sample budget"));
    }
    Ok(())
}

pub(super) fn block_frame_limit(prepared: &PreparedAudio, start: u64, requested: u32) -> u32 {
    let remaining = prepared.duration_samples.saturating_sub(start);
    let cap = u64::from(requested.min(MAX_AUDIO_BLOCK_FRAMES)).min(remaining) as u32;
    let (mut low, mut high) = (0, cap);
    // Union participation and summed intersection lengths are monotonic in
    // prefix length. Integer search retains the final sample before voice 33
    // enters, including exact end==start boundaries, without dropping a voice.
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if fits(prepared, start, start + u64::from(middle)) {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    low
}
