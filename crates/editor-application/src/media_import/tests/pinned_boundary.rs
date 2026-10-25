use super::*;

fn boundary() -> (Fixture, MediaImportRequest) {
    let mut fixture = Fixture::new();
    fixture.media.get_mut("available").unwrap().duration = Some(t(120001, 120000));
    let mut input = fixture.request();
    let mut video = fixture.video(MediaTiming::PinnedTimeline {
        source_start: t(0, 1),
        duration: t(60001, 120000),
        rate: RationalRate {
            numerator: 2,
            denominator: 1,
        },
        maintain_pitch: true,
    });
    video.start = t(0, 1);
    input.placements.push(video);
    (fixture, input)
}

#[test]
fn classic_pinned_boundary_keeps_extent_and_nominal_request_in_public_plan() {
    let (fixture, input) = boundary();
    let result = plan(&fixture, &input).unwrap();
    let element = elements(&result)[0];
    assert_eq!(element.timeline_range.duration, t(60001, 120000));
    assert_eq!(
        element.source_span,
        ElementSourceSpan::ClassicTrimmed {
            trim_start: t(0, 1),
            trim_end: t(0, 1),
            source_duration: Some(t(120001, 120000)),
        }
    );
    assert_eq!(
        result.decisions.placements[0].source_range,
        Some(TimeRange {
            start: t(0, 1),
            duration: t(120002, 120000),
        })
    );
    let public = crate::scene::public::plan_public(&fixture, &result.transaction).unwrap();
    assert_eq!(public.sequences[0].tracks[0].elements[0], *element);
    assert_eq!(fixture.snapshot.project.assets.len(), 0);
}

#[test]
fn pinned_boundary_exception_rejects_other_overshoot_and_exact_legacy_profile() {
    let (mut fixture, mut input) = boundary();
    let MediaPlacementContent::Video { timing, .. } = &mut input.placements[0].content else {
        panic!()
    };
    *timing = MediaTiming::PinnedTimeline {
        source_start: t(0, 1),
        duration: t(60002, 120000),
        rate: RationalRate {
            numerator: 2,
            denominator: 1,
        },
        maintain_pitch: true,
    };
    assert_eq!(plan(&fixture, &input).unwrap_err().code, "INVALID_ARGUMENT");
    let MediaPlacementContent::Video { timing, .. } = &mut input.placements[0].content else {
        panic!()
    };
    *timing = MediaTiming::SourceRange {
        range: TimeRange {
            start: t(0, 1),
            duration: t(120002, 120000),
        },
        rate: RationalRate {
            numerator: 2,
            denominator: 1,
        },
        maintain_pitch: true,
    };
    assert_eq!(plan(&fixture, &input).unwrap_err().code, "INVALID_ARGUMENT");
    let SettingsProfile::ClassicGlobal { settings } = fixture.snapshot.project.settings.clone()
    else {
        panic!()
    };
    fixture.snapshot.project.settings = SettingsProfile::LegacyPerScene {
        formats: vec![LegacySequenceFormat {
            sequence_id: "s".into(),
            fps: settings.fps,
            canvas_size: settings.canvas_size,
            background: settings.background,
        }],
    };
    let MediaPlacementContent::Video { timing, .. } = &mut input.placements[0].content else {
        panic!()
    };
    *timing = MediaTiming::PinnedTimeline {
        source_start: t(0, 1),
        duration: t(60001, 120000),
        rate: RationalRate {
            numerator: 2,
            denominator: 1,
        },
        maintain_pitch: true,
    };
    assert_eq!(plan(&fixture, &input).unwrap_err().code, "INVALID_ARGUMENT");
}
