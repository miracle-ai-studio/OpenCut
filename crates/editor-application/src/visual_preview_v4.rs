//! Pure ephemeral Classic frame preparation. Reuses durable-frame IR/binding,
//! never submits jobs, holds native resource paths or evaluates clocks in clients.
use crate::classic_visual::ClassicVisualSession;
use opencut_api_contracts::visual_preview_v4::*;
use opencut_composition::project_v4::PreparedVisualFrameV4;
use opencut_contracts::{EditorError, Time, project_v4::ProjectSnapshotV4};
use opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1;
use opencut_time::RationalTime;

pub const VISUAL_PREVIEW_MAX_LAG_MS: u32 = 150;
fn seconds(time: &Time) -> Result<f64, EditorError> {
    let t = RationalTime::try_from(time)?;
    Ok(t.ticks() as f64 / f64::from(t.timebase()))
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
/// Internal, bounded preparation of one immutable frame. Hosts may retain it
/// for decoder binding, but must independently revalidate live authority and
/// the sound publication fence before and after every use.
pub struct PreparedPictureV4 {
    prepared: PreparedVisualFrameV4,
    policy: LocalVisualExecutionPolicyV1,
}
impl PreparedPictureV4 {
    pub fn new(
        snapshot: &ProjectSnapshotV4,
        sequence: &str,
        time: &Time,
        policy: &LocalVisualExecutionPolicyV1,
    ) -> Result<Self, EditorError> {
        let prepared = crate::project_v4_frame_jobs::prepare(snapshot, sequence, time)?;
        // Enforce the existing layer/resource/pixel budgets before retention.
        ClassicVisualSession::new(&prepared, policy)?;
        Ok(Self {
            prepared,
            policy: policy.clone(),
        })
    }
    pub fn time(&self) -> &Time {
        &self.prepared.identity().time
    }
    pub fn result(
        &self,
        input: &VisualPreviewFrameV4,
        consumer: &Time,
    ) -> Result<VisualPreviewFrameResultV4, EditorError> {
        let time = input.time.as_ref().unwrap_or(consumer);
        if time != self.time() {
            return Err(EditorError::new(
                "INVALID_VISUAL_PLAN",
                "Prepared picture time changed",
            ));
        }
        validate_picture_time(time, consumer)?;
        let session = ClassicVisualSession::new(&self.prepared, &self.policy)?;
        picture_result(&session, input, consumer)
    }
}
pub fn prepare_picture(
    snapshot: &ProjectSnapshotV4,
    sequence: &str,
    input: &VisualPreviewFrameV4,
    consumer: &Time,
    policy: &LocalVisualExecutionPolicyV1,
) -> Result<VisualPreviewFrameResultV4, EditorError> {
    let time = input.time.as_ref().unwrap_or(consumer);
    validate_picture_time(time, consumer)?;
    PreparedPictureV4::new(snapshot, sequence, time, policy)?.result(input, consumer)
}
fn picture_result(
    session: &ClassicVisualSession<'_>,
    input: &VisualPreviewFrameV4,
    consumer: &Time,
) -> Result<VisualPreviewFrameResultV4, EditorError> {
    let time = session.prepared().identity().time.clone();
    let policy = session.policy();
    let plan = session.typed_plan()?;
    let decode_admissions = plan
        .layers
        .iter()
        .map(|l| Ok((l.element_id.clone(), session.decode_admission()?)))
        .collect::<Result<_, EditorError>>()?;
    let bound = input
        .decoded
        .as_ref()
        .map(|d| session.project_decoded_sources(d))
        .transpose()?;
    Ok(VisualPreviewFrameResultV4 {
        schema_version: 1,
        plan,
        decode_admissions,
        bound,
        admission: VisualPreviewAdmissionV4 {
            max_frame_pixels: policy.max_frame_pixels.to_string(),
            max_decoded_pixels: policy.max_decoded_pixels.to_string(),
            max_gpu_bytes: policy.max_gpu_bytes.to_string(),
            max_rgba_bytes: policy.max_rgba_bytes.to_string(),
            max_encoded_bytes: policy.max_encoded_bytes.to_string(),
        },
        time_seconds: seconds(&time)?,
        consumer_time_seconds: seconds(consumer)?,
        max_lag_ms: VISUAL_PREVIEW_MAX_LAG_MS,
    })
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
}
