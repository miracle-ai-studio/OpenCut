//! Serialize preview dispatch and retirement around a durable, authenticated
//! request slot. A new host incarnation can retire old slots, never recreate an
//! old process's ephemeral session. No restoration acquires a new owner/lease.
use super::*;
use opencut_editor_application::audio_preview_recovery::{self as recovery, *};
#[cfg(test)]
mod tests;

fn instance() -> &'static str {
    static INSTANCE: OnceLock<String> = OnceLock::new();
    INSTANCE.get_or_init(new_identity)
}
fn fence() -> &'static Mutex<()> {
    static FENCE: Mutex<()> = Mutex::new(());
    &FENCE
}
fn context(host: &mut Automation) {
    host.app.set_execution_context(ExecutionContext::for_caller(
        host.source,
        host.caller.clone(),
    ));
}
fn retired() -> EditorError {
    EditorError::new(
        "PREVIEW_RECOVERY_RETIRED",
        "The original preview generation has retired; it cannot be resumed",
    )
}
fn same_receipt(current: &AudioPreviewSession, receipt: &AudioPreviewSession) -> bool {
    current.session_id == receipt.session_id
        && current.identity == receipt.identity
        && current.generation == receipt.generation
        && current.overlay_epoch == receipt.overlay_epoch
        && current.owner_id == receipt.owner_id
        && current.lease_id == receipt.lease_id
        && current.state == receipt.state
}
fn live_receipt(host: &mut Automation, receipt: &AudioPreviewSession) -> Result<(), EditorError> {
    let e = entry(&host.database, &receipt.session_id)?;
    authorize(
        host,
        &e,
        !matches!(
            receipt.state,
            AudioPreviewState::Stopped | AudioPreviewState::Failed
        ),
    )?;
    if !same_receipt(&e.view()?, receipt) {
        return Err(retired());
    }
    Ok(())
}
pub(crate) fn stage(
    host: &mut Automation,
    input: StageAudioPreviewRecovery,
) -> Result<AudioPreviewRecoveryRecord, EditorError> {
    let _fence = lock(fence())?;
    context(host);
    let payload = recovery::payload(&input)?;
    let project = &recovery::create(&input.request).project_id;
    // Exact Stage replay must still return its opaque ID after native expiry or
    // retirement. A lost Stage response otherwise becomes impossible to clean.
    if let Some(existing) = host.app.read_audio_preview_recovery(project, None)? {
        if existing.view.request_digest != recovery::digest(&payload) {
            return Err(EditorError::new(
                "RECOVERY_CONFLICT",
                "Resolve the original preview request before replacing it",
            ));
        }
        return host.app.stage_audio_preview_recovery(&input, instance());
    }
    if let AudioPreviewRecoveryRequest::Control {
        create: original_create,
        request,
    } = &input.request
    {
        let e = entry(&host.database, &request.session_id)?;
        authorize(
            host,
            &e,
            !matches!(request.command, AudioPreviewCommand::Stop),
        )?;
        if &e.request != original_create
            || e.creator != host.caller.principal_id()
            || request.owner_id != host.caller.principal_id()
        {
            return Err(error("NOT_AUTHORIZED"));
        }
        let current = e.view()?;
        if request.lease_id != current.lease_id || request.owner_id != current.owner_id {
            return Err(error("NOT_AUTHORIZED"));
        }
    }
    host.app.stage_audio_preview_recovery(&input, instance())
}
pub(crate) fn inspect(
    host: &mut Automation,
    input: InspectAudioPreviewRecovery,
) -> Result<AudioPreviewRecoveryInspection, EditorError> {
    let _fence = lock(fence())?;
    context(host);
    let record = host
        .app
        .read_audio_preview_recovery(&input.project_id, None)?
        .map(|r| r.view);
    Ok(AudioPreviewRecoveryInspection { record })
}
pub(crate) fn dispatch(
    host: &mut Automation,
    input: DispatchAudioPreviewRecovery,
) -> Result<AudioPreviewSession, EditorError> {
    let _fence = lock(fence())?;
    context(host);
    recovery::identifier(&input.recovery_id)?;
    let stored = host
        .app
        .read_audio_preview_recovery(&input.project_id, Some(&input.recovery_id))?
        .ok_or_else(|| error("NOT_FOUND"))?;
    if stored.host_instance != instance()
        || stored.view.status == AudioPreviewRecoveryStatus::Retired
    {
        host.app
            .retire_audio_preview_recovery(&input.project_id, &input.recovery_id)?;
        return Err(retired());
    }
    if let Some(receipt) = stored.receipt {
        live_receipt(host, &receipt)?;
        return Ok(receipt);
    }
    let session = if stored.dispatched {
        // A crash/IO failure may occur after native execution but before its
        // durable receipt. Read the original native receipt; never run an old
        // command again after its native idempotency history has been evicted.
        match &stored.original.request {
            AudioPreviewRecoveryRequest::Create { request } => {
                let e = lock(registries())?
                    .get(&database_key(&host.database)?)
                    .and_then(|r| {
                        r.entries
                            .values()
                            .find(|e| {
                                e.creator == host.caller.principal_id() && e.request == *request
                            })
                            .cloned()
                    })
                    .ok_or_else(retired)?;
                authorize(host, &e, true)?;
                let view = e.view()?;
                if view.generation != "0" || view.state != AudioPreviewState::Ready {
                    return Err(retired());
                }
                view
            }
            AudioPreviewRecoveryRequest::Control { request, .. } => {
                let e = entry(&host.database, &request.session_id)?;
                authorize(
                    host,
                    &e,
                    !matches!(request.command, AudioPreviewCommand::Stop),
                )?;
                let receipt = lock(&e.state)?
                    .machine
                    .recover_control_receipt(request)?
                    .ok_or_else(retired)?;
                receipt
            }
        }
    } else {
        host.app
            .begin_audio_preview_recovery(&input.project_id, &input.recovery_id)?;
        match stored.original.request {
            AudioPreviewRecoveryRequest::Create { request } => super::create(host, request)?,
            AudioPreviewRecoveryRequest::Control { request, .. } => super::control(host, request)?,
        }
    };
    live_receipt(host, &session)?;
    host.app
        .complete_audio_preview_recovery(&input.project_id, &input.recovery_id, &session)?;
    Ok(session)
}
fn retire_original(
    host: &mut Automation,
    stored: &StoredAudioPreviewRecovery,
) -> Result<(), EditorError> {
    // Process-local sessions cannot outlive this host incarnation. Importantly,
    // absence is not an instruction to call Create with the retained old key.
    if stored.host_instance != instance() {
        return Ok(());
    }
    let original = recovery::create(&stored.original.request);
    let matching = lock(registries())?
        .get(&database_key(&host.database)?)
        .and_then(|r| {
            r.entries
                .values()
                .find(|e| e.creator == host.caller.principal_id() && e.request == *original)
                .cloned()
        });
    let Some(e) = matching else { return Ok(()) };
    authorize(host, &e, false)?;
    let mut state = lock(&e.state)?;
    if state.expire(now())? {
        e.credit.notify_all();
    }
    let current = state.machine.snapshot(now());
    if matches!(
        current.state,
        AudioPreviewState::Stopped | AudioPreviewState::Failed
    ) {
        return Ok(());
    }
    let expected = match &stored.original.request {
        AudioPreviewRecoveryRequest::Create { .. } => stored
            .receipt
            .clone()
            .or_else(|| (current.generation == "0").then(|| current.clone())),
        AudioPreviewRecoveryRequest::Control { request, .. } => {
            let receipt = state.machine.recover_control_receipt(request)?;
            stored.receipt.clone().or(receipt).or_else(|| {
                (current.generation == request.generation
                    && current.owner_id == request.owner_id
                    && current.lease_id == request.lease_id)
                    .then(|| current.clone())
            })
        }
    };
    let Some(expected) = expected else {
        return Ok(());
    };
    // An independently advanced/reaquired session belongs to a different
    // generation. Retire the slot without touching that newer session.
    if current.generation != expected.generation
        || current.overlay_epoch != expected.overlay_epoch
        || current.owner_id != expected.owner_id
        || current.lease_id != expected.lease_id
    {
        return Ok(());
    }
    let stop = AudioPreviewControl {
        session_id: current.session_id.clone(),
        generation: current.generation.clone(),
        owner_id: current.owner_id.clone(),
        lease_id: current.lease_id.clone(),
        control_id: format!("recovery-stop-{}", stored.view.recovery_id),
        command: AudioPreviewCommand::Stop,
    };
    match state.machine.control(&stop, now()) {
        Ok(_) => {
            state.picture = None;
            state.text_coverage = None;
            state.resources.clear();
            e.credit.notify_all();
        }
        Err(error) if error.code == "PREVIEW_EXPIRED" => return Ok(()),
        Err(error) => return Err(error),
    }
    drop(state);
    e.stop_actor()
}
pub(crate) fn ack(
    host: &mut Automation,
    input: AckAudioPreviewRecovery,
) -> Result<AudioPreviewRecoveryAck, EditorError> {
    let _fence = lock(fence())?;
    context(host);
    recovery::identifier(&input.recovery_id)?;
    let stored = host
        .app
        .read_audio_preview_recovery(&input.project_id, Some(&input.recovery_id))?;
    if let Some(stored) = stored {
        if input.disposition == AudioPreviewRecoveryDisposition::Retire {
            // Close first, then clean up. If cleanup fails the redacted retired
            // record remains discoverable and dispatch stays permanently fenced.
            host.app
                .retire_audio_preview_recovery(&input.project_id, &input.recovery_id)?;
            retire_original(host, &stored)?;
        } else if stored.view.status != AudioPreviewRecoveryStatus::Completed {
            return Err(EditorError::new(
                "RECOVERY_UNCONFIRMED",
                "Only an observed completed preview request may be acknowledged",
            ));
        }
        host.app
            .ack_audio_preview_recovery(&input.project_id, &input.recovery_id)?;
    }
    Ok(AudioPreviewRecoveryAck {
        recovery_id: input.recovery_id,
    })
}

/// Keepalive callers must not depend on a Stage response to send retirement.
/// Reuse the original durable slot and the existing fenced retirement; never
/// dispatch the retained request. Each operation takes its own fence so a
/// concurrent original dispatch is either prevented or retired by its receipt.
pub(crate) fn retire(
    host: &mut Automation,
    input: StageAudioPreviewRecovery,
) -> Result<AudioPreviewRecoveryAck, EditorError> {
    let record = stage(host, input)?;
    ack(
        host,
        AckAudioPreviewRecovery {
            project_id: record.project_id,
            recovery_id: record.recovery_id,
            disposition: AudioPreviewRecoveryDisposition::Retire,
        },
    )
}
