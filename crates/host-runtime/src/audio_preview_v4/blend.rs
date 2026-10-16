//! Native fixed-font proof and complete frame binding share the original sound
//! session. Only one bounded frame/proof set lives in each session.
use super::text::CachedTextCoverage;
use super::*;
use opencut_api_contracts::visual_preview_blend_v1::*;
use opencut_composition::project_v4::NativeTextCoverageV1;
use opencut_contracts::project_v4::FontResourceV4;
use opencut_editor_application::{
    classic_blend_frame_v1::admit_blend_frame_v1, font_import_v1::FontImportPolicyV1,
    visual_preview_blend_v1::PreparedBlendPictureV1, visual_preview_v4::validate_picture_time,
};
use opencut_native_services::classic_visual::VerifiedClassicTextResources;
use std::collections::BTreeSet;

/// Match only native proofs from this session, then restore the current frame's
/// exact order. The caller publishes nothing until its final live fence passes.
pub(super) fn complete_coverage<'a>(
    current: impl IntoIterator<Item = (&'a str, &'a FontResourceV4, &'a str)>,
    cached: &[NativeTextCoverageV1],
    prove: impl FnOnce(&BTreeSet<String>) -> Result<Vec<NativeTextCoverageV1>, EditorError>,
) -> Result<Vec<NativeTextCoverageV1>, EditorError> {
    let current = current.into_iter().take(61).collect::<Vec<_>>();
    let invalid = || {
        EditorError::new(
            "INVALID_VISUAL_PLAN",
            "Incremental font proof differs from the current frame",
        )
    };
    if current.len() > 60 {
        return Err(invalid());
    }
    let matches = |proof: &NativeTextCoverageV1, expected: &(&str, &FontResourceV4, &str)| {
        proof.element_id == expected.0 && &proof.font == expected.1 && proof.content == expected.2
    };
    let mut seen = BTreeSet::new();
    let mut missing = BTreeSet::new();
    let mut covered = Vec::with_capacity(current.len());
    for expected in &current {
        if !seen.insert(expected.0) {
            return Err(invalid());
        }
        let proof = cached
            .iter()
            .find(|proof| matches(proof, expected))
            .cloned();
        if proof.is_none() {
            missing.insert(expected.0.to_owned());
        }
        covered.push(proof);
    }
    // Even an all-hit frame must pass the complete native envelope/checkpoint.
    for proof in prove(&missing)? {
        if !missing.remove(&proof.element_id) {
            return Err(invalid());
        }
        let index = current
            .iter()
            .position(|expected| matches(&proof, expected))
            .ok_or_else(invalid)?;
        covered[index] = Some(proof);
    }
    if !missing.is_empty() {
        return Err(invalid());
    }
    covered
        .into_iter()
        .map(|proof| proof.ok_or_else(invalid))
        .collect()
}

pub(crate) fn blend_picture(
    host: &mut Automation,
    input: VisualPreviewBlendFrameV1,
) -> Result<VisualPreviewBlendFrameResultV1, EditorError> {
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
                input.decoded.is_some()
                    && c.session == input.session
                    && input.time.as_ref() == Some(c.prepared.time())
            })
            .and_then(|c| match &c.prepared {
                PreparedPicture::Blend(prepared) => Some(prepared.clone()),
                _ => None,
            });
        let coverage = state
            .text_coverage
            .as_ref()
            .filter(|c| c.session == input.session)
            .map(|c| c.coverage.clone());
        (state.machine.snapshot(now()), cached, coverage)
    };
    let time = input.time.as_ref().unwrap_or(&view.authoritative_time);
    validate_picture_time(time, &view.authoritative_time)?;
    let (prepared, new_coverage) = if let Some(cached) = cached {
        (cached, None)
    } else {
        let selected = host
            .app
            .fixed_v4_frame_snapshot(&e.request.project_id, &e.request.revision)?;
        let frame = opencut_editor_application::project_v4_blend_frame_jobs::prepare(
            &selected.snapshot,
            &e.request.sequence_id,
            time,
        )?;
        admit_blend_frame_v1(&frame, &selected.processing_policy)?;
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
                }),
            coverage.as_deref().unwrap_or_default(),
            |missing| {
                VerifiedClassicTextResources::new_caption_fonts_subset_v3(
                    host.app.asset_store_root(),
                    frame.execution().execution(),
                    missing,
                    &FontImportPolicyV1::default(),
                    || e.checkpoint(&host.app),
                )
            },
        )?;
        let prepared = Arc::new(PreparedBlendPictureV1::new(
            frame,
            selected.processing_policy,
            proofs.clone(),
        )?);
        (prepared, Some(proofs))
    };
    let result = prepared.result(&input, &view.authoritative_time)?;
    authorize(host, &e, true)?;
    let mut state = lock(&e.state)?;
    state
        .machine
        .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
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
    if input.decoded.is_none() {
        state.picture = Some(CachedPicture {
            session: input.session,
            prepared: PreparedPicture::Blend(prepared),
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::{
        AssetVersion,
        project_v4::{FontStyleV4, ResourceOriginV4},
    };

    fn proof(id: &str, content: &str) -> NativeTextCoverageV1 {
        NativeTextCoverageV1 {
            element_id: id.into(),
            content: content.into(),
            font: FontResourceV4 {
                id: "fixed-font".into(),
                family: "Fixture".into(),
                weight: 400,
                style: FontStyleV4::Normal,
                face_index: 0,
                content: AssetVersion {
                    id: "av_fixed".into(),
                    content_hash: "a".repeat(64),
                    size: 100,
                },
                origin: ResourceOriginV4::LocalImport {
                    original_name: "fixture.ttf".into(),
                },
                license: None,
            },
        }
    }
    fn keys(
        current: &[NativeTextCoverageV1],
    ) -> impl Iterator<Item = (&str, &FontResourceV4, &str)> {
        current.iter().map(|proof| {
            (
                proof.element_id.as_str(),
                &proof.font,
                proof.content.as_str(),
            )
        })
    }

    #[test]
    fn incremental_coverage_two_three_two_keeps_order_and_discards_departed_text() {
        let initial = vec![proof("latin", "Title"), proof("chinese", "中文")];
        let two = complete_coverage(keys(&initial), &[], |missing| {
            assert_eq!(missing, &BTreeSet::from(["latin".into(), "chinese".into()]));
            Ok(initial.iter().rev().cloned().collect())
        })
        .unwrap();
        assert_eq!(two, initial);
        let current = vec![
            proof("caption", "New caption"),
            two[1].clone(),
            two[0].clone(),
        ];
        let three = complete_coverage(keys(&current), &two, |missing| {
            assert_eq!(missing, &BTreeSet::from(["caption".into()]));
            Ok(vec![current[0].clone()])
        })
        .unwrap();
        assert_eq!(three, current);
        let mut checked_empty = false;
        let returned = complete_coverage(keys(&initial), &three, |missing| {
            assert!(missing.is_empty());
            checked_empty = true;
            Ok(vec![])
        })
        .unwrap();
        assert!(checked_empty);
        assert_eq!(returned, initial);
    }

    #[test]
    fn incremental_coverage_rechecks_content_and_complete_font_identity() {
        let cached = vec![proof("title", "Old title"), proof("unchanged", "中文")];
        for changed in 0..4 {
            let mut current = cached.clone();
            match changed {
                0 => current[0].content = "New title".into(),
                1 => current[0].font.content.content_hash = "b".repeat(64),
                2 => current[0].font.face_index = 1,
                _ => current[0].font.family = "Changed family".into(),
            }
            let result = complete_coverage(keys(&current), &cached, |missing| {
                assert_eq!(missing, &BTreeSet::from(["title".into()]));
                Ok(vec![current[0].clone()])
            })
            .unwrap();
            assert_eq!(result, current);
        }
    }

    #[test]
    fn incremental_coverage_rejects_incomplete_extra_duplicate_or_changed_proofs() {
        let cached = vec![proof("title", "Title")];
        let before = cached.clone();
        let current = vec![cached[0].clone(), proof("caption", "New caption")];
        let mut wrong_font = current[1].clone();
        wrong_font.font.weight = 700;
        for returned in [
            vec![],
            vec![proof("foreign", "New caption")],
            vec![current[1].clone(), current[1].clone()],
            vec![proof("caption", "Changed caption")],
            vec![wrong_font],
            vec![current[1].clone(), cached[0].clone()],
        ] {
            assert_eq!(
                complete_coverage(keys(&current), &cached, |_| Ok(returned))
                    .unwrap_err()
                    .code,
                "INVALID_VISUAL_PLAN"
            );
            assert_eq!(cached, before);
        }
    }

    #[test]
    fn incremental_coverage_all_hits_still_propagate_revocation_and_budget_failure() {
        let cached = vec![proof("title", "Title")];
        for code in ["JOB_CANCELLED", "NOT_AUTHORIZED", "FONT_BUDGET_EXCEEDED"] {
            assert_eq!(
                complete_coverage(keys(&cached), &cached, |missing| {
                    assert!(missing.is_empty());
                    Err(EditorError::new(code, "Rejected before publication"))
                })
                .unwrap_err()
                .code,
                code
            );
        }
        assert_eq!(cached, vec![proof("title", "Title")]);
        let too_many = (0..61)
            .map(|i| proof(&i.to_string(), "x"))
            .collect::<Vec<_>>();
        assert_eq!(
            complete_coverage(keys(&too_many), &[], |_| panic!("Unbounded proof request"))
                .unwrap_err()
                .code,
            "INVALID_VISUAL_PLAN"
        );
    }
}
