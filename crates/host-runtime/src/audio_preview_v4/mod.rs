//! Process-local host assembly for fixed-revision audio. The application owns
//! clocks and control semantics; this host owns authorization, CAS and lifetime.
mod producer;
mod stream;
mod text;
use crate::{Automation, Caller, ExecutionContext};
use opencut_composition::project_v4::{PcmFormatV4, PreparedPreviewAudioV4};
use opencut_contracts::EditorError;
use opencut_editor_application::audio_preview_v4::*;
use opencut_editor_application::project_v4_frame_jobs::V4FrameJobSubmissionStore;
use opencut_job_execution::project_v4_audio::TrustedJobAuthorityV1;
use opencut_native_services::classic_audio_preview::new_identity;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, MutexGuard, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::Instant,
};
pub use stream::AudioPreviewStream;
pub(crate) use text::{authorize_font, open_font, text_picture};

pub(super) fn error(code: &str) -> EditorError {
    EditorError::new(
        code,
        "Fixed audio-preview session is unavailable or no longer current",
    )
}
pub(super) fn lock<T>(value: &Mutex<T>) -> Result<MutexGuard<'_, T>, EditorError> {
    value.lock().map_err(|_| error("INTERNAL_ERROR"))
}
pub(super) fn now() -> u64 {
    static CLOCK: OnceLock<Instant> = OnceLock::new();
    CLOCK
        .get_or_init(Instant::now)
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
fn database_key(path: &Path) -> Result<PathBuf, EditorError> {
    std::fs::canonicalize(path).map_err(|_| error("STORAGE_ERROR"))
}
struct SessionState {
    machine: AudioPreviewMachineV4,
    authority: TrustedJobAuthorityV1,
    picture: Option<CachedPicture>,
    text_picture: Option<text::CachedTextPicture>,
    text_coverage: Option<text::CachedTextCoverage>,
}
struct CachedPicture {
    session: AudioPreviewStreamV4,
    prepared: Arc<opencut_editor_application::visual_preview_v4::PreparedPictureV4>,
}
impl SessionState {
    fn expire(&mut self, at: u64) -> Result<bool, EditorError> {
        let changed = self.machine.expire(at)?;
        // Another endpoint may already have made this session terminal. Cache
        // disposal must not depend on being the first expiry observer.
        if matches!(
            self.machine.snapshot(at).state,
            AudioPreviewStateV4::Stopped | AudioPreviewStateV4::Failed
        ) {
            self.picture = None;
            self.text_picture = None;
            self.text_coverage = None;
        }
        Ok(changed)
    }
}
struct Actor {
    sender: mpsc::SyncSender<producer::BlockRequest>,
    thread: JoinHandle<()>,
}
struct Entry {
    database: PathBuf,
    request: AudioPreviewCreateV4,
    creator: String,
    prepared: Arc<PreparedPreviewAudioV4>,
    policy: AudioPreviewPolicyV4,
    state: Mutex<SessionState>,
    credit: Condvar,
    actor: Mutex<Option<Actor>>,
    stream_claim: Mutex<Option<(u64, String)>>,
    shutdown: AtomicBool,
}
impl Entry {
    fn view(&self) -> Result<AudioPreviewSessionV4, EditorError> {
        let mut state = lock(&self.state)?;
        if state.expire(now())? {
            state.picture = None;
            state.text_picture = None;
            state.text_coverage = None;
            self.credit.notify_all();
        }
        Ok(state.machine.snapshot(now()))
    }
    fn occupies_slot(&self) -> bool {
        let active = self.view().is_ok_and(|v| {
            !matches!(
                v.state,
                AudioPreviewStateV4::Stopped | AudioPreviewStateV4::Failed
            )
        });
        active
            || lock(&self.actor).is_ok_and(|a| a.as_ref().is_some_and(|a| !a.thread.is_finished()))
    }
    fn checkpoint(&self, app: &opencut_sqlite_store::EditorApplication) -> Result<(), EditorError> {
        let authority = {
            let mut state = lock(&self.state)?;
            if state.expire(now())? {
                state.picture = None;
                state.text_picture = None;
                state.text_coverage = None;
                self.credit.notify_all();
            }
            if self.shutdown.load(Ordering::Acquire)
                || matches!(
                    state.machine.snapshot(now()).state,
                    AudioPreviewStateV4::Stopped | AudioPreviewStateV4::Failed
                )
            {
                return Err(error("PREVIEW_STOPPED"));
            }
            state.authority.clone()
        };
        app.revalidate_audio_preview(&self.request.project_id, &self.request.revision, &authority)
    }
    fn ensure_actor(self: &Arc<Self>) -> Result<(), EditorError> {
        let mut actor = lock(&self.actor)?;
        if actor.as_ref().is_some_and(|a| !a.thread.is_finished()) {
            return Ok(());
        }
        if let Some(old) = actor.take() {
            old.thread.join().map_err(|_| error("INTERNAL_ERROR"))?;
        }
        if self.shutdown.load(Ordering::Acquire) {
            return Err(error("PREVIEW_STOPPED"));
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let entry = self.clone();
        let thread = std::thread::Builder::new()
            .name("opencut-audio-preview".into())
            .spawn(move || producer::run(entry, receiver))
            .map_err(|_| error("IO_ERROR"))?;
        *actor = Some(Actor { sender, thread });
        Ok(())
    }
    fn signal_shutdown(&self) -> Result<(), EditorError> {
        let mut state = lock(&self.state)?;
        state.picture = None;
        state.text_picture = None;
        state.text_coverage = None;
        self.shutdown.store(true, Ordering::Release);
        self.credit.notify_all();
        Ok(())
    }
    fn stop_actor(&self) -> Result<(), EditorError> {
        self.signal_shutdown()?;
        let actor = lock(&self.actor)?.take();
        if let Some(actor) = actor {
            actor.thread.join().map_err(|_| error("INTERNAL_ERROR"))?;
        }
        Ok(())
    }
}
#[derive(Default)]
struct Registry {
    entries: HashMap<String, Arc<Entry>>,
}
fn registries() -> &'static Mutex<HashMap<PathBuf, Registry>> {
    static REGISTRIES: OnceLock<Mutex<HashMap<PathBuf, Registry>>> = OnceLock::new();
    REGISTRIES.get_or_init(Mutex::default)
}
fn entry(database: &Path, id: &str) -> Result<Arc<Entry>, EditorError> {
    if id.len() > 1024 {
        return Err(error("INVALID_ARGUMENT"));
    }
    lock(registries())?
        .get(&database_key(database)?)
        .and_then(|r| r.entries.get(id))
        .cloned()
        .ok_or_else(|| error("NOT_FOUND"))
}
fn trusted(caller: &Caller) -> TrustedJobAuthorityV1 {
    match caller {
        Caller::LocalOwner => TrustedJobAuthorityV1::LocalOwner,
        Caller::Credential(c) => TrustedJobAuthorityV1::Credential {
            principal_id: c.principal_id().into(),
            credential_id: c.credential_id().into(),
        },
    }
}
fn authorize(host: &mut Automation, entry: &Entry, original: bool) -> Result<(), EditorError> {
    host.app.set_execution_context(ExecutionContext::for_caller(
        host.source,
        host.caller.clone(),
    ));
    host.app
        .revalidate_v4_audio_preview(&entry.request.project_id, &entry.request.revision)?;
    if original {
        entry.checkpoint(&host.app)?;
    }
    Ok(())
}
pub(crate) fn create(
    host: &mut Automation,
    input: AudioPreviewCreateV4,
) -> Result<AudioPreviewSessionV4, EditorError> {
    validate_audio_preview_create(&input)?;
    crate::classic_audio_runtime::preview_config()?;
    host.app.set_execution_context(ExecutionContext::for_caller(
        host.source,
        host.caller.clone(),
    ));
    let selected = host
        .app
        .fixed_audio_preview_snapshot(&input.project_id, &input.revision)?;
    validate_audio_preview_policy(&selected.policy)?;
    let database = database_key(&host.database)?;
    let prepared = Arc::new(
        PreparedPreviewAudioV4::new(
            &selected.snapshot,
            &input.sequence_id,
            PcmFormatV4 {
                sample_rate: input.sample_rate,
                channels: 2,
            },
        )
        .map_err(|e| EditorError::new(e.code, e.message))?,
    );
    let mut all = lock(registries())?;
    let registry = all.entry(database.clone()).or_default();
    if let Some(existing) = registry.entries.values().find(|e| {
        e.creator == host.caller.principal_id()
            && e.request.idempotency_key == input.idempotency_key
    }) {
        if existing.request != input {
            return Err(error("IDEMPOTENCY_CONFLICT"));
        }
        let original = lock(&existing.state)?.authority.clone();
        host.app
            .revalidate_audio_preview(&input.project_id, &input.revision, &original)?;
        return existing.view();
    }
    // Retain a bounded terminal receipt set; active sessions and native actors
    // must never be evicted to make room for a new create.
    if registry.entries.len() >= 32 {
        let victim = registry.entries.iter().find_map(|(id, e)| {
            let finished = lock(&e.actor)
                .ok()?
                .as_ref()
                .is_none_or(|a| a.thread.is_finished());
            let terminal = matches!(
                e.view().ok()?.state,
                AudioPreviewStateV4::Stopped | AudioPreviewStateV4::Failed
            );
            (finished && terminal).then(|| id.clone())
        });
        if let Some(id) = victim {
            registry.entries.remove(&id);
        } else {
            return Err(error("PREVIEW_LIMIT"));
        }
    }
    let active = registry
        .entries
        .values()
        .filter(|e| e.occupies_slot())
        .count();
    if active >= AUDIO_PREVIEW_MAX_SESSIONS {
        return Err(error("PREVIEW_LIMIT"));
    }
    let id = new_identity();
    let machine = AudioPreviewMachineV4::new(
        &input,
        id.clone(),
        host.caller.principal_id().into(),
        new_identity(),
        prepared.extent(),
        now(),
    )?;
    let entry = Arc::new(Entry {
        database,
        request: input,
        creator: host.caller.principal_id().into(),
        prepared,
        policy: selected.policy,
        state: Mutex::new(SessionState {
            machine,
            authority: selected.authority,
            picture: None,
            text_picture: None,
            text_coverage: None,
        }),
        credit: Condvar::new(),
        actor: Mutex::new(None),
        stream_claim: Mutex::new(None),
        shutdown: AtomicBool::new(false),
    });
    entry.ensure_actor()?;
    let view = entry.view()?;
    registry.entries.insert(id, entry);
    Ok(view)
}
pub(crate) fn inspect(
    host: &mut Automation,
    input: AudioPreviewInspectV4,
) -> Result<AudioPreviewSessionV4, EditorError> {
    let e = entry(&host.database, &input.session_id)?;
    authorize(host, &e, false)?;
    e.view()
}
pub(crate) fn events(
    host: &mut Automation,
    input: AudioPreviewEventsInputV4,
) -> Result<AudioPreviewEventsV4, EditorError> {
    let e = entry(&host.database, &input.session_id)?;
    authorize(host, &e, false)?;
    let mut state = lock(&e.state)?;
    if state.expire(now())? {
        e.credit.notify_all();
    }
    state.machine.events(&input)
}
pub(crate) fn control(
    host: &mut Automation,
    input: AudioPreviewControlV4,
) -> Result<AudioPreviewSessionV4, EditorError> {
    let e = entry(&host.database, &input.session_id)?;
    // Stop remains idempotently inspectable after its actor has exited.
    authorize(
        host,
        &e,
        !matches!(input.command, AudioPreviewCommandV4::Stop),
    )?;
    if host.caller.principal_id() != input.owner_id {
        return Err(error("NOT_AUTHORIZED"));
    }
    let gain = match &input.command {
        AudioPreviewCommandV4::SetTemporaryGain { gain } => Some(
            e.prepared
                .validate_temporary_gain(&gain.track_id, &gain.element_id, gain.decibels_tenths)
                .map_err(|x| EditorError::new(x.code, x.message))?,
        ),
        _ => None,
    };
    let transition = {
        let mut state = lock(&e.state)?;
        let transition = state
            .machine
            .control_validated(&input, gain.as_ref(), now())?;
        if transition.effect != AudioPreviewEffectV4::None {
            state.picture = None;
            state.text_picture = None;
            state.text_coverage = None;
        }
        // Predicate changes and notification share the stream wait mutex.
        e.credit.notify_all();
        transition
    };
    if transition.session.state == AudioPreviewStateV4::Stopped {
        e.stop_actor()?;
    }
    Ok(transition.session)
}
pub(crate) fn acquire(
    host: &mut Automation,
    input: AudioPreviewAcquireV4,
) -> Result<AudioPreviewSessionV4, EditorError> {
    let e = entry(&host.database, &input.session_id)?;
    authorize(host, &e, false)?;
    // Serialize admission against creates/other acquisitions. A lease-expired
    // producer is fully torn down before a new owner's source preparation.
    let all = lock(registries())?;
    let current = e.view()?;
    if current.lease_remaining_ms == "0" {
        let registry = all.get(&e.database).ok_or_else(|| error("NOT_FOUND"))?;
        let active = registry
            .entries
            .values()
            .filter(|other| !Arc::ptr_eq(other, &e) && other.occupies_slot())
            .count();
        if active >= AUDIO_PREVIEW_MAX_SESSIONS {
            return Err(error("PREVIEW_LIMIT"));
        }
        // Expire before joining so both idle and preparing actors observe the
        // cancellation through the original-authority CAS checkpoint.
        {
            let mut state = lock(&e.state)?;
            if state.expire(now())? {
                e.credit.notify_all();
            }
        }
        if let Some(actor) = lock(&e.actor)?.take() {
            actor.thread.join().map_err(|_| error("INTERNAL_ERROR"))?;
        }
    }
    let view = {
        let mut state = lock(&e.state)?;
        let next = state.machine.acquire(
            &input,
            host.caller.principal_id().into(),
            new_identity(),
            now(),
        )?;
        if next.effect != AudioPreviewEffectV4::None {
            state.picture = None;
            state.text_picture = None;
            state.text_coverage = None;
            state.authority = trusted(&host.caller);
        } else {
            host.app.revalidate_audio_preview(
                &e.request.project_id,
                &e.request.revision,
                &state.authority,
            )?;
        }
        e.credit.notify_all();
        next.session
    };
    e.ensure_actor()?;
    Ok(view)
}
pub(crate) fn authorize_stream(host: &mut Automation, session_id: &str) -> Result<(), EditorError> {
    let e = entry(&host.database, session_id)?;
    authorize(host, &e, true)
}
pub(crate) fn open_stream(
    host: &mut Automation,
    input: AudioPreviewStreamV4,
) -> Result<AudioPreviewStream, EditorError> {
    let e = entry(&host.database, &input.session_id)?;
    authorize(host, &e, true)?;
    lock(&e.state)?
        .machine
        .verify_stream(&input, host.caller.principal_id(), now())?;
    AudioPreviewStream::new(e, input)
}
pub fn shutdown(database: &Path) -> Result<(), EditorError> {
    let registry = lock(registries())?.remove(&database_key(database)?);
    if let Some(registry) = registry {
        for e in registry.entries.values() {
            e.signal_shutdown()?;
        }
        let mut failure = None;
        for e in registry.entries.values() {
            if let Err(err) = e.stop_actor() {
                failure = Some(err);
            }
        }
        if let Some(err) = failure {
            return Err(err);
        }
    }
    Ok(())
}

/// Picture planning shares the sound session's original authorization and
/// publication fence. No new actor, resource handle or mutable-head read.
pub(crate) fn picture(
    host: &mut Automation,
    input: opencut_api_contracts::visual_preview_v4::VisualPreviewFrameV4,
) -> Result<opencut_api_contracts::visual_preview_v4::VisualPreviewFrameResultV4, EditorError> {
    let e = entry(&host.database, &input.session.session_id)?;
    authorize(host, &e, true)?;
    let (view, cached) = {
        let state = lock(&e.state)?;
        state
            .machine
            .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
        let cached = state
            .picture
            .as_ref()
            .filter(|c| {
                input.decoded.is_some()
                    && c.session == input.session
                    && input.time.as_ref() == Some(c.prepared.time())
            })
            .map(|c| c.prepared.clone());
        (state.machine.snapshot(now()), cached)
    };
    opencut_editor_application::visual_preview_v4::validate_picture_time(
        input.time.as_ref().unwrap_or(&view.authoritative_time),
        &view.authoritative_time,
    )?;
    // One private prepared frame, never a public DTO or mutable-head cache.
    // The adapter's visual policy is immutable within this host runtime.
    // Misses/direct bindings retain the original full preparation semantics.
    let prepared = if let Some(cached) = cached {
        cached
    } else {
        let selected = host
            .app
            .fixed_v4_frame_snapshot(&e.request.project_id, &e.request.revision)?;
        Arc::new(
            opencut_editor_application::visual_preview_v4::PreparedPictureV4::new(
                &selected.snapshot,
                &e.request.sequence_id,
                input.time.as_ref().unwrap_or(&view.authoritative_time),
                &selected.processing_policy,
            )?,
        )
    };
    let result = prepared.result(&input, &view.authoritative_time)?;
    authorize(host, &e, true)?;
    let mut state = lock(&e.state)?;
    state
        .machine
        .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
    opencut_editor_application::visual_preview_v4::validate_picture_time(
        &result.plan.identity.time,
        &state.machine.snapshot(now()).authoritative_time,
    )?;
    if input.decoded.is_none() {
        state.picture = Some(CachedPicture {
            session: input.session,
            prepared,
        });
    }
    Ok(result)
}
