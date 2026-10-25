//! Single original-intent transaction, with acquisition skipped on replay.
use super::*;
use crate::catalog_resources::{CatalogSoundAcquire, bind_resource, validate_request};
use opencut_contracts::catalog_resources::ImportCatalogResource;
pub fn import(
    store: &mut (impl SceneStore + public::ProjectPublicResources),
    native: &impl CatalogSoundAcquire,
    request: &ImportCatalogResource,
    checkpoint: &mut dyn crate::font_import::FontImportCheckpoint,
) -> Result<TransactionResult, EditorError> {
    validate_request(request)?;
    let intent = SceneCommitIntent::CatalogImport(request);
    let source = match persistence::begin(store, intent)? {
        Ok(source) => source,
        Err(receipt) => return Ok(receipt),
    };
    let StoredProjectDocument::Editable(project) = &source.document else {
        return Err(invalid("Catalog resources require a V4 project"));
    };
    if project.assets.iter().any(|a| a.id == request.asset_id) {
        return Err(invalid("Catalog asset identity already exists"));
    }
    checkpoint.check()?;
    let (asset, _publication) = if let Some(uploaded) = &request.uploaded {
        (
            store.authorized_project_media_scene(
                &request.project_id,
                &uploaded.media_id,
                &uploaded.version,
            )?,
            None,
        )
    } else {
        let acquired = native.acquire(&request.catalog_id, checkpoint)?;
        (acquired.resource, Some(acquired.lease))
    };
    let asset = bind_resource(request, asset)?;
    checkpoint.check()?;
    let proposed = plan(
        &EditorSnapshot {
            project: (**project).clone(),
            revision: source.revision.clone(),
        },
        &ApplySceneProject {
            project_id: request.project_id.clone(),
            expected_revision: request.expected_revision.clone(),
            idempotency_key: request.idempotency_key.clone(),
            label: format!("Acquire {}", asset.name),
            operations: vec![ProjectOperation::AssetPut { asset }],
        },
    )?;
    let prepared = persistence::prepare(&source, proposed, intent)?;
    checkpoint.check()?;
    store.commit_scene_atomic(intent, prepared)
}

pub fn generate(
    store: &mut impl SceneStore,
    native: &impl crate::catalog_resources::CatalogSoundGenerate,
    request: &opencut_contracts::catalog_resources::GenerateCatalogSound,
    checkpoint: &mut dyn crate::font_import::FontImportCheckpoint,
) -> Result<TransactionResult, EditorError> {
    crate::catalog_resources::validate_generate(request)?;
    let intent = SceneCommitIntent::CatalogGenerate(request);
    let source = match persistence::begin(store, intent)? {
        Ok(source) => source,
        Err(receipt) => return Ok(receipt),
    };
    let StoredProjectDocument::Editable(project) = &source.document else {
        return Err(invalid("Local sounds require a V4 project"));
    };
    if project.assets.iter().any(|a| a.id == request.asset_id) {
        return Err(invalid("Local sound asset identity already exists"));
    }
    checkpoint.check()?;
    let acquired = native.generate(&request.recipe, checkpoint)?;
    let asset = crate::catalog_resources::bind_generated(request, acquired.resource)?;
    let _publication = acquired.lease;
    checkpoint.check()?;
    let proposed = plan(
        &EditorSnapshot {
            project: (**project).clone(),
            revision: source.revision.clone(),
        },
        &ApplySceneProject {
            project_id: request.project_id.clone(),
            expected_revision: request.expected_revision.clone(),
            idempotency_key: request.idempotency_key.clone(),
            label: format!("Generate {}", asset.name),
            operations: vec![ProjectOperation::AssetPut { asset }],
        },
    )?;
    let prepared = persistence::prepare(&source, proposed, intent)?;
    checkpoint.check()?;
    store.commit_scene_atomic(intent, prepared)
}
