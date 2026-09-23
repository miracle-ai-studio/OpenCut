//! Bind SDK-generated stream IDs to the owning HTTP session before caching.
use crate::mcp_replay::ReplayStore;
use rmcp::{
    model::{ClientJsonRpcMessage, GetExtensions, ServerJsonRpcMessage},
    transport::{
        WorkerTransport,
        streamable_http_server::session::{
            EventStore, EventStoreError, EventStream, ServerSseMessage, SessionId, SessionManager,
            local::{
                LocalSessionManager, LocalSessionManagerError, LocalSessionWorker, SessionConfig,
            },
        },
    },
};
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    time::Duration,
};

struct SessionEvents {
    owner: Arc<OnceLock<SessionId>>,
    replay: Arc<ReplayStore>,
}
#[async_trait::async_trait]
impl EventStore for SessionEvents {
    async fn store_event(
        &self,
        stream: &str,
        event: &ServerSseMessage,
    ) -> Result<String, EventStoreError> {
        let owner = self
            .owner
            .get()
            .ok_or_else(|| std::io::Error::other("Session identity is not bound"))?;
        self.replay
            .store_event(&format!("{owner}:{stream}"), event)
            .await
    }
    async fn replay_events_after(&self, id: &str) -> Result<EventStream, EventStoreError> {
        let owner = self
            .owner
            .get()
            .ok_or_else(|| std::io::Error::other("Session identity is not bound"))?;
        let events = self
            .replay
            .replay(owner, id)
            .map_err(|_| std::io::Error::other("Invalid or expired MCP event cursor"))?;
        Ok(Box::pin(futures_util::stream::iter(events)))
    }
}
pub struct BoundSession {
    manager: Arc<LocalSessionManager>,
    identity: OnceLock<crate::http_identity::Identity>,
}
pub struct Sessions {
    pub sessions: tokio::sync::RwLock<HashMap<SessionId, Arc<BoundSession>>>,
    replay: Arc<ReplayStore>,
    config: SessionConfig,
}
impl Sessions {
    pub async fn identity(&self, id: &SessionId) -> Option<crate::http_identity::Identity> {
        self.sessions
            .read()
            .await
            .get(id)
            .and_then(|entry| entry.identity.get().cloned())
    }
    pub async fn scoped_bindings(&self) -> Vec<(SessionId, crate::http_identity::Identity)> {
        self.sessions
            .read()
            .await
            .iter()
            .filter_map(|(id, entry)| {
                entry
                    .identity
                    .get()
                    .filter(|identity| !identity.caller.is_owner())
                    .map(|identity| (id.clone(), identity.clone()))
            })
            .collect()
    }

    pub fn new(replay: Arc<ReplayStore>, timeout: u64) -> Self {
        let mut config = SessionConfig::default();
        config.keep_alive = Some(Duration::from_secs(timeout));
        config.init_timeout = Some(Duration::from_secs(10));
        config.sse_retry = Some(Duration::from_secs(1));
        config.completed_cache_ttl = Duration::from_secs(1);
        Self {
            sessions: Default::default(),
            replay,
            config,
        }
    }
    async fn get(
        &self,
        id: &SessionId,
    ) -> Result<Arc<LocalSessionManager>, LocalSessionManagerError> {
        self.sessions
            .read()
            .await
            .get(id)
            .map(|entry| entry.manager.clone())
            .ok_or_else(|| LocalSessionManagerError::SessionNotFound(id.clone()))
    }
}
impl SessionManager for Sessions {
    type Error = LocalSessionManagerError;
    type Transport = WorkerTransport<LocalSessionWorker>;
    async fn create_session(&self) -> Result<(SessionId, Self::Transport), Self::Error> {
        let owner = Arc::new(OnceLock::new());
        let mut manager =
            LocalSessionManager::default().with_event_store(Arc::new(SessionEvents {
                owner: owner.clone(),
                replay: self.replay.clone(),
            }));
        manager.session_config = self.config.clone();
        let manager = Arc::new(manager);
        let (id, transport) = manager.create_session().await?;
        owner
            .set(id.clone())
            .expect("new session identity bound once");
        self.sessions.write().await.insert(
            id.clone(),
            Arc::new(BoundSession {
                manager,
                identity: OnceLock::new(),
            }),
        );
        Ok((id, transport))
    }
    async fn initialize_session(
        &self,
        id: &SessionId,
        message: ClientJsonRpcMessage,
    ) -> Result<ServerJsonRpcMessage, Self::Error> {
        let identity = match &message {
            ClientJsonRpcMessage::Request(request) => request
                .request
                .extensions()
                .get::<axum::http::request::Parts>()
                .and_then(|parts| parts.extensions.get::<crate::http_identity::Identity>())
                .cloned(),
            _ => None,
        }
        .ok_or_else(|| LocalSessionManagerError::SessionNotFound(id.clone()))?;
        let entry = self
            .sessions
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| LocalSessionManagerError::SessionNotFound(id.clone()))?;
        entry
            .identity
            .set(identity)
            .map_err(|_| LocalSessionManagerError::SessionNotFound(id.clone()))?;
        entry.manager.initialize_session(id, message).await
    }
    async fn has_session(&self, id: &SessionId) -> Result<bool, Self::Error> {
        Ok(self.sessions.read().await.contains_key(id))
    }
    async fn close_session(&self, id: &SessionId) -> Result<(), Self::Error> {
        let manager = self.sessions.write().await.remove(id);
        if let Some(manager) = manager {
            manager.manager.close_session(id).await?;
        }
        Ok(())
    }
    async fn create_stream(
        &self,
        id: &SessionId,
        message: ClientJsonRpcMessage,
    ) -> Result<
        impl futures_util::Stream<Item = ServerSseMessage> + Send + Sync + 'static,
        Self::Error,
    > {
        let manager = self.get(id).await?;
        let stream = manager.create_stream(id, message).await?;
        Ok(Box::pin(stream) as EventStream)
    }
    async fn create_standalone_stream(
        &self,
        id: &SessionId,
    ) -> Result<
        impl futures_util::Stream<Item = ServerSseMessage> + Send + Sync + 'static,
        Self::Error,
    > {
        let manager = self.get(id).await?;
        let stream = manager.create_standalone_stream(id).await?;
        Ok(Box::pin(stream) as EventStream)
    }
    async fn accept_message(
        &self,
        id: &SessionId,
        message: ClientJsonRpcMessage,
    ) -> Result<(), Self::Error> {
        self.get(id).await?.accept_message(id, message).await
    }
    async fn resume(
        &self,
        id: &SessionId,
        last: String,
    ) -> Result<
        impl futures_util::Stream<Item = ServerSseMessage> + Send + Sync + 'static,
        Self::Error,
    > {
        // Production HTTP resumes through the gate for explicit HTTP errors.
        // This adapter still enforces ownership if invoked by the SDK directly.
        let _ = self.get(id).await?;
        let events=self.replay.replay(id,&last).map_err(|_|LocalSessionManagerError::SessionError(rmcp::transport::streamable_http_server::session::local::SessionError::InvalidEventId))?;
        Ok(futures_util::stream::iter(events))
    }
    fn event_store(&self) -> Option<Arc<dyn EventStore>> {
        Some(self.replay.clone())
    }
}
