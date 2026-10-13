//! Fixed sampled text/media facts on the existing sound-consumption clock.
use crate::{
    classic_text_frame_v2::{ClassicTextFrameSessionV2, admit_text_frame_v2},
    visual_preview_v4::{VISUAL_PREVIEW_MAX_LAG_MS, validate_picture_time},
};
use opencut_api_contracts::{
    visual_preview_text_v2::*, visual_preview_v4::VisualPreviewAdmissionV4,
};
use opencut_composition::project_v4::{NativeTextCoverageV1, PreparedTextFrameV2};
use opencut_contracts::{EditorError, Time};
use opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1;
use opencut_time::RationalTime;

pub struct PreparedTextPictureV2 {
    prepared: PreparedTextFrameV2,
    policy: LocalVisualExecutionPolicyV1,
    coverage: Vec<NativeTextCoverageV1>,
}
impl PreparedTextPictureV2 {
    pub fn new(
        prepared: PreparedTextFrameV2,
        policy: LocalVisualExecutionPolicyV1,
        coverage: Vec<NativeTextCoverageV1>,
    ) -> Result<Self, EditorError> {
        admit_text_frame_v2(&prepared, &policy)?;
        ClassicTextFrameSessionV2::new(&prepared, &policy, &coverage)?;
        Ok(Self {
            prepared,
            policy,
            coverage,
        })
    }
    pub fn time(&self) -> &Time {
        &self.prepared.identity().time
    }
    pub fn result(
        &self,
        input: &VisualPreviewTextFrameV2,
        consumer: &Time,
    ) -> Result<VisualPreviewTextFrameResultV2, EditorError> {
        let time = input.time.as_ref().unwrap_or(consumer);
        if time != self.time() || (input.rasters.is_some() && input.decoded.is_none()) {
            return Err(EditorError::new(
                "INVALID_VISUAL_PLAN",
                "Text preview binding requires the original frame and decoded sources",
            ));
        }
        validate_picture_time(time, consumer)?;
        let session = ClassicTextFrameSessionV2::new(&self.prepared, &self.policy, &self.coverage)?;
        let plan = session.typed_plan()?;
        let decode_admissions = plan
            .media
            .layers
            .iter()
            .map(|layer| Ok((layer.element_id.clone(), session.decode_admission()?)))
            .collect::<Result<_, EditorError>>()?;
        let (bound_media, bound) = match input.decoded.as_ref() {
            Some(decoded) => {
                let (media, bound) =
                    session.project_preview_sources(decoded, input.rasters.as_deref())?;
                (Some(media), bound)
            }
            None => (None, None),
        };
        let seconds = |time: &Time| -> Result<f64, EditorError> {
            let time = RationalTime::try_from(time)?;
            Ok(time.ticks() as f64 / f64::from(time.timebase()))
        };
        let policy = &self.policy;
        Ok(VisualPreviewTextFrameResultV2 {
            schema_version: 2,
            plan,
            decode_admissions,
            bound_media,
            bound,
            admission: VisualPreviewAdmissionV4 {
                max_frame_pixels: policy.max_frame_pixels.to_string(),
                max_decoded_pixels: policy.max_decoded_pixels.to_string(),
                max_gpu_bytes: policy.max_gpu_bytes.to_string(),
                max_rgba_bytes: policy.max_rgba_bytes.to_string(),
                max_encoded_bytes: policy.max_encoded_bytes.to_string(),
            },
            time_seconds: seconds(time)?,
            consumer_time_seconds: seconds(consumer)?,
            max_lag_ms: VISUAL_PREVIEW_MAX_LAG_MS,
        })
    }
}
