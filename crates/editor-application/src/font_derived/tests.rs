use super::*;
use crate::edit_recovery as recovery;
use opencut_contracts::edit_recovery::*;
use std::cell::Cell;
fn request() -> ImportFont {
    ImportFont {
        project_id: "project".into(),
        expected_revision: "fixed".into(),
        idempotency_key: "derived-import".into(),
        catalog_id: catalog().unwrap()[0].source.face.id.clone(),
        font_id: "derived-font".into(),
        required_text: "AV To\n中文".into(),
    }
}
#[test]
fn derived_font_canonical_proof_preserves_numeric_axis_representation() {
    let value: Value = serde_json::from_str(r#"{"z":100.0,"a":400}"#).unwrap();
    assert_eq!(canonical(&value).unwrap(), r#"{"a":400,"z":100.0}"#);
    let pin = source(&request().catalog_id).unwrap();
    validate_pinned_source(&pin).unwrap();
    let mut changed = pin.clone();
    changed.source.proof_json.push(' ');
    assert!(validate_pinned_source(&changed).is_err());
    changed = pin.clone();
    changed.source.axes[0].coordinate.push('0');
    assert!(validate_pinned_source(&changed).is_err());
    changed = pin;
    changed.source.face.derivation_id.push('0');
    assert!(validate_pinned_source(&changed).is_err());
}
#[test]
fn derived_font_resource_requires_exact_origin_output_and_original_license() {
    let request = request();
    let source = source(&request.catalog_id).unwrap().source;
    let font = expected_import_resource(&request).unwrap();
    proof::validate_resource_source(&font, &source).unwrap();
    for case in 0..5 {
        let mut changed = font.clone();
        match case {
            0 => changed.family.push('x'),
            1 => changed.weight = if font.weight == 400 { 700 } else { 400 },
            2 => changed.content.content_hash = "0".repeat(64),
            3 => {
                changed.origin = ResourceOrigin::Generated {
                    provider: "other".into(),
                    generation_id: source.face.derivation_id.clone(),
                }
            }
            _ => changed.license.as_mut().unwrap().notice = Some("stripped".into()),
        };
        assert!(proof::validate_resource_source(&changed, &source).is_err())
    }
}
struct Provider(Cell<u32>);
impl DerivedFontSource for Provider {
    fn obtain(
        &self,
        _: &PinnedDerivedFont,
        _: &FontFetchPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError> {
        checkpoint.check()?;
        self.0.set(self.0.get() + 1);
        Ok(b"incorrect font".to_vec())
    }
}
struct Inspector;
impl FontInspector for Inspector {
    fn inspect(
        &self,
        _: &[u8],
        _: &str,
        _: &FontImportPolicy,
        _: &mut dyn FontImportCheckpoint,
    ) -> Result<ParsedFontFace, EditorError> {
        panic!("Unpinned bytes must never enter the font inspector")
    }
}
#[test]
fn derived_font_output_integrity_precedes_native_inspection() {
    let provider = Provider(Cell::new(0));
    let mut check = || Ok(());
    let error = prepare_derived_font(
        &provider,
        &Inspector,
        &request(),
        &FontFetchPolicy::default(),
        &FontImportPolicy::default(),
        &mut check,
    )
    .err()
    .unwrap();
    assert_eq!(error.code, "FONT_SOURCE_INTEGRITY_ERROR");
    assert_eq!(provider.0.get(), 1);
}
#[test]
fn derived_font_cancellation_precedes_provider() {
    let provider = Provider(Cell::new(0));
    let mut check = || Err(EditorError::new("CANCELLED", "cancelled"));
    let error = prepare_derived_font(
        &provider,
        &Inspector,
        &request(),
        &FontFetchPolicy::default(),
        &FontImportPolicy::default(),
        &mut check,
    )
    .err()
    .unwrap();
    assert_eq!(error.code, "CANCELLED");
    assert_eq!(provider.0.get(), 0);
}
#[test]
fn derived_font_recovery_preserves_original_request_and_scope() {
    let staged = StageEditRecovery {
        context: EditRecoveryContext {
            intent: opencut_contracts::edit_recovery::EditRecoveryIntent::FontImport {
                request: request(),
            },
            sequence_id: Some("scene".into()),
        },
    };
    let prepared = recovery::prepare(&staged).unwrap();
    assert_eq!(
        recovery::decode_payload(prepared.payload(), prepared.digest(), "project").unwrap(),
        staged
    );
    assert!(recovery::decode_payload(prepared.payload(), prepared.digest(), "other").is_err());
    assert!(
        recovery::decode_payload(
            &(prepared.payload().to_owned() + " "),
            prepared.digest(),
            "project"
        )
        .is_err()
    );
    let mut changed = staged.clone();
    let EditRecoveryIntent::FontImport { request } = &mut changed.context.intent else {
        panic!()
    };
    request.required_text.push('X');
    assert_ne!(
        recovery::prepare(&changed).unwrap().digest(),
        prepared.digest()
    );
}
