//! Exact stored shapes inside one archive, never a conversion or fallback.
use super::*;
use opencut_contracts::{
    AssetVersion, ProjectDocument, historical_visual::HistoricalVisualDocument,
    project_readonly::ReadOnlyProjectDocument,
};

#[derive(Clone, Debug, PartialEq)]
pub enum ArchivedDocument {
    Legacy(ProjectDocument),
    HistoricalVisual(HistoricalVisualDocument),
    Editable(EditorDocument),
    ReadOnly(ReadOnlyProjectDocument),
}

impl Serialize for ArchivedDocument {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Legacy(p) => p.serialize(serializer),
            Self::HistoricalVisual(p) => p.serialize(serializer),
            Self::Editable(p) => p.serialize(serializer),
            Self::ReadOnly(p) => p.serialize(serializer),
        }
    }
}
impl<'de> Deserialize<'de> for ArchivedDocument {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        // Choose the original outer shape exactly once. Malformed current
        // content cannot be retried as a simpler historical document.
        match value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
        {
            Some(1 | 2) => serde_json::from_value(value).map(Self::Legacy),
            Some(3) => serde_json::from_value(value).map(Self::HistoricalVisual),
            Some(4) => serde_json::from_value(value).map(Self::Editable),
            Some(5) => serde_json::from_value(value).map(Self::ReadOnly),
            _ => {
                return Err(serde::de::Error::custom(
                    "Unknown stored project document schema",
                ));
            }
        }
        .map_err(serde::de::Error::custom)
    }
}
impl From<EditorDocument> for ArchivedDocument {
    fn from(document: EditorDocument) -> Self {
        Self::Editable(document)
    }
}
impl ArchivedDocument {
    pub fn project_id(&self) -> &str {
        match self {
            Self::Legacy(p) => &p.id,
            Self::HistoricalVisual(p) => &p.base.id,
            Self::Editable(p) => &p.id,
            Self::ReadOnly(p) => &p.id,
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Self::Legacy(p) => &p.name,
            Self::HistoricalVisual(p) => &p.base.name,
            Self::Editable(p) => &p.name,
            Self::ReadOnly(p) => &p.name,
        }
    }
    pub fn editable(&self) -> Result<&EditorDocument, EditorError> {
        match self {
            Self::Editable(p) => Ok(p),
            _ => Err(invalid("Historical action requires an editable document")),
        }
    }
    pub fn editor_content(&self) -> Option<&EditorDocument> {
        match self {
            Self::Editable(p) => Some(p),
            Self::ReadOnly(p) => Some(&p.base),
            _ => None,
        }
    }
    pub fn legacy_content(&self) -> Option<&ProjectDocument> {
        match self {
            Self::Legacy(p) => Some(p),
            Self::HistoricalVisual(p) => Some(&p.base),
            _ => None,
        }
    }
    pub fn fonts(&self) -> &[FontResource] {
        self.editor_content().map_or(&[], |p| p.fonts.as_slice())
    }
    pub fn resources(&self) -> Result<Vec<(bool, &str, AssetVersion)>, EditorError> {
        if let Some(p) = self.editor_content() {
            return Ok(p
                .assets
                .iter()
                .map(|a| (false, a.id.as_str(), a.version.clone()))
                .chain(
                    p.fonts
                        .iter()
                        .map(|f| (true, f.id.as_str(), f.content.clone())),
                )
                .collect());
        }
        self.legacy_content().expect("exhaustive document shapes").assets.iter().map(|a| {
            // Imported historical media may store its original immutable
            // identity in metadata instead of the later explicit field. Use
            // the same projection as authorized project-resource reads; never
            // rewrite the stored snapshot or open its mutable provenance URI.
            let version = crate::media_facts::legacy_asset_version(a).ok_or_else(|| EditorError::new(
                "PACKAGE_DEPENDENCY_MISSING",
                "Historical media lacks an observed immutable version; no replacement bytes are guessed",
            ))?;
            Ok((false, a.id.as_str(), version))
        }).collect()
    }
    pub fn validate(&self) -> Result<(), EditorError> {
        match self {
            Self::Legacy(p) => opencut_editor_core::validate_project(p),
            Self::HistoricalVisual(p) => {
                opencut_editor_core::historical_visual::validate_project_historical_visual(p)
            }
            Self::Editable(p) => opencut_editor_core::scene::validate_project_scene(p),
            Self::ReadOnly(p) => crate::project_readonly::validate_document(p),
        }
    }
    pub(super) fn validate_content(
        &self,
        revision: &str,
        proofs: Vec<FontDerivedSource>,
        require_bundled_fonts: bool,
    ) -> Result<(), EditorError> {
        self.validate()?;
        canonical_bytes(self, MAX_DOCUMENT_BYTES)?;
        if require_bundled_fonts && matches!(self, Self::ReadOnly(_)) {
            // Opaque JSON may name artifacts/native sources outside base.assets,
            // including overwritten duplicate keys. Preserving its bytes does
            // not prove those dependencies are portable. Do not emit a package
            // claiming self-contained resources until that closure is captured.
            return Err(EditorError::new(
                "PACKAGE_DEPENDENCY_UNSUPPORTED",
                "Self-contained preservation requires the opaque plugin dependency closure; linked packaging preserves the original references without installing authority",
            ));
        }
        if let Some(project) = self.editor_content() {
            return crate::historical_snapshot_package::validate_export_content(
                &crate::historical_snapshot_package::HistoricalSnapshotSource {
                    snapshot: EditorSnapshot {
                        project: project.clone(),
                        revision: revision.into(),
                    },
                    derived_fonts: proofs,
                },
                require_bundled_fonts,
            );
        }
        if !proofs.is_empty() {
            return Err(invalid("Historical base cannot claim derived fonts"));
        }
        if require_bundled_fonts && !self.external_fonts().is_empty() {
            return Err(EditorError::new(
                "PACKAGE_DEPENDENCY_MISSING",
                "Historical captions require external fonts; use linked packaging or resolve their original dependencies",
            ));
        }
        Ok(())
    }
    pub fn external_fonts(&self) -> Vec<String> {
        self.legacy_content().map_or_else(Vec::new, |project| {
            crate::project_package::dependencies(&crate::project_package::PackageExportSource {
                snapshot: opencut_contracts::ProjectSnapshot {
                    project: project.clone(),
                    revision: String::new(),
                },
                plugin_grants: vec![],
            })
            .fonts_not_bundled
        })
    }
    pub(super) fn converted(
        &self,
        versions: &BTreeMap<String, AssetVersion>,
    ) -> Result<EditorDocument, EditorError> {
        use crate::scene::StoredProjectDocument as D;
        let source = match self {
            Self::Legacy(p) => D::Legacy(Box::new(p.clone())),
            Self::HistoricalVisual(p) => D::HistoricalVisual(Box::new(p.clone())),
            Self::Editable(p) => D::Editable(Box::new(p.clone())),
            Self::ReadOnly(_) => {
                return Err(invalid(
                    "Preserved plugin content cannot be implicitly converted or restored",
                ));
            }
        };
        crate::scene::converted(&source, versions)
    }
    pub(super) fn remap_project(&mut self, target: &str) {
        match self {
            Self::Legacy(p) => p.id = target.into(),
            Self::HistoricalVisual(p) => p.base.id = target.into(),
            Self::Editable(p) => p.id = target.into(),
            Self::ReadOnly(p) => {
                p.id = target.into();
                p.base.id = target.into();
            }
        }
    }
}
