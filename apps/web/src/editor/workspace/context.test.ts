import {expect,it} from "vitest";
import {sameEditingWorkspace} from "./context";
import {setupAudioPreview} from "./scene/audio-preview-fixture";
it("isolates moving counters while retaining seeks, edits, errors and playback authority changes",async()=>{
  const f=await setupAudioPreview();
  try {
    const session=f.sessionFor({project_id:f.state().head.project.id,revision:f.state().head.revision,
      sequence_id:f.state().sequenceId,sample_rate:48000,start_time:f.state().playhead,idempotency_key:"view"});
    const base={...f.controller.getSnapshot(),scene:{...f.state(),audioPreview:{...f.state().audioPreview,
      phase:"playing" as const,session}}};
    const progress={...base,scene:{...base.scene,playhead:{ticks:"4800",timebase:48000},
      audioPreview:{...base.scene.audioPreview,session:{...session,consumed_frames:"8192"},deviceUnderflowEvents:1}}};
    expect(sameEditingWorkspace(base,progress)).toBe(true);
    expect(sameEditingWorkspace(base,{...progress,scene:{...progress.scene,externalRevision:"new-head"}})).toBe(false);
    expect(sameEditingWorkspace(base,{...progress,scene:{...progress.scene,selectedId:"image"}})).toBe(false);
    expect(sameEditingWorkspace(base,{...progress,scene:{...progress.scene,audioPreview:{...progress.scene.audioPreview,pictureError:"revoked"}}})).toBe(false);
    for(const patch of [{generation:"2"},{owner_id:"other"},{overlay_epoch:"2"}])
      expect(sameEditingWorkspace(base,{...progress,scene:{...progress.scene,audioPreview:{...progress.scene.audioPreview,session:{...session,...patch}}}})).toBe(false);
    const paused={...base,scene:{...base.scene,audioPreview:{...base.scene.audioPreview,phase:"paused" as const}}};
    expect(sameEditingWorkspace(paused,{...paused,scene:{...paused.scene,playhead:progress.scene.playhead}})).toBe(false);
  }finally{f.cleanup();}
});

it("keeps controller selection identity stable across scene cursor publications", async () => {
  const f = await setupAudioPreview();
  try {
    const before = f.controller.getSnapshot();
    const asPlaying = (state: typeof before) => ({ ...state, scene: { ...state.scene!, audioPreview: { ...state.scene!.audioPreview, phase: "playing" as const } } });
    for (let i = 1; i <= 60; i++) {
      f.controller.scene.seek({ ticks: String(i), timebase: 30 });
      const next = f.controller.getSnapshot();
      expect(next.selections).toBe(before.selections);
      expect(sameEditingWorkspace(asPlaying(before), asPlaying(next))).toBe(true);
    }
    f.controller.scene.select("image");
    expect(sameEditingWorkspace(asPlaying(before), asPlaying(f.controller.getSnapshot()))).toBe(false);
  } finally { f.cleanup(); }
});
