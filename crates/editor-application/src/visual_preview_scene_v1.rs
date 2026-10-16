//! Scene pictures reuse the D0 consumer-clock checks and all resource admission.
use crate::{
    classic_scene_frame_v1::{bind_scene_frame_v1, plan_v1},
    visual_preview_blend_v1::PreparedBlendPictureV1,
};
use opencut_api_contracts::visual_preview_scene_v1::*;
use opencut_composition::project_v4::{NativeTextCoverageV1, PreparedSceneFrameV1};
use opencut_contracts::{EditorError, Time};
use opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1;

pub struct PreparedScenePictureV1 {
    prepared: PreparedSceneFrameV1,
    original: PreparedBlendPictureV1,
    policy: LocalVisualExecutionPolicyV1,
    coverage: Vec<NativeTextCoverageV1>,
}
impl PreparedScenePictureV1 {
    pub fn new(
        prepared: PreparedSceneFrameV1,
        policy: LocalVisualExecutionPolicyV1,
        coverage: Vec<NativeTextCoverageV1>,
    ) -> Result<Self, EditorError> {
        let original = PreparedBlendPictureV1::new(
            prepared.blend().clone(),
            crate::classic_scene_frame_v1::scene_execution_policy_v1(&prepared, &policy)?,
            crate::classic_scene_frame_v1::title_coverage(&prepared, &coverage)?,
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
        input: &VisualPreviewSceneFrameV1,
        consumer: &Time,
    ) -> Result<VisualPreviewSceneFrameResultV1, EditorError> {
        let child = self.original.result(input, consumer)?;
        let bound = child
            .bound
            .as_ref()
            .map(|graphic| bind_scene_frame_v1(&self.prepared, graphic))
            .transpose()?;
        Ok(VisualPreviewSceneFrameResultV1 {
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
