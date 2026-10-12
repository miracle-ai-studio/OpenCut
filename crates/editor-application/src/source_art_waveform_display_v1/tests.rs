use super::*;
use crate::source_art_v1::{SOURCE_PEAKS_ALGORITHM_V1, SourceArtPolicyV1};
use opencut_contracts::{AssetKind, AssetVersion, CreateProject, FrameRate, Time, TimeRange};
use std::cell::Cell;

fn t(ticks: i64, timebase: u32) -> Time {
    Time {
        ticks: ticks.to_string(),
        timebase,
    }
}
struct Fixture {
    captured: AuthorizedSourceArtV1,
    revoked: bool,
    reads: Cell<usize>,
}
impl Fixture {
    fn new() -> Self {
        let mut project = crate::project_v4::new_document(&CreateProject {
            id: "project".into(),
            name: "Waveform".into(),
            sequence_id: "scene".into(),
            width: 16,
            height: 16,
            fps: FrameRate {
                numerator: 30,
                denominator: 1,
            },
        })
        .unwrap();
        project.assets.push(AssetV4 {
            id: "asset".into(),
            name: "Audio".into(),
            kind: AssetKind::Audio,
            version: AssetVersion {
                id: format!("av_{}", "a".repeat(64)),
                content_hash: "a".repeat(64),
                size: 1,
            },
            duration: Some(t(4, 1)),
            origin: ResourceOriginV4::LocalImport {
                original_name: "source.wav".into(),
            },
            metadata: MediaMetadataV4 {
                mime_type: Some("audio/wav".into()),
                intrinsic_size: None,
                frame_rate: None,
                rotation_degrees: None,
                audio: AudioPresenceV4::Present { metadata: None },
            },
            license: None,
        });
        let mut track = project.sequences[0].tracks[0].clone();
        track.id = "audio".into();
        track.kind = TrackKindV4::Audio;
        // These track facts must not multiply/suppress the Classic clip display.
        track.muted = true;
        track.enabled = false;
        track.gain = GainV4::Decibels { value: -20.0 };
        track.elements.push(TimelineElementV4 {
            id: "element".into(),
            name: "Audio".into(),
            timeline_range: TimeRange {
                start: t(5, 1),
                duration: t(1, 1),
            },
            source_span: ElementSourceSpanV4::ClassicTrimmed {
                trim_start: t(1, 1),
                trim_end: t(2, 1),
                source_duration: Some(t(4, 1)),
            },
            enabled: false,
            content: ElementContentV4::Audio {
                asset_id: "asset".into(),
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
            },
            visual: None,
            channels: vec![],
        });
        project.sequences[0].tracks.push(track);
        Self {
            captured: AuthorizedSourceArtV1 {
                snapshot: ProjectSnapshotV4 {
                    project,
                    revision: "fixed".into(),
                },
                authority: TrustedJobAuthorityV1::LocalOwner,
                scope: "owner/project".into(),
                policy: SourceArtPolicyV1 {
                    max_encoded_bytes: 32 * 1024 * 1024,
                    max_pcm_bytes: 256 * 1024 * 1024,
                    max_rss_bytes: 2 * 1024 * 1024 * 1024,
                    deadline_ms: 60000,
                },
            },
            revoked: false,
            reads: Cell::new(0),
        }
    }
    fn element(&mut self) -> &mut TimelineElementV4 {
        &mut self.captured.snapshot.project.sequences[0].tracks[1].elements[0]
    }
    fn audio(&mut self) -> &mut AudioStyleV4 {
        let ElementContentV4::Audio { audio, .. } = &mut self.element().content else {
            unreachable!()
        };
        audio
    }
    fn request(&self) -> SourceArtWaveformDisplayRequestV1 {
        SourceArtWaveformDisplayRequestV1 {
            project_id: "project".into(),
            revision: "fixed".into(),
            asset_id: "asset".into(),
            request_id: "waiter".into(),
            sample_rate: 48000,
            sequence_id: "scene".into(),
            track_id: "audio".into(),
            element_id: "element".into(),
            viewport: SourceArtWaveformViewportV1 {
                clip_left_px: 0.0,
                clip_right_px: 2.0,
                pixels_per_second: 100.0,
                css_height: 20.0,
                device_pixel_ratio: 1.0,
            },
            temporary_gain: None,
        }
    }
    fn decoded(&self, peak: f32) -> DecodedSourceArtV1 {
        DecodedSourceArtV1::Waveform {
            identity: SourceArtIdentityV1 {
                algorithm: SOURCE_PEAKS_ALGORITHM_V1.into(),
                browser_version: "pinned".into(),
                browser_sha256: "b".repeat(64),
                node_sha256: "c".repeat(64),
                module_sha256: "d".repeat(64),
            },
            version: self.captured.snapshot.project.assets[0].version.clone(),
            sample_rate: 48000,
            channels: 3,
            total_samples: 192000,
            bucket_size: 128,
            amplitudes: vec![peak; 1500],
        }
    }
    fn display(
        &self,
        input: &SourceArtWaveformDisplayRequestV1,
        peak: f32,
    ) -> SourceArtWaveformDisplayResponseV1 {
        project_waveform_display(
            &prepare_waveform_display(self, input).unwrap(),
            &self.decoded(peak),
        )
        .unwrap()
    }
}
impl SourceArtReadV1 for Fixture {
    fn fixed_source_art_snapshot(
        &self,
        _: &str,
        _: &str,
    ) -> Result<AuthorizedSourceArtV1, EditorError> {
        self.reads.set(self.reads.get() + 1);
        Ok(self.captured.clone())
    }
    fn revalidate_source_art(
        &self,
        _: &str,
        _: &str,
        _: &TrustedJobAuthorityV1,
    ) -> Result<(), EditorError> {
        if self.revoked {
            Err(EditorError::new(
                "NOT_AUTHORIZED",
                "Original authority revoked",
            ))
        } else {
            Ok(())
        }
    }
}
fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

#[test]
fn independent_db_floor_height_burn_and_element_mute_oracle() {
    let mut fixture = Fixture::new();
    let input = fixture.request();
    let silent = fixture.display(&input, 0.01);
    assert_eq!(silent.bars[0].device_height, 0);
    assert_eq!(silent.bars[0].height_fraction, 0.0);
    let quiet = fixture.display(&input, 0.1);
    close(quiet.bars[0].height_fraction, 0.3535533905932738);
    assert_eq!(
        (quiet.bars[0].device_top, quiet.bars[0].device_height),
        (13, 7)
    );
    // Float32 1 is exact, avoiding a decimal .1*10 threshold near 1.
    let unity = fixture.display(&input, 1.0);
    assert_eq!(unity.bars[0].output_amplitude, 1.0);
    assert!(!unity.bars[0].burn);
    let burned = fixture.display(&input, 1.25);
    assert!(burned.bars[0].burn);
    assert_eq!(
        (burned.bars[0].device_height, burned.bars[0].burn_height),
        (20, 1)
    );
    fixture.audio().muted = true;
    let muted = fixture.display(&input, 1.25);
    assert_eq!(
        (
            muted.bars[0].gain,
            muted.bars[0].device_height,
            muted.bars[0].burn
        ),
        (0.0, 0, false)
    );
    fixture.audio().muted = false;
    fixture.audio().gain = GainV4::Decibels { value: -60.0 };
    let low_gain = fixture.display(&input, 1.25);
    close(low_gain.bars[0].gain, 0.001);
    assert_eq!(low_gain.bars[0].device_height, 0);
    assert_eq!(
        fixture.reads.get(),
        6,
        "one fixed snapshot read per preparation"
    );
}

#[test]
fn whole_clip_midpoint_amplitude_interpolation_preserves_trim_rate_and_rejects_invalid_overlays() {
    let mut fixture = Fixture::new();
    fixture.element().timeline_range.duration = t(4, 3);
    fixture.element().source_span = ElementSourceSpanV4::ClassicTrimmed {
        trim_start: t(1, 1),
        trim_end: t(1, 1),
        source_duration: Some(t(4, 1)),
    };
    if let ElementContentV4::Audio { retime, .. } = &mut fixture.element().content {
        retime.rate = RationalRateV4 {
            numerator: 3,
            denominator: 2,
        };
    }
    fixture.element().channels.push(AnimationChannelV4::Scalar {
        id: "gain".into(),
        target: ChannelTargetV4::Element {
            element_id: "element".into(),
        },
        binding: ScalarBindingV4::Property {
            parameter: ScalarParameterV4::GainDecibels,
        },
        keys: vec![
            ScalarKeyV4 {
                id: "low".into(),
                segment_id: None,
                time: t(0, 120000),
                value: -20.0,
                left_handle: None,
                right_handle: None,
                segment_to_next: ScalarSegmentV4::Step,
                tangent_mode: TangentModeV4::Auto,
            },
            ScalarKeyV4 {
                id: "high".into(),
                segment_id: None,
                time: t(80000, 120000),
                value: 0.0,
                left_handle: None,
                right_handle: None,
                segment_to_next: ScalarSegmentV4::Step,
                tangent_mode: TangentModeV4::Auto,
            },
        ],
        extrapolation: ExtrapolationV4 {
            before: ExtrapolationModeV4::Hold,
            after: ExtrapolationModeV4::Hold,
        },
    });
    let original = fixture.captured.snapshot.clone();
    let mut input = fixture.request();
    // The first bar center is exactly D/2: left 39.5px, pps60, D4/3.
    input.viewport.clip_left_px = 39.5;
    input.viewport.clip_right_px = 41.5;
    input.viewport.pixels_per_second = 60.0;
    let prepared = prepare_waveform_display(&fixture, &input).unwrap();
    let displayed = project_waveform_display(&prepared, &fixture.decoded(1.0)).unwrap();
    close(displayed.bars[0].gain, 0.55); // .1 + (1-.1)*.5, not dB interpolation.
    assert_eq!(displayed.bars[0].source_start_sample, "95400");
    assert_eq!(displayed.bars[0].source_end_sample, "97800");
    assert_eq!(fixture.captured.snapshot, original);
    let samples = projection::gain_samples(&prepared).unwrap();
    close(samples[99], 0.1);
    close(samples[100], 1.0);
    assert_eq!(samples.len(), 200);
    input.temporary_gain = Some(0);
    assert!(
        prepare_waveform_display(&fixture, &input).is_err(),
        "animated static override must fail"
    );
    fixture.element().channels.clear();
    let changed = fixture.display(&input, 1.0);
    assert_eq!(changed.bars[0].gain, 1.0);
    fixture.captured.snapshot.project.sequences[0].tracks[1].locked = true;
    assert_eq!(
        prepare_waveform_display(&fixture, &input).unwrap_err().code,
        "LOCKED_TRACK"
    );
    fixture.captured.snapshot.project.sequences[0].tracks[1].locked = false;
    fixture.audio().gain = GainV4::LinearLegacy { value: 1.0 };
    assert_eq!(
        prepare_waveform_display(&fixture, &input).unwrap_err().code,
        "UNSUPPORTED_WAVEFORM_DISPLAY_PROFILE"
    );
    fixture.audio().gain = GainV4::Decibels { value: 0.0 };
    input.revision = "wrong".into();
    assert_eq!(
        prepare_waveform_display(&fixture, &input).unwrap_err().code,
        "STORAGE_ERROR"
    );
    input.revision = "fixed".into();
    fixture.revoked = true;
    assert_eq!(
        prepare_waveform_display(&fixture, &input).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
}

#[test]
fn independent_visible_stride_device_pixels_tail_and_budget_oracle() {
    let fixture = Fixture::new();
    let mut input = fixture.request();
    input.viewport.clip_right_px = 5.7;
    input.viewport.device_pixel_ratio = 2.0;
    let displayed = fixture.display(&input, 0.1);
    assert_eq!(displayed.bars.len(), 2); // Only [0,2] and [2,4], no remainder bucket.
    assert_eq!((displayed.canvas.width, displayed.canvas.height), (12, 40));
    assert_eq!(
        (
            displayed.bars[0].device_left,
            displayed.bars[0].device_right
        ),
        (0, 2)
    );
    assert_eq!(
        (
            displayed.bars[1].device_left,
            displayed.bars[1].device_right
        ),
        (4, 6)
    );
    assert_eq!(
        (
            displayed.bars[0].device_top,
            displayed.bars[0].device_height
        ),
        (26, 14)
    );
    assert_eq!(displayed.bars[0].source_start_sample, "48000");
    assert_eq!(displayed.bars[0].source_end_sample, "48960");
    assert_eq!(displayed.bars[1].source_end_sample, "49920");
    let burned = fixture.display(&input, 1.25);
    assert_eq!(burned.bars[0].burn_height, 2);
    input.viewport.clip_right_px = 0.75;
    assert_eq!(fixture.display(&input, 0.1).bars.len(), 1);
    // A minimum one-CSS-pixel bar may start above a fractional canvas.
    // Classic Math.round(-0.5) is zero, unlike Rust's default -1 tie.
    input.viewport.css_height = 2.0 / 3.0;
    input.viewport.device_pixel_ratio = 1.0;
    let fractional = fixture.display(&input, 0.1);
    assert_eq!(fractional.canvas.height, 1);
    assert_eq!(
        (
            fractional.bars[0].device_top,
            fractional.bars[0].device_height
        ),
        (0, 1)
    );
    input.viewport.css_height = 20.0;
    input.viewport.device_pixel_ratio = 2.0;
    input.viewport.clip_left_px = 500.0;
    input.viewport.clip_right_px = 502.0;
    assert_eq!(fixture.display(&input, 0.1).bars[0].device_height, 0);
    input.viewport.clip_left_px = 0.0;
    input.viewport.clip_right_px = 8194.0;
    assert_eq!(
        validate_request(&input).unwrap_err().code,
        "SOURCE_ART_BUDGET_EXCEEDED"
    );
    input.viewport.clip_right_px = 2.0;
    input.viewport.css_height = 5_000_000.0;
    assert_eq!(
        validate_request(&input).unwrap_err().code,
        "SOURCE_ART_BUDGET_EXCEEDED"
    );
    input.viewport.css_height = 20.0;
    input.viewport.pixels_per_second = f64::NAN;
    assert_eq!(
        validate_request(&input).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
    input.viewport.pixels_per_second = 100.0;
    let prepared = prepare_waveform_display(&fixture, &input).unwrap();
    let mut mismatched = fixture.decoded(0.1);
    if let DecodedSourceArtV1::Waveform { version, .. } = &mut mismatched {
        version.size += 1;
    }
    assert_eq!(
        project_waveform_display(&prepared, &mismatched)
            .unwrap_err()
            .code,
        "ARTIFACT_VERIFICATION_FAILED"
    );
}
