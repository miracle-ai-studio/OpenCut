use super::*;
use opencut_contracts::{AssetVersion, CreateProject, FrameRate, Time, TimeRange};
use std::cell::Cell;

fn t(ticks: i64, timebase: u32) -> Time {
    Time {
        ticks: ticks.to_string(),
        timebase,
    }
}
fn range(start: i64, duration: i64, base: u32) -> TimeRange {
    TimeRange {
        start: t(start, base),
        duration: t(duration, base),
    }
}
fn policy() -> SourceArtPolicyV1 {
    SourceArtPolicyV1 {
        max_encoded_bytes: 32 * 1024 * 1024,
        max_pcm_bytes: 256 * 1024 * 1024,
        max_rss_bytes: 2 * 1024 * 1024 * 1024,
        deadline_ms: 60000,
    }
}
fn identity(algorithm: &str) -> SourceArtIdentityV1 {
    SourceArtIdentityV1 {
        algorithm: algorithm.into(),
        browser_version: "pinned-browser".into(),
        browser_sha256: "a".repeat(64),
        node_sha256: "b".repeat(64),
        module_sha256: "c".repeat(64),
    }
}
struct Fixture {
    authorized: AuthorizedSourceArtV1,
    checks: Cell<usize>,
    revoked: bool,
}
impl Fixture {
    fn new() -> Self {
        let mut project = crate::project_v4::new_document(&CreateProject {
            id: "project".into(),
            name: "Source art".into(),
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
            name: "Source".into(),
            kind: AssetKind::Audio,
            version: AssetVersion {
                id: format!("av_{}", "d".repeat(64)),
                content_hash: "d".repeat(64),
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
        track.elements.push(TimelineElementV4 {
            id: "voice".into(),
            name: "Source voice".into(),
            timeline_range: range(1, 4, 3),
            source_span: ElementSourceSpanV4::ClassicTrimmed {
                trim_start: t(1, 1),
                trim_end: t(1, 1),
                source_duration: Some(t(4, 1)),
            },
            enabled: true,
            content: ElementContentV4::Audio {
                asset_id: "asset".into(),
                audio: AudioStyleV4 {
                    stream: None,
                    gain: GainV4::Decibels { value: -6.0 },
                    muted: true,
                },
                retime: RetimeV4 {
                    rate: RationalRateV4 {
                        numerator: 3,
                        denominator: 2,
                    },
                    maintain_pitch: true,
                    mapping: RetimeMappingV4::ClassicRoundedTicks { timebase: 120000 },
                },
            },
            visual: None,
            channels: vec![],
        });
        project.sequences[0].tracks.push(track);
        opencut_editor_core::project_v4::validate_project_v4(&project).unwrap();
        Self {
            authorized: AuthorizedSourceArtV1 {
                snapshot: ProjectSnapshotV4 {
                    project,
                    revision: "fixed".into(),
                },
                authority: TrustedJobAuthorityV1::LocalOwner,
                scope: "owner/project".into(),
                policy: policy(),
            },
            checks: Cell::new(0),
            revoked: false,
        }
    }
    fn request(&self, projection: WaveformProjectionV1) -> SourceArtRequestV1 {
        SourceArtRequestV1 {
            project_id: "project".into(),
            revision: "fixed".into(),
            asset_id: "asset".into(),
            request_id: "waiter".into(),
            query: SourceArtQueryV1::Waveform {
                sample_rate: 48000,
                projection,
            },
        }
    }
}
impl SourceArtReadV1 for Fixture {
    fn fixed_source_art_snapshot(
        &self,
        _: &str,
        _: &str,
    ) -> Result<AuthorizedSourceArtV1, EditorError> {
        Ok(self.authorized.clone())
    }
    fn revalidate_source_art(
        &self,
        _: &str,
        _: &str,
        _: &TrustedJobAuthorityV1,
    ) -> Result<(), EditorError> {
        self.checks.set(self.checks.get() + 1);
        if self.revoked {
            Err(EditorError::new(
                "NOT_AUTHORIZED",
                "Revoked original caller",
            ))
        } else {
            Ok(())
        }
    }
}
fn source(bins: u32) -> WaveformProjectionV1 {
    WaveformProjectionV1::Source { range: None, bins }
}
fn waveform(total: u64, amplitudes: Vec<f32>) -> DecodedSourceArtV1 {
    DecodedSourceArtV1::Waveform {
        identity: identity(SOURCE_PEAKS_ALGORITHM_V1),
        version: AssetVersion {
            id: format!("av_{}", "d".repeat(64)),
            content_hash: "d".repeat(64),
            size: 1,
        },
        sample_rate: 48000,
        channels: 3,
        total_samples: total,
        bucket_size: 128,
        amplitudes,
    }
}
fn projected(prepared: &PreparedSourceArtV1, decoded: &DecodedSourceArtV1) -> Vec<f32> {
    let response = project_source_art(prepared, decoded).unwrap();
    assert_eq!(response.schema_version, 1);
    let SourceArtDataV1::Waveform { amplitudes, .. } = response.data else {
        panic!("waveform expected")
    };
    amplitudes
}

#[test]
fn whole_source_projection_preserves_all_channel_peaks_and_eof() {
    let fixture = Fixture::new();
    let prepared = prepare_source_art(&fixture, &fixture.request(source(4))).unwrap();
    let decoded = waveform(512, vec![0.25, 0.75, 1.25, 0.125]);
    assert_eq!(
        projected(&prepared, &decoded),
        vec![0.25, 0.75, 1.25, 0.125]
    );
    // Source peaks are unaffected by this element's mute, -6dB or keepPitch.
    let request = fixture.request(WaveformProjectionV1::Source {
        range: Some(range(511, 3, 48000)),
        bins: 3,
    });
    let prepared = prepare_source_art(&fixture, &request).unwrap();
    assert_eq!(projected(&prepared, &decoded), vec![0.125, 0.0, 0.0]);
    assert_eq!(projected(&prepared, &waveform(0, vec![0.0])), vec![0.0; 3]);
}

#[test]
fn element_nonzero_trim_fractional_rate_and_final_partial_bin_have_independent_sample_oracle() {
    let fixture = Fixture::new();
    let request = fixture.request(WaveformProjectionV1::Element {
        sequence_id: "scene".into(),
        track_id: "audio".into(),
        element_id: "voice".into(),
        local_range: range(1, 257, 48000),
        bins: 3,
        bin_duration: t(128, 48000),
    });
    let prepared = prepare_source_art(&fixture, &request).unwrap();
    let mut peaks = vec![0.0; 1500];
    peaks[375] = 0.25;
    peaks[376] = 0.5;
    peaks[377] = 0.75;
    peaks[378] = 1.0;
    peaks[379] = 9.0;
    // 48000 + localSamples*3/2 gives sample windows [48001,48194),
    // [48193,48386), [48385,48387). Intersecting source128 buckets are
    // [375,377), [376,379), [378,379); the 9.0 peak is outside the clipped end.
    assert_eq!(
        projected(&prepared, &waveform(192000, peaks)),
        vec![0.5, 1.0, 1.0]
    );
    let mut invalid = request.clone();
    if let SourceArtQueryV1::Waveform {
        projection: WaveformProjectionV1::Element { bins, .. },
        ..
    } = &mut invalid.query
    {
        *bins = 2;
    }
    assert_eq!(
        validate_request(&invalid).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
    let mut beyond = request;
    if let SourceArtQueryV1::Waveform {
        projection:
            WaveformProjectionV1::Element {
                local_range,
                bins,
                bin_duration,
                ..
            },
        ..
    } = &mut beyond.query
    {
        *local_range = range(1, 1, 1);
        *bins = 1;
        *bin_duration = t(1, 1);
    }
    assert_eq!(
        prepare_source_art(&fixture, &beyond).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
}

#[test]
fn element_source_projection_uses_classic_effective_rate_without_mutating_authored_clock() {
    let mut fixture = Fixture::new();
    let element = &mut fixture.authorized.snapshot.project.sequences[0].tracks[1].elements[0];
    element.timeline_range.duration = t(1, 4);
    if let ElementContentV4::Audio { retime, .. } = &mut element.content {
        retime.rate = RationalRateV4 {
            numerator: 8,
            denominator: 1,
        };
    }
    let original = fixture.authorized.snapshot.clone();
    let request = fixture.request(WaveformProjectionV1::Element {
        sequence_id: "scene".into(),
        track_id: "audio".into(),
        element_id: "voice".into(),
        local_range: range(0, 128, 48000),
        bins: 1,
        bin_duration: t(128, 48000),
    });
    let prepared = prepare_source_art(&fixture, &request).unwrap();
    let mut peaks = vec![0.0; 1500];
    peaks[379] = 0.75;
    peaks[380] = 8.0;
    // Effective rate5: source [48000,48640), exactly five128 buckets.
    assert_eq!(projected(&prepared, &waveform(192000, peaks)), vec![0.75]);
    assert_eq!(fixture.authorized.snapshot, original);
}

#[test]
fn preparation_fences_fixed_scope_and_revoked_original_authority() {
    let mut fixture = Fixture::new();
    let request = fixture.request(source(1));
    fixture.revoked = true;
    assert_eq!(
        prepare_source_art(&fixture, &request).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(fixture.checks.get(), 1);
    fixture.revoked = false;
    fixture.authorized.snapshot.revision = "different".into();
    assert_eq!(
        prepare_source_art(&fixture, &request).unwrap_err().code,
        "STORAGE_ERROR"
    );
    fixture.authorized.snapshot.revision = "fixed".into();
    fixture.authorized.scope = "x".repeat(4096);
    assert!(prepare_source_art(&fixture, &request).is_ok());
    fixture.authorized.scope.push('x');
    assert_eq!(
        prepare_source_art(&fixture, &request).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
    assert!(
        validate_cancel(&SourceArtCancelV1 {
            project_id: "project".into(),
            revision: "fixed".into(),
            request_id: "waiter".into()
        })
        .is_ok()
    );
}

#[test]
fn raw_cache_identity_includes_source_facts_policy_and_authority_but_not_viewport_or_waiter() {
    let fixture = Fixture::new();
    let prepared = prepare_source_art(&fixture, &fixture.request(source(4))).unwrap();
    let runtime = identity(SOURCE_PEAKS_ALGORITHM_V1);
    let key = source_art_decode_key(&prepared, &runtime).unwrap();
    let mut same = prepared.clone();
    same.request.request_id = "other-waiter".into();
    same.request.revision = "historical".into();
    same.request.query = SourceArtQueryV1::Waveform {
        sample_rate: 48000,
        projection: source(1),
    };
    assert_eq!(source_art_decode_key(&same, &runtime).unwrap(), key);
    for changed in 0..6 {
        let mut other = prepared.clone();
        let mut runtime = runtime.clone();
        match changed {
            0 => other.scope = "credential2/project".into(),
            1 => other.asset.kind = AssetKind::Video,
            2 => other.asset.metadata.audio = AudioPresenceV4::Unknown {},
            3 => other.asset.version.size += 1,
            4 => other.policy.deadline_ms -= 1,
            _ => runtime.module_sha256 = "e".repeat(64),
        }
        assert_ne!(source_art_decode_key(&other, &runtime).unwrap(), key);
    }
}

#[test]
fn malformed_or_over_budget_waveform_receipts_are_rejected_before_projection() {
    let fixture = Fixture::new();
    let prepared = prepare_source_art(&fixture, &fixture.request(source(1))).unwrap();
    for decoded in [
        waveform(129, vec![0.5]),
        waveform(128, vec![f32::NAN]),
        waveform(128, vec![-0.5]),
    ] {
        assert_eq!(
            project_source_art(&prepared, &decoded).unwrap_err().code,
            "ARTIFACT_VERIFICATION_FAILED"
        );
    }
    let mut decoded = waveform(128, vec![0.5]);
    if let DecodedSourceArtV1::Waveform { sample_rate, .. } = &mut decoded {
        *sample_rate = 44100;
    }
    assert_eq!(
        project_source_art(&prepared, &decoded).unwrap_err().code,
        "ARTIFACT_VERIFICATION_FAILED"
    );
    let mut wrong_version = waveform(128, vec![0.5]);
    if let DecodedSourceArtV1::Waveform { version, .. } = &mut wrong_version {
        version.size += 1;
    }
    assert_eq!(
        project_source_art(&prepared, &wrong_version)
            .unwrap_err()
            .code,
        "ARTIFACT_VERIFICATION_FAILED"
    );
    let mut bounded = prepared.clone();
    bounded.policy.max_pcm_bytes = 1538; // 128*3*4+summary4+encoded3=1543.
    assert_eq!(
        project_source_art(&bounded, &waveform(128, vec![0.5]))
            .unwrap_err()
            .code,
        "SOURCE_ART_BUDGET_EXCEEDED"
    );
    let mut request = fixture.request(source(0));
    assert_eq!(
        validate_request(&request).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
    request.query = SourceArtQueryV1::Waveform {
        sample_rate: 16000,
        projection: source(1),
    };
    assert_eq!(
        validate_request(&request).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
}

// A frame header is sufficient for the pure receipt dimension check. This
// fixture asserts validation only, not successful JPEG decoding or rendering.
fn jpeg_header(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = vec![0xff, 0xd8, 0xff, 0xc0, 0, 17, 8];
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&[3, 1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0, 0xff, 0xd9]);
    bytes
}

#[test]
fn thumbnail_receipt_checks_classic_fit_actual_bytes_and_explicit_missing_sample() {
    let mut fixture = Fixture::new();
    fixture.authorized.snapshot.project.assets[0].kind = AssetKind::Video;
    let mut request = fixture.request(source(1));
    request.query = SourceArtQueryV1::Thumbnail {};
    let prepared = prepare_source_art(&fixture, &request).unwrap();
    let observation = SourceArtThumbnailObservationV1::Video {
        track_id: 7,
        source_width: 1500,
        source_height: 3000,
        requested_seconds: 1.0,
        actual_timestamp_seconds: Some(0.96),
        frame_duration_seconds: Some(0.04),
    };
    assert_eq!(validation::thumbnail_size(3000, 6000).unwrap(), (360, 720));
    assert_eq!(validation::thumbnail_size(8, 4).unwrap(), (8, 4));
    let decoded = DecodedSourceArtV1::ThumbnailReady {
        identity: identity(THUMBNAIL_ALGORITHM_V1),
        version: prepared.asset().version.clone(),
        width: 360,
        height: 720,
        jpeg_bytes: jpeg_header(360, 720),
        observation: observation.clone(),
    };
    let response = project_source_art(&prepared, &decoded).unwrap();
    let SourceArtDataV1::ThumbnailReady {
        jpeg_bytes_hex,
        content_hash,
        ..
    } = response.data
    else {
        panic!("JPEG expected")
    };
    assert!(jpeg_bytes_hex.starts_with("ffd8"));
    assert!(jpeg_bytes_hex.ends_with("ffd9"));
    assert_eq!(
        content_hash,
        format!("{:x}", Sha256::digest(jpeg_header(360, 720)))
    );
    let mut wrong = decoded.clone();
    if let DecodedSourceArtV1::ThumbnailReady { jpeg_bytes, .. } = &mut wrong {
        *jpeg_bytes = jpeg_header(359, 720);
    }
    assert_eq!(
        project_source_art(&prepared, &wrong).unwrap_err().code,
        "ARTIFACT_VERIFICATION_FAILED"
    );
    let missing = DecodedSourceArtV1::ThumbnailNoFrame {
        identity: identity(THUMBNAIL_ALGORITHM_V1),
        version: prepared.asset().version.clone(),
        observation: observation.clone(),
        reason: SourceArtNoFrameReasonV1::NoSampleAtOneSecond,
    };
    assert_eq!(
        project_source_art(&prepared, &missing).unwrap_err().code,
        "ARTIFACT_VERIFICATION_FAILED"
    );
    let mut missing = missing;
    if let DecodedSourceArtV1::ThumbnailNoFrame {
        observation:
            SourceArtThumbnailObservationV1::Video {
                actual_timestamp_seconds,
                frame_duration_seconds,
                ..
            },
        ..
    } = &mut missing
    {
        *actual_timestamp_seconds = None;
        *frame_duration_seconds = None;
    }
    assert!(matches!(
        project_source_art(&prepared, &missing).unwrap().data,
        SourceArtDataV1::ThumbnailNoFrame { .. }
    ));
    let mut oversized = decoded;
    if let DecodedSourceArtV1::ThumbnailReady { jpeg_bytes, .. } = &mut oversized {
        *jpeg_bytes = vec![0; SOURCE_ART_MAX_JPEG_BYTES_V1 + 1];
    }
    assert_eq!(
        project_source_art(&prepared, &oversized).unwrap_err().code,
        "SOURCE_ART_BUDGET_EXCEEDED"
    );
}
