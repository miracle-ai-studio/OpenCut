import {expect,it,vi} from "vitest";
import {ScenePictureSource} from "./scene-picture-source";
import type {AudioPreviewSession} from "./types";
import type {VisualPreviewRenderer,VisualPreviewExecutionApi} from "./visual-preview-renderer";
const session={session_id:"session",generation:"1",overlay_epoch:"0",owner_id:"owner",lease_id:"lease",identity:{project_id:"p",revision:"r",sequence_id:"s",sample_rate:48000},presentation_time:{ticks:"0",timebase:1},total_frames:"2016000"} as AudioPreviewSession;
function fixture(){
 const closed=vi.fn();let revision="r",width=960;
 const render=vi.fn(async(...args:Parameters<VisualPreviewRenderer["render"]>)=>{
  const time=args[5]!.time!,n=Math.floor(Number(time.ticks)/time.timebase*25);
  return {frame:{prepare_ahead:true,plan:{identity:{projectId:"p",revision,sequenceId:"s",time:{ticks:String(n),timebase:25}}},next_time:{ticks:String(n+1),timebase:25},time_seconds:n/25},
   pixels:{width,height:540,close:closed},runtime:{gpuBackend:"webgpu"}} as unknown as Awaited<ReturnType<VisualPreviewRenderer["render"]>>;
 });
 const source=new ScenePictureSource({render},{} as VisualPreviewExecutionApi,session);
 return {source,render,closed,changeRevision:()=>revision="other",changeWidth:()=>width=640};
}
it("uses native frame boundaries and session duration, reuses the first bitmap, and forwards retained-pixel admission",async()=>{
 const f=fixture(),signal=new AbortController().signal;await f.source.initialize(signal);
 expect(f.source.descriptor()).toEqual({project_id:"p",revision:"r",sequence_id:"s",width:960,height:540,duration:{ticks:"2016000",timebase:48000},frame_step_seconds:.04});
 expect(f.source.preparationFrames).toBe(24);
 expect(f.source.hasFrame({ticks:"42",timebase:1})).toBe(false);
 await f.source.frame({ticks:"0",timebase:1},signal);expect(f.render).toHaveBeenCalledTimes(1);expect(f.render.mock.calls[0][1]).toEqual({session_id:"session",generation:"1",overlay_epoch:"0",owner_id:"owner",lease_id:"lease"});
 const next=await f.source.frame({ticks:"1",timebase:25},signal,123456);
 expect(next.frame.next_time).toEqual({ticks:"2",timebase:25});expect(f.render.mock.calls[1][5]).toEqual({time:{ticks:"1",timebase:25},reservedBytes:123456,end:{ticks:"2016000",timebase:48000},captureSurface:true});
});
it("rejects a changed revision or raster and closes late pixels after cancellation",async()=>{
 for(const change of ["changeRevision","changeWidth"] as const){const f=fixture(),signal=new AbortController().signal;await f.source.initialize(signal);await f.source.frame(session.presentation_time,signal);f[change]();await expect(f.source.frame({ticks:"1",timebase:25},signal)).rejects.toThrow("fixed session");expect(f.closed).toHaveBeenCalledOnce();}
 const f=fixture(),cancel=new AbortController();f.render.mockImplementationOnce(async()=>{cancel.abort();return {pixels:{close:f.closed}} as unknown as Awaited<ReturnType<VisualPreviewRenderer["render"]>>;});
 await expect(f.source.initialize(cancel.signal)).rejects.toBeDefined();expect(f.closed).toHaveBeenCalledOnce();
});
