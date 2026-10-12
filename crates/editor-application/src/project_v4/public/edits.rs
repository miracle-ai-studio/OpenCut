use super::*;

fn fixed_project(
    store: &impl ProjectV4Read,
    project_id: &str,
    revision: &str,
) -> Result<ProjectDocumentV4, EditorError> {
    if revision.trim().is_empty() {
        return Err(invalid("Resource selector requires a fixed revision"));
    }
    let source = store.revision_v4_source(project_id, revision)?;
    if source.revision != revision || source.document.project_id() != project_id {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Resource selector revision scope differs",
        ));
    }
    let StoredProjectDocumentV4::V4(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Resource selector requires a V4 revision",
        ));
    };
    validate_project_v4(&project)?;
    Ok(*project)
}
fn operation(
    store: &impl ProjectV4PublicResources,
    project_id: &str,
    value: &PublicProjectOperationV4,
) -> Result<ProjectOperationV4, EditorError> {
    use ProjectOperationV4 as I;
    use PublicProjectOperationV4 as P;
    Ok(match value {
        P::AssetBind { id, name, source } => {
            let mut asset = match source {
                PublicAssetSelectorV4::ProjectAsset { revision, asset_id } => {
                    fixed_project(store, project_id, revision)?
                        .assets
                        .into_iter()
                        .find(|a| a.id == *asset_id)
                        .ok_or_else(|| missing("selected asset"))?
                }
                PublicAssetSelectorV4::ProjectMedia { media_id, version } => {
                    let asset = store.authorized_project_media_v4(project_id, media_id, version)?;
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
            source: PublicFontSelectorV4::ProjectFont { revision, font_id },
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
    store: &impl ProjectV4PublicResources,
    source: &StoredProjectRevisionV4,
    request: &ApplyPublicV4Project,
) -> Result<ProjectDocumentV4, EditorError> {
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
    let StoredProjectDocumentV4::V4(project) = &source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Promote before applying public V4 edits",
        ));
    };
    let resolved = ApplyV4Project {
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
        &ProjectSnapshotV4 {
            project: (**project).clone(),
            revision: source.revision.clone(),
        },
        &resolved,
    )
}
/// Authorized fixed-head proposal. This returns complete public editor data,
/// not compiled output or proof of rendering support. It publishes no receipt.
pub fn plan_public(
    store: &impl ProjectV4PublicResources,
    request: &ApplyPublicV4Project,
) -> Result<PublicProjectDocumentV4, EditorError> {
    require_idempotency_key(&request.idempotency_key)?;
    let intent = V4Intent::PublicApply(request);
    store.authorize_v4(intent)?;
    let source = persistence::checked_head(store, intent)?;
    let proposed = proposal(store, &source, request)?;
    Ok(project(&ProjectSnapshotV4 {
        project: proposed,
        revision: source.revision,
    })
    .project)
}
pub fn apply_public(
    store: &mut (impl ProjectV4Store + ProjectV4PublicResources),
    request: &ApplyPublicV4Project,
) -> Result<TransactionResult, EditorError> {
    let intent = V4Intent::PublicApply(request);
    let source = match persistence::begin(store, intent)? {
        Ok(source) => source,
        Err(replay) => return Ok(replay),
    };
    let proposed = proposal(store, &source, request)?;
    store.commit_v4_atomic(intent, persistence::prepare(&source, proposed, intent)?)
}
