//! Scene pictures reuse the D0 consumer-clock checks and all resource admission.
use crate::{
    blend_preview_binding::PreparedBlendBindings,
    classic_scene_frame::{bind_scene_frame, plan_frame},
};
use opencut_api_contracts::visual_preview::*;
use opencut_composition::scene::{NativeTextCoverage, PreparedSceneFrame};
use opencut_contracts::{EditorError, Time};
use opencut_job_execution::media_frame::LocalVisualExecutionPolicy;

pub struct PreparedPicture {
    prepared: PreparedSceneFrame,
    original: PreparedBlendBindings,
    policy: LocalVisualExecutionPolicy,
    coverage: Vec<NativeTextCoverage>,
}
impl PreparedPicture {
    pub fn new(
        prepared: PreparedSceneFrame,
        policy: LocalVisualExecutionPolicy,
        coverage: Vec<NativeTextCoverage>,
    ) -> Result<Self, EditorError> {
        let original = PreparedBlendBindings::new(
            prepared.blend().clone(),
            crate::classic_scene_frame::scene_execution_policy(&prepared, &policy)?,
            crate::classic_scene_frame::title_coverage(&prepared, &coverage)?,
        )?;
        Ok(Self {
            prepared,
            original,
            policy,
            coverage,
        })
    }
    pub fn media_version(
        &self,
        resource: &VisualResource,
    ) -> Result<opencut_contracts::AssetVersion, EditorError> {
        let media = self.prepared.execution().execution().execution().media();
        let session = crate::classic_visual::ClassicVisualSession::new(media, &self.policy)?;
        if session.resource(&resource.asset_id)? != *resource {
            return Err(EditorError::new(
                "NOT_AUTHORIZED",
                "Media differs from the fixed picture",
            ));
        }
        media
            .resources()
            .iter()
            .find(|r| r.asset_id == resource.asset_id)
            .map(|r| r.version.clone())
            .ok_or_else(|| EditorError::new("NOT_AUTHORIZED", "Media is outside the fixed picture"))
    }
    pub fn time(&self) -> &Time {
        self.original.time()
    }
    pub fn result(
        &self,
        input: &VisualPreviewFrame,
        consumer: &Time,
    ) -> Result<VisualPreviewFrameResult, EditorError> {
        let child = self.original.result(input, consumer)?;
        let bound = child
            .bound
            .as_ref()
            .map(|graphic| bind_scene_frame(&self.prepared, graphic))
            .transpose()?;
        let fps = &self.prepared.source_graph().format.fps;
        let frame_step = RationalTime::new(i64::from(fps.denominator), fps.numerator)?;
        // Low-FPS media can still contain continuously animated composition.
        // Sample at least every 100 ms so a held source frame cannot deadlock
        // the separate presentation/PCM credit fence.
        let step = if frame_step.cmp_exact(RationalTime::new(1, 10)?).is_gt() {
            RationalTime::new(1, 10)?
        } else {
            frame_step
        };
        Ok(VisualPreviewFrameResult {
            prepare_ahead: input.prepare_ahead,
            next_time: RationalTime::try_from(self.time())?
                .checked_add(step)?
                .into(),
            plan: plan_frame(&self.prepared, &self.policy, &self.coverage)?,
            admission: opencut_api_contracts::visual_preview::VisualPreviewAdmission {
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

use opencut_time::RationalTime;
/// Reserve resident output bitmaps before deriving decoder caps. Subtracting
/// only in the browser would leave native decode permits larger than the budget.
pub fn reserve_picture_memory(
    mut policy: LocalVisualExecutionPolicy,
    bytes: u32,
) -> Result<LocalVisualExecutionPolicy, EditorError> {
    let denied = || {
        EditorError::new(
            "VISUAL_BUDGET_EXCEEDED",
            "Buffered pictures exceed preview admission",
        )
    };
    if bytes > 64 * 1024 * 1024 {
        return Err(denied());
    }
    policy.max_rgba_bytes = policy
        .max_rgba_bytes
        .checked_sub(u64::from(bytes))
        .ok_or_else(denied)?;
    policy.max_gpu_bytes = policy
        .max_gpu_bytes
        .checked_sub(u64::from(bytes))
        .ok_or_else(denied)?;
    Ok(policy)
}
pub const VISUAL_PREVIEW_MAX_LAG_MS: u32 = 150;
pub const VISUAL_PREVIEW_MAX_PREPARE_MS: u32 = 1000;
pub fn validate_preparation_time(
    time: &Time,
    consumer: &Time,
    prepare: bool,
) -> Result<(), EditorError> {
    if !prepare {
        return validate_picture_time(time, consumer);
    }
    let t = RationalTime::try_from(time)?;
    let c = RationalTime::try_from(consumer)?;
    let delta = i128::from(t.ticks()) * i128::from(c.timebase())
        - i128::from(c.ticks()) * i128::from(t.timebase());
    let scale = i128::from(c.timebase()) * i128::from(t.timebase());
    if t.ticks() < 0
        || delta * 1000 > i128::from(VISUAL_PREVIEW_MAX_PREPARE_MS) * scale
        || delta * 1000 < -i128::from(VISUAL_PREVIEW_MAX_LAG_MS) * scale
    {
        return Err(EditorError::new(
            "PREVIEW_FRAME_STALE",
            "Preparation is outside the bounded consumer window",
        ));
    }
    Ok(())
}
pub fn validate_picture_time(time: &Time, consumer: &Time) -> Result<(), EditorError> {
    let t = RationalTime::try_from(time)?;
    let c = RationalTime::try_from(consumer)?;
    // Exact cross multiplication, bounded i64 ticks × u32 bases fits i128.
    let delta = i128::from(c.ticks()) * i128::from(t.timebase())
        - i128::from(t.ticks()) * i128::from(c.timebase());
    if t.ticks() < 0
        || delta < 0
        || delta * 1000
            > i128::from(VISUAL_PREVIEW_MAX_LAG_MS)
                * i128::from(c.timebase())
                * i128::from(t.timebase())
    {
        return Err(EditorError::new(
            "PREVIEW_FRAME_STALE",
            "Picture must be at or within 150 ms behind confirmed audio consumption",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn t(ticks: &str, timebase: u32) -> Time {
        Time {
            ticks: ticks.into(),
            timebase,
        }
    }
    #[test]
    fn exact_consumer_window_rejects_future_and_overdue_including_fractional_time() {
        let c = t("1", 3);
        assert!(validate_picture_time(&t("11", 60), &c).is_ok()); // exactly 150ms
        assert!(validate_picture_time(&t("10999", 60000), &c).is_err());
        assert!(validate_picture_time(&t("20001", 60000), &c).is_err());
        assert!(validate_picture_time(&t("1", 3), &c).is_ok());
        assert!(validate_picture_time(&t("-1", 3), &c).is_err());
    }
    #[test]
    fn preparation_has_a_bounded_future_window_without_relaxing_presentation() {
        let c = t("1", 3);
        assert!(validate_preparation_time(&t("4", 3), &c, true).is_ok());
        assert!(validate_preparation_time(&t("80001", 60000), &c, true).is_err());
        assert!(validate_preparation_time(&t("4", 3), &c, false).is_err());
        assert!(validate_preparation_time(&t("10999", 60000), &c, true).is_err());
        assert!(validate_preparation_time(&t("-1", 3), &c, true).is_err());
    }
}

pub use crate::text_preview_binding::require_font_batch;
