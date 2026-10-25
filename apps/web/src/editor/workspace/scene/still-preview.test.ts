import {afterEach,expect,it,vi} from "vitest";
import {StillPreview} from "./still-preview";
import type {SessionHost} from "./session";
import type {EditorApi} from "../api";
import type {AudioPreviewSession,AudioPreviewControl,AudioPreviewCreate} from "./types";
afterEach(()=>vi.useRealTimers());
function fixture(){
 let current:AudioPreviewSession|null=null,revision="r",creates=0;
 const create=vi.fn(async(request:AudioPreviewCreate)=>{creates++;current={session_id:`s${creates}`,generation:"1",overlay_epoch:"0",owner_id:"owner",lease_id:"lease",state:"ready",presentation_time:request.start_time,identity:{project_id:"p",revision:request.revision,sequence_id:"s",sample_rate:48000},actual_start_sample:"0",total_frames:"480000"} as AudioPreviewSession;return current;});
 const control=vi.fn(async(request:AudioPreviewControl)=>{const s=current!;
  if(request.command.kind==="seek")current={...s,generation:String(Number(s.generation)+1),presentation_time:request.command.sequence_time};
  else if(request.command.kind==="pause")current={...s,state:"paused"};
  return current!;
 });
 const api={scene:{audioPreviewCreate:create,audioPreviewControl:control},sdk:{hostCapabilities:async()=>({tools:[]})}} as unknown as EditorApi;
 const host={api:()=>api,generation:()=>1,viewGeneration:()=>1,state:()=>({head:{project:{id:"p"},revision},historical:null,sequenceId:"s"})} as unknown as SessionHost;
 const still=new StillPreview(host);
 return{still,create,control,setRevision:(value:string)=>{revision=value;},created:()=>creates};
}
it("prepares and seeks a paused native owner without creating any sound output",async()=>{
 const f=fixture();
 try{
  const first=await f.still.read({ticks:"2",timebase:1});expect(first.state).toBe("paused");
  expect(f.still.currentPaused({ticks:"4",timebase:2})).toBe(first);
  expect(f.still.currentPaused({ticks:"3",timebase:1})).toBeNull();
  const next=await f.still.read({ticks:"5",timebase:1});expect(next.presentation_time).toEqual({ticks:"5",timebase:1});
  expect(f.create).toHaveBeenCalledOnce();expect(f.control.mock.calls.map(([v])=>v.command.kind)).toEqual(["pause","seek"]);
 }finally{await f.still.close();}
 expect(f.control.mock.calls.filter(([v])=>v.command.kind==="stop")).toHaveLength(1);
});
it("retires a fixed revision before preparing the next revision",async()=>{
 const f=fixture();try{
  await f.still.read({ticks:"0",timebase:1});f.setRevision("new");
  expect(f.still.currentPaused({ticks:"0",timebase:1})).toBeNull();
  await f.still.read({ticks:"1",timebase:1});
  expect(f.created()).toBe(2);expect(f.control.mock.calls.map(([v])=>v.command.kind)).toEqual(["pause","stop","pause"]);
 }finally{await f.still.close();}
});
it("renews the paused lease and stops renewing after disposal",async()=>{
 vi.useFakeTimers();const f=fixture();await f.still.read({ticks:"0",timebase:1});
 await vi.advanceTimersByTimeAsync(15000);expect(f.control.mock.calls.filter(([v])=>v.command.kind==="heartbeat")).toHaveLength(3);
 await f.still.close();await vi.advanceTimersByTimeAsync(10000);expect(f.control.mock.calls.filter(([v])=>v.command.kind==="heartbeat")).toHaveLength(3);
});
it("closes a late native owner before allowing the sound session handoff",async()=>{
 const f=fixture(),original=f.create.getMockImplementation()!;let release!:()=>void;
 f.create.mockImplementationOnce(async request=>{await new Promise<void>(done=>{release=done;});return original(request);});
 const reading=f.still.read({ticks:"0",timebase:1}).catch(error=>error);await vi.waitFor(()=>expect(f.create).toHaveBeenCalled());
 const closing=f.still.close();release();expect((await reading).name).toBe("AbortError");await closing;
 expect(f.control.mock.calls.filter(([v])=>v.command.kind==="stop")).toHaveLength(1);
});
