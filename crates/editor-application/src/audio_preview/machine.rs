use super::*;
use opencut_composition::scene::PreviewGainOverride;
use opencut_contracts::{EditorError, Time};
use opencut_time::RationalTime;
use std::collections::VecDeque;

pub const AUDIO_PREVIEW_OWNER_LEASE_MS: u64 = 30_000;
pub const AUDIO_PREVIEW_TTL_MS: u64 = 3_600_000;
pub const AUDIO_PREVIEW_AUTHORIZATION_HEARTBEAT_MS: u64 = 1_000;
pub const AUDIO_PREVIEW_MAX_SESSIONS: usize = 4;
pub const AUDIO_PREVIEW_MAX_BLOCK_FRAMES: u32 = 4096;
pub const AUDIO_PREVIEW_BUFFER_BLOCKS: usize = 32;
pub const AUDIO_PREVIEW_MAX_BUFFERED_FRAMES: u64 =
    AUDIO_PREVIEW_MAX_BLOCK_FRAMES as u64 * AUDIO_PREVIEW_BUFFER_BLOCKS as u64;
const HISTORY: usize = 256;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioPreviewEffect {
    None,
    Start,
    Cancel,
    Replace,
    Released,
}
#[derive(Clone, Debug)]
pub struct AudioPreviewTransition {
    pub session: AudioPreviewSession,
    pub effect: AudioPreviewEffect,
}
#[derive(Clone)]
struct Receipt {
    key: String,
    request: String,
    owner: String,
    view: AudioPreviewSession,
}
#[derive(Clone)]
pub struct AudioPreviewMachine {
    view: AudioPreviewSession,
    generation: u64,
    overlay_epoch: u64,
    anchor: u64,
    total: u64,
    consumed: u64,
    output_frames: u64,
    produced: u64,
    pending_blocks: VecDeque<u64>,
    lease_until: u64,
    expires: u64,
    presentation_at: Option<u64>,
    report_sequence: Option<u64>,
    cursor: u64,
    events: VecDeque<AudioPreviewEvent>,
    receipts: VecDeque<Receipt>,
}
fn err(code: &str) -> EditorError {
    EditorError::new(
        code,
        "Audio preview request does not match the current session",
    )
}
fn decimal(value: &str) -> Result<u64, EditorError> {
    crate::classic_audio::decimal(value).map_err(|_| err("INVALID_ARGUMENT"))
}
fn bounded_id(id: &str) -> Result<(), EditorError> {
    if id.trim().is_empty() || id.len() > 256 || id.chars().any(char::is_control) {
        return Err(err("INVALID_ARGUMENT"));
    }
    Ok(())
}
fn bounded_principal(id: &str) -> Result<(), EditorError> {
    if id.trim().is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
        return Err(err("INVALID_ARGUMENT"));
    }
    Ok(())
}
fn sample_time(sample: u64, rate: u32) -> Result<Time, EditorError> {
    let sample = i64::try_from(sample).map_err(|_| err("INVALID_ARGUMENT"))?;
    Ok(RationalTime::new(sample, rate)?.into())
}
fn anchor(time: &Time, rate: u32, total: u64) -> Result<u64, EditorError> {
    let time = RationalTime::try_from(time)?;
    if time.ticks() < 0 {
        return Err(err("INVALID_ARGUMENT"));
    }
    let n = u128::from(time.ticks().unsigned_abs()) * u128::from(rate);
    let sample =
        u64::try_from(n / u128::from(time.timebase())).map_err(|_| err("INVALID_ARGUMENT"))?;
    // Check the original request against the half-open extent before flooring.
    if n >= u128::from(total) * u128::from(time.timebase()) {
        return Err(err("INVALID_ARGUMENT"));
    }
    sample_time(sample, rate)?;
    Ok(sample)
}
impl AudioPreviewMachine {
    pub fn new(
        input: &AudioPreviewCreate,
        session_id: String,
        owner_id: String,
        lease_id: String,
        total_frames: u64,
        now: u64,
    ) -> Result<Self, EditorError> {
        validate_audio_preview_create(input)?;
        for id in [&session_id, &lease_id] {
            bounded_id(id)?;
        }
        bounded_principal(&owner_id)?;
        if total_frames == 0 {
            return Err(err("INVALID_ARGUMENT"));
        }
        sample_time(total_frames, input.sample_rate)?;
        let anchor = anchor(&input.start_time, input.sample_rate, total_frames)?;
        let mut machine = Self {
            view: AudioPreviewSession {
                session_id,
                identity: AudioPreviewIdentity {
                    project_id: input.project_id.clone(),
                    revision: input.revision.clone(),
                    sequence_id: input.sequence_id.clone(),
                    profile: AudioPreviewProfile::ClassicRealtimeV31,
                    sample_rate: input.sample_rate,
                    channels: 2,
                },
                generation: "0".into(),
                overlay_epoch: "0".into(),
                owner_id,
                lease_id,
                state: AudioPreviewState::Ready,
                actual_start_sample: anchor.to_string(),
                total_frames: total_frames.to_string(),
                submitted_time: sample_time(anchor, input.sample_rate)?,
                presentation_time: sample_time(anchor, input.sample_rate)?,
                output: None,
                consumed_frames: "0".into(),
                produced_through_sample: anchor.to_string(),
                buffered_frames: "0".into(),
                presentation_fresh: false,
                stalled: false,
                lease_remaining_ms: AUDIO_PREVIEW_OWNER_LEASE_MS.to_string(),
                expires_in_ms: AUDIO_PREVIEW_TTL_MS.to_string(),
                temporary_gain: None,
                failure_code: None,
                next_event_cursor: "0".into(),
            },
            generation: 0,
            overlay_epoch: 0,
            anchor,
            total: total_frames,
            consumed: 0,
            output_frames: 0,
            produced: anchor,
            pending_blocks: VecDeque::new(),
            lease_until: now
                .checked_add(AUDIO_PREVIEW_OWNER_LEASE_MS)
                .ok_or_else(|| err("PREVIEW_LIMIT"))?,
            expires: now
                .checked_add(AUDIO_PREVIEW_TTL_MS)
                .ok_or_else(|| err("PREVIEW_LIMIT"))?,
            presentation_at: None,
            report_sequence: None,
            cursor: 0,
            events: VecDeque::new(),
            receipts: VecDeque::new(),
        };
        machine.event("ready", now)?;
        Ok(machine)
    }
    pub fn snapshot(&self, now: u64) -> AudioPreviewSession {
        let mut view = self.view.clone();
        view.lease_remaining_ms = self.lease_until.saturating_sub(now).to_string();
        view.expires_in_ms = self.expires.saturating_sub(now).to_string();
        view.presentation_fresh = self
            .presentation_at
            .is_some_and(|at| now.saturating_sub(at) <= AUDIO_PREVIEW_AUTHORIZATION_HEARTBEAT_MS);
        view
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn overlay_epoch(&self) -> u64 {
        self.overlay_epoch
    }
    pub fn actual_start_sample(&self) -> u64 {
        self.anchor
    }
    pub fn produced_through_sample(&self) -> u64 {
        self.produced
    }
    pub fn consumed_scene_sample(&self) -> u64 {
        self.anchor + self.consumed
    }
    pub fn presentation_scene_sample(&self) -> u64 {
        self.anchor + self.output_frames
    }
    pub fn temporary_gain(&self) -> Option<PreviewGainOverride> {
        self.view
            .temporary_gain
            .as_ref()
            .map(|g| PreviewGainOverride {
                track_id: g.track_id.clone(),
                element_id: g.element_id.clone(),
                decibels_tenths: g.decibels_tenths,
            })
    }
    pub fn verify_stream(
        &self,
        input: &AudioPreviewStream,
        principal: &str,
        now: u64,
    ) -> Result<(), EditorError> {
        self.verify_visual_frame(input, principal, now)?;
        if self.view.state != AudioPreviewState::Playing {
            return Err(err("PREVIEW_NOT_PLAYING"));
        }
        Ok(())
    }
    pub fn verify_visual_frame(
        &self,
        input: &AudioPreviewStream,
        principal: &str,
        now: u64,
    ) -> Result<(), EditorError> {
        self.verify_owner(&input.session_id, &input.owner_id, &input.lease_id, now)?;
        if principal != self.view.owner_id {
            return Err(err("NOT_AUTHORIZED"));
        }
        if decimal(&input.generation)? != self.generation
            || decimal(&input.overlay_epoch)? != self.overlay_epoch
        {
            return Err(err("PREVIEW_GENERATION_CONFLICT"));
        }
        if matches!(
            self.view.state,
            AudioPreviewState::Stopped | AudioPreviewState::Failed
        ) {
            return Err(err("PREVIEW_STOPPED"));
        }
        Ok(())
    }
    fn verify_owner(
        &self,
        session: &str,
        owner: &str,
        lease: &str,
        now: u64,
    ) -> Result<(), EditorError> {
        if session != self.view.session_id {
            return Err(err("NOT_FOUND"));
        }
        if owner != self.view.owner_id || lease != self.view.lease_id {
            return Err(err("NOT_AUTHORIZED"));
        }
        if now >= self.lease_until || now >= self.expires {
            return Err(err("PREVIEW_EXPIRED"));
        }
        Ok(())
    }
    fn event(&mut self, kind: &str, now: u64) -> Result<(), EditorError> {
        self.cursor = self
            .cursor
            .checked_add(1)
            .ok_or_else(|| err("PREVIEW_LIMIT"))?;
        self.events.push_back(AudioPreviewEvent {
            cursor: self.cursor.to_string(),
            generation: self.generation.to_string(),
            overlay_epoch: self.overlay_epoch.to_string(),
            kind: kind.into(),
            server_monotonic_ms: now.to_string(),
        });
        if self.events.len() > HISTORY {
            self.events.pop_front();
        }
        self.view.next_event_cursor = self.cursor.to_string();
        Ok(())
    }
    pub fn events(
        &self,
        input: &AudioPreviewEventsInput,
    ) -> Result<AudioPreviewEvents, EditorError> {
        if input.session_id != self.view.session_id {
            return Err(err("NOT_FOUND"));
        }
        let cursor = input
            .cursor
            .as_deref()
            .map(decimal)
            .transpose()?
            .unwrap_or(0);
        if cursor > self.cursor {
            return Err(err("INVALID_CURSOR"));
        }
        if self
            .events
            .front()
            .is_some_and(|e| decimal(&e.cursor).is_ok_and(|n| n > cursor.saturating_add(1)))
        {
            return Err(err("CURSOR_EXPIRED"));
        }
        Ok(AudioPreviewEvents {
            events: self
                .events
                .iter()
                .filter(|e| decimal(&e.cursor).is_ok_and(|n| n > cursor))
                .cloned()
                .collect(),
            next_cursor: self.cursor.to_string(),
        })
    }
    fn replay(
        &self,
        key: &str,
        owner: &str,
        request: &str,
    ) -> Result<Option<AudioPreviewTransition>, EditorError> {
        if let Some(r) = self
            .receipts
            .iter()
            .find(|r| r.key == key && r.owner == owner)
        {
            if r.request != request {
                return Err(err("IDEMPOTENCY_KEY_REUSED"));
            }
            return Ok(Some(AudioPreviewTransition {
                session: r.view.clone(),
                effect: AudioPreviewEffect::None,
            }));
        }
        Ok(None)
    }
    /// Inspect an exact control's retained receipt without executing its command
    /// or refreshing a lease. Reload recovery must never replay Play/Seek merely
    /// to discover which generation should be retired.
    pub fn recover_control_receipt(
        &self,
        input: &AudioPreviewControl,
    ) -> Result<Option<AudioPreviewSession>, EditorError> {
        bounded_id(&input.control_id)?;
        let request = serde_json::to_string(input).map_err(|_| err("SERIALIZATION_ERROR"))?;
        Ok(self
            .replay(&input.control_id, &input.owner_id, &request)?
            .map(|r| r.session))
    }
    fn receipt(&mut self, key: String, owner: String, request: String, view: AudioPreviewSession) {
        self.receipts.push_back(Receipt {
            key,
            owner,
            request,
            view,
        });
        if self.receipts.len() > HISTORY {
            self.receipts.pop_front();
        }
    }
    pub fn control(
        &mut self,
        input: &AudioPreviewControl,
        now: u64,
    ) -> Result<AudioPreviewTransition, EditorError> {
        self.control_validated(input, None, now)
    }
    pub fn control_validated(
        &mut self,
        input: &AudioPreviewControl,
        gain: Option<&PreviewGainOverride>,
        now: u64,
    ) -> Result<AudioPreviewTransition, EditorError> {
        self.verify_owner(&input.session_id, &input.owner_id, &input.lease_id, now)?;
        bounded_id(&input.control_id)?;
        let request = serde_json::to_string(input).map_err(|_| err("SERIALIZATION_ERROR"))?;
        if let Some(replay) = self.replay(&input.control_id, &input.owner_id, &request)? {
            return Ok(replay);
        }
        if decimal(&input.generation)? != self.generation {
            return Err(err("PREVIEW_GENERATION_CONFLICT"));
        }
        let mut next = self.clone();
        let effect = next.apply(&input.command, gain, now)?;
        let view = next.snapshot(now);
        next.receipt(
            input.control_id.clone(),
            input.owner_id.clone(),
            request,
            view.clone(),
        );
        *self = next;
        Ok(AudioPreviewTransition {
            session: view,
            effect,
        })
    }
    fn reanchor(&mut self, sample: u64) -> Result<(), EditorError> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| err("PREVIEW_LIMIT"))?;
        self.anchor = sample;
        self.consumed = 0;
        self.output_frames = 0;
        self.view.output = None;
        self.produced = sample;
        self.pending_blocks.clear();
        self.presentation_at = None;
        self.report_sequence = None;
        self.view.generation = self.generation.to_string();
        self.view.actual_start_sample = sample.to_string();
        self.view.consumed_frames = "0".into();
        self.view.produced_through_sample = sample.to_string();
        self.view.buffered_frames = "0".into();
        self.view.stalled = false;
        self.view.submitted_time = sample_time(sample, self.view.identity.sample_rate)?;
        self.view.presentation_time = self.view.submitted_time.clone();
        Ok(())
    }
    fn apply(
        &mut self,
        command: &AudioPreviewCommand,
        validated: Option<&PreviewGainOverride>,
        now: u64,
    ) -> Result<AudioPreviewEffect, EditorError> {
        if matches!(
            self.view.state,
            AudioPreviewState::Stopped | AudioPreviewState::Failed
        ) && !matches!(command, AudioPreviewCommand::Stop)
        {
            return Err(err("PREVIEW_STOPPED"));
        }
        let mut effect = AudioPreviewEffect::None;
        match command {
            AudioPreviewCommand::Play => {
                if self.view.state != AudioPreviewState::Playing {
                    if self.presentation_scene_sample() >= self.total {
                        return Err(err("PREVIEW_ENDED"));
                    }
                    self.reanchor(self.presentation_scene_sample())?;
                    self.view.state = AudioPreviewState::Playing;
                    effect = AudioPreviewEffect::Start;
                }
            }
            AudioPreviewCommand::Pause => {
                if self.view.state != AudioPreviewState::Paused {
                    self.reanchor(self.presentation_scene_sample())?;
                    self.view.state = AudioPreviewState::Paused;
                    effect = AudioPreviewEffect::Cancel;
                }
            }
            AudioPreviewCommand::Seek { sequence_time } => {
                let start = anchor(sequence_time, self.view.identity.sample_rate, self.total)?;
                self.reanchor(start)?;
                effect = if self.view.state == AudioPreviewState::Playing {
                    AudioPreviewEffect::Replace
                } else {
                    AudioPreviewEffect::Cancel
                };
                if self.view.state == AudioPreviewState::Ended {
                    self.view.state = AudioPreviewState::Paused;
                }
            }
            AudioPreviewCommand::Stop => {
                if self.view.state != AudioPreviewState::Stopped {
                    self.reanchor(self.presentation_scene_sample())?;
                    self.view.state = AudioPreviewState::Stopped;
                    effect = AudioPreviewEffect::Cancel;
                }
            }
            AudioPreviewCommand::Heartbeat => {
                self.lease_until = now
                    .checked_add(AUDIO_PREVIEW_OWNER_LEASE_MS)
                    .ok_or_else(|| err("PREVIEW_LIMIT"))?;
            }
            AudioPreviewCommand::ReleaseOwner => {
                self.reanchor(self.presentation_scene_sample())?;
                self.view.state = AudioPreviewState::Paused;
                self.lease_until = now;
                effect = AudioPreviewEffect::Released;
            }
            AudioPreviewCommand::SetTemporaryGain { gain } => {
                bounded_id(&gain.gesture_id)?;
                let sequence = decimal(&gain.update_sequence)?;
                let v = validated.ok_or_else(|| err("INVALID_ARGUMENT"))?;
                if v.track_id != gain.track_id
                    || v.element_id != gain.element_id
                    || v.decibels_tenths != gain.decibels_tenths
                    || !(-600..=200).contains(&gain.decibels_tenths)
                {
                    return Err(err("INVALID_ARGUMENT"));
                }
                if let Some(old) = &self.view.temporary_gain {
                    if old.gesture_id == gain.gesture_id
                        && (old.track_id != gain.track_id
                            || old.element_id != gain.element_id
                            || sequence <= decimal(&old.update_sequence)?)
                    {
                        return Err(err("PREVIEW_TELEMETRY_ORDER"));
                    }
                    if old.gesture_id != gain.gesture_id {
                        return Err(err("PREVIEW_GESTURE_CONFLICT"));
                    }
                }
                self.view.temporary_gain = Some(gain.clone());
                self.change_overlay()?;
                effect = if self.view.state == AudioPreviewState::Playing {
                    AudioPreviewEffect::Replace
                } else {
                    AudioPreviewEffect::Cancel
                };
            }
            AudioPreviewCommand::ClearTemporaryGain { gesture_id } => {
                bounded_id(gesture_id)?;
                if self
                    .view
                    .temporary_gain
                    .as_ref()
                    .is_some_and(|g| g.gesture_id != *gesture_id)
                {
                    return Err(err("PREVIEW_GESTURE_CONFLICT"));
                }
                if self.view.temporary_gain.take().is_some() {
                    self.change_overlay()?;
                    effect = if self.view.state == AudioPreviewState::Playing {
                        AudioPreviewEffect::Replace
                    } else {
                        AudioPreviewEffect::Cancel
                    };
                }
            }
            AudioPreviewCommand::ReportConsumed {
                sequence,
                consumed_frames,
                playing: _,
                stalled,
                output,
            } => {
                if !matches!(
                    self.view.state,
                    AudioPreviewState::Playing
                        | AudioPreviewState::Paused
                        | AudioPreviewState::Ended
                ) {
                    return Err(err("PREVIEW_NOT_PLAYING"));
                }
                let sequence = decimal(sequence)?;
                let consumed = decimal(consumed_frames)?;
                if self.report_sequence.is_some_and(|old| sequence <= old)
                    || consumed < self.consumed
                {
                    return Err(err("PREVIEW_TELEMETRY_ORDER"));
                }
                if self
                    .anchor
                    .checked_add(consumed)
                    .is_none_or(|sample| sample > self.produced || sample > self.total)
                {
                    return Err(err("INVALID_ARGUMENT"));
                }
                let output_frames = output
                    .as_ref()
                    .map(|value| decimal(&value.frames))
                    .transpose()?;
                if output_frames
                    .is_some_and(|frames| frames < self.output_frames || frames > consumed)
                    || output.as_ref().is_some_and(|value| {
                        value
                            .uncertainty_frames
                            .is_some_and(|frames| frames > self.view.identity.sample_rate)
                    })
                {
                    return Err(err("INVALID_ARGUMENT"));
                }
                self.consumed = consumed;
                let confirmed = self.consumed_scene_sample();
                while self
                    .pending_blocks
                    .front()
                    .is_some_and(|end| *end <= confirmed)
                {
                    self.pending_blocks.pop_front();
                }
                self.report_sequence = Some(sequence);
                if let Some(frames) = output_frames {
                    self.output_frames = frames;
                    self.view.output = output.clone();
                    self.presentation_at = Some(now);
                    self.view.presentation_time = sample_time(
                        self.presentation_scene_sample(),
                        self.view.identity.sample_rate,
                    )?;
                }
                self.view.consumed_frames = consumed.to_string();
                self.view.submitted_time =
                    sample_time(self.consumed_scene_sample(), self.view.identity.sample_rate)?;
                self.view.buffered_frames =
                    (self.produced - self.consumed_scene_sample()).to_string();
                self.view.stalled = *stalled;
                if self.presentation_scene_sample() == self.total {
                    self.view.state = AudioPreviewState::Ended;
                }
            }
        }
        self.event(
            match command {
                AudioPreviewCommand::ReportConsumed { .. } => "presentation",
                AudioPreviewCommand::Heartbeat => "heartbeat",
                _ => "controlled",
            },
            now,
        )?;
        Ok(effect)
    }
    fn change_overlay(&mut self) -> Result<(), EditorError> {
        self.overlay_epoch = self
            .overlay_epoch
            .checked_add(1)
            .ok_or_else(|| err("PREVIEW_LIMIT"))?;
        self.view.overlay_epoch = self.overlay_epoch.to_string();
        self.reanchor(self.presentation_scene_sample())
    }
    /// Returns only credit inside the bounded PCM FIFO. This does not advance
    /// production; the host calls produced only after PCM and a fresh fence.
    pub fn next_block(&self, now: u64) -> Result<Option<(u64, u32)>, EditorError> {
        if self.view.state != AudioPreviewState::Playing {
            return Ok(None);
        }
        if now >= self.lease_until || now >= self.expires {
            return Err(err("PREVIEW_EXPIRED"));
        }
        if self.pending_blocks.len() >= AUDIO_PREVIEW_BUFFER_BLOCKS {
            return Ok(None);
        }
        let remaining = self.total - self.produced;
        let credit = AUDIO_PREVIEW_MAX_BUFFERED_FRAMES
            .saturating_sub(self.produced - self.consumed_scene_sample());
        let count = remaining.min(u64::from(AUDIO_PREVIEW_MAX_BLOCK_FRAMES));
        // A quantum-sized credit must not fragment the bounded transfer slots
        // into tiny PCM packets. Wait for one full block, except the final tail.
        Ok((count > 0 && credit >= count).then_some((self.produced, count as u32)))
    }
    pub fn produced(
        &mut self,
        generation: u64,
        overlay_epoch: u64,
        start: u64,
        count: u32,
        now: u64,
    ) -> Result<(), EditorError> {
        if generation != self.generation || overlay_epoch != self.overlay_epoch {
            return Err(err("PREVIEW_GENERATION_CONFLICT"));
        }
        let Some((expected, max_count)) = self.next_block(now)? else {
            return Err(err("PREVIEW_NOT_PLAYING"));
        };
        if start != expected || count == 0 || count > max_count {
            return Err(err("INVALID_ARGUMENT"));
        }
        self.produced += u64::from(count);
        self.pending_blocks.push_back(self.produced);
        self.view.produced_through_sample = self.produced.to_string();
        self.view.buffered_frames = (self.produced - self.consumed_scene_sample()).to_string();
        Ok(())
    }
    /// An unexpected disconnect pauses without changing generation, so the
    /// owner can still report its final old-stream consumption before Play.
    pub fn disconnect(&mut self, generation: u64, now: u64) -> Result<bool, EditorError> {
        if generation != self.generation
            || self.view.state != AudioPreviewState::Playing
            || self.produced == self.total
        {
            return Ok(false);
        }
        self.view.state = AudioPreviewState::Paused;
        self.event("stream_disconnected", now)?;
        Ok(true)
    }
    pub fn expire(&mut self, now: u64) -> Result<bool, EditorError> {
        if (now >= self.lease_until || now >= self.expires)
            && !matches!(
                self.view.state,
                AudioPreviewState::Stopped | AudioPreviewState::Failed
            )
        {
            self.reanchor(self.presentation_scene_sample())?;
            self.view.state = AudioPreviewState::Stopped;
            self.view.failure_code = Some("PREVIEW_EXPIRED".into());
            self.event("expired", now)?;
            return Ok(true);
        }
        Ok(false)
    }
    pub fn fail(&mut self, code: &str, now: u64) -> Result<(), EditorError> {
        self.reanchor(self.presentation_scene_sample())?;
        self.view.state = AudioPreviewState::Failed;
        self.view.failure_code = Some(code.into());
        self.event("failed", now)
    }
    pub fn acquire(
        &mut self,
        input: &AudioPreviewAcquire,
        owner: String,
        lease: String,
        now: u64,
    ) -> Result<AudioPreviewTransition, EditorError> {
        bounded_principal(&owner)?;
        bounded_id(&lease)?;
        bounded_id(&input.control_id)?;
        if input.session_id != self.view.session_id {
            return Err(err("NOT_FOUND"));
        }
        let request = serde_json::to_string(input).map_err(|_| err("SERIALIZATION_ERROR"))?;
        if let Some(r) = self.replay(&input.control_id, &owner, &request)? {
            return Ok(r);
        }
        if decimal(&input.expected_generation)? != self.generation {
            return Err(err("PREVIEW_GENERATION_CONFLICT"));
        }
        if now >= self.expires {
            return Err(err("PREVIEW_EXPIRED"));
        }
        if now < self.lease_until {
            return Err(err("PREVIEW_OWNER_ACTIVE"));
        }
        if self.view.state == AudioPreviewState::Failed
            || (self.view.state == AudioPreviewState::Stopped
                && self.view.failure_code.as_deref() != Some("PREVIEW_EXPIRED"))
        {
            return Err(err("PREVIEW_STOPPED"));
        }
        let mut next = self.clone();
        next.reanchor(next.presentation_scene_sample())?;
        next.view.owner_id = owner.clone();
        next.view.lease_id = lease;
        next.view.state = AudioPreviewState::Paused;
        next.view.failure_code = None;
        // A new owner never inherits somebody else's transient gesture.
        next.view.temporary_gain = None;
        next.overlay_epoch = next
            .overlay_epoch
            .checked_add(1)
            .ok_or_else(|| err("PREVIEW_LIMIT"))?;
        next.view.overlay_epoch = next.overlay_epoch.to_string();
        next.lease_until = now
            .checked_add(AUDIO_PREVIEW_OWNER_LEASE_MS)
            .ok_or_else(|| err("PREVIEW_LIMIT"))?;
        next.event("owner_acquired", now)?;
        let view = next.snapshot(now);
        next.receipt(input.control_id.clone(), owner, request, view.clone());
        *self = next;
        Ok(AudioPreviewTransition {
            session: view,
            effect: AudioPreviewEffect::Cancel,
        })
    }
}
