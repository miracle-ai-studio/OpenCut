//! Cache scheduling is an application use case over the existing render port.
//! The browser receives exact timeline mappings, never evaluates an edit graph.
use crate::{agent_render::RenderSubmission, media_video_jobs as video};
use opencut_api_contracts::{agent_render::VideoRenderInput, visual_preview::*};
use opencut_contracts::{EditorError, Time, scene::EditorSnapshot, export_video::*};
use opencut_time::RationalTime;
use sha2::{Digest, Sha256};

pub const MAX_CACHE_BYTES: u32 = 64 * 1024 * 1024;
/// Only the active exact window and its bounded forward reserve can outrank
/// background work. The caller must supply an authorized fixed-session plan.
pub fn preferred_jobs(plan:&VisualPreviewCache, time:&Time)->Result<Vec<String>,EditorError> {
    let time=RationalTime::try_from(time)?;
    for (index,segment) in plan.segments.iter().enumerate() {
        let start=RationalTime::try_from(&segment.start)?;
        let end=RationalTime::try_from(&segment.end)?;
        if time.cmp_exact(start).is_ge() && time.cmp_exact(end).is_lt() {
            return Ok(plan.segments[index..].iter().take(3).map(|s|s.job_id.clone()).collect());
        }
    }
    Ok(Vec::new())
}
/// Two foreground claims followed by a FIFO opportunity prevent an active
/// preview from starving exports or other background consumers.
#[derive(Default)]
pub struct PreviewClaimBudget(u8);
impl PreviewClaimBudget {
    pub fn allows_preference(&self)->bool {self.0<2}
    pub fn claimed(&mut self, preferred:bool) {self.0=if preferred {self.0.saturating_add(1)}else{0};}
}
pub fn prepare(
    store: &mut impl RenderSubmission,
    snapshot: &EditorSnapshot,
    sequence: &str,
    policy: &video::LocalVideoExecutionPolicy,
    runtime: &str,
    priority_time: &Time,
    reuse_only:bool,
    mut checkpoint: impl FnMut() -> Result<(), EditorError>,
) -> Result<VisualPreviewCache, EditorError> {
    let fixed = video::prepare_with_frame_window(snapshot, sequence, policy, video::VideoFrameProfile::SceneV1, false, None)?;
    let identity = fixed.identity;
    let total = crate::classic_audio::decimal(&identity.frame_count)?;
    let fps = identity.fps;
    let size = opencut_composition::scene::preview_canvas(opencut_contracts::scene::CanvasSize {width:identity.width,height:identity.height});
    let options = ExportVideoOptions { runtime_fingerprint:Some(runtime.into()), resolution: RasterResolution::Preview540, format:ExportFormat::Mp4, quality:ExportQuality::High, include_audio:false };
    let key = format!("{:x}", Sha256::digest(serde_json::to_vec(&(
        "preview-raster-cache-1", snapshot, sequence, &options, size, &fps, runtime,
    )).map_err(|_| EditorError::new("INVALID_ARGUMENT", "Cannot identify preview input"))?));
    let step = RationalTime::new(i64::from(fps.denominator), fps.numerator)?;
    let seconds = |time: &Time| -> Result<f64,EditorError> { let t=RationalTime::try_from(time)?; Ok(t.ticks() as f64/f64::from(t.timebase())) };
    let per_segment = (u64::from(fps.numerator) * 2).div_ceil(u64::from(fps.denominator)).max(1);
    // Bound both accepted work and compressed client residency; disk output is
    // also subject to the shared native storage reservation/publication fences.
    if total.div_ceil(per_segment) > 300 { return Err(EditorError::new("PREVIEW_CACHE_LIMIT", "Preview cache exceeds ten minutes of admitted segments")); }
    let mut windows=(0..total).step_by(per_segment as usize).collect::<Vec<_>>();
    let target=RationalTime::try_from(priority_time)?;
    let mut center=0;
    for (index,first) in windows.iter().enumerate(){
        if RationalTime::try_from(&video::grid_time(&fps,*first)?)?.cmp_exact(target).is_le() {center=index;}
    }
    if target.cmp_exact(RationalTime::new(0,1)?).is_lt() || target.cmp_exact(RationalTime::try_from(&identity.timeline_duration)?).is_ge() {
        return Err(EditorError::new("INVALID_ARGUMENT", "Preview cache requires an active timeline position"));
    }
    // Admit current + two forward windows, preserving full content identity,
    // duration and CFR grid. Continuation requires live session authorization.
    windows=windows.into_iter().skip(center).take(3).collect();
    let mut segments=Vec::new();
    'windows: for first in windows {
        checkpoint()?;
        let count=per_segment.min(total-first);
        let start=video::grid_time(&fps,first)?;
        let end=video::grid_time(&fps,first+count)?;
        let mut request=VideoRenderInput {
            idempotency_key:format!("preview-cache:{key}:{first}:{count}"),
            project_id:snapshot.project.id.clone(),revision:snapshot.revision.clone(),sequence_id:sequence.into(),options:options.clone(),
            range:Some(opencut_api_contracts::agent_render::VideoFrameRange{start_frame:first.to_string(),frame_count:count.to_string()}),
        };
        let mut job=if reuse_only {
            match store.cached_video(request.clone())? {
                Some(job)=>job,
                None if first!=center as u64*per_segment=>continue,
                None=>return Err(EditorError::new("PREVIEW_CACHE_MISSING","No existing current preview window")),
            }
        }else{store.video(request.clone())?};
        // An explicit cancellation or failed attempt cannot poison this content
        // forever. Retry identities depend on the terminal receipt, so concurrent
        // consumers still share exactly one replacement job.
        for _ in 0..8 {
            if !matches!(job.state,crate::JobState::Failed|crate::JobState::Cancelled){break;}
            checkpoint()?;
            request.idempotency_key=format!("preview-cache:{key}:{first}:{count}:after:{}",job.job_id);
            job=if reuse_only {match store.cached_video(request.clone())? {
                Some(job)=>job,
                None if first!=center as u64*per_segment=>continue 'windows,
                None=>return Err(EditorError::new("PREVIEW_CACHE_MISSING","No reusable repaired current window")),
            }}else{store.video(request.clone())?};
        }
        if reuse_only && first!=center as u64*per_segment && matches!(job.state,crate::JobState::Failed|crate::JobState::Cancelled){continue 'windows;}
        if matches!(job.state,crate::JobState::Failed|crate::JobState::Cancelled){return Err(EditorError::new("PREVIEW_CACHE_FAILED","Preview optimization requires a repaired render attempt"));}
        let frames=(first..first+count).map(|index| {
            let time=video::grid_time(&fps,index)?;
            Ok(VisualPreviewCachedFrame {time_seconds:seconds(&time)?,time,next_time:video::grid_time(&fps,index+1)?,
                segment_time_seconds:seconds(&video::grid_time(&fps,index-first)?)?})
        }).collect::<Result<Vec<_>,EditorError>>()?;
        segments.push((first,VisualPreviewCacheSegment {start_seconds:seconds(&start)?,end_seconds:seconds(&end)?,start,end,job_id:job.job_id,frames}));
    }
    checkpoint()?;
    segments.sort_by_key(|(first,_)|*first);
    let segments=segments.into_iter().map(|(_,segment)|segment).collect();
    Ok(VisualPreviewCache {project_id:snapshot.project.id.clone(),revision:snapshot.revision.clone(),sequence_id:sequence.into(),cache_key:key,
        width:size.width,height:size.height,duration_seconds:seconds(&identity.timeline_duration)?, duration:identity.timeline_duration,
        frame_step:step.into(),frame_step_seconds:f64::from(fps.denominator)/f64::from(fps.numerator),max_encoded_bytes:MAX_CACHE_BYTES,segments})
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_api_contracts::agent_render::{AudioRenderInput,FrameRenderInput};
    #[derive(Default)]
    pub(super) struct Port { pub(super) requests:Vec<VideoRenderInput>, cancel_first:bool, pub(super) cached:std::collections::HashMap<String,crate::JobRecord> }
    impl RenderSubmission for Port {
        fn frame(&mut self,_:FrameRenderInput)->Result<crate::JobRecord,EditorError>{panic!("cache must use shared video port")}
        fn audio(&mut self,_:AudioRenderInput)->Result<crate::JobRecord,EditorError>{panic!("cache never renders audio")}
        fn cached_video(&mut self,input:VideoRenderInput)->Result<Option<crate::JobRecord>,EditorError>{Ok(self.cached.get(&input.idempotency_key).cloned())}
        fn video(&mut self,input:VideoRenderInput)->Result<crate::JobRecord,EditorError>{
            if let Some(job)=self.cached.get(&input.idempotency_key){return Ok(job.clone());}
            self.requests.push(input.clone());
            let job=crate::JobRecord {job_id:format!("job_{}",self.requests.len()),state:if self.cancel_first&&self.requests.len()==1 {crate::JobState::Cancelled}else{crate::JobState::Queued},attempt:0,
                kind:"render.export_video_range.v1".into(),input:serde_json::Value::Null,result:None,error:None,lease_owner:None,lease_until:None,cancel_requested:false,progress:None};
            self.cached.insert(input.idempotency_key,job.clone());Ok(job)
        }
    }
    #[test]
    fn cache_is_content_and_runtime_bound_with_exact_contiguous_native_frame_mappings() {
        let mut fixed=crate::media_video_jobs::tests::input();
        fixed.snapshot.project.sequences[0].tracks[0].elements[0].timeline_range.duration=Time{ticks:"5".into(),timebase:1};
        let mut port=Port::default();
        let plan=prepare(&mut port,&fixed.snapshot,"scene",&fixed.processing_policy,&"a".repeat(64),&Time{ticks:"0".into(),timebase:1},false,||Ok(())).unwrap();
        assert_eq!(plan.segments.len(),3);assert_eq!(port.requests.len(),3);
        assert_eq!(plan.segments.iter().map(|s|s.frames.len()).collect::<Vec<_>>(),[60,60,30]);
        assert_eq!(plan.segments[0].end,plan.segments[1].start);
        assert_eq!(plan.segments[2].end_seconds,5.);
        assert_eq!(plan.segments[1].frames[0].segment_time_seconds,0.);
        assert_eq!(plan.segments[1].frames[0].time_seconds,2.);
        for request in &port.requests {assert!(!request.options.include_audio);assert_eq!(request.options.resolution,RasterResolution::Preview540);assert_eq!(request.revision,fixed.snapshot.revision);}
        let again=prepare(&mut Port::default(),&fixed.snapshot,"scene",&fixed.processing_policy,&"a".repeat(64),&Time{ticks:"0".into(),timebase:1},false,||Ok(())).unwrap();
        assert_eq!(plan.cache_key,again.cache_key);
        let other=prepare(&mut Port::default(),&fixed.snapshot,"scene",&fixed.processing_policy,&"b".repeat(64),&Time{ticks:"0".into(),timebase:1},false,||Ok(())).unwrap();assert_ne!(plan.cache_key,other.cache_key);
        fixed.snapshot.project.name="Edited project".into();
        let edited=prepare(&mut Port::default(),&fixed.snapshot,"scene",&fixed.processing_policy,&"a".repeat(64),&Time{ticks:"0".into(),timebase:1},false,||Ok(())).unwrap();assert_ne!(plan.cache_key,edited.cache_key);
    }
    #[test]
    fn nearby_submission_priority_preserves_chronological_manifest_and_content_identity() {
        let mut fixed=crate::media_video_jobs::tests::input();
        fixed.snapshot.project.sequences[0].tracks[0].elements[0].timeline_range.duration=Time{ticks:"5".into(),timebase:1};
        let mut first=Port::default();
        let nearby=prepare(&mut first,&fixed.snapshot,"scene",&fixed.processing_policy,&"a".repeat(64),&Time{ticks:"32".into(),timebase:10},false,||Ok(())).unwrap();
        assert_eq!(first.requests.iter().map(|r|r.range.as_ref().unwrap().start_frame.as_str()).collect::<Vec<_>>(),["60","120"]);
        assert_eq!(nearby.segments.iter().map(|s|s.start_seconds).collect::<Vec<_>>(),[2.,4.]);
        let beginning=prepare(&mut Port::default(),&fixed.snapshot,"scene",&fixed.processing_policy,&"a".repeat(64),&Time{ticks:"0".into(),timebase:1},false,||Ok(())).unwrap();
        assert_eq!(nearby.cache_key,beginning.cache_key);
    }
    #[test]
    fn reuse_only_does_not_submit_render_work_when_the_snapshot_has_no_cache() {
        let fixed=crate::media_video_jobs::tests::input();let mut port=Port::default();
        let error=prepare(&mut port,&fixed.snapshot,"scene",&fixed.processing_policy,&"a".repeat(64),&Time{ticks:"0".into(),timebase:1},true,||Ok(())).unwrap_err();
        assert_eq!(error.code,"PREVIEW_CACHE_MISSING");assert!(port.requests.is_empty());
    }
    #[test]
    fn foreground_window_tracks_exact_boundaries_and_preserves_a_fifo_opportunity() {
        let mut fixed=crate::media_video_jobs::tests::input();
        fixed.snapshot.project.sequences[0].tracks[0].elements[0].timeline_range.duration=Time{ticks:"8".into(),timebase:1};
        let plan=prepare(&mut Port::default(),&fixed.snapshot,"scene",&fixed.processing_policy,&"a".repeat(64),&Time{ticks:"0".into(),timebase:1},false,||Ok(())).unwrap();
        assert_eq!(preferred_jobs(&plan,&Time{ticks:"7".into(),timebase:2}).unwrap(),plan.segments[1..].iter().map(|s|s.job_id.clone()).collect::<Vec<_>>());
        assert_eq!(preferred_jobs(&plan,&Time{ticks:"4".into(),timebase:1}).unwrap(),plan.segments[2..].iter().map(|s|s.job_id.clone()).collect::<Vec<_>>());
        assert!(preferred_jobs(&plan,&Time{ticks:"8".into(),timebase:1}).unwrap().is_empty());
        let mut budget=PreviewClaimBudget::default();
        assert!(budget.allows_preference());budget.claimed(true);
        assert!(budget.allows_preference());budget.claimed(true);
        assert!(!budget.allows_preference());budget.claimed(false);
        assert!(budget.allows_preference());
    }
    #[test]
    fn cancelled_cache_attempts_share_one_receipt_bound_replacement_and_cancelled_preparation_stops_submission() {
        let fixed=crate::media_video_jobs::tests::input();let mut port=Port{cancel_first:true,..Port::default()};
        prepare(&mut port,&fixed.snapshot,"scene",&fixed.processing_policy,&"a".repeat(64),&Time{ticks:"0".into(),timebase:1},false,||Ok(())).unwrap();
        assert_eq!(port.requests.len(),2);assert!(port.requests[1].idempotency_key.ends_with(":after:job_1"));
        let mut denied=Port::default();
        let error=prepare(&mut denied,&fixed.snapshot,"scene",&fixed.processing_policy,&"a".repeat(64),&Time{ticks:"0".into(),timebase:1},false,||Err(EditorError::new("PREVIEW_EXPIRED","expired"))).unwrap_err();
        assert_eq!(error.code,"PREVIEW_EXPIRED");assert!(denied.requests.is_empty());
    }
}

#[cfg(test)]
mod demand_tests {
    use super::*;
    use super::tests::Port;
    #[test]
    fn demand_windows_share_overlaps_preserve_full_duration_and_reject_eof() {
        let mut fixed=crate::media_video_jobs::tests::input();
        fixed.snapshot.project.sequences[0].tracks[0].elements[0].timeline_range.duration=Time{ticks:"42".into(),timebase:1};
        let mut port=Port::default();
        let a=prepare(&mut port,&fixed.snapshot,"scene",&fixed.processing_policy,&"a".repeat(64),&Time{ticks:"0".into(),timebase:1},false,||Ok(())).unwrap();
        assert_eq!(port.requests.len(),3);assert_eq!(a.duration,Time{ticks:"42".into(),timebase:1});
        let b=prepare(&mut port,&fixed.snapshot,"scene",&fixed.processing_policy,&"a".repeat(64),&Time{ticks:"4".into(),timebase:1},false,||Ok(())).unwrap();
        assert_eq!(port.requests.len(),5);assert_eq!(a.segments[2].job_id,b.segments[0].job_id);assert_eq!(a.cache_key,b.cache_key);
        assert_eq!(b.segments.iter().map(|s|s.start_seconds).collect::<Vec<_>>(),[4.,6.,8.]);
        let last=prepare(&mut port,&fixed.snapshot,"scene",&fixed.processing_policy,&"a".repeat(64),&Time{ticks:"41".into(),timebase:1},false,||Ok(())).unwrap();
        assert_eq!(last.segments.len(),1);assert_eq!(last.segments[0].end_seconds,42.);
        let count=port.requests.len();
        for time in [Time{ticks:"42".into(),timebase:1},Time{ticks:"-1".into(),timebase:1}] {
            assert_eq!(prepare(&mut port,&fixed.snapshot,"scene",&fixed.processing_policy,&"a".repeat(64),&time,false,||Ok(())).unwrap_err().code,"INVALID_ARGUMENT");
        }
        assert_eq!(port.requests.len(),count);
    }
    #[test]
    fn reuse_current_window_does_not_require_or_submit_missing_reserves() {
        let mut fixed=crate::media_video_jobs::tests::input();
        fixed.snapshot.project.sequences[0].tracks[0].elements[0].timeline_range.duration=Time{ticks:"5".into(),timebase:1};
        let mut port=Port::default();
        let plan=prepare(&mut port,&fixed.snapshot,"scene",&fixed.processing_policy,&"a".repeat(64),&Time{ticks:"0".into(),timebase:1},false,||Ok(())).unwrap();
        let count=port.requests.len();let current=&plan.segments[0].job_id;
        let reserve=&plan.segments[1].job_id;port.cached.retain(|_,job|&job.job_id==current||&job.job_id==reserve);
        for job in port.cached.values_mut(){if &job.job_id==reserve {job.state=crate::JobState::Cancelled;}}
        let reused=prepare(&mut port,&fixed.snapshot,"scene",&fixed.processing_policy,&"a".repeat(64),&Time{ticks:"0".into(),timebase:1},true,||Ok(())).unwrap();
        assert_eq!(reused.segments.len(),1);assert_eq!(&reused.segments[0].job_id,current);assert_eq!(port.requests.len(),count);
    }
}
