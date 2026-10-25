use super::*;

pub(super) struct Cursor {
    stream: NativePcmStream,
    current: Option<NativePcmBlock>,
    previous_last: Option<(u64, [f32; 2])>,
    observed: u64,
    eof: Option<u64>,
}
impl Cursor {
    pub(super) fn new(stream: NativePcmStream) -> Self {
        Self {
            stream,
            current: None,
            previous_last: None,
            observed: 0,
            eof: None,
        }
    }
    pub(super) fn observed_frames(&self) -> u64 {
        self.observed
    }
    pub(super) fn eof_frames(&self) -> Option<u64> {
        self.eof
    }
    pub(super) fn frame(
        &mut self,
        index: u64,
        cancel: &CancellationToken,
    ) -> Result<Option<[f32; 2]>, ProjectRenderError> {
        loop {
            if cancel.is_cancelled() {
                return Err(MediaError::Cancelled.into());
            }
            if let Some((last, frame)) = self.previous_last
                && last == index
            {
                return Ok(Some(frame));
            }
            if let Some(block) = &self.current {
                if index < block.start_sample {
                    return Err(error(
                        "INVALID_PROJECT",
                        "Audio source cursor moved backwards across a discarded block",
                    ));
                }
                let local = index - block.start_sample;
                if local < block.frame_count() as u64 {
                    let i = local as usize * 2;
                    return Ok(Some([block.samples[i], block.samples[i + 1]]));
                }
            }
            if self.eof.is_some() {
                return Ok(None);
            }
            let next = self.stream.read(cancel)?;
            if let Some(block) = next {
                if block.start_sample != self.observed || block.frame_count() == 0 {
                    return Err(error(
                        "MEDIA_INVALID_TOOL_OUTPUT",
                        "PCM decoder sample sequence discontinuity",
                    ));
                }
                if let Some(prior) = self.current.take() {
                    let end = prior.samples.len();
                    self.previous_last = Some((
                        prior.start_sample + prior.frame_count() as u64 - 1,
                        [prior.samples[end - 2], prior.samples[end - 1]],
                    ));
                }
                self.observed = block.start_sample + block.frame_count() as u64;
                self.current = Some(block);
            } else {
                self.eof = self.stream.decoded_frame_count();
                return Ok(None);
            }
        }
    }
}
