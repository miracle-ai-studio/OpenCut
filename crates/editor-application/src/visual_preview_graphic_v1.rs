//! One aggregate graphic picture on the existing authorized sound-consumer clock.
use crate::{
    classic_graphic_frame_v1::{bind_graphic_frame_v1, graphic_execution_policy_v1, plan_v1},
    visual_preview_caption_v3::PreparedCaptionPictureV3,
};
use opencut_api_contracts::{
    visual_preview_caption_v3::VisualPreviewCaptionFrameV3, visual_preview_graphic_v1::*,
    visual_preview_v4::VisualPreviewAdmissionV4,
};
use opencut_composition::project_v4::{NativeTextCoverageV1, PreparedGraphicFrameV1};
use opencut_contracts::{EditorError, Time};
use opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1;

pub struct PreparedGraphicPictureV1 {
    prepared: PreparedGraphicFrameV1,
    original: PreparedCaptionPictureV3,
    policy: LocalVisualExecutionPolicyV1,
    coverage: Vec<NativeTextCoverageV1>,
}
impl PreparedGraphicPictureV1 {
    pub fn new(
        prepared: PreparedGraphicFrameV1,
        policy: LocalVisualExecutionPolicyV1,
        coverage: Vec<NativeTextCoverageV1>,
    ) -> Result<Self, EditorError> {
        // Reserve generated sources before granting delegated decode admission.
        let execution_policy = graphic_execution_policy_v1(&prepared, &policy)?;
        let original = PreparedCaptionPictureV3::new(
            prepared.execution().clone(),
            execution_policy,
            coverage.clone(),
        )?;
        Ok(Self {
            prepared,
            original,
            policy,
            coverage,
        })
    }
    pub fn time(&self) -> &Time {
        self.original.time()
    }
    pub fn result(
        &self,
        input: &VisualPreviewGraphicFrameV1,
        consumer: &Time,
    ) -> Result<VisualPreviewGraphicFrameResultV1, EditorError> {
        if input.graphics.is_some()
            && (input.decoded.is_none() || input.rasters.is_none() || input.captions.is_none())
        {
            return Err(EditorError::new(
                "INVALID_VISUAL_PLAN",
                "Graphic binding requires the original decoded, text and caption observations",
            ));
        }
        let child = self.original.result(
            &VisualPreviewCaptionFrameV3 {
                session: input.session.clone(),
                time: input.time.clone(),
                decoded: input.decoded.clone(),
                rasters: input.rasters.clone(),
                captions: input.captions.clone(),
            },
            consumer,
        )?;
        let bound = match (&child.bound, &input.captions, &input.graphics) {
            (Some(execution), Some(captions), Some(graphics)) => Some(bind_graphic_frame_v1(
                &self.prepared,
                execution,
                captions,
                graphics,
            )?),
            _ => None,
        };
        let policy = &self.policy;
        Ok(VisualPreviewGraphicFrameResultV1 {
            schema_version: 1,
            plan: plan_v1(&self.prepared, policy, &self.coverage)?,
            // The renderer owns one aggregate frame; only delegated decode gets
            // the remaining budget. Never claim a fabricated reduced GPU peak.
            admission: VisualPreviewAdmissionV4 {
                max_frame_pixels: policy.max_frame_pixels.to_string(),
                max_decoded_pixels: policy.max_decoded_pixels.to_string(),
                max_gpu_bytes: policy.max_gpu_bytes.to_string(),
                max_rgba_bytes: policy.max_rgba_bytes.to_string(),
                max_encoded_bytes: policy.max_encoded_bytes.to_string(),
            },
            decode_admissions: child.decode_admissions,
            bound_media: child.bound_media,
            bound_execution: child.bound,
            bound,
            time_seconds: child.time_seconds,
            consumer_time_seconds: child.consumer_time_seconds,
            max_lag_ms: child.max_lag_ms,
        })
    }
}
#[cfg(test)]
mod tests;
