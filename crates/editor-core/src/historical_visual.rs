//! Pure schema 3 visual-graph validation. Schema 2 editing remains untouched.
use opencut_contracts::{
    AssetKind, Clip, EditorError, ProjectDocument, Time, TrackKind,
    historical_visual::{
        EffectNode, HistoricalVisualDocument, HistoricalVisualKeyframeCurve,
        HistoricalVisualSequenceVisual, MaskNode, MaskShape, ShapePrimitive, TransitionNode,
        VisualEffect,
    },
};
use opencut_time::{RationalTime, range_end, validate_range};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

fn invalid(message: impl Into<String>) -> EditorError {
    EditorError::new("INVALID_PROJECT", message)
}
fn time(value: &Time) -> Result<RationalTime, EditorError> {
    RationalTime::try_from(value).map_err(|_| invalid("invalid schema 3 time"))
}
fn bounded(value: f64, low: f64, high: f64) -> bool {
    value.is_finite() && value >= low && value <= high
}
fn unique(ids: &mut HashSet<String>, id: &str) -> Result<(), EditorError> {
    if id.is_empty() || id.len() > 128 || !ids.insert(id.into()) {
        return Err(invalid(format!("invalid or duplicate visual ID: {id}")));
    }
    Ok(())
}
fn visual_ids(base: &ProjectDocument) -> HashSet<String> {
    let mut ids = HashSet::from([base.id.clone()]);
    for asset in &base.assets {
        ids.insert(asset.id.clone());
    }
    for sequence in &base.sequences {
        ids.insert(sequence.id.clone());
        for cue in &sequence.caption_cues {
            ids.insert(cue.id.clone());
        }
        for track in &sequence.tracks {
            ids.insert(track.id.clone());
            for clip in &track.clips {
                ids.insert(clip.id.clone());
                for channel in &clip.channels {
                    for keyframe in &channel.keyframes {
                        ids.insert(keyframe.id.clone());
                    }
                }
            }
        }
    }
    ids
}
fn validate_shape(
    shape: &opencut_contracts::historical_visual::ShapeClip,
    width: u32,
    height: u32,
) -> Result<(), EditorError> {
    validate_range(&shape.timeline_range, true)
        .map_err(|_| invalid("invalid shape timeline range"))?;
    if time(&shape.timeline_range.start)?.ticks() < 0 {
        return Err(invalid("shape start must be nonnegative"));
    }
    let max_width = f64::from(width) * 4.0;
    let max_height = f64::from(height) * 4.0;
    if !bounded(shape.opacity, 0.0, 1.0)
        || !bounded(shape.position_x, -max_width, max_width)
        || !bounded(shape.position_y, -max_height, max_height)
        || !bounded(shape.rotation_degrees, -360.0, 360.0)
    {
        return Err(invalid("shape transform is outside canvas bounds"));
    }
    let dimensions_valid = match shape.primitive {
        ShapePrimitive::Rectangle {
            width_px,
            height_px,
        } => bounded(width_px, 1.0, max_width) && bounded(height_px, 1.0, max_height),
        ShapePrimitive::Ellipse {
            radius_x_px,
            radius_y_px,
        } => {
            bounded(radius_x_px, 0.5, max_width / 2.0)
                && bounded(radius_y_px, 0.5, max_height / 2.0)
        }
    };
    if !dimensions_valid {
        return Err(invalid("shape dimensions are invalid"));
    }
    Ok(())
}
fn validate_effect(effect: &EffectNode) -> Result<(), EditorError> {
    let valid = match effect.effect {
        VisualEffect::ColorAdjust {
            exposure_ev,
            contrast,
            saturation,
        } => {
            bounded(exposure_ev, -8.0, 8.0)
                && bounded(contrast, 0.0, 4.0)
                && bounded(saturation, 0.0, 4.0)
        }
        VisualEffect::GaussianBlur { radius_px } => bounded(radius_px, 0.0, 128.0),
    };
    if !valid {
        return Err(invalid("visual effect parameter is invalid"));
    }
    Ok(())
}
fn validate_mask(mask: &MaskNode) -> Result<(), EditorError> {
    let (x, y, width, height) = match mask.shape {
        MaskShape::Rectangle {
            x,
            y,
            width,
            height,
        }
        | MaskShape::Ellipse {
            x,
            y,
            width,
            height,
        } => (x, y, width, height),
    };
    if !bounded(x, 0.0, 1.0)
        || !bounded(y, 0.0, 1.0)
        || !bounded(width, f64::EPSILON, 1.0)
        || !bounded(height, f64::EPSILON, 1.0)
        || x + width > 1.0
        || y + height > 1.0
        || !bounded(mask.feather, 0.0, 0.5)
    {
        return Err(invalid("visual mask bounds or feather are invalid"));
    }
    Ok(())
}
fn half(duration: &Time) -> Result<RationalTime, EditorError> {
    let duration = time(duration)?;
    if duration.ticks() <= 0 {
        return Err(invalid("transition duration must be positive"));
    }
    duration
        .checked_scale(RationalTime::new(1, 1)?, RationalTime::new(2, 1)?)
        .map_err(|_| invalid("transition duration overflows"))
}
fn clip<'a>(
    clips: &'a HashMap<&str, (&str, &'a Clip)>,
    id: &str,
) -> Result<(&'a str, &'a Clip), EditorError> {
    clips
        .get(id)
        .copied()
        .ok_or_else(|| invalid(format!("transition clip {id} is missing")))
}
fn validate_transition(
    node: &TransitionNode,
    clips: &HashMap<&str, (&str, &Clip)>,
    base: &ProjectDocument,
) -> Result<(), EditorError> {
    if node.outgoing_clip_id == node.incoming_clip_id {
        return Err(invalid("transition endpoints must differ"));
    }
    let (out_track, outgoing) = clip(clips, &node.outgoing_clip_id)?;
    let (in_track, incoming) = clip(clips, &node.incoming_clip_id)?;
    if out_track != in_track {
        return Err(invalid("transition clips must share a track"));
    }
    let cut = time(&range_end(&outgoing.timeline_range)?)?;
    if cut.cmp_exact(time(&incoming.timeline_range.start)?) != Ordering::Equal {
        return Err(invalid("transition clips must meet at one cut"));
    }
    let half = half(&node.duration)?;
    for clip in [outgoing, incoming] {
        if time(&clip.timeline_range.duration)?.cmp_exact(half) == Ordering::Less {
            return Err(invalid("transition half exceeds a visible clip"));
        }
        let source = clip
            .source_range
            .as_ref()
            .ok_or_else(|| invalid("transition requires media source ranges"))?;
        if time(&source.duration)?.cmp_exact(time(&clip.timeline_range.duration)?)
            != Ordering::Equal
        {
            return Err(invalid(
                "transition currently requires one-to-one source time",
            ));
        }
    }
    let outgoing_asset = outgoing
        .asset_id
        .as_ref()
        .and_then(|id| base.assets.iter().find(|asset| &asset.id == id))
        .ok_or_else(|| invalid("outgoing transition asset is missing"))?;
    let incoming_asset = incoming
        .asset_id
        .as_ref()
        .and_then(|id| base.assets.iter().find(|asset| &asset.id == id))
        .ok_or_else(|| invalid("incoming transition asset is missing"))?;
    if outgoing_asset.kind != AssetKind::Video || incoming_asset.kind != AssetKind::Video {
        return Err(invalid("crossfade requires two video assets"));
    }
    let available_after = time(
        outgoing_asset
            .duration
            .as_ref()
            .ok_or_else(|| invalid("outgoing asset duration is unknown"))?,
    )?
    .checked_sub(time(&range_end(
        outgoing
            .source_range
            .as_ref()
            .expect("checked source range"),
    )?)?)?;
    let available_before = time(
        &incoming
            .source_range
            .as_ref()
            .expect("checked source range")
            .start,
    )?;
    if available_after.cmp_exact(half) == Ordering::Less
        || available_before.cmp_exact(half) == Ordering::Less
    {
        return Err(invalid("crossfade lacks verified source handles"));
    }
    Ok(())
}

pub fn validate_project_historical_visual(
    project: &HistoricalVisualDocument,
) -> Result<(), EditorError> {
    if project.schema_version != 3 || project.base.schema_version != 2 {
        return Err(invalid("schema 3 requires a schema 2 base project"));
    }
    crate::validate_project(&project.base)?;
    let mut ids = visual_ids(&project.base);
    let mut sequence_ids = HashSet::new();
    for visual in &project.visual {
        if !sequence_ids.insert(&visual.sequence_id) {
            return Err(invalid("duplicate visual sequence"));
        }
        let sequence = project
            .base
            .sequences
            .iter()
            .find(|s| s.id == visual.sequence_id)
            .ok_or_else(|| invalid("visual sequence is missing from base project"))?;
        validate_sequence(visual, sequence, &project.base, &mut ids)?;
    }
    let mut curve_keys = HashSet::new();
    if project.keyframe_curves.len() > 100_000 {
        return Err(invalid("keyframe curve count exceeds limit"));
    }
    for curve in &project.keyframe_curves {
        unique(&mut ids, &curve.id)?;
        validate_keyframe_curve(curve, &project.base, &mut curve_keys)?;
    }
    Ok(())
}

fn validate_keyframe_curve<'a>(
    curve: &'a HistoricalVisualKeyframeCurve,
    base: &ProjectDocument,
    keys: &mut HashSet<(&'a str, &'a str, &'a str, &'a str)>,
) -> Result<(), EditorError> {
    if curve.schema_version != 1
        || curve.sequence_id.is_empty()
        || curve.clip_id.is_empty()
        || curve.parameter.is_empty()
        || curve.keyframe_id.is_empty()
        || !keys.insert((
            &curve.sequence_id,
            &curve.clip_id,
            &curve.parameter,
            &curve.keyframe_id,
        ))
    {
        return Err(invalid("invalid or duplicate keyframe curve identity"));
    }
    if ![
        curve.curve.x1,
        curve.curve.y1,
        curve.curve.x2,
        curve.curve.y2,
    ]
    .into_iter()
    .all(|value| bounded(value, 0.0, 1.0))
    {
        return Err(invalid("cubic Bezier control points must be within 0..1"));
    }
    let clip = base
        .sequences
        .iter()
        .find(|sequence| sequence.id == curve.sequence_id)
        .and_then(|sequence| {
            sequence
                .tracks
                .iter()
                .flat_map(|track| track.clips.iter())
                .find(|clip| clip.id == curve.clip_id)
        })
        .ok_or_else(|| invalid("keyframe curve target clip does not exist"))?;
    let channel = clip
        .channels
        .iter()
        .find(|channel| channel.parameter == curve.parameter)
        .ok_or_else(|| invalid("keyframe curve channel does not exist"))?;
    let index = channel
        .keyframes
        .iter()
        .position(|keyframe| keyframe.id == curve.keyframe_id)
        .ok_or_else(|| invalid("keyframe curve start key does not exist"))?;
    if index + 1 >= channel.keyframes.len()
        || channel.keyframes[index].interpolation
            != opencut_contracts::KeyframeInterpolation::Linear
    {
        return Err(invalid(
            "keyframe curves require a non-final linear keyframe segment",
        ));
    }
    Ok(())
}

fn validate_sequence(
    visual: &HistoricalVisualSequenceVisual,
    sequence: &opencut_contracts::Sequence,
    base: &ProjectDocument,
    ids: &mut HashSet<String>,
) -> Result<(), EditorError> {
    if [
        visual.shapes.len(),
        visual.effects.len(),
        visual.masks.len(),
        visual.transitions.len(),
    ]
    .into_iter()
    .any(|count| count > 10_000)
    {
        return Err(invalid("visual graph node count exceeds limit"));
    }
    let mut targets = HashSet::new();
    let mut clips = HashMap::new();
    for track in &sequence.tracks {
        for clip in &track.clips {
            if matches!(track.kind, TrackKind::Video | TrackKind::Graphic) {
                targets.insert(clip.id.as_str());
                clips.insert(clip.id.as_str(), (track.id.as_str(), clip));
            }
        }
    }
    for shape in &visual.shapes {
        unique(ids, &shape.id)?;
        if !sequence
            .tracks
            .iter()
            .any(|track| track.id == shape.track_id && track.kind == TrackKind::Graphic)
        {
            return Err(invalid("shape requires a graphic track"));
        }
        validate_shape(shape, sequence.width, sequence.height)?;
        targets.insert(shape.id.as_str());
    }
    for effect in &visual.effects {
        unique(ids, &effect.id)?;
        if !targets.contains(effect.target_id.as_str()) {
            return Err(invalid("effect target is missing or not visual"));
        }
        validate_effect(effect)?;
    }
    let mut masked = HashSet::new();
    for mask in &visual.masks {
        unique(ids, &mask.id)?;
        if !targets.contains(mask.target_id.as_str()) || !masked.insert(&mask.target_id) {
            return Err(invalid("mask target is missing or has another mask"));
        }
        validate_mask(mask)?;
    }
    for transition in &visual.transitions {
        unique(ids, &transition.id)?;
        validate_transition(transition, &clips, base)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::historical_visual::{
        EffectNode, HistoricalVisualCubicBezier, HistoricalVisualDocument,
        HistoricalVisualKeyframeCurve, HistoricalVisualSequenceVisual, MaskNode, MaskShape, Rgba,
        ShapeClip, Transition, TransitionNode, VisualEffect,
    };
    use opencut_contracts::{
        Clip, CreateProject, FrameRate, Keyframe, KeyframeChannel, KeyframeInterpolation,
        TimeRange, Track,
    };
    use serde_json::json;

    fn time(ticks: i64) -> Time {
        Time {
            ticks: ticks.to_string(),
            timebase: 1000,
        }
    }
    fn range(start: i64, duration: i64) -> TimeRange {
        TimeRange {
            start: time(start),
            duration: time(duration),
        }
    }
    fn fixture() -> HistoricalVisualDocument {
        let mut base = crate::create_project(&CreateProject {
            id: "p".into(),
            name: "Project".into(),
            sequence_id: "main".into(),
            width: 640,
            height: 360,
            fps: FrameRate {
                numerator: 25,
                denominator: 1,
            },
        })
        .unwrap();
        base.sequences[0].tracks.push(Track {
            id: "graphics".into(),
            name: "Graphics".into(),
            kind: TrackKind::Graphic,
            enabled: true,
            muted: false,
            locked: false,
            clips: vec![],
            params: Default::default(),
        });
        HistoricalVisualDocument {
            schema_version: 3,
            base,
            keyframe_curves: vec![],
            visual: vec![HistoricalVisualSequenceVisual {
                sequence_id: "main".into(),
                shapes: vec![ShapeClip {
                    id: "shape".into(),
                    track_id: "graphics".into(),
                    timeline_range: range(0, 1000),
                    primitive: ShapePrimitive::Rectangle {
                        width_px: 100.0,
                        height_px: 40.0,
                    },
                    fill: Rgba {
                        red: 255,
                        green: 0,
                        blue: 0,
                        alpha: 255,
                    },
                    position_x: 0.0,
                    position_y: 0.0,
                    rotation_degrees: 0.0,
                    opacity: 1.0,
                }],
                effects: vec![EffectNode {
                    id: "grade".into(),
                    target_id: "shape".into(),
                    effect: VisualEffect::ColorAdjust {
                        exposure_ev: 1.0,
                        contrast: 1.2,
                        saturation: 0.8,
                    },
                }],
                masks: vec![MaskNode {
                    id: "mask".into(),
                    target_id: "shape".into(),
                    shape: MaskShape::Ellipse {
                        x: 0.2,
                        y: 0.2,
                        width: 0.6,
                        height: 0.6,
                    },
                    inverted: false,
                    feather: 0.0,
                }],
                transitions: vec![],
            }],
        }
    }

    fn curve_fixture() -> HistoricalVisualDocument {
        let mut project = fixture();
        let clip = Clip {
            id: "clip".into(),
            asset_id: None,
            timeline_range: range(0, 1000),
            source_range: None,
            params: Default::default(),
            channels: vec![KeyframeChannel {
                parameter: "opacity".into(),
                keyframes: vec![
                    Keyframe {
                        id: "start".into(),
                        time: time(0),
                        value: json!(0.0),
                        interpolation: KeyframeInterpolation::Linear,
                    },
                    Keyframe {
                        id: "end".into(),
                        time: time(1000),
                        value: json!(1.0),
                        interpolation: KeyframeInterpolation::Linear,
                    },
                ],
            }],
        };
        project.base.sequences[0].tracks[0].clips.push(clip);
        project.keyframe_curves.push(HistoricalVisualKeyframeCurve {
            id: "curve-opacity-start".into(),
            schema_version: 1,
            sequence_id: "main".into(),
            clip_id: "clip".into(),
            parameter: "opacity".into(),
            keyframe_id: "start".into(),
            curve: HistoricalVisualCubicBezier {
                x1: 0.0,
                y1: 0.0,
                x2: 1.0,
                y2: 0.0,
            },
        });
        project
    }

    #[test]
    fn keyframe_curve_contract_requires_a_valid_linear_segment_and_control_points() {
        assert!(validate_project_historical_visual(&curve_fixture()).is_ok());

        let mut out_of_range = curve_fixture();
        out_of_range.keyframe_curves[0].curve.x1 = 1.01;
        assert!(validate_project_historical_visual(&out_of_range).is_err());

        let mut final_key = curve_fixture();
        final_key.keyframe_curves[0].keyframe_id = "end".into();
        assert!(validate_project_historical_visual(&final_key).is_err());

        let mut wrong_version = curve_fixture();
        wrong_version.keyframe_curves[0].schema_version = 2;
        assert!(validate_project_historical_visual(&wrong_version).is_err());

        let mut duplicate_id = curve_fixture();
        duplicate_id.keyframe_curves[0].id = "clip".into();
        assert!(validate_project_historical_visual(&duplicate_id).is_err());
    }
    #[test]
    fn shape_effect_and_hard_mask_are_strict_and_versioned() {
        let mut project = fixture();
        validate_project_historical_visual(&project).unwrap();
        let mut wire = serde_json::to_value(&project).unwrap();
        wire["visual"][0]["shapes"][0]["executable"] = "/tmp/ffmpeg".into();
        assert!(serde_json::from_value::<HistoricalVisualDocument>(wire).is_err());
        project.visual[0].shapes[0].opacity = f64::NAN;
        assert_eq!(
            validate_project_historical_visual(&project)
                .unwrap_err()
                .code,
            "INVALID_PROJECT"
        );
        project.visual[0].shapes[0].opacity = 1.0;
        project.visual[0].masks[0].target_id = "missing".into();
        assert_eq!(
            validate_project_historical_visual(&project)
                .unwrap_err()
                .code,
            "INVALID_PROJECT"
        );
    }
    #[test]
    fn crossfade_rejects_missing_real_source_handles() {
        let mut project = fixture();
        project.base.assets=serde_json::from_value(serde_json::json!([
            {"id":"a","name":"A","kind":"video","uri":"asset://a","duration":{"ticks":"2000","timebase":1000},"metadata":{}},
            {"id":"b","name":"B","kind":"video","uri":"asset://b","duration":{"ticks":"2000","timebase":1000},"metadata":{}}
        ])).unwrap();
        project.base.sequences[0].tracks.push(serde_json::from_value(serde_json::json!({
            "id":"video","name":"Video","kind":"video","enabled":true,"muted":false,"locked":false,"params":{},
            "clips":[
                {"id":"out","asset_id":"a","timeline_range":{"start":time(0),"duration":time(1000)},
                 "source_range":{"start":time(0),"duration":time(1000)},"params":{},"channels":[]},
                {"id":"in","asset_id":"b","timeline_range":{"start":time(1000),"duration":time(1000)},
                 "source_range":{"start":time(0),"duration":time(1000)},"params":{},"channels":[]}
            ]})).unwrap());
        project.visual[0].transitions.push(TransitionNode {
            id: "cross".into(),
            outgoing_clip_id: "out".into(),
            incoming_clip_id: "in".into(),
            duration: time(400),
            transition: Transition::Crossfade,
        });
        assert_eq!(
            validate_project_historical_visual(&project)
                .unwrap_err()
                .code,
            "INVALID_PROJECT"
        );
        project.base.sequences[0].tracks[1].clips[0]
            .source_range
            .as_mut()
            .unwrap()
            .duration = time(800);
        project.base.sequences[0].tracks[1].clips[0]
            .source_range
            .as_mut()
            .unwrap()
            .start = time(200);
        project.base.sequences[0].tracks[1].clips[1]
            .source_range
            .as_mut()
            .unwrap()
            .start = time(200);
        // The first clip now has post-roll, the second has pre-roll.
        project.base.sequences[0].tracks[1].clips[0]
            .source_range
            .as_mut()
            .unwrap()
            .duration = time(1000);
        validate_project_historical_visual(&project).unwrap();
    }
}
