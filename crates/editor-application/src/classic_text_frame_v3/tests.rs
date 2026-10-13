use super::*;
fn metric(content: &str, width: f64) -> CaptionWrapMeasurementV3 {
    CaptionWrapMeasurementV3 {
        content: content.into(),
        width,
    }
}
#[test]
fn greedy_wrap_preserves_words_explicit_lines_and_continuous_chinese() {
    let original = "  AV   To world\r\n\n中文编辑连续不拆分  ";
    let observations = vec![
        metric("AV To", 40.0),
        metric("AV To world", 110.0),
        metric("AV To", 40.0),
        metric("world", 70.0),
        metric("", 0.0),
        metric("中文编辑连续不拆分", 180.0),
    ];
    assert_eq!(
        wrapping::verify_wrap(original, 80.0, &observations).unwrap(),
        "AV To\nworld\n\n中文编辑连续不拆分"
    );
}
#[test]
fn greedy_wrap_never_accepts_swapped_text_or_inconsistent_measurement() {
    let original = "AV To world";
    let mut observations = vec![
        metric("AV To", 40.0),
        metric("AV To world", 110.0),
        metric("AV To", 40.0),
        metric("world", 70.0),
    ];
    assert!(wrapping::verify_wrap(original, 80.0, &observations).is_ok());
    observations[0].content = "Other text".into();
    assert!(wrapping::verify_wrap(original, 80.0, &observations).is_err());
    observations[0].content = "AV To".into();
    observations[2].width = 41.0;
    assert!(wrapping::verify_wrap(original, 80.0, &observations).is_err());
}
#[test]
fn oversized_word_is_not_split_or_dropped() {
    assert_eq!(
        wrapping::verify_wrap("averylongword", 10.0, &[metric("averylongword", 300.0)]).unwrap(),
        "averylongword"
    );
}

fn caption_fixture() -> PreparedTextFrameV3 {
    use opencut_contracts::project_v4::*;
    let mut graph = crate::classic_text_frame_v2::tests::graph();
    let SampledContentV4::Text { text } = graph.layers[1].content.clone() else {
        panic!("title fixture")
    };
    graph.layers[1].content = SampledContentV4::Caption {
        text: text.content,
        style: CaptionStyleV4 {
            font: text.font,
            font_size: text.font_size,
            color: text.color,
            background_color: None,
            max_width_px: None,
            line_height_px: Some(12.0),
            text_align: text.align,
            layout: CaptionLayoutProfileV4::PinnedCaptionV4,
        },
    };
    // Captions' compiler fixes an identity transform; the retained fixture's
    // other opacity/media facts are independently delegated to the V2 kernel.
    prepare_text_frame_v3(&graph).unwrap()
}
#[test]
fn source_caption_identity_survives_only_the_versioned_execution_partition() {
    let prepared = caption_fixture();
    assert!(matches!(
        prepared.source_graph().layers[1].content,
        SampledContentV4::Caption { .. }
    ));
    assert!(prepare_text_frame_v2(prepared.source_graph()).is_err());
    assert_eq!(prepared.captions()[0].max_width_px, 3.2);
    assert_eq!(prepared.captions()[0].bottom_margin_px, 0.1);
    assert_eq!(prepared.execution().texts()[0].text.content, "AB\nC");
}
#[test]
fn pure_caption_receipt_checks_original_content_position_and_measurement_without_native_claims() {
    let prepared = caption_fixture();
    let policy = crate::classic_text_frame_v2::tests::policy();
    let coverage = crate::classic_text_frame_v2::tests::proof(prepared.execution());
    let plan = plan_v3(&prepared, &policy, &coverage).unwrap();
    let mut raster = crate::classic_text_frame_v2::tests::raster(&plan.execution);
    raster.layout.transform.center_y = 2.0 - 0.1 - 12.0;
    let caption = CaptionLayoutReceiptV3 {
        element_id: "title".into(),
        original_text_hash: hash("AB\nC"),
        wrapped_text: "AB\nC".into(),
        measurements: vec![metric("AB", 8.0), metric("C", 4.0)],
    };
    assert!(validate_caption_receipts_v3(&prepared, &[raster.clone()], &[caption.clone()]).is_ok());
    let mut bad = caption.clone();
    bad.wrapped_text = "Other content".into();
    assert!(validate_caption_receipts_v3(&prepared, &[raster.clone()], &[bad]).is_err());
    raster.layout.transform.center_y += 1.0;
    assert!(validate_caption_receipts_v3(&prepared, &[raster], &[caption]).is_err());
}
