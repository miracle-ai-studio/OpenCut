//! Classify the actual immutable source once;publish one safe read view.
use crate::{
    project_readonly::{self, EffectAvailability, StoredReadOnlySnapshot},
    project_resources::ProjectResourceRead,
    scene::public,
};
use opencut_contracts::{
    EditorError, ProjectDocument,
    historical_visual::{HistoricalVisualKeyframeCurve, HistoricalVisualSequenceVisual},
    project_inspection::*,
    project_resources::{InspectResource, ResourceKind, ResourceView},
    scene::EditorSnapshot,
};

pub enum InspectionSource {
    Editor(EditorSnapshot),
    Historical {
        base: ProjectDocument,
        revision: String,
        format: HistoricalContentKind,
        visual: Vec<HistoricalVisualSequenceVisual>,
        curves: Vec<HistoricalVisualKeyframeCurve>,
    },
    Preserved(StoredReadOnlySnapshot),
}
impl InspectionSource {
    fn scope(&self) -> (&str, &str) {
        match self {
            Self::Editor(s) => (&s.project.id, &s.revision),
            Self::Historical { base, revision, .. } => (&base.id, revision),
            Self::Preserved(s) => (&s.document.id, &s.revision),
        }
    }
}
pub trait ProjectInspectionRead: ProjectResourceRead {
    /// Read under current project authority and return the actual stored content.
    /// Native locators stay private;the public projection happens below.
    fn read_inspection_source(
        &self,
        request: &InspectProject,
    ) -> Result<InspectionSource, EditorError>;
}
fn historical(
    base: ProjectDocument,
    revision: String,
    format: HistoricalContentKind,
    visual: Vec<HistoricalVisualSequenceVisual>,
    curves: Vec<HistoricalVisualKeyframeCurve>,
) -> HistoricalProjectSnapshot {
    let project_id = base.id.clone();
    let assets = base
        .assets
        .into_iter()
        .map(|asset| {
            let version = crate::media_facts::legacy_asset_version(&asset)
                .filter(|v| opencut_editor_core::validate_asset_version(v).is_ok());
            let resource = version.as_ref().map(|version| {
                let input = InspectResource {
                    project_id: project_id.clone(),
                    revision: revision.clone(),
                    kind: ResourceKind::Asset,
                    resource_id: asset.id.clone(),
                };
                ResourceView {
                    project_id: input.project_id.clone(),
                    revision: input.revision.clone(),
                    kind: input.kind,
                    resource_id: input.resource_id.clone(),
                    version: version.clone(),
                    mime_type: crate::project_resources::media_mime(
                        asset.metadata.get("mime_type").and_then(|v| v.as_str()),
                    ),
                    content_path: crate::project_resources::content_path(&input),
                }
            });
            // Imported technical metadata is useful to readers;native discovery
            // paths and arbitrary execution metadata are not part of this view.
            let metadata = asset
                .metadata
                .into_iter()
                .filter_map(|(key, value)| safe_metadata(&key, value).map(|value| (key, value)))
                .collect();
            HistoricalAssetView {
                id: asset.id,
                name: asset.name,
                kind: asset.kind,
                uri: resource
                    .as_ref()
                    .map(|r| r.content_path.clone())
                    .unwrap_or_default(),
                duration: asset.duration,
                version,
                metadata,
                resource,
            }
        })
        .collect();
    HistoricalProjectSnapshot {
        project: HistoricalProjectDocument {
            id: base.id,
            name: base.name,
            default_sequence_id: base.default_sequence_id,
            sequences: base.sequences,
            assets,
        },
        revision,
        read_only: true,
        format,
        visual,
        curves,
    }
}
/// Historical metadata was an open map. Project known technical fields through
/// their existing scalar/typed shapes;never forward arbitrary nested values.
fn safe_metadata(key: &str, value: serde_json::Value) -> Option<serde_json::Value> {
    use opencut_contracts::scene::{AudioMetadata, AudioPresence};
    match key {
        "width" | "height" | "size" => value.as_u64().map(serde_json::Value::from),
        "rotation" | "rotation_degrees" => value
            .as_f64()
            .filter(|v| v.is_finite())
            .map(serde_json::Value::from),
        "has_audio" => value.as_bool().map(serde_json::Value::from),
        "frame_rate" => serde_json::from_value::<opencut_contracts::FrameRate>(value)
            .ok()
            .filter(|rate| rate.numerator > 0 && rate.denominator > 0)
            .and_then(|v| serde_json::to_value(v).ok()),
        "audio_metadata" => serde_json::from_value::<AudioMetadata>(value)
            .ok()
            .filter(safe_audio)
            .and_then(|v| serde_json::to_value(v).ok()),
        "audio" => serde_json::from_value::<AudioPresence>(value)
            .ok()
            .filter(|audio| match audio {
                AudioPresence::Present {
                    metadata: Some(metadata),
                } => safe_audio(metadata),
                _ => true,
            })
            .and_then(|v| serde_json::to_value(v).ok()),
        "mime_type" => {
            crate::project_resources::media_mime(value.as_str()).map(serde_json::Value::from)
        }
        "content_hash" => value
            .as_str()
            .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
            .map(|v| v.into()),
        _ => None,
    }
}
fn safe_audio(metadata: &opencut_contracts::scene::AudioMetadata) -> bool {
    metadata.sample_rate > 0
        && metadata.channels > 0
        && metadata.channel_layout.as_ref().is_none_or(|v| {
            v.len() <= 64
                && v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b" ._()+-".contains(&b))
        })
}
pub fn inspect(
    store: &impl ProjectInspectionRead,
    availability: &impl EffectAvailability,
    request: &InspectProject,
) -> Result<ProjectInspection, EditorError> {
    let source = store.read_inspection_source(request)?;
    let (project, revision) = source.scope();
    if project != request.project_id
        || revision.trim().is_empty()
        || request
            .revision
            .as_deref()
            .is_some_and(|expected| expected != revision)
    {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Project inspection scope differs from the requested immutable source",
        ));
    }
    let (project, revision) = (project.to_owned(), revision.to_owned());
    let view = match source {
        InspectionSource::Editor(snapshot) => {
            opencut_editor_core::scene::validate_project_scene(&snapshot.project)?;
            ProjectInspection::Editor {
                snapshot: public::project(&snapshot),
            }
        }
        InspectionSource::Historical {
            base,
            revision,
            format,
            visual,
            curves,
        } => ProjectInspection::Historical {
            snapshot: historical(base, revision, format, visual, curves),
        },
        InspectionSource::Preserved(snapshot) => ProjectInspection::Preserved {
            view: project_readonly::describe(&snapshot, availability, &mut || {
                store.require_resource_revision_read(&project, &revision)
            })?,
        },
    };
    store.require_resource_revision_read(&project, &revision)?;
    Ok(view)
}
