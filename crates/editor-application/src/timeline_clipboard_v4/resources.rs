//! Exact fixed-revision dependencies. A replaced logical resource is aliased,
//! never restored over the current head's other dependent elements.
use super::*;

fn font_id(value: &FontBindingV4, out: &mut BTreeSet<String>) {
    if let FontBindingV4::Locked { font_id } = value {
        out.insert(font_id.clone());
    }
}
pub(super) fn dependencies<'a>(
    project: &'a ProjectDocumentV4,
    items: &[CopiedItem],
) -> Result<(Vec<&'a AssetV4>, Vec<&'a FontResourceV4>), EditorError> {
    let mut asset_ids = BTreeSet::new();
    let mut font_ids = BTreeSet::new();
    for item in items {
        match &item.element.content {
            ElementContentV4::Video { asset_id, .. }
            | ElementContentV4::Audio { asset_id, .. }
            | ElementContentV4::Image { asset_id }
            | ElementContentV4::Sticker { asset_id, .. } => {
                asset_ids.insert(asset_id.clone());
            }
            ElementContentV4::Text { text } => font_id(&text.font, &mut font_ids),
            ElementContentV4::Graphic { .. } | ElementContentV4::EffectLayer { .. } => {}
        }
        if let Some(visual) = &item.element.visual {
            for mask in &visual.masks {
                if let MaskShapeV4::Text { mask } = &mask.shape {
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
    pub(super) fn apply(&self, element: &mut TimelineElementV4) {
        let replace_font = |font: &mut FontBindingV4| {
            if let FontBindingV4::Locked { font_id } = font
                && let Some(new) = self.fonts.get(font_id)
            {
                *font_id = new.clone();
            }
        };
        match &mut element.content {
            ElementContentV4::Video { asset_id, .. }
            | ElementContentV4::Audio { asset_id, .. }
            | ElementContentV4::Image { asset_id }
            | ElementContentV4::Sticker { asset_id, .. } => {
                if let Some(new) = self.assets.get(asset_id) {
                    *asset_id = new.clone();
                }
            }
            ElementContentV4::Text { text } => replace_font(&mut text.font),
            ElementContentV4::Graphic { .. } | ElementContentV4::EffectLayer { .. } => {}
        }
        if let Some(visual) = &mut element.visual {
            for mask in &mut visual.masks {
                if let MaskShapeV4::Text { mask } = &mut mask.shape {
                    replace_font(&mut mask.font);
                }
            }
        }
    }
}
pub(super) fn restore(
    source: &ProjectSnapshotV4,
    items: &[CopiedItem],
    target: &mut ProjectDocumentV4,
    transaction: &mut ApplyPublicV4Project,
    decision: &mut PasteTimelineDecisionV4,
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
                .push(PublicProjectOperationV4::AssetBind {
                    id: next.clone(),
                    name: old.name.clone(),
                    source: PublicAssetSelectorV4::ProjectAsset {
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
            transaction
                .operations
                .push(PublicProjectOperationV4::FontBind {
                    id: next.clone(),
                    source: PublicFontSelectorV4::ProjectFont {
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
