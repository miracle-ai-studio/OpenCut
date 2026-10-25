//! One Float32 stereo WAVE layout shared by output admission and validation.
//! RF64 carries64-bit lengths in the standard WAVE container extension.
use opencut_contracts::EditorError;

pub struct ClassicAudioWavLayout {
    pub header_bytes: u64,
    pub total_bytes: u64,
    pub rf64: bool,
}

impl ClassicAudioWavLayout {
    pub fn new(sample_rate: u32, frames: u64) -> Result<Self, EditorError> {
        if !matches!(sample_rate, 44_100 | 48_000) || frames == 0 {
            return Err(invalid(
                "WAVE requires positive stereo frames at 44100 or 48000 Hz",
            ));
        }
        let data = frames
            .checked_mul(8)
            .ok_or_else(|| invalid("WAVE data size overflow"))?;
        let riff_size = data
            .checked_add(50)
            .ok_or_else(|| invalid("WAVE size overflow"))?;
        let rf64 = riff_size > u64::from(u32::MAX);
        let header_bytes = if rf64 { 94 } else { 58 };
        let total_bytes = data
            .checked_add(header_bytes)
            .filter(|bytes| *bytes <= i64::MAX as u64)
            .ok_or_else(|| invalid("WAVE exceeds native file addressability"))?;
        Ok(Self {
            header_bytes,
            total_bytes,
            rf64,
        })
    }
}

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_OUTPUT_PCM", message)
}
