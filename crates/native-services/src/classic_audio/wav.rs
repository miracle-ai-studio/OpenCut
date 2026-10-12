use opencut_contracts::EditorError;
use opencut_editor_application::classic_audio::ClassicAudioPcmSink;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
};

const HEADER_BYTES: u64 = 58;
const FRAME_BYTES: u64 = 8;
const BLOCK_FRAMES: usize = 4096;

/// Private attempt WAV output. The host creates/admit-leases the read-write
/// file before constructing this sink and owns cleanup on every failure.
/// Completion returns a synced file; it does not publish an artifact.
pub struct ClassicAudioWavSink {
    file: File,
    frames: u64,
    total_bytes: u64,
    next: [u64; 2],
    failed: bool,
}

impl ClassicAudioWavSink {
    /// Exact storage admission size, including non-PCM fmt and fact chunks.
    pub fn checked_bytes(sample_rate: u32, frames: u64) -> Result<u64, EditorError> {
        if !matches!(sample_rate, 44_100 | 48_000) || frames == 0 {
            return Err(invalid(
                "WAV requires positive stereo frames at 44100 or 48000 Hz",
            ));
        }
        let data = frames
            .checked_mul(FRAME_BYTES)
            .filter(|&bytes| bytes <= u64::from(u32::MAX))
            .ok_or_else(|| invalid("WAV data exceeds the RIFF u32 limit"))?;
        let total = data
            .checked_add(HEADER_BYTES)
            .filter(|&bytes| bytes - 8 <= u64::from(u32::MAX))
            .ok_or_else(|| invalid("WAV container exceeds the RIFF u32 limit"))?;
        u32::try_from(frames).map_err(|_| invalid("WAV fact frame count exceeds u32"))?;
        Ok(total)
    }

    pub fn new(mut file: File, sample_rate: u32, frames: u64) -> Result<Self, EditorError> {
        let total_bytes = Self::checked_bytes(sample_rate, frames)?;
        let metadata = file.metadata().map_err(io_error)?;
        if !metadata.is_file() || metadata.len() != 0 {
            return Err(invalid(
                "WAV output must be a newly created empty regular file",
            ));
        }
        let data_bytes = (frames * FRAME_BYTES) as u32;
        let mut header = [0u8; HEADER_BYTES as usize];
        header[0..4].copy_from_slice(b"RIFF");
        header[4..8].copy_from_slice(&((total_bytes - 8) as u32).to_le_bytes());
        header[8..16].copy_from_slice(b"WAVEfmt ");
        header[16..20].copy_from_slice(&18u32.to_le_bytes());
        header[20..22].copy_from_slice(&3u16.to_le_bytes());
        header[22..24].copy_from_slice(&2u16.to_le_bytes());
        header[24..28].copy_from_slice(&sample_rate.to_le_bytes());
        header[28..32].copy_from_slice(&(sample_rate * 8).to_le_bytes());
        header[32..34].copy_from_slice(&8u16.to_le_bytes());
        header[34..36].copy_from_slice(&32u16.to_le_bytes());
        // cbSize is zero: IEEE float has no extra format extension.
        header[38..42].copy_from_slice(b"fact");
        header[42..46].copy_from_slice(&4u32.to_le_bytes());
        header[46..50].copy_from_slice(&(frames as u32).to_le_bytes());
        header[50..54].copy_from_slice(b"data");
        header[54..58].copy_from_slice(&data_bytes.to_le_bytes());
        // Extension of an empty regular file supplies zeroes for unwritten
        // channels, without retaining a full-scene Rust PCM allocation.
        file.set_len(total_bytes).map_err(io_error)?;
        file.seek(SeekFrom::Start(0)).map_err(io_error)?;
        file.write_all(&header).map_err(io_error)?;
        Ok(Self {
            file,
            frames,
            total_bytes,
            next: [0, 0],
            failed: false,
        })
    }

    fn write_chunk(
        &mut self,
        channel: u32,
        offset: u64,
        samples: &[f32],
    ) -> Result<(), EditorError> {
        if self.failed || channel > 1 || samples.is_empty() || samples.len() > BLOCK_FRAMES {
            return Err(invalid("Invalid or failed WAV PCM chunk"));
        }
        let channel = channel as usize;
        let end = offset
            .checked_add(samples.len() as u64)
            .filter(|&end| end <= self.frames)
            .ok_or_else(|| invalid("WAV PCM chunk exceeds the admitted frame range"))?;
        if offset != self.next[channel] || samples.iter().any(|sample| !sample.is_finite()) {
            return Err(invalid(
                "WAV PCM must be finite and strictly ordered per channel",
            ));
        }
        let position = HEADER_BYTES + offset * FRAME_BYTES;
        let mut interleaved = [0u8; BLOCK_FRAMES * FRAME_BYTES as usize];
        let bytes = &mut interleaved[..samples.len() * FRAME_BYTES as usize];
        self.file
            .seek(SeekFrom::Start(position))
            .map_err(io_error)?;
        self.file.read_exact(bytes).map_err(io_error)?;
        for (frame, sample) in samples.iter().enumerate() {
            let at = frame * 8 + channel * 4;
            bytes[at..at + 4].copy_from_slice(&sample.to_le_bytes());
        }
        self.file
            .seek(SeekFrom::Start(position))
            .map_err(io_error)?;
        self.file.write_all(bytes).map_err(io_error)?;
        self.next[channel] = end;
        Ok(())
    }

    pub fn finalize(self) -> Result<File, EditorError> {
        if self.failed {
            return Err(invalid("WAV execution previously rejected PCM"));
        }
        if self.next != [self.frames; 2] {
            return Err(EditorError::new(
                "INCOMPLETE_OUTPUT_PCM",
                "WAV is missing channel frames",
            ));
        }
        if self.file.metadata().map_err(io_error)?.len() != self.total_bytes {
            return Err(invalid("WAV output length changed during execution"));
        }
        self.file.sync_all().map_err(io_error)?;
        Ok(self.file)
    }
}

impl ClassicAudioPcmSink for ClassicAudioWavSink {
    fn chunk(&mut self, channel: u32, offset: u64, samples: &[f32]) -> Result<(), EditorError> {
        let result = self.write_chunk(channel, offset, samples);
        self.failed |= result.is_err();
        result
    }
}

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_OUTPUT_PCM", message)
}
fn io_error(_: std::io::Error) -> EditorError {
    EditorError::new("AUDIO_WAV_IO_FAILED", "Private WAV output I/O failed")
}
