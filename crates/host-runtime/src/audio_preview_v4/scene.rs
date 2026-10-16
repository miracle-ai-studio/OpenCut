//! Scene frame binding on the original fixed consumer session.
use super::blend::complete_coverage;
use super::text::CachedTextCoverage;
use super::*;
use opencut_api_contracts::visual_preview_scene_v1::*;
use opencut_editor_application::project_v4::ProjectV4Read;
use opencut_editor_application::{
    classic_scene_frame_v1::admit_scene_frame_v1, font_import_v1::FontImportPolicyV1,
    visual_preview_scene_v1::PreparedScenePictureV1, visual_preview_v4::validate_picture_time,
};
use opencut_native_services::classic_visual::VerifiedClassicTextResources;
pub(crate) fn scene_picture(
    host: &mut Automation,
    input: VisualPreviewSceneFrameV1,
) -> Result<VisualPreviewSceneFrameResultV1, EditorError> {
    scene_picture_inner(host, input, None, None).map(|result| result.0)
}
pub(crate) fn transform_draft_picture(
    host: &mut Automation,
    input: opencut_api_contracts::visual_transform_draft_v1::VisualTransformDraftFrameV1,
) -> Result<
    opencut_api_contracts::visual_transform_draft_v1::VisualTransformDraftFrameResultV1,
    EditorError,
> {
    if input.draft_id.is_empty()
        || input.draft_id.len() > 128
        || input.draft_id.chars().any(char::is_control)
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Transform draft identity is invalid",
        ));
    }
    let (frame, digest) = scene_picture_inner(host, input.frame, Some(&input.transaction), None)?;
    Ok(
        opencut_api_contracts::visual_transform_draft_v1::VisualTransformDraftFrameResultV1 {
            draft_id: input.draft_id,
            transaction_digest: digest.ok_or_else(|| error("INTERNAL_ERROR"))?,
            frame,
        },
    )
}
pub(crate) fn transform_draft_picture_v2(
    host: &mut Automation,
    input: opencut_api_contracts::visual_transform_draft_v2::VisualTransformDraftFrameV2,
) -> Result<
    opencut_api_contracts::visual_transform_draft_v2::VisualTransformDraftFrameResultV2,
    EditorError,
> {
    if input.draft_id.is_empty()
        || input.draft_id.len() > 128
        || input.draft_id.chars().any(char::is_control)
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Transform draft identity is invalid",
        ));
    }
    let (frame, digest) = scene_picture_inner(
        host,
        input.frame,
        Some(&input.transaction),
        Some(input.gesture_intent),
    )?;
    Ok(
        opencut_api_contracts::visual_transform_draft_v1::VisualTransformDraftFrameResultV1 {
            draft_id: input.draft_id,
            transaction_digest: digest.ok_or_else(|| error("INTERNAL_ERROR"))?,
            frame,
        },
    )
}
fn scene_picture_inner(
    host: &mut Automation,
    input: VisualPreviewSceneFrameV1,
    draft: Option<&opencut_contracts::project_v4_public::ApplyPublicV4Project>,
    gesture_intent: Option<opencut_contracts::element_transform_v2::TransformGestureIntentV2>,
) -> Result<(VisualPreviewSceneFrameResultV1, Option<String>), EditorError> {
    let e = entry(&host.database, &input.session.session_id)?;
    authorize(host, &e, true)?;
    let (view, cached, coverage) = {
        let state = lock(&e.state)?;
        state
            .machine
            .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
        let cached = state
            .picture
            .as_ref()
            .filter(|c| {
                draft.is_none()
                    && input.decoded.is_some()
                    && c.session == input.session
                    && input.time.as_ref() == Some(c.prepared.time())
            })
            .and_then(|c| match &c.prepared {
                PreparedPicture::Scene(prepared) => Some(prepared.clone()),
                _ => None,
            });
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
        if view.state != opencut_api_contracts::audio_preview_v4::AudioPreviewStateV4::Paused
            || transaction.project_id != e.request.project_id
            || transaction.expected_revision != e.request.revision
            || input.time.as_ref().is_none_or(|time| {
                opencut_time::RationalTime::try_from(time).ok()
                    != opencut_time::RationalTime::try_from(&view.authoritative_time).ok()
            })
        {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Transform draft requires the original paused scene and exact consumer time",
            ));
        }
    }
    let time = input.time.as_ref().unwrap_or(&view.authoritative_time);
    let mut digest = None;
    validate_picture_time(time, &view.authoritative_time)?;
    let (prepared, new_coverage) = if let Some(cached) = cached {
        (cached, None)
    } else {
        let mut selected = host
            .app
            .fixed_v4_frame_snapshot(&e.request.project_id, &e.request.revision)?;
        if let Some(transaction) = draft {
            let (snapshot, identity) = if let Some(intent) = gesture_intent {
                opencut_editor_application::element_transform_v2::prepare_draft_v2(
                    &host.app,
                    transaction,
                    &e.request.sequence_id,
                    time,
                    intent,
                )?
            } else {
                opencut_editor_application::visual_transform_draft_v1::prepare(
                    &host.app,
                    transaction,
                    &e.request.sequence_id,
                    time,
                )?
            };
            selected.snapshot = snapshot;
            digest = Some(identity);
        }
        let frame = opencut_editor_application::project_v4_scene_frame_jobs::prepare(
            &selected.snapshot,
            &e.request.sequence_id,
            time,
        )?;
        admit_scene_frame_v1(&frame, &selected.processing_policy)?;
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
                VerifiedClassicTextResources::new_scene_fonts_subset_v1(
                    host.app.asset_store_root(),
                    &frame,
                    missing,
                    &FontImportPolicyV1::default(),
                    || e.checkpoint(&host.app),
                )
            },
        )?;
        let prepared = Arc::new(PreparedScenePictureV1::new(
            frame,
            selected.processing_policy,
            proofs.clone(),
        )?);
        (prepared, Some(proofs))
    };
    let result = prepared.result(&input, &view.authoritative_time)?;
    authorize(host, &e, true)?;
    if let Some(transaction) = draft {
        // A concurrent commit must not publish an obsolete transform draft.
        host.app.authorize_v4(
            opencut_editor_application::project_v4::V4Intent::PublicApply(transaction),
        )?;
        let head = host.app.head_v4_source(&transaction.project_id)?;
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
        if current.state != opencut_api_contracts::audio_preview_v4::AudioPreviewStateV4::Paused
            || opencut_time::RationalTime::try_from(&result.plan.identity.time)?
                != opencut_time::RationalTime::try_from(&current.authoritative_time)?
        {
            return Err(EditorError::new(
                "PREVIEW_GENERATION_CONFLICT",
                "The paused transform consumer changed",
            ));
        }
    }
    validate_picture_time(
        &result.plan.identity.time,
        &state.machine.snapshot(now()).authoritative_time,
    )?;
    if let Some(coverage) = new_coverage {
        state.text_coverage = Some(CachedTextCoverage {
            session: input.session.clone(),
            coverage,
        });
    }
    if draft.is_none() && input.decoded.is_none() {
        state.picture = Some(CachedPicture {
            session: input.session,
            prepared: PreparedPicture::Scene(prepared),
        });
    }
    Ok((result, digest))
}
