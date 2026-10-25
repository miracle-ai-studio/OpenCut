//! Original brand import intent; no new resource store or alternate commit path.
use super::*;
use opencut_contracts::brand_resources::ImportBrandResource;
pub fn import(
    store: &mut (impl SceneStore + public::ProjectPublicResources),
    request: &ImportBrandResource,
    checkpoint: &mut dyn crate::font_import::FontImportCheckpoint,
) -> Result<TransactionResult, EditorError> {
    crate::brand_resources::validate_request(request)?;
    let intent = SceneCommitIntent::BrandImport(request);
    let source = match persistence::begin(store, intent)? {
        Ok(source) => source,
        Err(receipt) => return Ok(receipt),
    };
    let StoredProjectDocument::Editable(project) = &source.document else {
        return Err(invalid("Brand acquisition requires an editable project"));
    };
    if project.assets.iter().any(|a| a.id == request.asset_id) {
        return Err(invalid("Brand resource identity already exists"));
    }
    checkpoint.check()?;
    let observed = store.authorized_project_media_scene(
        &request.project_id,
        &request.uploaded.media_id,
        &request.uploaded.version,
    )?;
    let asset = crate::brand_resources::bind_resource(request, observed)?;
    let proposed = plan(
        &EditorSnapshot {
            project: (**project).clone(),
            revision: source.revision.clone(),
        },
        &ApplySceneProject {
            project_id: request.project_id.clone(),
            expected_revision: request.expected_revision.clone(),
            idempotency_key: request.idempotency_key.clone(),
            label: format!("Acquire brand {}", asset.name),
            operations: vec![ProjectOperation::AssetPut { asset }],
        },
    )?;
    let prepared = persistence::prepare(&source, proposed, intent)?;
    checkpoint.check()?;
    store.commit_scene_atomic(intent, prepared)
}
