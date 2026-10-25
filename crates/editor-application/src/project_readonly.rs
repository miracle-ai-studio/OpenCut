//! Preservation and refusal are application semantics, not a renderer fallback.
use crate::scene::{SceneRead, StoredProjectDocument};
pub use opencut_contracts::project_readonly::*;
use opencut_contracts::{EditorError, project_inspection::InspectProject, scene::*};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredReadOnlySnapshot {
    pub document: ReadOnlyProjectDocument,
    pub revision: String,
}
pub struct PreparedReadOnlyCreate {
    snapshot: StoredReadOnlySnapshot,
    request_json: String,
    event_id: String,
}
impl PreparedReadOnlyCreate {
    pub fn snapshot(&self) -> &StoredReadOnlySnapshot {
        &self.snapshot
    }
    pub fn request_json(&self) -> &str {
        &self.request_json
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
}
pub trait ReadOnlyProjectStore: SceneRead {
    fn authorize_readonly_create(&self, input: &CreateReadOnlyProject) -> Result<(), EditorError>;
    fn replay_readonly_create(
        &self,
        input: &CreateReadOnlyProject,
    ) -> Result<Option<StoredReadOnlySnapshot>, EditorError>;
    /// Reauthorize source/owner and fixed document; hold CAS guard, then atomically
    /// publish head/revision/receipt/bindings/event/audit. No intermediate V4 commit.
    fn commit_readonly_create(
        &mut self,
        input: &CreateReadOnlyProject,
        prepared: PreparedReadOnlyCreate,
    ) -> Result<StoredReadOnlySnapshot, EditorError>;
    fn read_readonly_project(
        &self,
        input: &InspectProject,
    ) -> Result<StoredReadOnlySnapshot, EditorError>;
    fn checkpoint_readonly_read(&self, project: &str, revision: &str) -> Result<(), EditorError>;
}
pub trait EffectAvailability {
    fn unavailable_reason(
        &self,
        plugin: &EffectPluginRef,
    ) -> Result<UnavailableEffectReason, EditorError>;
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
pub fn export_unavailable(document: &ReadOnlyProjectDocument) -> EditorError {
    EditorError::new("MISSING_PLUGIN_DEPENDENCY", "Required effect plugins are unavailable; this preserved project is read-only and cannot be exported")
        .with_details(serde_json::json!({"read_only":true,"inspect_tool":"opencut_project_inspect","plugins":document.opaque_effects.iter().map(|e|&e.plugin).collect::<Vec<_>>()}))
}
pub fn readonly_error() -> EditorError {
    EditorError::new(
        "PROJECT_READ_ONLY",
        "Unavailable effect data is preserved; editing requires supported effect dependencies and an explicit conversion",
    )
}
pub fn validate_document(document: &ReadOnlyProjectDocument) -> Result<(), EditorError> {
    if document.schema_version != 5
        || document.id != document.base.id
        || document.name != document.base.name
        || !id(&document.id)
        || !id(&document.name)
        || document.opaque_effects.is_empty()
        || document.opaque_effects.len() > 32
    {
        return Err(invalid("Invalid read-only preservation document"));
    }
    opencut_editor_core::scene::validate_project_scene(&document.base)?;
    let mut identifiers = BTreeSet::new();
    identifiers.insert(document.base.id.as_str());
    for asset in &document.base.assets {
        identifiers.insert(asset.id.as_str());
    }
    for font in &document.base.fonts {
        identifiers.insert(font.id.as_str());
    }
    let mut targets = BTreeMap::new();
    for scene in &document.base.sequences {
        identifiers.insert(scene.id.as_str());
        for caption in &scene.captions {
            identifiers.insert(caption.id.as_str());
        }
        for transition in &scene.transitions {
            identifiers.insert(transition.id.as_str());
        }
        for bookmark in &scene.bookmarks {
            identifiers.insert(bookmark.id.as_str());
        }
        for track in &scene.tracks {
            identifiers.insert(track.id.as_str());
            for element in &track.elements {
                identifiers.insert(element.id.as_str());
                if let ElementContent::EffectLayer { effect } = &element.content {
                    identifiers.insert(effect.id.as_str());
                }
                if let Some(visual) = &element.visual {
                    for effect in &visual.effects {
                        identifiers.insert(effect.id.as_str());
                    }
                    for mask in &visual.masks {
                        identifiers.insert(mask.id.as_str());
                    }
                    targets.insert(
                        (scene.id.as_str(), track.id.as_str(), element.id.as_str()),
                        visual.effects.len(),
                    );
                }
            }
        }
    }
    let mut occupied = BTreeSet::new();
    let mut total = 0usize;
    for effect in &document.opaque_effects {
        total = total.saturating_add(effect.payload_json.len());
        if !id(&effect.id)
            || !identifiers.insert(effect.id.as_str())
            || effect.payload_json.len() > 64 * 1024
            || total > 512 * 1024
        {
            return Err(invalid("Opaque effects exceed identity or payload limits"));
        }
        let key = (
            effect.target.sequence_id.as_str(),
            effect.target.track_id.as_str(),
            effect.target.element_id.as_str(),
        );
        let known = *targets
            .get(&key)
            .ok_or_else(|| invalid("Opaque effect target is not a fixed visual element"))?;
        let extras = document
            .opaque_effects
            .iter()
            .filter(|e| e.target == effect.target)
            .count();
        if effect.position as usize >= known + extras || !occupied.insert((key, effect.position)) {
            return Err(invalid(
                "Opaque effect positions must preserve an unambiguous combined stack",
            ));
        }
        crate::maintenance::validate_object(
            &crate::maintenance::plugin_publication::Object::PluginPackage {
                plugin_id: effect.plugin.id.clone(),
                version: effect.plugin.version.clone(),
                package_sha256: effect.plugin.package_sha256.clone(),
            },
        )?;
        if effect.plugin.contribution.len() > 512
            || !effect.plugin.contribution.split('.').all(|s| {
                !s.is_empty()
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            })
            || !effect
                .plugin
                .contribution
                .starts_with(&format!("{}.", effect.plugin.id))
        {
            return Err(invalid("Invalid effect contribution identity"));
        }
        fn bounded(value: &Value, depth: usize) -> bool {
            depth <= 64
                && match value {
                    Value::Array(xs) => xs.iter().all(|x| bounded(x, depth + 1)),
                    Value::Object(xs) => xs.values().all(|x| bounded(x, depth + 1)),
                    _ => true,
                }
        }
        let value: Value = serde_json::from_str(&effect.payload_json)
            .map_err(|_| invalid("Opaque effect payload must be valid JSON"))?;
        if !bounded(&value, 0) {
            return Err(invalid("Opaque effect JSON exceeds nesting limit"));
        }
    }
    if serde_json::to_vec(document)
        .map_err(|_| invalid("Cannot serialize read-only document"))?
        .len()
        > 4 * 1024 * 1024
    {
        return Err(invalid("Read-only document exceeds preservation budget"));
    }
    Ok(())
}
pub fn revision(document: &ReadOnlyProjectDocument) -> Result<String, EditorError> {
    Ok(crate::content_id(
        "rev",
        crate::encode_json(document, "preserved project")?.as_bytes(),
    ))
}
pub fn prepare(
    input: &CreateReadOnlyProject,
    source: EditorSnapshot,
) -> Result<PreparedReadOnlyCreate, EditorError> {
    crate::require_idempotency_key(&input.idempotency_key)?;
    if source.project.id != input.source_project_id || source.revision != input.source_revision {
        return Err(invalid("Read-only source differs from fixed request"));
    }
    let mut base = source.project;
    base.id = input.project_id.clone();
    base.name = input.name.clone();
    let document = ReadOnlyProjectDocument {
        schema_version: 5,
        id: input.project_id.clone(),
        name: input.name.clone(),
        base,
        opaque_effects: input.opaque_effects.clone(),
    };
    validate_document(&document)?;
    let revision = revision(&document)?;
    Ok(PreparedReadOnlyCreate {
        snapshot: StoredReadOnlySnapshot { document, revision },
        request_json: crate::encode_json(input, "read-only creation request")?,
        event_id: format!("evt_{}", uuid::Uuid::new_v4().simple()),
    })
}
pub fn create(
    store: &mut impl ReadOnlyProjectStore,
    input: &CreateReadOnlyProject,
) -> Result<StoredReadOnlySnapshot, EditorError> {
    crate::require_idempotency_key(&input.idempotency_key)?;
    store.authorize_readonly_create(input)?;
    if let Some(previous) = store.replay_readonly_create(input)? {
        return Ok(previous);
    }
    let source = store.revision_scene_source(&input.source_project_id, &input.source_revision)?;
    let StoredProjectDocument::Editable(project) = source.document else {
        return Err(invalid(
            "Preservation requires a fixed V4 source; unknown nodes cannot be silently projected",
        ));
    };
    let prepared = prepare(
        input,
        EditorSnapshot {
            project: *project,
            revision: source.revision,
        },
    )?;
    store.commit_readonly_create(input, prepared)
}
pub fn manifest(snapshot: &StoredReadOnlySnapshot) -> Result<ReadOnlyProjectManifest, EditorError> {
    validate_document(&snapshot.document)?;
    let d = &snapshot.document;
    let known_content = crate::scene::public::project(&EditorSnapshot {
        project: d.base.clone(),
        revision: snapshot.revision.clone(),
    })
    .project;
    Ok(ReadOnlyProjectManifest {
        schema_version: 1,
        project_id: d.id.clone(),
        name: d.name.clone(),
        revision: snapshot.revision.clone(),
        document_schema_version: 5,
        known_content,
        opaque_effects: d.opaque_effects.clone(),
    })
}
pub fn inspect(
    store: &impl ReadOnlyProjectStore,
    availability: &impl EffectAvailability,
    input: &InspectProject,
) -> Result<ReadOnlyProjectView, EditorError> {
    let snapshot = store.read_readonly_project(input)?;
    describe(&snapshot, availability, &mut || {
        store.checkpoint_readonly_read(&snapshot.document.id, &snapshot.revision)
    })
}
pub fn describe(
    snapshot: &StoredReadOnlySnapshot,
    availability: &impl EffectAvailability,
    checkpoint: &mut dyn FnMut() -> Result<(), EditorError>,
) -> Result<ReadOnlyProjectView, EditorError> {
    let file = manifest(&snapshot)?;
    let mut effects = Vec::new();
    for effect in &file.opaque_effects {
        checkpoint()?;
        effects.push(ReadOnlyEffect {
            id: effect.id.clone(),
            target: effect.target.clone(),
            position: effect.position,
            enabled: effect.enabled,
            plugin: effect.plugin.clone(),
            payload_sha256: format!("{:x}", Sha256::digest(effect.payload_json.as_bytes())),
            payload_size_bytes: effect.payload_json.len() as u32,
            unavailable_reason: availability.unavailable_reason(&effect.plugin)?,
        });
    }
    checkpoint()?;
    Ok(ReadOnlyProjectView {
        project_id: file.project_id,
        name: file.name,
        revision: file.revision,

        read_only: true,
        known_content: file.known_content,
        effects,
        export_failure_code: "MISSING_PLUGIN_DEPENDENCY".into(),
    })
}
