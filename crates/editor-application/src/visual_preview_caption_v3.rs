//! One bounded immutable caption picture on the existing sound consumer clock.
use crate::{
    classic_text_frame_v2::ClassicTextFrameSessionV2,
    classic_text_frame_v3::{bind_caption_frame_v3, plan_v3},
    visual_preview_text_v2::PreparedTextPictureV2,
};
use opencut_api_contracts::{
    visual_preview_caption_v3::*, visual_preview_text_v2::VisualPreviewTextFrameV2,
};
use opencut_composition::project_v4::{NativeTextCoverageV1, PreparedTextFrameV3};
use opencut_contracts::{EditorError, Time};
use opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1;
pub struct PreparedCaptionPictureV3 {
    prepared: PreparedTextFrameV3,
    policy: LocalVisualExecutionPolicyV1,
    coverage: Vec<NativeTextCoverageV1>,
}
impl PreparedCaptionPictureV3 {
    pub fn new(
        prepared: PreparedTextFrameV3,
        policy: LocalVisualExecutionPolicyV1,
        coverage: Vec<NativeTextCoverageV1>,
    ) -> Result<Self, EditorError> {
        crate::classic_text_frame_v2::admit_text_frame_v2(prepared.execution(), &policy)?;
        ClassicTextFrameSessionV2::new(prepared.execution(), &policy, &coverage)?;
        Ok(Self {
            prepared,
            policy,
            coverage,
        })
    }
    pub fn time(&self) -> &Time {
        &self.prepared.execution().identity().time
    }
    pub fn result(
        &self,
        input: &VisualPreviewCaptionFrameV3,
        consumer: &Time,
    ) -> Result<VisualPreviewCaptionFrameResultV3, EditorError> {
        if input.rasters.is_some() != input.captions.is_some()
            || (input.rasters.is_some() && input.decoded.is_none())
        {
            return Err(EditorError::new(
                "INVALID_VISUAL_PLAN",
                "Caption binding requires decoded media and both original caption and raster observations",
            ));
        }
        let original = PreparedTextPictureV2::new(
            self.prepared.execution().clone(),
            self.policy.clone(),
            self.coverage.clone(),
        )?;
        let media = original.result(
            &VisualPreviewTextFrameV2 {
                session: input.session.clone(),
                time: input.time.clone(),
                decoded: input.decoded.clone(),
                rasters: None,
            },
            consumer,
        )?;
        let bound = match (&input.decoded, &input.rasters, &input.captions) {
            (Some(decoded), Some(rasters), Some(captions)) => Some(bind_caption_frame_v3(
                &self.prepared,
                &self.policy,
                &self.coverage,
                decoded,
                rasters,
                captions,
            )?),
            _ => None,
        };
        Ok(VisualPreviewCaptionFrameResultV3 {
            schema_version: 3,
            plan: plan_v3(&self.prepared, &self.policy, &self.coverage)?,
            admission: media.admission,
            decode_admissions: media.decode_admissions,
            bound_media: media.bound_media,
            bound,
            time_seconds: media.time_seconds,
            consumer_time_seconds: media.consumer_time_seconds,
            max_lag_ms: media.max_lag_ms,
        })
    }
}
#[cfg(test)]
mod tests;
