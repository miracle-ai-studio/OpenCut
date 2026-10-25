use super::*;

fn fixed_project(
    store: &impl SceneRead,
    project_id: &str,
    revision: &str,
) -> Result<EditorDocument, EditorError> {
    if revision.trim().is_empty() {
        return Err(invalid("Resource selector requires a fixed revision"));
    }
    let source = store.revision_scene_source(project_id, revision)?;
    if source.revision != revision || source.document.project_id() != project_id {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Resource selector revision scope differs",
        ));
    }
    let StoredProjectDocument::Editable(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Resource selector requires a V4 revision",
        ));
    };
    validate_project_scene(&project)?;
    Ok(*project)
}
fn operation(
    store: &impl ProjectPublicResources,
    project_id: &str,
    value: &ProjectEditOperation,
) -> Result<ProjectOperation, EditorError> {
    use ProjectEditOperation as P;
    use ProjectOperation as I;
    Ok(match value {
        P::AssetBind { id, name, source } => {
            let mut asset = match source {
                AssetSelector::ProjectAsset { revision, asset_id } => {
                    fixed_project(store, project_id, revision)?
                        .assets
                        .into_iter()
                        .find(|a| a.id == *asset_id)
                        .ok_or_else(|| missing("selected asset"))?
                }
                AssetSelector::ProjectMedia { media_id, version } => {
                    let asset =
                        store.authorized_project_media_scene(project_id, media_id, version)?;
                    if asset.version != *version {
                        return Err(EditorError::new(
                            "STORAGE_ERROR",
                            "Authorized media returned another immutable identity",
                        ));
                    }
                    asset
                }
            };
            asset.id = id.clone();
            asset.name = name.clone();
            I::AssetPut { asset }
        }
        P::FontBind {
            id,
            source: FontSelector::ProjectFont { revision, font_id },
        } => {
            let mut font = fixed_project(store, project_id, revision)?
                .fonts
                .into_iter()
                .find(|f| f.id == *font_id)
                .ok_or_else(|| missing("selected font"))?;
            font.id = id.clone();
            I::FontPut { font }
        }
        P::Rename { name } => I::Rename { name: name.clone() },
        P::SettingsSet { settings } => I::SettingsSet {
            settings: settings.clone(),
        },
        P::DefaultSequenceSet { sequence_id } => I::DefaultSequenceSet {
            sequence_id: sequence_id.clone(),
        },
        P::AssetDelete { asset_id } => I::AssetDelete {
            asset_id: asset_id.clone(),
        },
        P::FontDelete { font_id } => I::FontDelete {
            font_id: font_id.clone(),
        },
        P::SequencePut { sequence } => I::SequencePut {
            sequence: sequence.clone(),
        },
        P::SequenceDelete { sequence_id } => I::SequenceDelete {
            sequence_id: sequence_id.clone(),
        },
        P::TrackPut { sequence_id, track } => I::TrackPut {
            sequence_id: sequence_id.clone(),
            track: track.clone(),
        },
        P::TrackDelete {
            sequence_id,
            track_id,
        } => I::TrackDelete {
            sequence_id: sequence_id.clone(),
            track_id: track_id.clone(),
        },
        P::TrackLockSet {
            sequence_id,
            track_id,
            locked,
        } => I::TrackLockSet {
            sequence_id: sequence_id.clone(),
            track_id: track_id.clone(),
            locked: *locked,
        },
        P::ElementPut {
            sequence_id,
            track_id,
            element,
        } => I::ElementPut {
            sequence_id: sequence_id.clone(),
            track_id: track_id.clone(),
            element: element.clone(),
        },
        P::ElementDelete {
            sequence_id,
            element_id,
        } => I::ElementDelete {
            sequence_id: sequence_id.clone(),
            element_id: element_id.clone(),
        },
        P::CaptionPut {
            sequence_id,
            caption,
        } => I::CaptionPut {
            sequence_id: sequence_id.clone(),
            caption: (**caption).clone(),
        },
        P::CaptionDelete {
            sequence_id,
            caption_id,
        } => I::CaptionDelete {
            sequence_id: sequence_id.clone(),
            caption_id: caption_id.clone(),
        },
        P::TransitionPut {
            sequence_id,
            transition,
        } => I::TransitionPut {
            sequence_id: sequence_id.clone(),
            transition: transition.clone(),
        },
        P::TransitionDelete {
            sequence_id,
            transition_id,
        } => I::TransitionDelete {
            sequence_id: sequence_id.clone(),
            transition_id: transition_id.clone(),
        },
        P::Bookmark { operation } => I::Bookmark {
            operation: operation.clone(),
        },
    })
}
fn proposal(
    store: &impl ProjectPublicResources,
    source: &StoredProjectRevision,
    request: &ApplyProject,
) -> Result<EditorDocument, EditorError> {
    // Bound work before resource reads, using the same edit limits as plan().
    if request.operations.is_empty()
        || request.operations.len() > 128
        || request.label.trim().is_empty()
        || request.label.len() > 4096
    {
        return Err(invalid(
            "Choose 1..128 operations and a label of 1..4096 bytes",
        ));
    }
    let StoredProjectDocument::Editable(project) = &source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Promote before applying public V4 edits",
        ));
    };
    let resolved = ApplySceneProject {
        project_id: request.project_id.clone(),
        expected_revision: request.expected_revision.clone(),
        idempotency_key: request.idempotency_key.clone(),
        label: request.label.clone(),
        operations: request
            .operations
            .iter()
            .map(|value| operation(store, &request.project_id, value))
            .collect::<Result<_, _>>()?,
    };
    super::super::plan(
        &EditorSnapshot {
            project: (**project).clone(),
            revision: source.revision.clone(),
        },
        &resolved,
    )
}
/// Authorized fixed-head proposal. This returns complete public editor data,
/// not compiled output or proof of rendering support. It publishes no receipt.
pub fn plan_public(
    store: &impl ProjectPublicResources,
    request: &ApplyProject,
) -> Result<PublicProjectDocument, EditorError> {
    Ok(project(&plan_public_snapshot(store, request)?).project)
}
/// Internal proposal used by composable planners; private resources never enter
/// the returned public transaction or the transport-facing plan result.
pub(crate) fn plan_public_snapshot(
    store: &impl ProjectPublicResources,
    request: &ApplyProject,
) -> Result<EditorSnapshot, EditorError> {
    require_idempotency_key(&request.idempotency_key)?;
    let intent = SceneCommitIntent::PublicApply(request);
    store.authorize_plan(intent)?;
    let source = persistence::checked_head(store, intent)?;
    let proposed = proposal(store, &source, request)?;
    Ok(EditorSnapshot {
        project: proposed,
        revision: source.revision,
    })
}
pub fn apply_public(
    store: &mut (impl SceneStore + ProjectPublicResources),
    request: &ApplyProject,
) -> Result<TransactionResult, EditorError> {
    let intent = SceneCommitIntent::PublicApply(request);
    let source = match persistence::begin(store, intent)? {
        Ok(source) => source,
        Err(replay) => return Ok(replay),
    };
    let proposed = proposal(store, &source, request)?;
    store.commit_scene_atomic(intent, persistence::prepare(&source, proposed, intent)?)
}
