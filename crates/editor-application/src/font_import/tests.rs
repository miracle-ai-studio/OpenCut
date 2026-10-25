use super::*;
use std::cell::Cell;

struct Inspector {
    calls: Cell<usize>,
    weight: u16,
}
impl FontInspector for Inspector {
    fn inspect(
        &self,
        _: &[u8],
        text: &str,
        _: &FontImportPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<ParsedFontFace, EditorError> {
        checkpoint.check()?;
        self.calls.set(self.calls.get() + 1);
        assert_eq!(text, "AV To\n中文编辑");
        Ok(ParsedFontFace {
            family: "Example Sans".into(),
            weight: self.weight,
            style: FontStyle::Normal,
            glyph_count: 128,
            checked_codepoints: 9,
        })
    }
}
fn spec() -> FontImportSpec {
    FontImportSpec {
        font_id: "font-example".into(),
        family: "Example Sans".into(),
        weight: 400,
        style: FontStyle::Normal,
        origin: ResourceOrigin::Online {
            provider: "google-fonts".into(),
            source_url:
                "https://raw.githubusercontent.com/google/fonts/fixed/ofl/example/regular.ttf"
                    .into(),
        },
        license: ResourceLicense {
            identifier: Some("OFL-1.1".into()),
            source_url: Some(
                "https://raw.githubusercontent.com/google/fonts/fixed/ofl/example/OFL.txt".into(),
            ),
            notice: Some(
                "Synthetic test notice\nPreserve its exact newline and attribution.".into(),
            ),
            attribution: Some("Synthetic fixture author".into()),
        },
        required_text: "AV To\n中文编辑".into(),
    }
}
fn inspector(weight: u16) -> Inspector {
    Inspector {
        calls: Cell::new(0),
        weight,
    }
}

#[test]
fn locks_exact_bytes_and_retains_provenance_and_notice_without_normalization() {
    let input = spec();
    let prepared = prepare_font(
        &inspector(400),
        b"abc",
        &input,
        &FontImportPolicy::default(),
        &mut || Ok(()),
    )
    .unwrap();
    assert_eq!(
        prepared.resource.content.content_hash,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        prepared.resource.content.id,
        format!("av_{}", prepared.resource.content.content_hash)
    );
    assert_eq!(prepared.resource.content.size, 3);
    assert_eq!(prepared.resource.face_index, 0);
    assert_eq!(prepared.resource.origin, input.origin);
    assert_eq!(prepared.resource.license, Some(input.license));
    assert_eq!(prepared.checked_codepoints, 9);
}

#[test]
fn face_mismatch_never_synthesizes_the_requested_weight() {
    assert_eq!(
        prepare_font(
            &inspector(700),
            b"abc",
            &spec(),
            &FontImportPolicy::default(),
            &mut || Ok(())
        )
        .unwrap_err()
        .code,
        "FONT_FACE_MISMATCH"
    );
}

#[test]
fn rejects_unattributed_and_deceptive_sources_before_native_work() {
    let native = inspector(400);
    for url in [
        "http://fonts.gstatic.com/font.ttf",
        "https://fonts.gstatic.com.evil.test/font.ttf",
        "https://user@fonts.gstatic.com/font.ttf",
        "https://github.com/other/fonts/font.ttf",
    ] {
        let mut input = spec();
        input.origin = ResourceOrigin::Online {
            provider: "google-fonts".into(),
            source_url: url.into(),
        };
        assert!(
            prepare_font(
                &native,
                b"abc",
                &input,
                &FontImportPolicy::default(),
                &mut || Ok(())
            )
            .is_err()
        );
    }
    let mut input = spec();
    input.license.notice = None;
    assert!(
        prepare_font(
            &native,
            b"abc",
            &input,
            &FontImportPolicy::default(),
            &mut || Ok(())
        )
        .is_err()
    );
    assert_eq!(native.calls.get(), 0);
}

#[test]
fn cancellation_and_authority_errors_propagate_before_preparation() {
    let native = inspector(400);
    for code in ["JOB_CANCELLED", "PERMISSION_DENIED", "DEADLINE_EXCEEDED"] {
        assert_eq!(
            prepare_font(
                &native,
                b"abc",
                &spec(),
                &FontImportPolicy::default(),
                &mut || Err(EditorError::new(code, "upstream checkpoint"))
            )
            .unwrap_err()
            .code,
            code
        );
    }
    assert_eq!(native.calls.get(), 0);
}

#[test]
fn cancellation_during_hashing_does_not_return_a_prepared_resource() {
    let mut checks = 0;
    let error = prepare_font(
        &inspector(400),
        &vec![0; 128 * 1024],
        &spec(),
        &FontImportPolicy::default(),
        &mut || {
            checks += 1;
            if checks == 4 {
                Err(EditorError::new("JOB_CANCELLED", "cancel during hashing"))
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "JOB_CANCELLED");
}

#[test]
fn byte_and_text_policies_cannot_expand_the_hard_envelope() {
    let native = inspector(400);
    let policy = FontImportPolicy {
        max_font_bytes: 2,
        ..Default::default()
    };
    assert_eq!(
        prepare_font(&native, b"abc", &spec(), &policy, &mut || Ok(()))
            .unwrap_err()
            .code,
        "FONT_BUDGET_EXCEEDED"
    );
    let policy = FontImportPolicy {
        max_font_bytes: MAX_FONT_BYTES + 1,
        ..Default::default()
    };
    assert_eq!(
        prepare_font(&native, b"abc", &spec(), &policy, &mut || Ok(()))
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
    assert!(validate_font_input(b"abc", "hello\tworld", &FontImportPolicy::default()).is_err());
    assert_eq!(native.calls.get(), 0);
}
