//! Current session font authorization and native coverage; no alternate frame producer.
use super::*;
use opencut_api_contracts::visual_preview::*;
use opencut_composition::scene::NativeTextCoverage;
use opencut_contracts::scene::FontResource;
use std::collections::BTreeSet;
pub(super) struct CachedTextCoverage {
    pub session: AudioPreviewStream,
    pub coverage: Vec<NativeTextCoverage>,
}
/// Match only native proofs from this session, then restore the current frame's
/// exact order. The caller publishes nothing until its final live fence passes.
pub(super) fn complete_coverage<'a>(
    current: impl IntoIterator<Item = (&'a str, &'a FontResource, &'a str)>,
    cached: &[NativeTextCoverage],
    prove: impl FnOnce(&BTreeSet<String>) -> Result<Vec<NativeTextCoverage>, EditorError>,
) -> Result<Vec<NativeTextCoverage>, EditorError> {
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
    let matches = |proof: &NativeTextCoverage, expected: &(&str, &FontResource, &str)| {
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

/// Font reads must reference a font participating in a currently admitted
/// complete frame. Captured native proof binds its exact immutable CAS version.
fn font_version(
    host: &mut Automation,
    input: &VisualPreviewFont,
) -> Result<opencut_contracts::AssetVersion, EditorError> {
    let e = entry(&host.database, &input.session.session_id)?;
    authorize(host, &e, true)?;
    let state = lock(&e.state)?;
    state
        .machine
        .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
    let coverage = state
        .text_coverage
        .as_ref()
        .filter(|c| c.session == input.session)
        .ok_or_else(|| error("NOT_AUTHORIZED"))?;
    coverage
        .coverage
        .iter()
        .find(|proof| proof.font.id == input.font_id)
        .map(|proof| proof.font.content.clone())
        .ok_or_else(|| error("NOT_AUTHORIZED"))
}
pub(crate) fn authorize_font(
    host: &mut Automation,
    input: &VisualPreviewFont,
) -> Result<(), EditorError> {
    font_version(host, input).map(|_| ())
}
pub(crate) fn authorize_font_batch(
    host: &mut Automation,
    input: VisualPreviewFontBatchAuthorize,
) -> Result<VisualPreviewFontBatchAuthorized, EditorError> {
    let e = entry(&host.database, &input.session.session_id)?;
    authorize(host, &e, true)?;
    let state = lock(&e.state)?;
    state
        .machine
        .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
    let coverage = state
        .text_coverage
        .as_ref()
        .filter(|c| c.session == input.session)
        .ok_or_else(|| error("NOT_AUTHORIZED"))?;
    opencut_editor_application::visual_preview::require_font_batch(
        &input.font_ids,
        coverage.coverage.iter().map(|p| p.font.id.as_str()),
    )?;
    // A batch never publishes a partial prefix or extends the original lease.
    state
        .machine
        .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
    Ok(VisualPreviewFontBatchAuthorized {
        font_ids: input.font_ids,
    })
}

pub(crate) fn open_font(
    host: &mut Automation,
    input: VisualPreviewFont,
) -> Result<(std::fs::File, u64), EditorError> {
    let version = font_version(host, &input)?;
    let opened = crate::transport_io::open_verified_project_resource(&host.database, &version)?;
    if font_version(host, &input)? != version {
        return Err(error("NOT_AUTHORIZED"));
    }
    Ok(opened)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::{
        AssetVersion,
        scene::{FontStyle, ResourceOrigin},
    };

    fn proof(id: &str, content: &str) -> NativeTextCoverage {
        NativeTextCoverage {
            element_id: id.into(),
            content: content.into(),
            font: FontResource {
                id: "fixed-font".into(),
                family: "Fixture".into(),
                weight: 400,
                style: FontStyle::Normal,
                face_index: 0,
                content: AssetVersion {
                    id: "av_fixed".into(),
                    content_hash: "a".repeat(64),
                    size: 100,
                },
                origin: ResourceOrigin::LocalImport {
                    original_name: "fixture.ttf".into(),
                },
                license: None,
            },
        }
    }
    fn keys(current: &[NativeTextCoverage]) -> impl Iterator<Item = (&str, &FontResource, &str)> {
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
