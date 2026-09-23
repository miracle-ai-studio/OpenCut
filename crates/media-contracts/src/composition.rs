//! Backend-independent composition primitives. Input handles are chosen by the host.
use crate::{FontResolutionPolicy, FrameRate, MediaTime, TextAlign};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Timeline<I = String> {
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    pub duration: MediaTime,
    pub video_clips: Vec<TimelineVideoClip<I>>,
    pub image_clips: Vec<TimelineImageClip<I>>,
    pub audio_clips: Vec<TimelineAudioClip<I>>,
    pub captions: Vec<TimelineCaption>,
}
impl<I> Timeline<I> {
    /// Bind logical inputs without changing timing, layers, curves or captions.
    pub fn try_map_inputs<J, E>(
        self,
        mut bind: impl FnMut(I) -> Result<J, E>,
    ) -> Result<Timeline<J>, E> {
        Ok(Timeline {
            width: self.width,
            height: self.height,
            frame_rate: self.frame_rate,
            duration: self.duration,
            video_clips: self
                .video_clips
                .into_iter()
                .map(|c| c.try_map_input(&mut bind))
                .collect::<Result<_, _>>()?,
            image_clips: self
                .image_clips
                .into_iter()
                .map(|c| c.try_map_input(&mut bind))
                .collect::<Result<_, _>>()?,
            audio_clips: self
                .audio_clips
                .into_iter()
                .map(|c| c.try_map_input(&mut bind))
                .collect::<Result<_, _>>()?,
            captions: self.captions,
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TimelineVideoClip<I = String> {
    pub input: I,
    pub source_start: MediaTime,
    pub source_duration: MediaTime,
    pub timeline_start: MediaTime,
    pub timeline_duration: MediaTime,
    #[serde(default)]
    pub stream: Option<u32>,
    #[serde(default = "default_one_f32")]
    pub opacity: f32,
    #[serde(default)]
    pub position_x: f64,
    #[serde(default)]
    pub position_y: f64,
    #[serde(default = "default_one_f64")]
    pub scale_x: f64,
    #[serde(default = "default_one_f64")]
    pub scale_y: f64,
    #[serde(default)]
    pub rotation_degrees: f64,
    #[serde(default)]
    pub z_index: i32,
    #[serde(default)]
    pub curves: TimelineVisualCurves,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TimelineImageClip<I = String> {
    pub input: I,
    pub timeline_start: MediaTime,
    pub timeline_duration: MediaTime,
    #[serde(default)]
    pub stream: Option<u32>,
    #[serde(default = "default_one_f32")]
    pub opacity: f32,
    #[serde(default)]
    pub position_x: f64,
    #[serde(default)]
    pub position_y: f64,
    #[serde(default = "default_one_f64")]
    pub scale_x: f64,
    #[serde(default = "default_one_f64")]
    pub scale_y: f64,
    #[serde(default)]
    pub rotation_degrees: f64,
    #[serde(default)]
    pub z_index: i32,
    #[serde(default)]
    pub curves: TimelineVisualCurves,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TimelineVisualCurves {
    #[serde(default)]
    pub opacity: EvaluatedCurve,
    #[serde(default)]
    pub position_x: EvaluatedCurve,
    #[serde(default)]
    pub position_y: EvaluatedCurve,
    #[serde(default)]
    pub scale_x: EvaluatedCurve,
    #[serde(default)]
    pub scale_y: EvaluatedCurve,
    #[serde(default)]
    pub rotation_degrees: EvaluatedCurve,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EvaluatedCurve {
    #[serde(default)]
    pub segments: Vec<EvaluatedSegment>,
}

/// A pre-evaluated affine value on the clip-local half-open interval `[start, end)`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EvaluatedSegment {
    pub start: MediaTime,
    pub end: MediaTime,
    pub start_value: f64,
    pub end_value: f64,
}
fn default_one_f32() -> f32 {
    1.0
}
fn default_one_f64() -> f64 {
    1.0
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TimelineAudioClip<I = String> {
    pub input: I,
    pub source_start: MediaTime,
    pub source_duration: MediaTime,
    pub timeline_start: MediaTime,
    pub timeline_duration: MediaTime,
    pub gain: f32,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub stream: Option<u32>,
    #[serde(default)]
    pub gain_curve: EvaluatedCurve,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TimelineCaption {
    pub start: MediaTime,
    pub duration: MediaTime,
    pub text: String,
    pub font_family: String,
    pub font_size: f64,
    pub color: String,
    pub background_color: Option<String>,
    #[serde(default)]
    pub font_policy: FontResolutionPolicy,
    #[serde(default)]
    pub max_width: Option<u32>,
    #[serde(default)]
    pub line_height: Option<f32>,
    #[serde(default)]
    pub text_align: TextAlign,
}

impl<I> TimelineVideoClip<I> {
    fn try_map_input<J, E>(
        self,
        bind: &mut impl FnMut(I) -> Result<J, E>,
    ) -> Result<TimelineVideoClip<J>, E> {
        Ok(TimelineVideoClip {
            input: bind(self.input)?,
            source_start: self.source_start,
            source_duration: self.source_duration,
            timeline_start: self.timeline_start,
            timeline_duration: self.timeline_duration,
            stream: self.stream,
            opacity: self.opacity,
            position_x: self.position_x,
            position_y: self.position_y,
            scale_x: self.scale_x,
            scale_y: self.scale_y,
            rotation_degrees: self.rotation_degrees,
            z_index: self.z_index,
            curves: self.curves,
        })
    }
}

impl<I> TimelineImageClip<I> {
    fn try_map_input<J, E>(
        self,
        bind: &mut impl FnMut(I) -> Result<J, E>,
    ) -> Result<TimelineImageClip<J>, E> {
        Ok(TimelineImageClip {
            input: bind(self.input)?,
            timeline_start: self.timeline_start,
            timeline_duration: self.timeline_duration,
            stream: self.stream,
            opacity: self.opacity,
            position_x: self.position_x,
            position_y: self.position_y,
            scale_x: self.scale_x,
            scale_y: self.scale_y,
            rotation_degrees: self.rotation_degrees,
            z_index: self.z_index,
            curves: self.curves,
        })
    }
}

impl<I> TimelineAudioClip<I> {
    fn try_map_input<J, E>(
        self,
        bind: &mut impl FnMut(I) -> Result<J, E>,
    ) -> Result<TimelineAudioClip<J>, E> {
        Ok(TimelineAudioClip {
            input: bind(self.input)?,
            source_start: self.source_start,
            source_duration: self.source_duration,
            timeline_start: self.timeline_start,
            timeline_duration: self.timeline_duration,
            gain: self.gain,
            optional: self.optional,
            stream: self.stream,
            gain_curve: self.gain_curve,
        })
    }
}
