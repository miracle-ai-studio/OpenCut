//! Read-only native V4 media planning. Authorized observations supply metadata;
//! final public selectors are re-resolved by the ordinary atomic apply use case.
use crate::scene::{SceneCommitIntent, StoredProjectDocument, public::ProjectPublicResources};
use opencut_contracts::{
    AssetKind, EditorError, FrameRate, Time, TimeRange, media_import::*, project_public::*,
    scene::*, scene_ops::*,
};
use opencut_time::RationalTime;
use std::collections::{BTreeMap, BTreeSet};

mod placement;
mod removal;
mod settings;
mod workspace;
pub use removal::plan_removal;
#[cfg(test)]
mod tests;

fn invalid(message: impl Into<String>) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn missing(message: impl Into<String>) -> EditorError {
    EditorError::new("NOT_FOUND", message)
}
fn time(value: &Time) -> Result<RationalTime, EditorError> {
    RationalTime::try_from(value)
}

fn validate(input: &MediaImportRequest) -> Result<(), EditorError> {
    crate::require_idempotency_key(&input.idempotency_key)?;
    if [
        &input.project_id,
        &input.expected_revision,
        &input.sequence_id,
        &input.label,
    ]
    .iter()
    .any(|s| s.trim().is_empty())
        || input.label.len() > 4096
    {
        return Err(invalid(
            "Import needs project, fixed revision, sequence and label",
        ));
    }
    if input.bindings.len() > 60
        || input.placements.len() > 60
        || (input.bindings.is_empty() && input.placements.is_empty())
    {
        return Err(invalid(
            "Import needs a binding or placement, with at most 60 of each",
        ));
    }
    let mut assets = BTreeSet::new();
    let mut media = BTreeSet::new();
    let mut elements = BTreeSet::new();
    for b in &input.bindings {
        if b.media_id.trim().is_empty()
            || b.asset_id.trim().is_empty()
            || b.name.trim().is_empty()
            || !assets.insert(&b.asset_id)
            || !media.insert(&b.media_id)
        {
            return Err(invalid(
                "Import bindings need distinct media and logical asset IDs",
            ));
        }
    }
    for p in &input.placements {
        if p.element_id.trim().is_empty()
            || p.name.trim().is_empty()
            || p.track_id.trim().is_empty()
            || !elements.insert(&p.element_id)
            || p.asset_id.trim().is_empty()
        {
            return Err(invalid(
                "Placement needs a distinct new element, explicit track and asset",
            ));
        }
    }
    Ok(())
}

/// Plans against the current fixed head, with current Read/Edit/write-control
/// checks. No mutation, receipt, locator capture, or V3 projection occurs here.
pub fn plan(
    store: &impl ProjectPublicResources,
    input: &MediaImportRequest,
) -> Result<MediaImportPlan, EditorError> {
    validate(input)?;
    workspace::validate_tracks(input)?;
    let mut transaction = ApplyProject {
        branch_name: None,
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: input.label.clone(),
        operations: input
            .bindings
            .iter()
            .map(|b| ProjectEditOperation::AssetBind {
                id: b.asset_id.clone(),
                name: b.name.clone(),
                source: AssetSelector::ProjectMedia {
                    media_id: b.media_id.clone(),
                    version: b.version.clone(),
                },
            })
            .collect(),
    };
    store.authorize_project(SceneCommitIntent::PublicApply(&transaction))?;
    let source = store.head_scene_source(&input.project_id)?;
    if source.document.project_id() != input.project_id {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Import head scope differs",
        ));
    }
    if source.revision != input.expected_revision {
        return Err(crate::revision_conflict(
            &input.expected_revision,
            &source.revision,
        ));
    }
    let StoredProjectDocument::Editable(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Native import requires a V4 head",
        ));
    };
    opencut_editor_core::scene::validate_project_scene(&project)?;
    let sequence = project
        .sequences
        .iter()
        .find(|s| s.id == input.sequence_id)
        .ok_or_else(|| missing("Import sequence does not exist"))?;
    let mut assets = BTreeMap::new();
    let mut packet_rates = BTreeMap::new();
    let mut internal = Vec::new();
    let mut bound = Vec::new();
    let mut replaced = Vec::new();
    for b in &input.bindings {
        let mut asset =
            store.authorized_project_media_scene(&input.project_id, &b.media_id, &b.version)?;
        if asset.version != b.version {
            return Err(EditorError::new(
                "STORAGE_ERROR",
                "Authorized media returned another version",
            ));
        }
        if asset.kind == AssetKind::Video
            && let Some(observation) = store.authorized_project_media_packet_rate_scene(
                &input.project_id,
                &b.media_id,
                &b.version,
            )?
        {
            observation.validate().map_err(invalid)?;
            packet_rates.insert(b.asset_id.clone(), observation);
        }
        let existing = project.assets.iter().find(|a| a.id == b.asset_id);
        match (b.mode, existing) {
            (MediaBindingMode::Bind, Some(_)) => {
                return Err(invalid("Bind cannot replace an existing logical asset"));
            }
            (MediaBindingMode::Replace, None) => {
                return Err(missing("Replacement asset does not exist"));
            }
            (MediaBindingMode::Replace, Some(old)) if old.kind != asset.kind => {
                return Err(invalid("Replacement asset must preserve its kind"));
            }
            (MediaBindingMode::Bind, None) => bound.push(b.asset_id.clone()),
            (MediaBindingMode::Replace, Some(_)) => replaced.push(b.asset_id.clone()),
        }
        asset.id = b.asset_id.clone();
        asset.name = b.name.clone();
        internal.push(ProjectOperation::AssetPut {
            asset: asset.clone(),
        });
        assets.insert(b.asset_id.clone(), asset);
    }
    let mut placement_assets: BTreeMap<_, _> = project
        .assets
        .iter()
        .map(|asset| (asset.id.clone(), asset.clone()))
        .collect();
    placement_assets.extend(assets.clone());
    if let Some(first) = input.placements.first()
        && matches!(first.content, MediaPlacementContent::Video { .. })
        && !assets.contains_key(&first.asset_id)
        && placement_assets.contains_key(&first.asset_id)
        && let Some(observation) = store.project_asset_packet_rate_scene(
            &input.project_id,
            &input.expected_revision,
            &first.asset_id,
        )?
    {
        observation.validate().map_err(invalid)?;
        packet_rates.insert(first.asset_id.clone(), observation);
    }
    let mut tracks = sequence.tracks.clone();
    for declaration in &input.new_tracks {
        if project
            .sequences
            .iter()
            .flat_map(|s| &s.tracks)
            .any(|t| t.id == declaration.id)
        {
            return Err(invalid(
                "New track cannot replace any existing project track",
            ));
        }
        let track = workspace::new_track(declaration);
        tracks.push(track.clone());
        transaction.operations.push(ProjectEditOperation::TrackPut {
            sequence_id: input.sequence_id.clone(),
            track: track.clone(),
        });
        internal.push(ProjectOperation::TrackPut {
            sequence_id: input.sequence_id.clone(),
            track,
        });
    }
    let (next_settings, mut decisions) = settings::decide(
        &project,
        sequence,
        input,
        &assets,
        &placement_assets,
        &packet_rates,
    )?;
    decisions.bound_asset_ids = bound;
    decisions.replaced_asset_ids = replaced;
    if next_settings != project.settings {
        transaction
            .operations
            .push(ProjectEditOperation::SettingsSet {
                settings: next_settings.clone(),
            });
        internal.push(ProjectOperation::SettingsSet {
            settings: next_settings,
        });
    }
    let binding_fps = decisions
        .raised_fps
        .as_ref()
        .unwrap_or(&decisions.previous_fps);
    let classic = matches!(project.settings, SettingsProfile::ClassicGlobal { .. });
    let mut occupied = tracks
        .iter()
        .map(|t| (t.id.clone(), t.elements.clone()))
        .collect::<BTreeMap<_, _>>();
    for (placement_index, p) in input.placements.iter().enumerate() {
        if project
            .sequences
            .iter()
            .flat_map(|s| &s.tracks)
            .flat_map(|t| &t.elements)
            .any(|e| e.id == p.element_id)
        {
            return Err(invalid("Media placement may only create a new element"));
        }
        let track = tracks
            .iter()
            .find(|t| t.id == p.track_id)
            .ok_or_else(|| missing("Explicit placement track does not exist"))?;
        let asset = placement_assets
            .get(&p.asset_id)
            .ok_or_else(|| missing("Placement binding does not exist"))?;
        let (element, decision) = placement::build(
            p,
            track,
            &occupied[&track.id],
            &sequence.main_track_id,
            asset,
            if placement_index == 0 {
                binding_fps
            } else {
                decisions.first_visual_fps.as_ref().unwrap_or(binding_fps)
            },
            classic,
        )?;
        occupied
            .get_mut(&track.id)
            .expect("existing track")
            .push(element.clone());
        decisions.placements.push(decision);
        transaction
            .operations
            .push(ProjectEditOperation::ElementPut {
                sequence_id: input.sequence_id.clone(),
                track_id: track.id.clone(),
                element: Box::new(element.clone()),
            });
        internal.push(ProjectOperation::ElementPut {
            sequence_id: input.sequence_id.clone(),
            track_id: track.id.clone(),
            element: Box::new(element),
        });
    }
    // The shared aggregate planner checks dependency, lock, transition, channel,
    // font, time, collision and profile invariants on the complete proposal.
    crate::scene::plan(
        &EditorSnapshot {
            project: *project,
            revision: source.revision,
        },
        &ApplySceneProject {
            project_id: transaction.project_id.clone(),
            expected_revision: transaction.expected_revision.clone(),
            idempotency_key: transaction.idempotency_key.clone(),
            label: transaction.label.clone(),
            operations: internal,
        },
    )?;
    store.authorize_project(SceneCommitIntent::PublicApply(&transaction))?;
    Ok(MediaImportPlan {
        original_request_digest: crate::identity::request_identity(
            "media_import_plan",
            &input.project_id,
            input,
        )?
        .current_digest()
        .into(),
        transaction,
        decisions,
    })
}
