//! Pure session state machine; clocks, identities and stream ownership come from the host.
use opencut_api_contracts::preview::*;
use opencut_contracts::{EditorError, FrameRate, Time};
use opencut_media_contracts::{FrameAlignment, MediaTime};
use opencut_time::RationalTime;
use std::collections::VecDeque;

pub const LEASE_MS: u64 = 30_000;
pub const TTL_MS: u64 = 3_600_000;
pub const FRESH_MS: u64 = 3_000;
const EVENTS: usize = 256;
fn error(code: &str) -> EditorError {
    EditorError::new(
        code,
        "Preview request does not match the current session state",
    )
}
fn decimal(value: &str) -> Result<u64, EditorError> {
    value
        .parse::<u64>()
        .ok()
        .filter(|n| n.to_string() == value)
        .ok_or_else(|| error("INVALID_ARGUMENT"))
}

pub struct Machine {
    view: Session,
    fps: FrameRate,
    generation: u64,
    lease_until: u64,
    expires: u64,
    received_at: Option<u64>,
    telemetry_sequence: Option<u64>,
    cursor: u64,
    events: VecDeque<Event>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    Start,
    Cancel,
}
impl Machine {
    pub fn new(
        input: &Create,
        id: String,
        owner: String,
        lease: String,
        fps: FrameRate,
        duration: Time,
        now: u64,
    ) -> Result<Self, EditorError> {
        let start = align(&input.start_time, &fps, input.alignment, &duration)?;
        let mut machine = Self {
            view: Session {
                session_id: id,
                project_id: input.project_id.clone(),
                revision: input.revision.clone(),
                sequence_id: input.sequence_id.clone(),
                generation: "0".into(),
                state: State::Ready,
                actual_start: start.clone(),
                duration,
                owner_id: owner,
                lease_id: lease,
                lease_remaining_ms: LEASE_MS.to_string(),
                expires_in_ms: TTL_MS.to_string(),
                presentation: None,
                produced_through: None,
                buffered_bytes: "0".into(),
                failure_code: None,
                resume_position: start,
                resume_position_confirmed: false,
                next_event_cursor: "0".into(),
                restart_recoverable: false,
                asset_locking: "media_content_locked".into(),
            },
            fps,
            generation: 0,
            lease_until: now + LEASE_MS,
            expires: now + TTL_MS,
            received_at: None,
            telemetry_sequence: None,
            cursor: 0,
            events: VecDeque::new(),
        };
        machine.event("ready", now);
        Ok(machine)
    }
    fn event(&mut self, kind: &str, now: u64) {
        self.cursor += 1;
        self.events.push_back(Event {
            cursor: self.cursor.to_string(),
            generation: self.generation.to_string(),
            kind: kind.into(),
            server_monotonic_ms: now.to_string(),
        });
        if self.events.len() > EVENTS {
            self.events.pop_front();
        }
        self.view.next_event_cursor = self.cursor.to_string();
    }
    pub fn expired(&self, now: u64) -> bool {
        now >= self.lease_until || now >= self.expires
    }
    pub fn expire(&mut self, now: u64) -> bool {
        if self.expired(now) && self.view.state != State::Stopped {
            self.view.state = State::Stopped;
            self.view.failure_code = Some("PREVIEW_EXPIRED".into());
            self.event("stopped", now);
            true
        } else {
            false
        }
    }
    pub fn snapshot(&self, now: u64) -> Session {
        let mut view = self.view.clone();
        view.lease_remaining_ms = self.lease_until.saturating_sub(now).to_string();
        view.expires_in_ms = self.expires.saturating_sub(now).to_string();
        if let (Some(p), Some(at)) = (&mut view.presentation, self.received_at) {
            let age = now.saturating_sub(at);
            p.staleness_ms = age.to_string();
            p.fresh = age <= FRESH_MS;
        }
        view
    }
    pub fn events(&self, cursor: Option<&str>) -> Result<Events, EditorError> {
        let after = cursor.map(decimal).transpose()?.unwrap_or(0);
        if after > self.cursor {
            return Err(error("INVALID_CURSOR"));
        }
        if self
            .events
            .front()
            .is_some_and(|event| event.cursor.parse::<u64>().unwrap_or(0) > after.saturating_add(1))
        {
            return Err(error("CURSOR_EXPIRED"));
        }
        Ok(Events {
            events: self
                .events
                .iter()
                .filter(|e| e.cursor.parse::<u64>().unwrap_or(0) > after)
                .cloned()
                .collect(),
            next_cursor: self.cursor.to_string(),
        })
    }
    pub fn verify(
        &self,
        generation: &str,
        owner: &str,
        lease: &str,
        now: u64,
    ) -> Result<(), EditorError> {
        if owner != self.view.owner_id || lease != self.view.lease_id {
            return Err(error("NOT_AUTHORIZED"));
        }
        if decimal(generation)? != self.generation {
            return Err(error("PREVIEW_GENERATION_CONFLICT"));
        }
        if self.expired(now) {
            return Err(error("PREVIEW_EXPIRED"));
        }
        Ok(())
    }
    pub fn control(&mut self, input: &Control, now: u64) -> Result<Effect, EditorError> {
        self.verify(&input.generation, &input.owner_id, &input.lease_id, now)?;
        if input.session_id != self.view.session_id {
            return Err(error("NOT_FOUND"));
        }
        // Fully validate the next anchor before invalidating a live generation.
        let anchor = match &input.command {
            Command::Seek {
                sequence_time,
                alignment,
            } => Some(align(
                sequence_time,
                &self.fps,
                *alignment,
                &self.view.duration,
            )?),
            Command::Play => Some(align(
                &self.view.resume_position,
                &self.fps,
                FrameAlignment::Floor,
                &self.view.duration,
            )?),
            _ => None,
        };
        if self.view.state == State::Stopped && !matches!(input.command, Command::Stop) {
            return Err(error("PREVIEW_STOPPED"));
        }
        match &input.command {
            Command::Play | Command::Seek { .. } => {
                if matches!(input.command, Command::Play) && self.view.state == State::Producing {
                    return Ok(Effect::None);
                }
                self.generation = self
                    .generation
                    .checked_add(1)
                    .ok_or_else(|| error("PREVIEW_LIMIT"))?;
                self.view.generation = self.generation.to_string();
                self.view.actual_start = anchor.unwrap();
                self.view.resume_position = self.view.actual_start.clone();
                self.view.resume_position_confirmed = false;
                self.view.state = State::Producing;
                self.view.presentation = None;
                self.received_at = None;
                self.telemetry_sequence = None;
                self.view.produced_through = None;
                self.view.buffered_bytes = "0".into();
                self.view.failure_code = None;
                self.lease_until = now + LEASE_MS;
                self.event("producer_started", now);
                Ok(Effect::Start)
            }
            Command::Pause => {
                if self.view.state == State::Paused {
                    return Ok(Effect::None);
                }
                self.view.resume_position_confirmed = self
                    .received_at
                    .is_some_and(|at| now.saturating_sub(at) <= FRESH_MS);
                if self.view.resume_position_confirmed {
                    self.view.resume_position = self
                        .view
                        .presentation
                        .as_ref()
                        .unwrap()
                        .telemetry
                        .presented_time
                        .clone();
                }
                self.view.state = State::Paused;
                self.event("paused", now);
                Ok(Effect::Cancel)
            }
            Command::Stop => {
                if self.view.state != State::Stopped {
                    self.view.state = State::Stopped;
                    self.event("stopped", now);
                }
                Ok(Effect::Cancel)
            }
            Command::Heartbeat => {
                self.lease_until = now + LEASE_MS;
                Ok(Effect::None)
            }
            Command::ReleaseOwner => {
                if self
                    .received_at
                    .is_some_and(|at| now.saturating_sub(at) <= FRESH_MS)
                {
                    self.view.resume_position = self
                        .view
                        .presentation
                        .as_ref()
                        .unwrap()
                        .telemetry
                        .presented_time
                        .clone();
                    self.view.resume_position_confirmed = true;
                }
                self.lease_until = now;
                self.view.state = State::Paused;
                self.event("owner_released", now);
                Ok(Effect::Cancel)
            }
            Command::ReportPresentation { telemetry } => {
                if self.view.state != State::Producing && self.view.state != State::Ended {
                    return Err(error("PREVIEW_NOT_PLAYING"));
                }
                let sequence = decimal(&telemetry.sequence)?;
                if self.telemetry_sequence.is_some_and(|old| sequence <= old) {
                    return Err(error("PREVIEW_TELEMETRY_ORDER"));
                }
                let client_time = telemetry
                    .client_observed_at_ms
                    .parse::<i64>()
                    .map_err(|_| error("INVALID_ARGUMENT"))?;
                if client_time.to_string() != telemetry.client_observed_at_ms {
                    return Err(error("INVALID_ARGUMENT"));
                }
                let actual = RationalTime::try_from(&telemetry.presented_time)?;
                let clock = RationalTime::try_from(&telemetry.media_clock_time)?;
                let mapped = RationalTime::try_from(&self.view.actual_start)?.checked_add(clock)?;
                if clock.ticks() < 0
                    || actual.cmp_exact(mapped).is_ne()
                    || actual
                        .cmp_exact(RationalTime::try_from(&self.view.duration)?)
                        .is_ge()
                {
                    return Err(error("INVALID_ARGUMENT"));
                }
                self.telemetry_sequence = Some(sequence);
                self.received_at = Some(now);
                self.view.resume_position = telemetry.presented_time.clone();
                self.view.resume_position_confirmed = true;
                self.view.presentation = Some(Presentation {
                    telemetry: telemetry.clone(),
                    server_received_monotonic_ms: now.to_string(),
                    staleness_ms: "0".into(),
                    fresh: true,
                    client_timestamp_status: "unsynchronized".into(),
                });
                self.event("presentation", now);
                Ok(Effect::None)
            }
        }
    }
    pub fn acquire(
        &mut self,
        expected: &str,
        owner: String,
        lease: String,
        now: u64,
    ) -> Result<(), EditorError> {
        if decimal(expected)? != self.generation {
            return Err(error("PREVIEW_GENERATION_CONFLICT"));
        }
        if now >= self.expires {
            return Err(error("PREVIEW_EXPIRED"));
        }
        if now < self.lease_until {
            return Err(error("PREVIEW_OWNER_ACTIVE"));
        }
        // Explicit stop is terminal; expiry/release can transfer control within the session TTL.
        if self.view.state == State::Stopped
            && self.view.failure_code.as_deref() != Some("PREVIEW_EXPIRED")
        {
            return Err(error("PREVIEW_STOPPED"));
        }
        self.view.resume_position_confirmed = self
            .received_at
            .is_some_and(|at| now.saturating_sub(at) <= FRESH_MS);
        if self.view.resume_position_confirmed {
            self.view.resume_position = self
                .view
                .presentation
                .as_ref()
                .unwrap()
                .telemetry
                .presented_time
                .clone();
            self.view.resume_position_confirmed = true;
        }
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| error("PREVIEW_LIMIT"))?;
        self.view.generation = self.generation.to_string();
        self.view.owner_id = owner;
        self.view.lease_id = lease;
        self.view.state = State::Paused;
        self.view.failure_code = None;
        self.lease_until = now + LEASE_MS;
        self.view.presentation = None;
        self.received_at = None;
        self.telemetry_sequence = None;
        self.event("owner_changed", now);
        Ok(())
    }
    pub fn producer(
        &mut self,
        generation: &str,
        through: Option<Time>,
        buffered: u64,
        done: bool,
        failure: Option<&str>,
        now: u64,
    ) {
        if generation != self.view.generation || self.view.state != State::Producing {
            return;
        }
        self.view.produced_through = through;
        self.view.buffered_bytes = buffered.to_string();
        if let Some(code) = failure {
            self.view.state = State::Failed;
            self.view.failure_code = Some(code.into());
            self.event("failed", now);
        } else if done {
            self.view.state = State::Ended;
            self.event("ended", now);
        }
    }
}
pub fn align(
    time: &Time,
    fps: &FrameRate,
    alignment: FrameAlignment,
    duration: &Time,
) -> Result<Time, EditorError> {
    let end = RationalTime::try_from(duration)?;
    opencut_composition::select_project_frame_time(
        time,
        fps,
        alignment,
        MediaTime {
            ticks: end.ticks(),
            timebase: end.timebase(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn time(ticks: i64) -> Time {
        RationalTime::new(ticks, 25).unwrap().into()
    }
    fn machine() -> Machine {
        Machine::new(
            &Create {
                project_id: "p".into(),
                revision: "r".into(),
                sequence_id: "main".into(),
                start_time: time(0),
                alignment: FrameAlignment::Exact,
                audio_enabled: true,
                idempotency_key: "key".into(),
            },
            "session".into(),
            "owner".into(),
            "lease".into(),
            FrameRate {
                numerator: 25,
                denominator: 1,
            },
            time(250),
            0,
        )
        .unwrap()
    }
    fn command(machine: &Machine, command: Command) -> Control {
        Control {
            session_id: "session".into(),
            generation: machine.generation.to_string(),
            owner_id: "owner".into(),
            lease_id: "lease".into(),
            command,
        }
    }
    #[test]
    fn invalid_seek_preserves_generation_and_valid_seek_rejects_late_reports() {
        let mut m = machine();
        let play = command(&m, Command::Play);
        m.control(&play, 1).unwrap();
        let bad = command(
            &m,
            Command::Seek {
                sequence_time: time(250),
                alignment: FrameAlignment::Exact,
            },
        );
        assert!(m.control(&bad, 2).is_err());
        assert_eq!(m.generation, 1);
        assert_eq!(m.view.state, State::Producing);
        let old = command(&m, Command::Heartbeat);
        let seek = command(
            &m,
            Command::Seek {
                sequence_time: time(50),
                alignment: FrameAlignment::Exact,
            },
        );
        assert_eq!(m.control(&seek, 3).unwrap(), Effect::Start);
        assert_eq!(
            m.control(&old, 4).unwrap_err().code,
            "PREVIEW_GENERATION_CONFLICT"
        );
        assert!(
            RationalTime::try_from(&m.view.actual_start)
                .unwrap()
                .cmp_exact(RationalTime::try_from(&time(50)).unwrap())
                .is_eq()
        );
    }
    #[test]
    fn presentation_is_exact_ordered_and_never_inferred_from_production() {
        let mut m = machine();
        let play = command(&m, Command::Play);
        m.control(&play, 1).unwrap();
        m.producer("1", Some(time(150)), 99, false, None, 2);
        assert!(m.snapshot(2).presentation.is_none());
        let report = command(
            &m,
            Command::ReportPresentation {
                telemetry: Telemetry {
                    sequence: "1".into(),
                    presented_time: time(25),
                    media_clock_time: time(25),
                    playing: true,
                    stalled: false,
                    client_observed_at_ms: "-123".into(),
                },
            },
        );
        m.control(&report, 3).unwrap();
        assert_eq!(
            m.control(&report, 4).unwrap_err().code,
            "PREVIEW_TELEMETRY_ORDER"
        );
        assert!(!m.snapshot(4_000).presentation.unwrap().fresh);
        let pause = command(&m, Command::Pause);
        m.control(&pause, 5).unwrap();
        assert!(
            RationalTime::try_from(&m.view.resume_position)
                .unwrap()
                .cmp_exact(RationalTime::try_from(&time(25)).unwrap())
                .is_eq()
        );
        assert!(m.view.resume_position_confirmed);
        let play = command(&m, Command::Play);
        m.control(&play, 6).unwrap();
        m.producer("2", Some(time(200)), 0, false, None, 7);
        let pause = command(&m, Command::Pause);
        m.control(&pause, 8).unwrap();
        assert!(
            RationalTime::try_from(&m.view.resume_position)
                .unwrap()
                .cmp_exact(RationalTime::try_from(&time(25)).unwrap())
                .is_eq()
        );
        assert!(!m.view.resume_position_confirmed);
    }
    #[test]
    fn lease_expiry_and_bounded_event_cursor_are_explicit() {
        let mut m = machine();
        for i in 0..300 {
            m.event("test", i);
        }
        assert_eq!(m.events(Some("0")).unwrap_err().code, "CURSOR_EXPIRED");
        assert!(m.expire(LEASE_MS));
        assert_eq!(m.view.state, State::Stopped);
        let play = command(&m, Command::Play);
        assert_eq!(
            m.control(&play, LEASE_MS).unwrap_err().code,
            "PREVIEW_EXPIRED"
        );
    }
}
