//! Bounded, session/stream-bound transport replay. Durable edits live in SQLite.
use rmcp::transport::streamable_http_server::session::{
    EventStore, EventStoreError, EventStream, ServerSseMessage,
};
use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{Duration, Instant},
};

const MAX_EVENTS: usize = 2048;
const MAX_STREAM_EVENTS: usize = 64;
const MAX_BYTES: usize = 32 * 1024 * 1024;
const TTL: Duration = Duration::from_secs(60);
struct Entry {
    id: String,
    stream: String,
    event: ServerSseMessage,
    bytes: usize,
    created: Instant,
}
#[derive(Default)]
struct Cache {
    entries: VecDeque<Entry>,
    bytes: usize,
}
impl Cache {
    fn prune(&mut self) {
        while self
            .entries
            .front()
            .is_some_and(|e| e.created.elapsed() >= TTL)
        {
            self.bytes -= self.entries.pop_front().expect("front exists").bytes;
        }
    }
}
#[derive(Default)]
pub struct ReplayStore(Mutex<Cache>);
#[derive(Debug)]
pub enum ReplayError {
    Expired,
    WrongSession,
}
impl ReplayStore {
    pub fn replay(&self, session: &str, id: &str) -> Result<Vec<ServerSseMessage>, ReplayError> {
        let mut cache = self.0.lock().unwrap_or_else(|e| e.into_inner());
        cache.prune();
        let index = cache
            .entries
            .iter()
            .position(|e| e.id == id)
            .ok_or(ReplayError::Expired)?;
        let stream = &cache.entries[index].stream;
        if !stream.starts_with(&format!("{session}:")) {
            return Err(ReplayError::WrongSession);
        }
        Ok(cache
            .entries
            .iter()
            .skip(index + 1)
            .filter(|e| &e.stream == stream)
            .map(|e| e.event.clone())
            .collect())
    }
    pub fn clear(&self) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Cache::default();
    }
}
#[async_trait::async_trait]
impl EventStore for ReplayStore {
    async fn store_event(
        &self,
        stream_id: &str,
        event: &ServerSseMessage,
    ) -> Result<String, EventStoreError> {
        let id = format!("mcp-event:{}", uuid::Uuid::new_v4().simple());
        let mut event = event.clone();
        event.event_id = Some(id.clone());
        let bytes = event
            .message
            .as_ref()
            .map(|v| serde_json::to_vec(v.as_ref()))
            .transpose()?
            .map_or(0, |v| v.len())
            + id.len()
            + stream_id.len()
            + 64;
        let mut cache = self.0.lock().unwrap_or_else(|e| e.into_inner());
        cache.prune();
        if cache
            .entries
            .iter()
            .filter(|e| e.stream == stream_id)
            .count()
            >= MAX_STREAM_EVENTS
        {
            let index = cache
                .entries
                .iter()
                .position(|e| e.stream == stream_id)
                .expect("stream exists");
            cache.bytes -= cache.entries.remove(index).expect("index exists").bytes;
        }
        cache.bytes += bytes;
        cache.entries.push_back(Entry {
            id: id.clone(),
            stream: stream_id.into(),
            event,
            bytes,
            created: Instant::now(),
        });
        while cache.entries.len() > MAX_EVENTS || cache.bytes > MAX_BYTES {
            cache.bytes -= cache.entries.pop_front().expect("over capacity").bytes;
        }
        Ok(id)
    }
    async fn replay_events_after(&self, _id: &str) -> Result<EventStream, EventStoreError> {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Replay requires an active session-bound adapter",
        )
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn replay_is_scoped_and_expiration_is_explicit() {
        let store = ReplayStore::default();
        let event = ServerSseMessage::retry(Duration::from_secs(1));
        let anchor = store.store_event("a:one", &event).await.unwrap();
        store.store_event("b:one", &event).await.unwrap();
        store.store_event("a:two", &event).await.unwrap();
        let next = store.store_event("a:one", &event).await.unwrap();
        let replay = store.replay("a", &anchor).unwrap();
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].event_id.as_deref(), Some(next.as_str()));
        assert!(matches!(
            store.replay("b", &anchor),
            Err(ReplayError::WrongSession)
        ));
        assert!(store.replay_events_after(&anchor).await.is_err());
        store.0.lock().unwrap().entries.front_mut().unwrap().created = Instant::now() - TTL;
        assert!(matches!(
            store.replay("a", &anchor),
            Err(ReplayError::Expired)
        ));
    }
    #[tokio::test]
    async fn byte_quota_expires_old_anchors_without_truncating_results() {
        let store = ReplayStore::default();
        let event = ServerSseMessage::retry(Duration::from_secs(1));
        let stream = format!("a:{}", "x".repeat(MAX_BYTES / 2));
        let old = store.store_event(&stream, &event).await.unwrap();
        let newest = store.store_event(&stream, &event).await.unwrap();
        assert!(matches!(store.replay("a", &old), Err(ReplayError::Expired)));
        assert!(store.replay("a", &newest).unwrap().is_empty());
        assert!(store.0.lock().unwrap().bytes <= MAX_BYTES);
    }
}
