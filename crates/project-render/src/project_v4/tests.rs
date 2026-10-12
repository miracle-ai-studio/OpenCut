use super::*;
mod fixture;
mod pixels;
use fixture::*;

#[test]
fn unsupported_fields_fail_before_resolving_resources() {
    let mut graph = compile(&snapshot());
    graph.layers[0].visual.as_mut().unwrap().profile = VisualProfileV4::ClassicCanvasV31;
    assert_eq!(
        ProjectFrameAdapterV4::resolve(graph, |_| panic!("unsupported node must not resolve"))
            .unwrap_err()
            .code,
        "UNSUPPORTED_V4"
    );
    let mut graph = compile(&snapshot());
    graph.layers[1].visual.as_mut().unwrap().masks[0].feather =
        MaskFeatherV4::ClassicPixels { value: 2.0 };
    assert!(
        ProjectFrameAdapterV4::resolve(graph, |_| panic!())
            .unwrap_err()
            .message
            .contains("feather unit")
    );
    let mut graph = compile(&snapshot());
    graph.layers[1].visual.as_mut().unwrap().transform.scale_x = 2.0;
    assert!(
        ProjectFrameAdapterV4::resolve(graph, |_| panic!())
            .unwrap_err()
            .message
            .contains("scaled")
    );
    let mut graph = compile(&snapshot());
    graph.layers[0].visual.as_mut().unwrap().effective_opacity = 1.125;
    assert!(
        ProjectFrameAdapterV4::resolve(graph, |_| panic!())
            .unwrap_err()
            .message
            .contains("unbounded")
    );
}

#[test]
fn lowering_consumes_sampled_values_and_preserves_request_clock() {
    let graph = compile(&snapshot());
    assert_eq!(graph.time, t(3, 2));
    assert!((graph.layers[1].visual.as_ref().unwrap().effective_opacity - 0.5).abs() < 1e-12);
    let inputs = HashMap::from([("image".into(), PathBuf::from("/host/image.ppm"))]);
    let plan = lowering::lower(&graph, &inputs, PathBuf::from("/host/output.png"), false).unwrap();
    assert_eq!(plan.timeline.base.image_clips[0].opacity, 0.125);
    assert_eq!(plan.timeline.visual.shapes[1].opacity, 0.5);
    assert_eq!(plan.timeline.visual.masks[0].feather, 0.0);
    assert!(plan.timeline.base.video_clips.is_empty());
    assert_eq!(
        plan.sequence_time,
        MediaTime {
            ticks: 0,
            timebase: 1
        }
    );
    assert_eq!(graph.time, t(3, 2));
}
