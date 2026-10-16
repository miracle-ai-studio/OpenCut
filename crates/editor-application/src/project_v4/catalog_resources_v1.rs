//! Single original-intent transaction, with acquisition skipped on replay.
use super::*;
use crate::catalog_resources_v1::{CatalogSoundAcquireV1, bind_resource, validate_request};
use opencut_contracts::catalog_resources_v1::ImportCatalogResourceV1;
pub fn import(
    store: &mut (impl ProjectV4Store + public::ProjectV4PublicResources),
    native: &impl CatalogSoundAcquireV1,
    request: &ImportCatalogResourceV1,
    checkpoint: &mut dyn crate::font_import_v1::FontImportCheckpointV1,
) -> Result<TransactionResult, EditorError> {
    validate_request(request)?;
    let intent = V4Intent::CatalogImport(request);
    let source = match persistence::begin(store, intent)? {
        Ok(source) => source,
        Err(receipt) => return Ok(receipt),
    };
    let StoredProjectDocumentV4::V4(project) = &source.document else {
        return Err(invalid("Catalog resources require a V4 project"));
    };
    if project.assets.iter().any(|a| a.id == request.asset_id) {
        return Err(invalid("Catalog asset identity already exists"));
    }
    checkpoint.check()?;
    let (asset, _publication) = if let Some(uploaded) = &request.uploaded {
        (
            store.authorized_project_media_v4(
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
        &ProjectSnapshotV4 {
            project: (**project).clone(),
            revision: source.revision.clone(),
        },
        &ApplyV4Project {
            project_id: request.project_id.clone(),
            expected_revision: request.expected_revision.clone(),
            idempotency_key: request.idempotency_key.clone(),
            label: format!("Acquire {}", asset.name),
            operations: vec![ProjectOperationV4::AssetPut { asset }],
        },
    )?;
    let prepared = persistence::prepare(&source, proposed, intent)?;
    checkpoint.check()?;
    store.commit_v4_atomic(intent, prepared)
}

pub fn generate(
    store: &mut impl ProjectV4Store,
    native: &impl crate::catalog_resources_v1::CatalogSoundGenerateV1,
    request: &opencut_contracts::catalog_resources_v1::GenerateCatalogSoundV1,
    checkpoint: &mut dyn crate::font_import_v1::FontImportCheckpointV1,
) -> Result<TransactionResult, EditorError> {
    crate::catalog_resources_v1::validate_generate(request)?;
    let intent = V4Intent::CatalogGenerate(request);
    let source = match persistence::begin(store, intent)? {
        Ok(source) => source,
        Err(receipt) => return Ok(receipt),
    };
    let StoredProjectDocumentV4::V4(project) = &source.document else {
        return Err(invalid("Local sounds require a V4 project"));
    };
    if project.assets.iter().any(|a| a.id == request.asset_id) {
        return Err(invalid("Local sound asset identity already exists"));
    }
    checkpoint.check()?;
    let acquired = native.generate(&request.recipe, checkpoint)?;
    let asset = crate::catalog_resources_v1::bind_generated(request, acquired.resource)?;
    let _publication = acquired.lease;
    checkpoint.check()?;
    let proposed = plan(
        &ProjectSnapshotV4 {
            project: (**project).clone(),
            revision: source.revision.clone(),
        },
        &ApplyV4Project {
            project_id: request.project_id.clone(),
            expected_revision: request.expected_revision.clone(),
            idempotency_key: request.idempotency_key.clone(),
            label: format!("Generate {}", asset.name),
            operations: vec![ProjectOperationV4::AssetPut { asset }],
        },
    )?;
    let prepared = persistence::prepare(&source, proposed, intent)?;
    checkpoint.check()?;
    store.commit_v4_atomic(intent, prepared)
}
