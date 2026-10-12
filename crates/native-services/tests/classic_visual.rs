//! Actual isolated Chromium/WebGPU pixel checks. All PNG inputs are synthetic,
//! tiny and owned by a TempDir; no user media or queue-publication claim.
use opencut_asset_store::AssetStore;
use opencut_composition::{CompositionScope, project_v4::*};
use opencut_contracts::{
    Asset, AssetKind, CreateProject, EditorError, FrameRate, Time, TimeRange, project_v4::*,
};
use opencut_editor_application::{
    classic_visual::*, project_v4::new_document,
    project_v4_frame_jobs::LocalVisualExecutionPolicyV1,
};
use opencut_native_services::classic_visual::*;
use std::{collections::BTreeMap, path::Path, time::Duration};

fn time(n: i32, d: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: d,
    }
}
fn policy() -> LocalVisualExecutionPolicyV1 {
    LocalVisualExecutionPolicyV1 {
        max_frame_pixels: 16_777_216,
        max_decoded_pixels: 16_777_216,
        max_gpu_bytes: 512 * 1024 * 1024,
        max_rgba_bytes: 64 * 1024 * 1024,
        max_encoded_bytes: 32 * 1024 * 1024,
        max_output_bytes: 129 * 1024 * 1024,
        max_rss_bytes: 2 * 1024 * 1024 * 1024,
        deadline_ms: 45000,
    }
}
fn config() -> ClassicVisualProcessConfig {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    ClassicVisualProcessConfig {
        node: std::env::var_os("OPENCUT_NODE")
            .unwrap_or_else(|| "node".into())
            .into(),
        chromium: std::env::var_os("OPENCUT_CHROME")
            .expect("Set OPENCUT_CHROME to the actual installed browser")
            .into(),
        runner: workspace.join("packages/classic-visual-renderer/host/runner.mjs"),
        deadline: Duration::from_secs(45),
        policy: policy(),
        strict_cgroup: None,
    }
}
fn snapshot() -> ProjectSnapshotV4 {
    let mut project = new_document(&CreateProject {
        id: "project".into(),
        name: "Synthetic GPU".into(),
        sequence_id: "scene".into(),
        width: 16,
        height: 16,
        fps: FrameRate {
            numerator: 30,
            denominator: 1,
        },
    })
    .unwrap();
    project.sequences[0].main_track_id = "main".into();
    project.sequences[0].tracks[0].id = "main".into();
    ProjectSnapshotV4 {
        project,
        revision: "fixed-native-visual".into(),
    }
}
fn add_png(
    directory: &Path,
    snapshot: &mut ProjectSnapshotV4,
    id: &str,
    width: u32,
    height: u32,
    pixel: impl Fn(u32, u32) -> [u8; 4],
) {
    let path = directory.join(format!("synthetic-{id}.png"));
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                pixels.extend(pixel(x, y));
            }
        }
        writer.write_image_data(&pixels).unwrap();
    }
    std::fs::write(&path, &bytes).unwrap();
    let original = Asset {
        id: id.into(),
        name: id.into(),
        kind: AssetKind::Image,
        uri: path.to_string_lossy().into(),
        duration: None,
        version: None,
        metadata: BTreeMap::new(),
    };
    let pinned = AssetStore::open(directory.join("cas"))
        .unwrap()
        .pin_asset(&original)
        .unwrap();
    snapshot.project.assets.push(AssetV4 {
        id: id.into(),
        name: id.into(),
        kind: AssetKind::Image,
        version: pinned.version.unwrap(),
        duration: None,
        origin: ResourceOriginV4::LegacyLocator {
            uri: "unused-origin.png".into(),
        },
        // Deliberately wrong nominal size: only actual decode dimensions may
        // decide contain geometry; the CAS identity remains exact.
        metadata: MediaMetadataV4 {
            mime_type: Some("image/png".into()),
            intrinsic_size: Some(CanvasSizeV4 {
                width: 1,
                height: 1,
            }),
            frame_rate: None,
            rotation_degrees: None,
            audio: AudioPresenceV4::Absent {},
        },
        license: None,
    });
}
fn element(id: &str, asset_id: &str, opacity: f64, scale_x: f64) -> TimelineElementV4 {
    TimelineElementV4 {
        id: id.into(),
        name: id.into(),
        timeline_range: TimeRange {
            start: time(0, 1),
            duration: time(1, 1),
        },
        source_span: ElementSourceSpanV4::ClassicTrimmed {
            trim_start: time(0, 1),
            trim_end: time(0, 1),
            source_duration: None,
        },
        enabled: true,
        content: ElementContentV4::Image {
            asset_id: asset_id.into(),
        },
        channels: vec![],
        visual: Some(VisualStyleV4 {
            profile: VisualProfileV4::ClassicCanvasV31,
            transform: TransformV4 {
                position_x_px: 0.0,
                position_y_px: 0.0,
                scale_x,
                scale_y: 1.0,
                rotation_degrees: 0.0,
            },
            opacity,
            blend_mode: BlendModeV4::Normal,
            effects: vec![],
            masks: vec![],
        }),
    }
}
#[derive(Default)]
struct Rgba {
    bytes: Vec<u8>,
}
impl ClassicVisualRgbaSink for Rgba {
    fn chunk(&mut self, offset: u64, bytes: &[u8]) -> Result<(), EditorError> {
        assert_eq!(offset, self.bytes.len() as u64);
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}
fn render(root: &Path, snapshot: &ProjectSnapshotV4) -> (Rgba, ClassicVisualExecutionReceipt) {
    let graph = compile_frame_v4(snapshot, "scene", &time(1, 7), CompositionScope::Visual).unwrap();
    let prepared = prepare_visual_frame_v4(&graph).unwrap();
    let mut resources = VerifiedClassicVisualResources::new(root, &prepared, || Ok(())).unwrap();
    let handles = resources.handles();
    let mut session = ClassicVisualSession::new(&prepared, &policy()).unwrap();
    let mut rgba = Rgba::default();
    let execution = ClassicVisualProcess { config: config() }
        .execute(&mut session, &mut resources, &handles, &mut rgba)
        .unwrap();
    assert_eq!(rgba.bytes.len(), 16 * 16 * 4);
    assert_eq!(execution.receipt.identity, *prepared.identity());
    assert_eq!(execution.receipt.identity.time, time(1, 7));
    assert_eq!(
        execution.runtime_identity.identity.texture_format,
        "bgra8unorm"
    );
    assert_eq!(execution.runtime_identity.identity.gpu_backend, "webgpu");
    for hash in [
        &execution.runtime_identity.identity.browser_sha256,
        &execution.runtime_identity.identity.module_sha256,
        &execution.runtime_identity.identity.shader_sha256,
    ] {
        assert_eq!(hash.len(), 64);
        assert!(
            hash.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
    }
    #[cfg(unix)]
    unsafe {
        assert_eq!(
            libc::kill(execution.runtime_identity.browser_pid as i32, 0),
            -1,
            "Browser remains alive after completion"
        );
    }
    (rgba, execution)
}
fn pixel(rgba: &Rgba, x: usize, y: usize) -> [u8; 4] {
    rgba.bytes[(y * 16 + x) * 4..(y * 16 + x) * 4 + 4]
        .try_into()
        .unwrap()
}

#[test]
#[ignore = "Requires installed pinned browser/WebGPU runtime; run explicitly with OPENCUT_CHROME"]
fn actual_gpu_contain_layer_order_and_quarter_opacity_have_independent_rgb_oracles() {
    let directory = tempfile::tempdir().unwrap();
    let mut snapshot = snapshot();
    add_png(directory.path(), &mut snapshot, "blue", 16, 16, |_, _| {
        [0, 0, 255, 255]
    });
    add_png(directory.path(), &mut snapshot, "red", 32, 16, |_, _| {
        [255, 0, 0, 255]
    });
    snapshot.project.sequences[0].tracks[0]
        .elements
        .push(element("base", "blue", 1.0, 1.0));
    let mut upper = snapshot.project.sequences[0].tracks[0].clone();
    upper.id = "upper".into();
    upper.name = "Upper".into();
    upper.elements = vec![element("overlay", "red", 0.25, 1.0)];
    snapshot.project.sequences[0].tracks.push(upper);
    let (rgba, receipt) = render(&directory.path().join("cas"), &snapshot);
    // The wide source contains to 16x8 centered at (8,8), leaving blue above
    // and below. Quarter red over opaque blue independently gives 64/0/191.
    assert_eq!(pixel(&rgba, 8, 2), [0, 0, 255, 255]);
    assert_eq!(pixel(&rgba, 8, 8), [64, 0, 191, 255]);
    assert_eq!(pixel(&rgba, 8, 14), [0, 0, 255, 255]);
    assert_eq!(
        receipt
            .receipt
            .bound
            .layers
            .iter()
            .map(|l| l.element_id.as_str())
            .collect::<Vec<_>>(),
        ["base", "overlay"]
    );
    assert!(
        matches!(&receipt.receipt.bound.decoded[1], DecodedVisualSourceV4::ReadyImage { element_id, width: 32, height: 16 } if element_id == "overlay")
    );
    let quad = receipt.receipt.bound.layers[1].quad.as_ref().unwrap();
    assert_eq!((quad.width, quad.height), (16.0, 8.0));
}
#[test]
#[ignore = "Requires installed pinned browser/WebGPU runtime; run explicitly with OPENCUT_CHROME"]
fn actual_gpu_negative_scale_flips_asymmetric_source_without_replacing_identity() {
    let directory = tempfile::tempdir().unwrap();
    let mut snapshot = snapshot();
    add_png(
        directory.path(),
        &mut snapshot,
        "asymmetric",
        16,
        16,
        |x, _| {
            if x < 8 {
                [0, 255, 0, 255]
            } else {
                [255, 0, 0, 255]
            }
        },
    );
    snapshot.project.sequences[0].tracks[0]
        .elements
        .push(element("flipped", "asymmetric", 1.0, -1.0));
    let (rgba, receipt) = render(&directory.path().join("cas"), &snapshot);
    assert_eq!(pixel(&rgba, 2, 8), [255, 0, 0, 255]);
    assert_eq!(pixel(&rgba, 13, 8), [0, 255, 0, 255]);
    let quad = receipt.receipt.bound.layers[0].quad.as_ref().unwrap();
    assert!(quad.flip_x);
    assert!(!quad.flip_y);
    assert_eq!(receipt.receipt.bound.layers[0].asset_id, "asymmetric");
    assert_eq!(
        receipt.runtime_identity.identity.decoder_profile,
        opencut_editor_application::project_v4_frame_jobs::CLASSIC_VISUAL_DECODER_PROFILE_V4
    );
}

#[test]
#[ignore = "Requires installed pinned browser/WebGPU and FFmpeg; run explicitly with OPENCUT_CHROME"]
fn actual_video_trim_requests_green_second_frame_and_records_observed_pts() {
    let directory = tempfile::tempdir().unwrap();
    let raw = directory.path().join("two-colors.rgba");
    let path = directory.path().join("two-colors.webm");
    let mut bytes = Vec::new();
    for color in [[255u8, 0, 0, 255], [0, 255, 0, 255]] {
        for _ in 0..16 * 16 {
            bytes.extend(color);
        }
    }
    std::fs::write(&raw, bytes).unwrap();
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            "16x16",
            "-framerate",
            "2",
            "-i",
        ])
        .arg(&raw)
        .args([
            "-an",
            "-c:v",
            "libvpx-vp9",
            "-lossless",
            "1",
            "-pix_fmt",
            "yuv444p",
            "-color_range",
            "pc",
            "-y",
        ])
        .arg(&path)
        .output()
        .expect("Installed FFmpeg must encode the synthetic local fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let pinned = AssetStore::open(directory.path().join("cas"))
        .unwrap()
        .pin_asset(&Asset {
            id: "video".into(),
            name: "Two colors".into(),
            kind: AssetKind::Video,
            uri: path.to_string_lossy().into(),
            duration: Some(time(1, 1)),
            version: None,
            metadata: BTreeMap::new(),
        })
        .unwrap();
    let mut snapshot = snapshot();
    snapshot.project.assets.push(AssetV4 {
        id: "video".into(),
        name: "Two colors".into(),
        kind: AssetKind::Video,
        version: pinned.version.unwrap(),
        duration: Some(time(1, 1)),
        origin: ResourceOriginV4::LegacyLocator {
            uri: "unused-source.webm".into(),
        },
        metadata: MediaMetadataV4 {
            mime_type: Some("video/webm".into()),
            intrinsic_size: None,
            frame_rate: Some(FrameRate {
                numerator: 2,
                denominator: 1,
            }),
            rotation_degrees: None,
            audio: AudioPresenceV4::Absent {},
        },
        license: None,
    });
    let mut clip = element("trimmed", "video", 1.0, 1.0);
    clip.timeline_range.duration = time(1, 2);
    clip.source_span = ElementSourceSpanV4::ClassicTrimmed {
        trim_start: time(1, 2),
        trim_end: time(0, 1),
        source_duration: Some(time(1, 1)),
    };
    clip.content = ElementContentV4::Video {
        asset_id: "video".into(),
        video_stream: None,
        source_audio_enabled: false,
        audio: AudioStyleV4 {
            stream: None,
            gain: GainV4::Decibels { value: 0.0 },
            muted: false,
        },
        retime: RetimeV4 {
            rate: RationalRateV4 {
                numerator: 1,
                denominator: 1,
            },
            maintain_pitch: false,
            mapping: RetimeMappingV4::ClassicRoundedTicks { timebase: 120000 },
        },
    };
    snapshot.project.sequences[0].tracks[0].elements.push(clip);
    let (rgba, receipt) = render(&directory.path().join("cas"), &snapshot);
    // The two frames are authored red then green. A .5-second trim selects
    // green at scene 1/7; no identity/hash equality can establish this oracle.
    let actual = pixel(&rgba, 8, 8);
    assert!(
        actual[0] <= 3 && actual[1] >= 252 && actual[2] <= 3 && actual[3] == 255,
        "Expected green, observed {actual:?}"
    );
    let DecodedVisualSourceV4::ReadyVideo {
        requested_time,
        actual_timestamp_seconds,
        frame_duration_seconds,
        width,
        height,
        ..
    } = &receipt.receipt.bound.decoded[0]
    else {
        panic!("Expected an actual decoded video frame")
    };
    assert_eq!(*requested_time, time(77143, 120000));
    assert_eq!((*width, *height), (16, 16));
    assert!((*actual_timestamp_seconds - 0.5).abs() < 1e-9);
    assert!((*frame_duration_seconds - 0.5).abs() < 1e-9);
}

#[test]
#[ignore = "Requires installed pinned browser/WebGPU runtime; run explicitly with OPENCUT_CHROME"]
fn actual_gpu_clockwise_rotation_position_background_and_track_source_alpha_multiply() {
    let directory = tempfile::tempdir().unwrap();
    let mut snapshot = snapshot();
    add_png(
        directory.path(),
        &mut snapshot,
        "quadrants",
        8,
        4,
        |x, y| match (x < 4, y < 2) {
            (true, true) => [255, 0, 0, 128],
            (false, true) => [0, 255, 0, 128],
            (true, false) => [0, 0, 255, 128],
            (false, false) => [255, 255, 0, 128],
        },
    );
    let SettingsProfileV4::ClassicGlobal { settings } = &mut snapshot.project.settings else {
        panic!("Synthetic fixture requires Classic global settings")
    };
    settings.background = BackgroundV4::Color {
        color: ColorV4 {
            encoding: ColorEncodingV4::Srgb,
            red: 0.0,
            green: 0.0,
            blue: 1.0,
            alpha: 1.0,
        },
    };
    let track = &mut snapshot.project.sequences[0].tracks[0];
    track.opacity = 0.5;
    let mut quad = element("rotated", "quadrants", 1.0, 1.0);
    let visual = quad.visual.as_mut().unwrap();
    visual.transform.position_x_px = 2.0;
    visual.transform.rotation_degrees = 90.0;
    track.elements.push(quad);
    let (rgba, receipt) = render(&directory.path().join("cas"), &snapshot);
    // With screen Y down, +90 degrees sends TL red to upper right, TR green
    // to lower right, BL blue to upper left and BR yellow to lower left.
    // Source alpha128 times track .5 yields 64/255 over opaque blue. All
    // chosen pixels are inside their quadrants, away from filtering edges.
    assert_eq!(pixel(&rgba, 7, 2), [0, 0, 255, 255]);
    assert_eq!(pixel(&rgba, 12, 2), [64, 0, 191, 255]);
    assert_eq!(pixel(&rgba, 7, 13), [64, 64, 191, 255]);
    assert_eq!(pixel(&rgba, 12, 13), [0, 64, 191, 255]);
    assert_eq!(pixel(&rgba, 2, 8), [0, 0, 255, 255]);
    let bound = &receipt.receipt.bound;
    assert_eq!(bound.background, [0.0, 0.0, 1.0, 1.0]);
    let quad = bound.layers[0].quad.as_ref().unwrap();
    assert_eq!(
        (quad.center_x, quad.center_y, quad.width, quad.height),
        (10.0, 8.0, 16.0, 8.0)
    );
    assert_eq!(quad.rotation_radians, std::f32::consts::FRAC_PI_2);
    assert_eq!(quad.opacity, 0.5);
    assert!(
        matches!(&bound.decoded[0], DecodedVisualSourceV4::ReadyImage { element_id, width: 8, height: 4 } if element_id == "rotated")
    );
}
