//! Session-owned verified file handles; every read retains current authority.
use super::*;
use opencut_api_contracts::visual_preview::*;
use opencut_native_services::browser_resources::VerifiedResourceFile;

pub(super) struct SessionResource {
    session: AudioPreviewStream,
    resource: VisualResource,
    file: Arc<Mutex<VerifiedResourceFile>>,
}
fn current(
    host: &mut Automation,
    e: &Entry,
    session: &AudioPreviewStream,
) -> Result<(), EditorError> {
    authorize(host, e, true)?;
    lock(&e.state)?
        .machine
        .verify_visual_frame(session, host.caller.principal_id(), now())
}
fn opened(id: String, resource: &VisualResource) -> VisualPreviewResourceOpened {
    VisualPreviewResourceOpened {
        transfer_id: id,
        content_hash: resource.content_hash.clone(),
        byte_length: resource.size.clone(),
        max_chunk_bytes: 512 * 1024,
    }
}
pub(crate) fn open(
    host: &mut Automation,
    input: VisualPreviewResourceOpen,
) -> Result<VisualPreviewResourceOpened, EditorError> {
    let e = entry(&host.database, &input.session.session_id)?;
    current(host, &e, &input.session)?;
    // Only one initial hash per session at a time; never queue obsolete GB reads
    // or hold the consumer-state mutex while hashing the file.
    let _opening = e.resource_open_gate.try_lock().map_err(|err| match err {
        std::sync::TryLockError::WouldBlock => error("PREVIEW_RESOURCE_BUSY"),
        std::sync::TryLockError::Poisoned(_) => error("INTERNAL_ERROR"),
    })?;
    let (version, retained) = {
        let state = lock(&e.state)?;
        state
            .machine
            .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
        if state.resources.len() >= 32 {
            return Err(error("PREVIEW_RESOURCE_LIMIT"));
        }
        let version = state.pictures.iter().rev()
            .filter(|picture|picture.session==input.session)
            .find_map(|picture|picture.prepared.media_version(&input.resource).ok())
            .ok_or_else(||error("NOT_AUTHORIZED"))?;
        let retained = state
            .resources
            .values()
            .find(|r| r.session == input.session && r.resource == input.resource)
            .map(|r| r.file.clone());
        (version, retained)
    };
    let file = if let Some(file) = retained {
        file
    } else {
        let root = host
            .database
            .parent()
            .ok_or_else(|| error("STORAGE_ERROR"))?
            .join("asset-store");
        let mut last_authorization = now();
        Arc::new(Mutex::new(VerifiedResourceFile::open(
            &root,
            &version,
            &mut || {
                {
                    let state = lock(&e.state)?;
                    state.machine.verify_visual_frame(
                        &input.session,
                        host.caller.principal_id(),
                        now(),
                    )?;
                }
                if now().saturating_sub(last_authorization) >= 1000 {
                    current(host, &e, &input.session)?;
                    last_authorization = now();
                }
                Ok(())
            },
        )?))
    };
    current(host, &e, &input.session)?;
    let mut state = lock(&e.state)?;
    state
        .machine
        .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
    if state.resources.len() >= 32 {
        return Err(error("PREVIEW_RESOURCE_LIMIT"));
    }
    let id = new_identity();
    state.resources.insert(
        id.clone(),
        SessionResource {
            session: input.session,
            resource: input.resource.clone(),
            file,
        },
    );
    Ok(opened(id, &input.resource))
}
pub(crate) fn read(
    host: &mut Automation,
    input: &VisualPreviewResourceRead,
) -> Result<(Vec<u8>, bool), EditorError> {
    let offset = input
        .offset
        .parse::<u64>()
        .map_err(|_| error("INVALID_RESOURCE_RANGE"))?;
    if offset.to_string() != input.offset || input.count == 0 || input.count > 512 * 1024 {
        return Err(error("INVALID_RESOURCE_RANGE"));
    }
    let e = entry(&host.database, &input.session.session_id)?;
    current(host, &e, &input.session)?;
    let file = {
        let state = lock(&e.state)?;
        state
            .machine
            .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
        state
            .resources
            .get(&input.transfer_id)
            .filter(|r| r.session == input.session)
            .map(|r| r.file.clone())
            .ok_or_else(|| error("NOT_AUTHORIZED"))?
    };
    let result = lock(&file)?.read_preview_range(offset, input.count)?;
    current(host, &e, &input.session)?;
    let state = lock(&e.state)?;
    state
        .machine
        .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
    // A concurrent release/stop must not publish the last in-flight chunk.
    if !state
        .resources
        .get(&input.transfer_id)
        .is_some_and(|r| r.session == input.session && Arc::ptr_eq(&r.file, &file))
    {
        return Err(error("NOT_AUTHORIZED"));
    }
    Ok(result)
}
pub(crate) fn release(
    host: &mut Automation,
    input: VisualPreviewResourceRelease,
) -> Result<VisualPreviewResourceReleased, EditorError> {
    let e = entry(&host.database, &input.session.session_id)?;
    current(host, &e, &input.session)?;
    let mut state = lock(&e.state)?;
    state
        .machine
        .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
    if !state
        .resources
        .get(&input.transfer_id)
        .is_some_and(|r| r.session == input.session)
    {
        return Err(error("NOT_AUTHORIZED"));
    }
    state.resources.remove(&input.transfer_id);
    Ok(VisualPreviewResourceReleased { released: true })
}
