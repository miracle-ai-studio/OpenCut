//! Host-only sharing between live PCM delivery and durable video frame work.
//! The application machine owns PCM credit; this adapter never invents a clock.
use super::*;
use std::time::Duration;

fn needs_pcm(machine: &AudioPreviewMachine, stream_open: bool, shutdown: bool, at: u64) -> bool {
    stream_open
        && !shutdown
        // Preparing a cold video cache must not wait on the sound it precedes.
        && machine.consumed_scene_sample() > machine.actual_start_sample()
        && machine.next_block(at).is_ok_and(|next| next.is_some())
}

fn wait_for_capacity(
    mut demand: impl FnMut() -> Result<bool, EditorError>,
    mut checkpoint: impl FnMut() -> Result<(), EditorError>,
) -> Result<(), EditorError> {
    loop {
        // Preserve cancellation, authority, lease and original deadline fences
        // even when no new background frame has been submitted.
        checkpoint()?;
        if !demand()? { return Ok(()); }
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub(crate) fn yield_to_realtime_audio(
    database: &Path,
    checkpoint: impl FnMut() -> Result<(), EditorError>,
) -> Result<(), EditorError> {
    let key = database_key(database)?;
    wait_for_capacity(|| {
        // Do not hold the registry across a session lock or a wait.
        let entries: Vec<_> = lock(registries())?.get(&key)
            .map(|registry| registry.entries.values().cloned().collect())
            .unwrap_or_default();
        for entry in entries {
            let stream_open = lock(&entry.stream_claim)?.is_some();
            let state = lock(&entry.state)?;
            if needs_pcm(&state.machine, stream_open, entry.shutdown.load(Ordering::Acquire), now()) {
                return Ok(true);
            }
        }
        Ok(false)
    }, checkpoint)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn machine(total: u64) -> AudioPreviewMachine {
        let mut machine = AudioPreviewMachine::new(&AudioPreviewCreate {
            project_id:"p".into(), revision:"r".into(), sequence_id:"s".into(), sample_rate:48000,
            start_time:opencut_contracts::Time{ticks:"0".into(),timebase:1}, idempotency_key:"create".into(),
        },"session".into(),"owner".into(),"lease".into(),total,0).unwrap();
        command(&mut machine, "play", AudioPreviewCommand::Play);
        machine
    }
    fn command(machine: &mut AudioPreviewMachine, id: &str, command: AudioPreviewCommand) {
        let view=machine.snapshot(0);
        machine.control(&AudioPreviewControl{session_id:view.session_id,generation:view.generation,
            owner_id:view.owner_id,lease_id:view.lease_id,control_id:id.into(),command},1).unwrap();
    }
    fn consume(machine: &mut AudioPreviewMachine, count: u64) {
        command(machine,"device-consumed",AudioPreviewCommand::ReportConsumed{sequence:"0".into(),
            consumed_frames:count.to_string(),output:None,playing:true,stalled:false});
    }
    fn produce(machine: &mut AudioPreviewMachine, start: u64, count: u32) {
        machine.produced(machine.generation(),machine.overlay_epoch(),start,count,2).unwrap();
    }
    #[test]
    fn cold_preparation_cannot_deadlock_and_live_credit_yields_until_pcm_is_refilled() {
        let mut machine=machine(480000);
        assert!(!needs_pcm(&machine,true,false,3));
        for start in (0..AUDIO_PREVIEW_MAX_BUFFERED_FRAMES).step_by(4096) {produce(&mut machine,start,4096);}
        consume(&mut machine,4096);
        assert!(needs_pcm(&machine,true,false,3));
        produce(&mut machine,AUDIO_PREVIEW_MAX_BUFFERED_FRAMES,4096);
        assert!(!needs_pcm(&machine,true,false,3));
    }
    #[test]
    fn pause_seek_disconnection_shutdown_and_expiry_do_not_hold_background_work() {
        let mut machine=machine(480000);produce(&mut machine,0,4096);consume(&mut machine,4096);
        assert!(needs_pcm(&machine,true,false,3));
        assert!(!needs_pcm(&machine,false,false,3));assert!(!needs_pcm(&machine,true,true,3));
        assert!(!needs_pcm(&machine,true,false,60000));
        command(&mut machine,"pause",AudioPreviewCommand::Pause);
        assert!(!needs_pcm(&machine,true,false,3));
        command(&mut machine,"seek",AudioPreviewCommand::Seek{sequence_time:opencut_contracts::Time{ticks:"1".into(),timebase:1}});
        command(&mut machine,"resume",AudioPreviewCommand::Play);
        assert!(!needs_pcm(&machine,true,false,3));
    }
    #[test]
    fn a_short_produced_tail_never_waits_for_an_impossible_full_buffer() {
        let mut machine=machine(5000);produce(&mut machine,0,4096);produce(&mut machine,4096,904);consume(&mut machine,4096);
        assert!(!needs_pcm(&machine,true,false,3));
    }
    #[test]
    fn cancellation_is_checked_while_realtime_demand_continues() {
        let mut checkpoints=0;
        let result=wait_for_capacity(||Ok(true),||{checkpoints+=1;if checkpoints==2{Err(EditorError::new("CANCELLED","cancelled"))}else{Ok(())}});
        assert_eq!(result.unwrap_err().code,"CANCELLED");assert_eq!(checkpoints,2);
    }
    #[test]
    fn refilled_audio_releases_background_without_a_synthetic_progress_report() {
        let mut demands=0;let mut checkpoints=0;
        wait_for_capacity(||{demands+=1;Ok(demands<2)},||{checkpoints+=1;Ok(())}).unwrap();
        assert_eq!(demands,2);assert_eq!(checkpoints,2);
    }
}
