//! One data-package planner returns an ordinary complete editor transaction.
use super::*;
use crate::scene::{self, SceneCommitIntent, StoredProjectDocument};
use opencut_contracts::{
    project_public::{ApplyProject, ProjectEditOperation},
    scene::*,
};
use opencut_editor_core::scene::legacy::fragments::FragmentConverter;
use std::collections::{BTreeMap, HashSet};

fn create_id(ids: &mut HashSet<String>, id: &str) -> Result<(), EditorError> {
    if !ids.insert(id.into()) {
        return Err(EditorError::new(
            "ALREADY_EXISTS",
            "Preset insertion identity already exists",
        ));
    }
    Ok(())
}
fn kind(kind: SceneTrackKind) -> Result<opencut_contracts::TrackKind, EditorError> {
    use opencut_contracts::TrackKind as Historical;
    match kind {
        SceneTrackKind::Video => Ok(Historical::Video),
        SceneTrackKind::Audio => Ok(Historical::Audio),
        SceneTrackKind::Graphic => Ok(Historical::Graphic),
        _ => Err(invalid("Template clip requires a media track")),
    }
}
fn caption(
    converter: &mut FragmentConverter,
    project: &EditorDocument,
    input: &PresetPlanInput,
    cue: &opencut_contracts::CaptionCue,
    track: &str,
    used_fonts: &mut BTreeSet<String>,
    normalizations: &mut BTreeSet<String>,
) -> Result<Caption, EditorError> {
    let family = &cue.style.font_family;
    let font_id = input.font_bindings.get(family).ok_or_else(|| {
        EditorError::new(
            "FONT_BINDING_REQUIRED",
            "Preset captions require an explicit imported font binding",
        )
        .with_details(serde_json::json!({"family": family}))
    })?;
    if !project.fonts.iter().any(|font| &font.id == font_id) {
        return Err(EditorError::new(
            "NOT_FOUND",
            "Bound preset font is absent from the fixed revision",
        ));
    }
    let mut caption = converter.caption(cue, track)?;
    caption.style.font = FontBinding::Locked {
        font_id: font_id.clone(),
    };
    caption.style.layout = CaptionLayoutProfile::PinnedCaptionV4;
    used_fonts.insert(family.clone());
    normalizations.insert(format!("Caption family {family} explicitly binds imported font {font_id};pinned caption layout is used"));
    Ok(caption)
}
fn operations(
    project: &EditorDocument,
    input: &PresetPlanInput,
    expanded: &[Operation],
) -> Result<(Vec<ProjectEditOperation>, Vec<String>), EditorError> {
    let mut converter = FragmentConverter::new(project, expanded)?;
    let mut ids = converter.existing_identities().clone();
    let mut tracks: BTreeMap<_, _> = project
        .sequences
        .iter()
        .flat_map(|scene| {
            scene.tracks.iter().map(move |track| {
                (
                    (scene.id.clone(), track.id.clone()),
                    (track.kind, track.locked),
                )
            })
        })
        .collect();
    let mut used_fonts = BTreeSet::new();
    let mut used_tracks = BTreeSet::new();
    let mut normalizations = BTreeSet::new();
    let mut result = Vec::with_capacity(expanded.len());
    for operation in expanded {
        result.push(match operation {
            Operation::AddTrack { sequence_id, track } => {
                create_id(&mut ids, &track.id)?;
                let converted = converter.track(sequence_id, track)?;
                tracks.insert((sequence_id.clone(), track.id.clone()), (converted.kind, converted.locked));
                ProjectEditOperation::TrackPut { sequence_id: sequence_id.clone(), track: converted }
            }
            Operation::InsertClip { sequence_id, track_id, clip } => {
                create_id(&mut ids, &clip.id)?;
                let (track_kind, locked) = tracks.get(&(sequence_id.clone(), track_id.clone())).copied()
                    .ok_or_else(|| invalid("Template clip target track does not exist"))?;
                if locked { return Err(EditorError::new("TRACK_LOCKED", "Template clip target track is locked")); }
                let element = converter.element(sequence_id, track_id, kind(track_kind)?, clip)?;
                if element.visual.is_some() {
                    normalizations.insert("Media contain fit,center-relative pixel transform,curves and exact source clock retain the template's authored values;the current scene renderer explicitly observes and holds the terminal frame at EOF without claiming original FFmpeg pixels".into());
                }
                ProjectEditOperation::ElementPut { sequence_id: sequence_id.clone(), track_id: track_id.clone(), element: Box::new(element) }
            }
            Operation::InsertCaption { sequence_id, cue } => {
                create_id(&mut ids, &cue.id)?;
                let track_id = input.caption_track_bindings.get(&cue.id)
                    .ok_or_else(|| invalid("New preset caption requires an explicit owning track binding"))?;
                let (track_kind, locked) = tracks.get(&(sequence_id.clone(), track_id.clone())).copied()
                    .ok_or_else(|| invalid("Preset caption owning track does not exist"))?;
                if track_kind != SceneTrackKind::Caption { return Err(invalid("Preset caption owner must be a caption track")); }
                if locked { return Err(EditorError::new("TRACK_LOCKED", "Preset caption track is locked")); }
                used_tracks.insert(cue.id.clone());
                ProjectEditOperation::CaptionPut { sequence_id: sequence_id.clone(), caption: Box::new(caption(
                    &mut converter, project, input, cue, track_id, &mut used_fonts, &mut normalizations,
                )?) }
            }
            Operation::UpdateCaption { sequence_id, cue } => {
                let current = project.sequences.iter().find(|scene| &scene.id == sequence_id)
                    .and_then(|scene| scene.captions.iter().find(|caption| caption.id == cue.id))
                    .ok_or_else(|| invalid("Caption style target does not exist"))?;
                let mut updated = current.clone();
                // The style preset does not replace timing,source anchor,ownership
                // or unrelated data from the complete editor caption.
                updated.style = caption(&mut converter, project, input, cue, &current.track_id, &mut used_fonts, &mut normalizations)?.style;
                ProjectEditOperation::CaptionPut { sequence_id: sequence_id.clone(), caption: Box::new(updated) }
            }
            _ => return Err(invalid("Preset expansion escaped the declarative operation allowlist")),
        });
    }
    if input
        .font_bindings
        .keys()
        .any(|family| !used_fonts.contains(family))
        || input
            .caption_track_bindings
            .keys()
            .any(|cue| !used_tracks.contains(cue))
    {
        return Err(invalid("Unused preset font or caption ownership binding"));
    }
    Ok((result, normalizations.into_iter().collect()))
}
pub fn plan(
    store: &(impl PresetGrantStore + scene::public::ProjectPublicResources),
    catalog: &impl PresetCatalog,
    input: PresetPlanInput,
) -> Result<PresetPlan, EditorError> {
    crate::require_idempotency_key(&input.idempotency_key)?;
    if !valid_package_id(&input.target.id)
        || !valid_version(&input.target.version)
        || !valid_name(&input.preset_name)
        || input.label.trim().is_empty()
        || input.label.len() > 4096
        || input.font_bindings.len() > MAX_OPERATIONS
        || input.caption_track_bindings.len() > MAX_OPERATIONS
        || serde_json::to_vec(&input)
            .map_err(|_| invalid("Invalid parameters"))?
            .len()
            > MAX_PARAMETERS_BYTES
    {
        return Err(invalid("Invalid preset plan input"));
    }
    let inspection = PresetInput {
        target: input.target.clone(),
        preset_name: input.preset_name.clone(),
    };
    let ticket = store.authorize_preset(&inspection, true)?;
    let mut transaction = ApplyProject {
        project_id: input.target.project_id.clone(),
        branch_name: input.branch_name.clone(),
        expected_revision: input.revision.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: input.label.clone(),
        operations: vec![],
    };
    store.authorize_plan(SceneCommitIntent::PublicApply(&transaction))?;
    let source =
        scene::persistence::checked_head(store, SceneCommitIntent::PublicApply(&transaction))?;
    if source.revision != input.revision || source.document.project_id() != input.target.project_id
    {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Preset source differs from its fixed project revision",
        ));
    }
    let StoredProjectDocument::Editable(project) = source.document else {
        return Err(EditorError::new(
            "PROJECT_PROMOTION_REQUIRED",
            "Promote the historical project explicitly before planning edits",
        ));
    };
    opencut_editor_core::scene::validate_project_scene(&project)?;
    if serde_json::to_vec(&project)
        .map_err(|_| invalid("Invalid snapshot"))?
        .len()
        > MAX_SNAPSHOT_BYTES
    {
        return Err(invalid("Snapshot exceeds preset limit"));
    }
    let facts = ExpansionFacts {
        assets: project
            .assets
            .iter()
            .map(|asset| (asset.id.clone(), asset.version.clone()))
            .collect(),
        captions: project
            .sequences
            .iter()
            .flat_map(|scene| {
                scene
                    .captions
                    .iter()
                    .map(move |cue| ((scene.id.clone(), cue.id.clone()), cue.clone()))
            })
            .collect(),
    };
    let (declaration, spec) = catalog.load_spec(&inspection, &ticket.grant.package_sha256)?;
    store.revalidate_preset(&inspection, true, &ticket)?;
    let expanded = expand(spec, &input.parameters, &input.asset_bindings, &facts)?;
    let (edits, normalizations) = operations(&project, &input, &expanded)?;
    transaction.operations = edits;
    let preview = scene::public::plan_public(store, &transaction)?;
    store.authorize_plan(SceneCommitIntent::PublicApply(&transaction))?;
    store.revalidate_preset(&inspection, true, &ticket)?;
    Ok(PresetPlan {
        package_sha256: ticket.grant.package_sha256,
        spec_sha256: declaration.spec_sha256,
        grant_revision: ticket.grant.revision,
        transaction,
        preview,
        normalizations,
    })
}
