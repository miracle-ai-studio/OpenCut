//! Mask pictures reuse the D0 consumer-clock checks and all resource admission.
use crate::{
    classic_mask_frame_v1::{bind_mask_frame_v1, plan_v1},
    visual_preview_blend_v1::PreparedBlendPictureV1,
};
use opencut_api_contracts::visual_preview_mask_v1::*;
use opencut_composition::project_v4::{NativeTextCoverageV1, PreparedMaskFrameV1};
use opencut_contracts::{EditorError, Time};
use opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1;

pub struct PreparedMaskPictureV1 {
    prepared: PreparedMaskFrameV1,
    original: PreparedBlendPictureV1,
    policy: LocalVisualExecutionPolicyV1,
    coverage: Vec<NativeTextCoverageV1>,
}
impl PreparedMaskPictureV1 {
    pub fn new(
        prepared: PreparedMaskFrameV1,
        policy: LocalVisualExecutionPolicyV1,
        coverage: Vec<NativeTextCoverageV1>,
    ) -> Result<Self, EditorError> {
        let original = PreparedBlendPictureV1::new(
            prepared.blend().clone(),
            crate::classic_mask_frame_v1::mask_execution_policy_v1(&prepared, &policy)?,
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
        input: &VisualPreviewMaskFrameV1,
        consumer: &Time,
    ) -> Result<VisualPreviewMaskFrameResultV1, EditorError> {
        let child = self.original.result(input, consumer)?;
        let bound = child
            .bound
            .as_ref()
            .map(|graphic| bind_mask_frame_v1(&self.prepared, graphic))
            .transpose()?;
        Ok(VisualPreviewMaskFrameResultV1 {
            schema_version: 1,
            plan: plan_v1(&self.prepared, &self.policy, &self.coverage)?,
            admission: opencut_api_contracts::visual_preview_v4::VisualPreviewAdmissionV4 {
                max_frame_pixels: self.policy.max_frame_pixels.to_string(),
                max_decoded_pixels: self.policy.max_decoded_pixels.to_string(),
                max_gpu_bytes: self.policy.max_gpu_bytes.to_string(),
                max_rgba_bytes: self.policy.max_rgba_bytes.to_string(),
                max_encoded_bytes: self.policy.max_encoded_bytes.to_string(),
            },
            decode_admissions: child.decode_admissions,
            bound_media: child.bound_media,
            bound_execution: child.bound_execution,
            bound_graphic: child.bound_graphic,
            bound_blend: child.bound,
            bound,
            time_seconds: child.time_seconds,
            consumer_time_seconds: child.consumer_time_seconds,
            max_lag_ms: child.max_lag_ms,
        })
    }
}
