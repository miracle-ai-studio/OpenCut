use super::*;
use crate::classic_restore::{self, ClassicResources, analyze};
use crate::{classic_restore::ResolvedClassicFont, font_synthesis as synthesis};
use opencut_api_contracts::classic_restore::AnalyzeClassic;
use opencut_contracts::font_import::ImportFont;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn import(id: &str) -> ImportFont {
    ImportFont {
        project_id: "source".into(),
        expected_revision: "fixed".into(),
        idempotency_key: "font".into(),
        catalog_id: id.into(),
        font_id: "locked-font".into(),
        required_text: "AV".into(),
    }
}
fn import_derived(id: &str) -> ImportFont {
    ImportFont {
        project_id: "source".into(),
        expected_revision: "fixed".into(),
        idempotency_key: "font".into(),
        catalog_id: id.into(),
        font_id: "locked-font".into(),
        required_text: "AV".into(),
    }
}
fn request(family: &str, weight: u16, style: FontStyle) -> ClassicFontRequest {
    ClassicFontRequest {
        family: family.into(),
        font_weight: if weight == 700 {
            FontWeight::Bold
        } else {
            FontWeight::Normal
        },
        font_style: style,
    }
}
fn input(request: &ClassicFontRequest) -> AnalyzeClassic {
    let mut value: Value =
        serde_json::from_str(include_str!("../classic_restore/fixture.json")).unwrap();
    let params = &mut value["scenes"][0]["tracks"]["overlay"][0]["elements"][0]["params"];
    params["fontFamily"] = json!(request.family);
    params["fontWeight"] = json!(request.font_weight);
    params["fontStyle"] = json!(request.font_style);
    AnalyzeClassic {
        content: value.to_string(),
        target_project_id: "target".into(),
        media_mappings: vec![],
        font_mappings: vec![],
    }
}
fn resources(request: ClassicFontRequest, font: FontResource) -> ClassicResources {
    ClassicResources {
        media: BTreeMap::new(),
        fonts: vec![ResolvedClassicFont { request, font }],
    }
}
#[test]
fn synthesis_keeps_original_italic_request_real_normal_resource_and_source_digest() {
    let pin = synthesis::catalog().unwrap().remove(0);
    let font = synthesis::expected_import_resource(&import_derived(&pin.source.face.id)).unwrap();
    let requested = request(&font.family, 700, FontStyle::Italic);
    let input = input(&requested);
    let original = input.content.clone();
    let resources = resources(requested.clone(), font.clone());
    let new = analyze(&input, &resources).unwrap();
    assert!(
        new.analysis.blockers.is_empty(),
        "{:?}",
        new.analysis.blockers
    );
    assert_eq!(
        new.analysis.source_content_id,
        crate::classic_restore::source_content_id(&original)
    );
    assert_eq!(input.content, original);
    assert_eq!(resources.fonts[0].request, requested);
    let document = new.document.unwrap();
    assert_eq!(document.schema_version, 4);
    assert_eq!(document.fonts, vec![font.clone()]);
    let ElementContent::Text { text } = &document.sequences[0].tracks[1].elements[0].content else {
        panic!()
    };
    assert_eq!(text.font_style, FontStyle::Italic);
    assert_eq!(text.font_weight, FontWeight::Bold);
    assert_eq!(text.font, FontBinding::Locked { font_id: font.id });
}
#[test]
fn synthesis_rejects_forged_source_license_request_and_unused_mapping() {
    let pin = synthesis::catalog().unwrap().remove(0);
    let font = synthesis::expected_import_resource(&import_derived(&pin.source.face.id)).unwrap();
    let requested = request(&font.family, 700, FontStyle::Italic);
    for field in 0..4 {
        let mut altered = font.clone();
        match field {
            0 => altered.content.content_hash = "a".repeat(64),
            1 => altered.license.as_mut().unwrap().notice = Some("short notice".into()),
            2 => altered.family = "Caller alias".into(),
            _ => altered.style = FontStyle::Italic,
        }
        assert!(validate_font_mapping(&requested, &altered).is_err());
    }
    for bad in [
        request(&font.family, 700, FontStyle::Normal),
        request(&font.family, 400, FontStyle::Italic),
        request("Arbitrary", 700, FontStyle::Italic),
    ] {
        let mut source = input(&requested);
        let mut raw: Value = serde_json::from_str(&source.content).unwrap();
        raw["scenes"][0]["tracks"]["overlay"] = json!([]);
        source.content = raw.to_string();
        assert!(analyze(&source, &resources(bad, font.clone())).is_err());
    }
}
#[test]
fn renamed_family_requires_exact_compiled_proof_and_never_changes_requested_style() {
    let pin = crate::font_named::compiled_sources()
        .unwrap()
        .iter()
        .find(|pin| {
            pin.source.face.family
                != crate::font_named::display_family(&pin.source.face.id).unwrap()
        })
        .unwrap();
    let font =
        crate::font_named::expected_import_resource(&import_derived(&pin.source.face.id)).unwrap();
    let requested = request(
        &crate::font_named::display_family(&pin.source.face.id).unwrap(),
        font.weight,
        font.style,
    );
    validate_font_mapping(&requested, &font).unwrap();
    let resources = resources(requested.clone(), font.clone());
    let source = input(&requested);
    let new = analyze(&source, &resources).unwrap();
    assert!(new.document.is_some(), "{:?}", new.analysis.blockers);
    assert_eq!(new.document.unwrap().fonts[0], font);
    let mut wrong = font.clone();
    wrong.license.as_mut().unwrap().notice = Some("forged".into());
    assert!(validate_font_mapping(&requested, &wrong).is_err());
    let mut wrong = requested;
    wrong.family = "Caller-chosen display name".into();
    assert!(validate_font_mapping(&wrong, &font).is_err());
}
#[test]
fn static_historical_visual_alias_requires_complete_native_source_record() {
    let entry = crate::font_catalog::static_sources()
        .find(|e| e.display_family != e.approved.source.family)
        .unwrap();
    let font =
        crate::font_catalog::expected_import_resource(&import(entry.approved.source.catalog_id))
            .unwrap();
    let requested = request(entry.display_family, font.weight, font.style);
    validate_font_mapping(&requested, &font).unwrap();
    let resources = resources(requested.clone(), font.clone());
    let source = input(&requested);
    assert!(analyze(&source, &resources).unwrap().document.is_some());
    for field in 0..3 {
        let mut wrong = font.clone();
        match field {
            0 => {
                wrong.origin = ResourceOrigin::LocalImport {
                    original_name: "same-bytes.ttf".into(),
                }
            }
            1 => wrong.license.as_mut().unwrap().attribution = None,
            _ => wrong.face_index = 1,
        }
        assert!(validate_font_mapping(&requested, &wrong).is_err());
    }
}
#[test]
fn synthesis_text_mask_uses_the_same_explicit_binding() {
    let pin = synthesis::catalog().unwrap().remove(0);
    let font = synthesis::expected_import_resource(&import_derived(&pin.source.face.id)).unwrap();
    let requested = request(&font.family, 700, FontStyle::Italic);
    let mut source = input(&requested);
    let mut value: Value = serde_json::from_str(&source.content).unwrap();
    value["scenes"][0]["tracks"]["overlay"] = json!([{ "id":"graphic-track", "name":"Graphic", "type":"graphic", "elements":[{
        "id":"graphic", "name":"Masked", "type":"graphic", "definitionId":"rectangle", "startTime":0, "duration":240000, "trimStart":0, "trimEnd":0,
        "params":{}, "effects":[], "animations":{}, "masks":[{"id":"text-mask", "type":"text", "params":{
            "content":"AV", "fontFamily":requested.family, "fontWeight":"bold", "fontStyle":"italic", "fontSize":30, "textDecoration":"none", "letterSpacing":0, "lineHeight":1.2,
            "centerX":0, "centerY":0, "rotation":0, "scale":1, "feather":0, "inverted":false,
            "strokeColor":"#ffffff", "strokeWidth":0, "strokeAlign":"center"
        }}]
    }]}]);
    source.content = value.to_string();
    let resources = resources(requested, font);
    let new = analyze(&source, &resources).unwrap();
    assert!(new.document.is_some(), "{:?}", new.analysis.blockers);
}

#[test]
fn ordinary_and_unresolved_fonts_keep_the_scene_document_and_original_requests() {
    let entry = crate::font_catalog::static_sources()
        .find(|e| e.display_family == e.approved.source.family)
        .unwrap();
    let font =
        crate::font_catalog::expected_import_resource(&import(entry.approved.source.catalog_id))
            .unwrap();
    let requested = request(&font.family, font.weight, font.style);
    let source = input(&requested);
    let mapped = resources(requested.clone(), font);
    let mapped_result = analyze(&source, &mapped).unwrap();
    assert!(mapped_result.document.is_some());
    let empty = ClassicResources {
        media: BTreeMap::new(),
        fonts: vec![],
    };
    let unresolved = analyze(&source, &empty).unwrap();
    assert_eq!(unresolved.analysis.unresolved_fonts, vec![requested]);
    assert_eq!(
        unresolved.document,
        classic_restore::analyze(&source, &empty).unwrap().document
    );
}

#[test]
fn synthesis_of_renamed_or_alias_base_accepts_only_its_compiled_classic_label() {
    for derived in [false, true] {
        let pin = synthesis::catalog()
            .unwrap()
            .into_iter()
            .find(|pin| {
                synthesis::display_family(&pin.source.face.id).unwrap() != pin.binding.actual.family
                    && matches!(
                        synthesis::base_source(&pin.source.face.id).unwrap(),
                        synthesis::SynthesisBaseSource::RenamedOutline(_)
                    ) == derived
            })
            .unwrap();
        let font =
            synthesis::expected_import_resource(&import_derived(&pin.source.face.id)).unwrap();
        let requested = request(
            &synthesis::display_family(&pin.source.face.id).unwrap(),
            font.weight,
            FontStyle::Italic,
        );
        validate_font_mapping(&requested, &font).unwrap();
        let source = input(&requested);
        let res = resources(requested.clone(), font.clone());
        assert!(analyze(&source, &res).unwrap().document.is_some());
        let mut wrong = requested;
        wrong.family = "Caller chosen display family".into();
        assert!(validate_font_mapping(&wrong, &font).is_err());
    }
}
