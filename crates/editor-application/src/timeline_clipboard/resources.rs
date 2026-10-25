//! Exact fixed-revision dependencies. A replaced logical resource is aliased,
//! never restored over the current head's other dependent elements.
use super::*;

fn font_id(value: &FontBinding, out: &mut BTreeSet<String>) {
    if let FontBinding::Locked { font_id } = value {
        out.insert(font_id.clone());
    }
}
pub(super) fn dependencies<'a>(
    project: &'a EditorDocument,
    items: &[CopiedItem],
) -> Result<(Vec<&'a ProjectAsset>, Vec<&'a FontResource>), EditorError> {
    let mut asset_ids = BTreeSet::new();
    let mut font_ids = BTreeSet::new();
    for item in items {
        match &item.element.content {
            ElementContent::Video { asset_id, .. }
            | ElementContent::Audio { asset_id, .. }
            | ElementContent::Image { asset_id }
            | ElementContent::Sticker { asset_id, .. } => {
                asset_ids.insert(asset_id.clone());
            }
            ElementContent::Text { text } => font_id(&text.font, &mut font_ids),
            ElementContent::Graphic { .. } | ElementContent::EffectLayer { .. } => {}
        }
        if let Some(visual) = &item.element.visual {
            for mask in &visual.masks {
                if let LayerMaskShape::Text { mask } = &mask.shape {
                    font_id(&mask.font, &mut font_ids);
                }
            }
        }
    }
    let assets = asset_ids
        .iter()
        .map(|id| {
            project
                .assets
                .iter()
                .find(|a| &a.id == id)
                .ok_or_else(|| missing("Copied asset is missing from fixed revision"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let fonts = font_ids
        .iter()
        .map(|id| {
            project
                .fonts
                .iter()
                .find(|f| &f.id == id)
                .ok_or_else(|| missing("Copied font is missing from fixed revision"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((assets, fonts))
}
pub(super) struct ResourceRemap {
    assets: BTreeMap<String, String>,
    fonts: BTreeMap<String, String>,
}
impl ResourceRemap {
    pub(super) fn apply(&self, element: &mut TimelineElement) {
        let replace_font = |font: &mut FontBinding| {
            if let FontBinding::Locked { font_id } = font
                && let Some(new) = self.fonts.get(font_id)
            {
                *font_id = new.clone();
            }
        };
        match &mut element.content {
            ElementContent::Video { asset_id, .. }
            | ElementContent::Audio { asset_id, .. }
            | ElementContent::Image { asset_id }
            | ElementContent::Sticker { asset_id, .. } => {
                if let Some(new) = self.assets.get(asset_id) {
                    *asset_id = new.clone();
                }
            }
            ElementContent::Text { text } => replace_font(&mut text.font),
            ElementContent::Graphic { .. } | ElementContent::EffectLayer { .. } => {}
        }
        if let Some(visual) = &mut element.visual {
            for mask in &mut visual.masks {
                if let LayerMaskShape::Text { mask } = &mut mask.shape {
                    replace_font(&mut mask.font);
                }
            }
        }
    }
}
pub(super) fn restore(
    source: &EditorSnapshot,
    items: &[CopiedItem],
    target: &mut EditorDocument,
    transaction: &mut ApplyProject,
    decision: &mut PasteTimelineDecision,
    ids: &mut identities::Ids<'_>,
) -> Result<ResourceRemap, EditorError> {
    let (assets, fonts) = dependencies(&source.project, items)?;
    let mut remap = ResourceRemap {
        assets: BTreeMap::new(),
        fonts: BTreeMap::new(),
    };
    for old in assets {
        let equivalent = target.assets.iter().find(|current| {
            let mut expected = (*old).clone();
            expected.id = current.id.clone();
            expected.name = current.name.clone();
            expected == **current
        });
        let next = if let Some(current) = equivalent {
            current.id.clone()
        } else {
            let next = if identities::collect(target).contains(&old.id) {
                ids.fresh("resource", "cb_asset", &old.id)?
            } else {
                old.id.clone()
            };
            let mut restored = (*old).clone();
            restored.id = next.clone();
            target.assets.push(restored);
            transaction
                .operations
                .push(ProjectEditOperation::AssetBind {
                    id: next.clone(),
                    name: old.name.clone(),
                    source: AssetSelector::ProjectAsset {
                        revision: source.revision.clone(),
                        asset_id: old.id.clone(),
                    },
                });
            decision.restored_asset_ids.push(next.clone());
            next
        };
        remap.assets.insert(old.id.clone(), next);
    }
    for old in fonts {
        let equivalent = target.fonts.iter().find(|current| {
            let mut expected = (*old).clone();
            expected.id = current.id.clone();
            expected == **current
        });
        let next = if let Some(current) = equivalent {
            current.id.clone()
        } else {
            let next = if identities::collect(target).contains(&old.id) {
                ids.fresh("resource", "cb_font", &old.id)?
            } else {
                old.id.clone()
            };
            let mut restored = (*old).clone();
            restored.id = next.clone();
            target.fonts.push(restored);
            transaction.operations.push(ProjectEditOperation::FontBind {
                id: next.clone(),
                source: FontSelector::ProjectFont {
                    revision: source.revision.clone(),
                    font_id: old.id.clone(),
                },
            });
            decision.restored_font_ids.push(next.clone());
            next
        };
        remap.fonts.insert(old.id.clone(), next);
    }
    Ok(remap)
}
