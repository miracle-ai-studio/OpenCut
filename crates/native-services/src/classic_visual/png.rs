//! Streaming RGBA8 encoding into an already admitted, private file descriptor.
use opencut_contracts::EditorError;
use std::{
    fs::File,
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

const CHUNK_BYTES: usize = 65_536;
const MAX_ROW_BYTES: u64 = 262_144;

pub struct ClassicVisualPngSink {
    file: File,
    writer: Option<png::StreamWriter<'static, LimitedFile>>,
    written: Arc<AtomicU64>,
    expected: u64,
    received: u64,
    maximum: u64,
    failed: bool,
}
struct LimitedFile {
    file: File,
    written: Arc<AtomicU64>,
    maximum: u64,
}
impl Write for LimitedFile {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let written = self.written.load(Ordering::Relaxed);
        if bytes.len() as u64 > self.maximum.saturating_sub(written) {
            return Err(std::io::Error::other(
                "PNG output exceeded its admitted byte limit",
            ));
        }
        let count = self.file.write(bytes)?;
        self.written.fetch_add(count as u64, Ordering::Relaxed);
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
impl ClassicVisualPngSink {
    pub fn output_ceiling(width: u32, height: u32) -> Result<u64, EditorError> {
        Self::checked_bytes(width, height)
    }
    /// Conservative stored-deflate bound, including filters, IDAT framing and
    /// headers. The descriptor writer separately enforces this admission.
    pub fn checked_bytes(width: u32, height: u32) -> Result<u64, EditorError> {
        let row = u64::from(width).checked_mul(4).ok_or_else(invalid)?;
        if row == 0 || row > MAX_ROW_BYTES || height == 0 {
            return Err(invalid());
        }
        row.checked_add(1)
            .and_then(|row| row.checked_mul(u64::from(height)))
            .and_then(|raw| raw.checked_mul(2))
            .and_then(|n| n.checked_add(65_536))
            .ok_or_else(invalid)
    }
    pub fn new(
        file: File,
        width: u32,
        height: u32,
        admitted_bytes: u64,
    ) -> Result<Self, EditorError> {
        let maximum = Self::checked_bytes(width, height)?;
        let metadata = file.metadata().map_err(io_error)?;
        if maximum > admitted_bytes {
            return Err(budget());
        }
        if !metadata.is_file() || metadata.len() != 0 {
            return Err(invalid());
        }
        let written = Arc::new(AtomicU64::new(0));
        let target = LimitedFile {
            file: file.try_clone().map_err(io_error)?,
            written: written.clone(),
            maximum,
        };
        let mut encoder = png::Encoder::new(target, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::NoCompression);
        encoder.set_filter(png::Filter::NoFilter);
        let writer = encoder
            .write_header()
            .map_err(encode_error)?
            .into_stream_writer_with_size(4096)
            .map_err(encode_error)?;
        Ok(Self {
            file,
            writer: Some(writer),
            written,
            expected: u64::from(width) * u64::from(height) * 4,
            received: 0,
            maximum,
            failed: false,
        })
    }
    pub fn chunk(&mut self, offset: u64, bytes: &[u8]) -> Result<(), EditorError> {
        if self.failed
            || offset != self.received
            || bytes.is_empty()
            || bytes.len() > CHUNK_BYTES
            || !bytes.len().is_multiple_of(4)
            || bytes.len() as u64 > self.expected.saturating_sub(self.received)
        {
            self.failed = true;
            return Err(invalid());
        }
        if self
            .writer
            .as_mut()
            .ok_or_else(invalid)?
            .write_all(bytes)
            .is_err()
        {
            self.failed = true;
            return Err(encode_error(()));
        }
        self.received += bytes.len() as u64;
        Ok(())
    }
    pub fn finalize(mut self) -> Result<File, EditorError> {
        if self.failed || self.received != self.expected {
            return Err(invalid());
        }
        self.writer
            .take()
            .ok_or_else(invalid)?
            .finish()
            .map_err(encode_error)?;
        let length = self.file.metadata().map_err(io_error)?.len();
        if length != self.written.load(Ordering::Relaxed) || length > self.maximum || length < 57 {
            return Err(invalid());
        }
        self.file.sync_all().map_err(io_error)?;
        Ok(self.file)
    }
}
impl opencut_editor_application::classic_visual::ClassicVisualRgbaSink for ClassicVisualPngSink {
    fn chunk(&mut self, offset_bytes: u64, bytes: &[u8]) -> Result<(), EditorError> {
        self.chunk(offset_bytes, bytes)
    }
}
fn invalid() -> EditorError {
    EditorError::new(
        "INVALID_OUTPUT_RGBA",
        "RGBA chunks or PNG dimensions are invalid or incomplete",
    )
}
fn budget() -> EditorError {
    EditorError::new(
        "VISUAL_BUDGET_EXCEEDED",
        "PNG output exceeds its admitted byte limit",
    )
}
fn io_error(_: std::io::Error) -> EditorError {
    EditorError::new(
        "VISUAL_OUTPUT_FAILED",
        "Private PNG descriptor could not be written",
    )
}
fn encode_error<T>(_: T) -> EditorError {
    EditorError::new(
        "VISUAL_OUTPUT_FAILED",
        "Bounded streaming PNG encoding failed",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, SeekFrom};
    #[test]
    fn writes_rgba_rows_across_different_chunk_boundaries() {
        let file = tempfile::tempfile().unwrap();
        let maximum = ClassicVisualPngSink::checked_bytes(2, 2).unwrap();
        let mut sink = ClassicVisualPngSink::new(file, 2, 2, maximum).unwrap();
        let expected = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 128, 0, 0, 0, 0];
        sink.chunk(0, &expected[..4]).unwrap();
        sink.chunk(4, &expected[4..12]).unwrap();
        sink.chunk(12, &expected[12..]).unwrap();
        let mut file = sink.finalize().unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut encoded = Vec::new();
        file.read_to_end(&mut encoded).unwrap();
        assert_eq!(&encoded[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&encoded[16..24], &[0, 0, 0, 2, 0, 0, 0, 2]);
        assert_eq!(&encoded[24..26], &[8, 6]);
        let mut reader = png::Decoder::new(std::io::Cursor::new(encoded))
            .read_info()
            .unwrap();
        let mut pixels = [0; 16];
        reader.next_frame(&mut pixels).unwrap();
        assert_eq!(pixels, expected);
    }
    #[test]
    fn rejects_budget_order_replay_missing_and_overflow_without_publication() {
        assert!(ClassicVisualPngSink::checked_bytes(0, 1).is_err());
        assert!(ClassicVisualPngSink::checked_bytes(u32::MAX, u32::MAX).is_err());
        let maximum = ClassicVisualPngSink::checked_bytes(1, 2).unwrap();
        assert_eq!(
            ClassicVisualPngSink::new(tempfile::tempfile().unwrap(), 1, 2, maximum - 1)
                .err()
                .unwrap()
                .code,
            "VISUAL_BUDGET_EXCEEDED"
        );
        for (offset, bytes) in [(4, vec![0; 4]), (0, vec![0; 3]), (0, vec![0; 12])] {
            let mut sink =
                ClassicVisualPngSink::new(tempfile::tempfile().unwrap(), 1, 2, maximum).unwrap();
            assert!(sink.chunk(offset, &bytes).is_err());
            assert!(sink.finalize().is_err());
        }
        let mut sink =
            ClassicVisualPngSink::new(tempfile::tempfile().unwrap(), 1, 2, maximum).unwrap();
        sink.chunk(0, &[255; 4]).unwrap();
        assert!(sink.chunk(0, &[255; 4]).is_err());
        assert!(sink.finalize().is_err());
        let mut sink =
            ClassicVisualPngSink::new(tempfile::tempfile().unwrap(), 1, 2, maximum).unwrap();
        sink.chunk(0, &[255; 4]).unwrap();
        assert!(sink.finalize().is_err());
    }
}
