//! Blend pictures reuse the D0 consumer-clock checks and all resource admission.
use crate::{
    classic_blend_frame_v1::{bind_blend_frame_v1, plan_v1},
    visual_preview_graphic_v1::PreparedGraphicPictureV1,
};
use opencut_api_contracts::visual_preview_blend_v1::*;
use opencut_composition::project_v4::{NativeTextCoverageV1, PreparedBlendFrameV1};
use opencut_contracts::{EditorError, Time};
use opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1;

pub struct PreparedBlendPictureV1 {
    prepared: PreparedBlendFrameV1,
    original: PreparedGraphicPictureV1,
    policy: LocalVisualExecutionPolicyV1,
    coverage: Vec<NativeTextCoverageV1>,
}
impl PreparedBlendPictureV1 {
    pub fn new(
        prepared: PreparedBlendFrameV1,
        policy: LocalVisualExecutionPolicyV1,
        coverage: Vec<NativeTextCoverageV1>,
    ) -> Result<Self, EditorError> {
        let original = PreparedGraphicPictureV1::new(
            prepared.execution().clone(),
            policy.clone(),
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
        input: &VisualPreviewBlendFrameV1,
        consumer: &Time,
    ) -> Result<VisualPreviewBlendFrameResultV1, EditorError> {
        let child = self.original.result(input, consumer)?;
        let bound = child
            .bound
            .as_ref()
            .map(|graphic| bind_blend_frame_v1(&self.prepared, graphic))
            .transpose()?;
        Ok(VisualPreviewBlendFrameResultV1 {
            schema_version: 1,
            plan: plan_v1(&self.prepared, &self.policy, &self.coverage)?,
            admission: child.admission,
            decode_admissions: child.decode_admissions,
            bound_media: child.bound_media,
            bound_execution: child.bound_execution,
            bound_graphic: child.bound,
            bound,
            time_seconds: child.time_seconds,
            consumer_time_seconds: child.consumer_time_seconds,
            max_lag_ms: child.max_lag_ms,
        })
    }
}
#[cfg(test)]
mod tests;
