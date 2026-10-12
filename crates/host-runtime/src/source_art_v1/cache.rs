use super::*;
use std::collections::HashMap;
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 256;
const MAX_FLIGHTS: usize = 2;
const TOMBSTONE_TTL: Duration = Duration::from_secs(60);

#[derive(Clone, Hash, PartialEq, Eq)]
pub(super) struct TicketKey {
    pub scope: String,
    pub revision: String,
    pub request_id: String,
}
impl TicketKey {
    pub fn new(prepared: &PreparedSourceArtV1) -> Self {
        Self {
            scope: prepared.scope().into(),
            revision: prepared.request().revision.clone(),
            request_id: prepared.request().request_id.clone(),
        }
    }
}
pub(super) struct Ticket {
    pub prepared: Arc<PreparedSourceArtV1>,
    pub started: Instant,
    failure: Mutex<Option<EditorError>>,
}
impl Ticket {
    pub fn new(prepared: Arc<PreparedSourceArtV1>) -> Self {
        Self {
            prepared,
            started: Instant::now(),
            failure: Mutex::default(),
        }
    }
    pub fn fail(&self, err: EditorError) -> Result<(), EditorError> {
        let mut failure = lock(&self.failure)?;
        if failure.is_none() {
            *failure = Some(err);
        }
        Ok(())
    }
    pub fn failure(&self) -> Result<Option<EditorError>, EditorError> {
        Ok(lock(&self.failure)?.clone())
    }
}
pub(super) struct Flight {
    pub prepared: Arc<PreparedSourceArtV1>,
    pub tickets: Mutex<HashMap<TicketKey, Arc<Ticket>>>,
    pub result: Mutex<Option<Result<Arc<DecodedSourceArtV1>, EditorError>>>,
    pub changed: Condvar,
}
impl Flight {
    pub fn has_other_active(&self, selected: &Ticket) -> Result<bool, EditorError> {
        for ticket in lock(&self.tickets)?.values() {
            if !std::ptr::eq(ticket.as_ref(), selected) && ticket.failure()?.is_none() {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
struct Cached {
    value: Arc<DecodedSourceArtV1>,
    bytes: usize,
    touch: u64,
}
#[derive(Default)]
struct State {
    cache: HashMap<String, Cached>,
    bytes: usize,
    touch: u64,
    flights: HashMap<String, Arc<Flight>>,
    tickets: HashMap<TicketKey, Arc<Ticket>>,
    cancelled: HashMap<TicketKey, Instant>,
    shutdown: bool,
    #[cfg(test)]
    completed: usize,
}
#[derive(Default)]
pub(super) struct Registry {
    state: Mutex<State>,
    changed: Condvar,
}
pub(super) enum Admission {
    Cached(Arc<DecodedSourceArtV1>),
    Flight { flight: Arc<Flight>, start: bool },
    Draining(Arc<Flight>),
}
fn cost(value: &DecodedSourceArtV1) -> usize {
    let identity = value.identity();
    let identity_bytes = identity.algorithm.capacity()
        + identity.browser_version.capacity()
        + identity.browser_sha256.capacity()
        + identity.node_sha256.capacity()
        + identity.module_sha256.capacity();
    let version = value.version();
    identity_bytes
        + version.id.capacity()
        + version.content_hash.capacity()
        + 1024
        + match value {
            DecodedSourceArtV1::Waveform { amplitudes, .. } => {
                amplitudes.capacity() * std::mem::size_of::<f32>()
            }
            DecodedSourceArtV1::ThumbnailReady { jpeg_bytes, .. } => jpeg_bytes.capacity(),
            DecodedSourceArtV1::ThumbnailNoFrame { .. } => 0,
        }
}
impl Registry {
    pub fn admit(
        &self,
        key: String,
        ticket_key: TicketKey,
        ticket: Arc<Ticket>,
    ) -> Result<Admission, EditorError> {
        let mut state = lock(&self.state)?;
        state
            .cancelled
            .retain(|_, time| time.elapsed() < TOMBSTONE_TTL);
        if state.shutdown {
            return Err(error("SOURCE_ART_STOPPED"));
        }
        if state.cancelled.contains_key(&ticket_key) {
            return Err(error("SOURCE_ART_CANCELLED"));
        }
        if state.tickets.contains_key(&ticket_key) {
            return Err(error("SOURCE_ART_REQUEST_CONFLICT"));
        }
        if state.tickets.len() >= MAX_ENTRIES {
            return Err(error("SOURCE_ART_BUDGET_EXCEEDED"));
        }
        state.touch = state.touch.wrapping_add(1);
        let touch = state.touch;
        if let Some(cached) = state.cache.get_mut(&key) {
            cached.touch = touch;
            let value = cached.value.clone();
            state.tickets.insert(ticket_key, ticket);
            return Ok(Admission::Cached(value));
        }
        if let Some(flight) = state.flights.get(&key).cloned() {
            // No participants join a terminating all-cancelled flight. Wait for
            // actual cleanup instead of launching a replacement over its gate.
            if !flight.has_other_active(&ticket)? {
                // This new request was never cancelled. Register its own
                // bounded, cancellable wait without reviving the old decoder.
                state.tickets.insert(ticket_key, ticket);
                return Ok(Admission::Draining(flight));
            }
            lock(&flight.tickets)?.insert(ticket_key.clone(), ticket.clone());
            state.tickets.insert(ticket_key, ticket);
            return Ok(Admission::Flight {
                flight,
                start: false,
            });
        }
        if state.flights.len() >= MAX_FLIGHTS {
            return Err(error("SOURCE_ART_BUDGET_EXCEEDED"));
        }
        let flight = Arc::new(Flight {
            prepared: ticket.prepared.clone(),
            tickets: Mutex::new(HashMap::from([(ticket_key.clone(), ticket.clone())])),
            result: Mutex::default(),
            changed: Condvar::new(),
        });
        state.tickets.insert(ticket_key, ticket);
        state.flights.insert(key, flight.clone());
        Ok(Admission::Flight {
            flight,
            start: true,
        })
    }
    pub fn complete(
        &self,
        key: &str,
        flight: &Arc<Flight>,
        mut result: Result<Arc<DecodedSourceArtV1>, EditorError>,
    ) -> Result<(), EditorError> {
        let mut state = lock(&self.state)?;
        #[cfg(test)]
        {
            state.completed += 1;
        }
        if let Ok(value) = &result {
            let bytes = cost(value);
            if state.shutdown {
                result = Err(error("SOURCE_ART_STOPPED"));
            } else if bytes > MAX_BYTES {
                result = Err(error("SOURCE_ART_BUDGET_EXCEEDED"));
            } else {
                // Published payloads remain charged while a completion receipt
                // or warm projection borrows them. Native process RSS/PCM is
                // separately bounded by the two decoder slots and policy.
                while state.cache.len() >= MAX_ENTRIES || state.bytes + bytes > MAX_BYTES {
                    let Some(oldest) = state
                        .cache
                        .iter()
                        .filter(|(_, cached)| Arc::strong_count(&cached.value) == 1)
                        .min_by_key(|(_, value)| value.touch)
                        .map(|(key, _)| key.clone())
                    else {
                        break;
                    };
                    if let Some(removed) = state.cache.remove(&oldest) {
                        state.bytes -= removed.bytes;
                    }
                }
                if state.cache.len() >= MAX_ENTRIES || state.bytes + bytes > MAX_BYTES {
                    result = Err(error("SOURCE_ART_BUDGET_EXCEEDED"));
                } else {
                    state.touch = state.touch.wrapping_add(1);
                    let touch = state.touch;
                    state.cache.insert(
                        key.into(),
                        Cached {
                            value: value.clone(),
                            bytes,
                            touch,
                        },
                    );
                    state.bytes += bytes;
                }
            }
        }
        if state
            .flights
            .get(key)
            .is_some_and(|current| Arc::ptr_eq(current, flight))
        {
            state.flights.remove(key);
        }
        *lock(&flight.result)? = Some(result);
        flight.changed.notify_all();
        self.changed.notify_all();
        Ok(())
    }
    pub fn release(
        &self,
        key: &TicketKey,
        ticket: &Arc<Ticket>,
        flight: &Flight,
    ) -> Result<(), EditorError> {
        let mut state = lock(&self.state)?;
        if state
            .tickets
            .get(key)
            .is_some_and(|current| Arc::ptr_eq(current, ticket))
        {
            state.tickets.remove(key);
        }
        lock(&flight.tickets)?.remove(key);
        self.changed.notify_all();
        if let Some(err) = ticket.failure()? {
            return Err(err);
        }
        if state.shutdown {
            return Err(error("SOURCE_ART_STOPPED"));
        }
        Ok(())
    }
    pub fn release_cached(&self, key: &TicketKey, ticket: &Arc<Ticket>) -> Result<(), EditorError> {
        let mut state = lock(&self.state)?;
        if state
            .tickets
            .get(key)
            .is_some_and(|current| Arc::ptr_eq(current, ticket))
        {
            state.tickets.remove(key);
        }
        self.changed.notify_all();
        if let Some(err) = ticket.failure()? {
            return Err(err);
        }
        if state.shutdown {
            return Err(error("SOURCE_ART_STOPPED"));
        }
        Ok(())
    }
    pub fn cancel(&self, key: TicketKey) -> Result<Option<Arc<Flight>>, EditorError> {
        let mut state = lock(&self.state)?;
        state
            .cancelled
            .retain(|_, time| time.elapsed() < TOMBSTONE_TTL);
        if let Some(ticket) = state.tickets.get(&key) {
            ticket.fail(error("SOURCE_ART_CANCELLED"))?;
        }
        let final_flight = state.flights.values().find_map(|flight| {
            let tickets = flight.tickets.lock().ok()?;
            if tickets.contains_key(&key)
                && tickets
                    .values()
                    .all(|ticket| ticket.failure().is_ok_and(|failure| failure.is_some()))
            {
                Some(flight.clone())
            } else {
                None
            }
        });
        if !state.cancelled.contains_key(&key) && state.cancelled.len() >= MAX_ENTRIES {
            return Err(error("SOURCE_ART_BUDGET_EXCEEDED"));
        }
        state.cancelled.insert(key, Instant::now());
        Ok(final_flight)
    }
    pub fn shutdown(&self) -> Result<Vec<Arc<Flight>>, EditorError> {
        let mut state = lock(&self.state)?;
        state.shutdown = true;
        for ticket in state.tickets.values() {
            ticket.fail(error("SOURCE_ART_STOPPED"))?;
        }
        Ok(state.flights.values().cloned().collect())
    }
    pub fn drain(&self) -> Result<(), EditorError> {
        let mut state = lock(&self.state)?;
        // Dropping a worker's final Arc is not a registry notification. A short
        // timed wait also observes that final release, without holding locks
        // needed by completion or warm/draining ticket cleanup.
        while !state.flights.is_empty()
            || !state.tickets.is_empty()
            || state
                .cache
                .values()
                .any(|cached| Arc::strong_count(&cached.value) != 1)
        {
            state = self
                .changed
                .wait_timeout(state, Duration::from_millis(50))
                .map_err(|_| error("INTERNAL_ERROR"))?
                .0;
        }
        state.cache.clear();
        state.bytes = 0;
        state.cancelled.clear();
        Ok(())
    }
    #[cfg(test)]
    pub fn stats(&self) -> (usize, usize, usize, usize) {
        let state = self.state.lock().unwrap();
        (
            state.completed,
            state.cache.len(),
            state.bytes,
            state.flights.len(),
        )
    }
}
