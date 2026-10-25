//! Scene frame binding on the original fixed consumer session.
use super::fonts::CachedTextCoverage;
use super::fonts::complete_coverage;
use super::*;
use opencut_api_contracts::visual_preview::*;
use opencut_contracts::Time;
use opencut_editor_application::scene::SceneRead;
use opencut_editor_application::{
    classic_scene_frame::admit_scene_frame, font_import::FontImportPolicy,
    visual_preview::PreparedPicture, visual_preview::validate_preparation_time,
};
use opencut_native_services::classic_visual::VerifiedClassicTextResources;
pub(crate) fn scene_picture(
    host: &mut Automation,
    input: VisualPreviewFrame,
) -> Result<VisualPreviewFrameResult, EditorError> {
    scene_picture_inner(host, input, None, None).map(|result| result.0)
}
pub(crate) fn edit_draft_picture(
    host: &mut Automation,
    input: opencut_api_contracts::visual_edit_draft::VisualEditDraftFrame,
) -> Result<
    opencut_api_contracts::visual_edit_draft::VisualEditDraftFrameResult,
    EditorError,
> {
    if input.draft_id.is_empty()
        || input.draft_id.len() > 128
        || input.draft_id.chars().any(char::is_control)
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Visual edit draft identity is invalid",
        ));
    }
    let (frame, digest) = scene_picture_inner(
        host,
        input.frame,
        Some(&input.transaction),
        Some(input.intent),
    )?;
    Ok(
        opencut_api_contracts::visual_edit_draft::VisualEditDraftFrameResult {
            draft_id: input.draft_id,
            transaction_digest: digest.ok_or_else(|| error("INTERNAL_ERROR"))?,
            frame,
        },
    )
}
fn scene_picture_inner(
    host: &mut Automation,
    mut input: VisualPreviewFrame,
    draft: Option<&opencut_contracts::project_public::ApplyProject>,
    intent: Option<opencut_contracts::visual_edit_draft::VisualEditDraftIntent>,
) -> Result<(VisualPreviewFrameResult, Option<String>), EditorError> {
    let e = entry(&host.database, &input.session.session_id)?;
    authorize(host, &e, true)?;
    let (view, cached, coverage) = {
        let state = lock(&e.state)?;
        state
            .machine
            .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
        let cached = state
            .pictures
            .iter().rev()
            .find(|c| {
                draft.is_none()
                    && c.resolution == input.resolution
                    && input.decoded.is_some()
                    && c.retained_picture_bytes == input.retained_picture_bytes
                    && c.session == input.session
                    && input.time.as_ref() == Some(c.prepared.time())
            })
            .map(|c| c.prepared.clone());
        let coverage = state
            .text_coverage
            .as_ref()
            .filter(|c| c.session == input.session)
            .map(|c| c.coverage.clone());
        (state.machine.snapshot(now()), cached, coverage)
    };
    // HTTP cancellation does not cancel spawn_blocking. Never queue unbounded
    // obsolete draft compiles behind that work or hold the sound-state mutex.
    let _draft_guard = if draft.is_some() {
        Some(
            e.transform_draft_gate
                .try_lock()
                .map_err(|lock_error| match lock_error {
                    std::sync::TryLockError::WouldBlock => EditorError::new(
                        "PREVIEW_DRAFT_BUSY",
                        "The previous transform draft is still finishing",
                    ),
                    std::sync::TryLockError::Poisoned(_) => error("INTERNAL_ERROR"),
                })?,
        )
    } else {
        None
    };
    if let Some(transaction) = draft {
        if input.prepare_ahead
            || input.resolution != opencut_contracts::export_video::RasterResolution::Source
            || view.state != opencut_api_contracts::audio_preview::AudioPreviewState::Paused
            || transaction.project_id != e.request.project_id
            || transaction.expected_revision != e.request.revision
            || input
                .time
                .as_ref()
                .is_none_or(|time| !same_consumer_time(time, &view.presentation_time))
        {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Visual edit draft requires the original paused scene and exact consumer time",
            ));
        }
    }
    // Preparation requests name a minimum next time. If consumption caught up,
    // sample the current authoritative cursor instead of decoding obsolete frames.
    // Binding requests keep the exact plan time and observations unchanged.
    if input.prepare_ahead
        && input.decoded.is_none()
        && input.time.as_ref().is_some_and(|time| {
            opencut_time::RationalTime::try_from(time)
                .ok()
                .zip(opencut_time::RationalTime::try_from(&view.presentation_time).ok())
                .is_some_and(|(time, consumer)| time.cmp_exact(consumer).is_lt())
        })
    {
        input.time = Some(view.presentation_time.clone());
    }
    let time = input.time.as_ref().unwrap_or(&view.presentation_time);
    let mut digest = None;
    validate_preparation_time(time, &view.presentation_time, input.prepare_ahead)?;
    let (prepared, new_coverage) = if let Some(cached) = cached {
        (cached, None)
    } else {
        let mut selected = host
            .app
            .fixed_scene_text_frame_snapshot(&e.request.project_id, &e.request.revision)?;
        if let Some(transaction) = draft {
            let intent = intent.ok_or_else(|| {
                EditorError::new(
                    "INVALID_ARGUMENT",
                    "Visual edit draft requires its explicit edit intent",
                )
            })?;
            let (snapshot, identity) =
                opencut_editor_application::visual_edit_draft::prepare(
                    &host.app,
                    transaction,
                    &e.request.sequence_id,
                    time,
                    intent,
                )?;
            selected.snapshot = snapshot;
            digest = Some(identity);
        }
        let prepare = if input.resolution == opencut_contracts::export_video::RasterResolution::Preview540 {
            opencut_editor_application::scene_frame_jobs::prepare_preview
        } else { opencut_editor_application::scene_frame_jobs::prepare };
        let frame = prepare(
            &selected.snapshot,
            &e.request.sequence_id,
            time,
        )?;
        selected.processing_policy =
            opencut_editor_application::visual_preview::reserve_picture_memory(
                selected.processing_policy,
                input.retained_picture_bytes,
            )?;
        admit_scene_frame(&frame, &selected.processing_policy)?;
        let proofs = complete_coverage(
            frame
                .execution()
                .execution()
                .execution()
                .texts()
                .iter()
                .map(|plan| {
                    (
                        plan.identity.element_id.as_str(),
                        &plan.font,
                        plan.text.content.as_str(),
                    )
                })
                .chain(frame.mask_texts().iter().map(|mask| {
                    (
                        mask.mask_id.as_str(),
                        &mask.font,
                        mask.mask.content.as_str(),
                    )
                })),
            coverage.as_deref().unwrap_or_default(),
            |missing| {
                VerifiedClassicTextResources::new_scene_fonts_subset(
                    host.app.asset_store_root(),
                    &frame,
                    missing,
                    &FontImportPolicy::default(),
                    || e.checkpoint(&host.app),
                )
            },
        )?;
        let prepared = Arc::new(PreparedPicture::new(
            frame,
            selected.processing_policy,
            proofs.clone(),
        )?);
        (prepared, Some(proofs))
    };
    let result = prepared.result(&input, &view.presentation_time)?;
    authorize(host, &e, true)?;
    if let Some(transaction) = draft {
        // A concurrent commit must not publish an obsolete transform draft.
        host.app.authorize_project(
            opencut_editor_application::scene::SceneCommitIntent::PublicApply(transaction),
        )?;
        let head = host.app.head_scene_source(&transaction.project_id)?;
        if head.revision != transaction.expected_revision {
            return Err(EditorError::new(
                "REVISION_CONFLICT",
                "The transform draft head changed",
            ));
        }
    }
    let mut state = lock(&e.state)?;
    state
        .machine
        .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
    if draft.is_some() {
        let current = state.machine.snapshot(now());
        if current.state != opencut_api_contracts::audio_preview::AudioPreviewState::Paused
            || !same_consumer_time(&result.plan.identity.time, &current.presentation_time)
        {
            return Err(EditorError::new(
                "PREVIEW_GENERATION_CONFLICT",
                "The paused transform consumer changed",
            ));
        }
    }
    validate_preparation_time(
        &result.plan.identity.time,
        &state.machine.snapshot(now()).presentation_time,
        input.prepare_ahead,
    )?;
    if let Some(coverage) = new_coverage {
        state.text_coverage = Some(CachedTextCoverage {
            session: input.session.clone(),
            coverage,
        });
    }
    if draft.is_none() && input.decoded.is_none() {
        state.remember_picture(CachedPicture {
            resolution: input.resolution,
            retained_picture_bytes: input.retained_picture_bytes,
            session: input.session,
            prepared,
        });
    }
    Ok((result, digest))
}

// A stopped timeline may name 0/1 while its paused audio consumer names 0/48000.
// Compare their exact instant at both admission and publication, never their
// serialization or a rounded floating-point timestamp. Invalid times fail closed.
fn same_consumer_time(time: &Time, consumer: &Time) -> bool {
    opencut_time::RationalTime::try_from(time)
        .ok()
        .zip(opencut_time::RationalTime::try_from(consumer).ok())
        .is_some_and(|(time, consumer)| time.cmp_exact(consumer).is_eq())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_consumer_fences_accept_equivalent_bases_but_reject_even_one_sample_drift() {
        let time = |ticks: &str, timebase| Time { ticks: ticks.into(), timebase };
        for (frame, consumer) in [
            (time("0", 1), time("0", 48000)),
            (time("1", 2), time("24000", 48000)),
            (time("9223372036854775807", u32::MAX), time("9223372036854775807", u32::MAX)),
        ] {
            assert!(same_consumer_time(&frame, &consumer));
            assert!(same_consumer_time(&consumer, &frame));
        }
        for (frame, consumer) in [
            (time("0", 1), time("1", 48000)),
            (time("1", 2), time("24001", 48000)),
            (time("1", 2), time("23999", 48000)),
            (time("invalid", 1), time("invalid", 1)),
            (time("0", 0), time("0", 0)),
        ] {
            assert!(!same_consumer_time(&frame, &consumer));
            assert!(!same_consumer_time(&consumer, &frame));
        }
    }
}
