//! Original brand import intent; no new resource store or alternate commit path.
use super::*;
use opencut_contracts::brand_resources_v1::ImportBrandResourceV1;
pub fn import(
    store: &mut (impl ProjectV4Store + public::ProjectV4PublicResources),
    request: &ImportBrandResourceV1,
    checkpoint: &mut dyn crate::font_import_v1::FontImportCheckpointV1,
) -> Result<TransactionResult, EditorError> {
    crate::brand_resources_v1::validate_request(request)?;
    let intent = V4Intent::BrandImport(request);
    let source = match persistence::begin(store, intent)? {
        Ok(source) => source,
        Err(receipt) => return Ok(receipt),
    };
    let StoredProjectDocumentV4::V4(project) = &source.document else {
        return Err(invalid("Brand acquisition requires a V4 project"));
    };
    if project.assets.iter().any(|a| a.id == request.asset_id) {
        return Err(invalid("Brand resource identity already exists"));
    }
    checkpoint.check()?;
    let observed = store.authorized_project_media_v4(
        &request.project_id,
        &request.uploaded.media_id,
        &request.uploaded.version,
    )?;
    let asset = crate::brand_resources_v1::bind_resource(request, observed)?;
    let proposed = plan(
        &ProjectSnapshotV4 {
            project: (**project).clone(),
            revision: source.revision.clone(),
        },
        &ApplyV4Project {
            project_id: request.project_id.clone(),
            expected_revision: request.expected_revision.clone(),
            idempotency_key: request.idempotency_key.clone(),
            label: format!("Acquire brand {}", asset.name),
            operations: vec![ProjectOperationV4::AssetPut { asset }],
        },
    )?;
    let prepared = persistence::prepare(&source, proposed, intent)?;
    checkpoint.check()?;
    store.commit_v4_atomic(intent, prepared)
}
