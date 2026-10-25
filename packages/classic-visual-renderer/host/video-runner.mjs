import fs from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { BrowserRealm,QuotaMonitor,PacketReader,PacketWriter,CallbackBroker,validateEnvelope } from "@opencut/classic-audio-renderer/internal-host-runtime";
import { validateAdmission,validateTextBegin,validateCaptionBegin,validateGraphicBegin,validateBlendBegin,validateMaskBegin,validateSceneBegin } from "./protocol.mjs";
import { videoBrowserHooks } from "./video-browser.mjs";
import { integer,reject,VisualExecutionError } from "../src/validation.mjs";
export function validateVideoBegin(packet) {
  validateEnvelope(packet);if(!["beginTextVideo","beginCaptionVideo","beginGraphicVideo","beginBlendVideo","beginMaskVideo","beginSceneVideo"].includes(packet.type))reject("INVALID_HOST_PROTOCOL","Expected typed persistent video begin");
  const limit=packet.executionLimit;
  if(Object.hasOwn(packet,"executionDeadlineMs")||!limit||Object.keys(limit).sort().join()!=="kind,milliseconds"||!["total","progress"].includes(limit.kind))reject("INVALID_HOST_PROTOCOL","Expected an exact captured video lifetime");
  const executionLimit={kind:limit.kind,milliseconds:integer(limit.milliseconds,"video lifetime",1,2147483647)};
  const canvas=packet.canvasEncoding;
  if(canvas!==undefined&&canvas!==null) {
    if(packet.type!=="beginSceneVideo"||Object.keys(canvas).sort().join()!=="bitrate,fpsDenominator,fpsNumerator,maxOutputBytes")reject("INVALID_HOST_PROTOCOL","Invalid canvas video selection");
    integer(canvas.fpsNumerator,"canvas FPS numerator",1);integer(canvas.fpsDenominator,"canvas FPS denominator",1);
    integer(canvas.bitrate,"canvas bitrate",1,2**32-1);integer(canvas.maxOutputBytes,"canvas bytes",1,8*1024*1024);
  }
  return {...structuredClone(packet),frameCount:integer(packet.frameCount,"video frames",1),executionLimit,admission:validateAdmission(packet.admission)};
}
export async function runVideoHost(input=process.stdin,output=process.stdout) {
  const controller=new AbortController(),reader=new PacketReader(input);
  let begin,writer,broker,monitor,realm,starting,work,terminal=false,finishing=false,busy=false,next=0,ready=false;
  const abort=error=>{if(terminal||controller.signal.aborted)return;if(error?.code==="AUDIO_DEADLINE_EXCEEDED")error=new VisualExecutionError("VIDEO_TIME_LIMIT","Video task exceeded its captured deadline");controller.abort(error);broker?.close(error);if(finishing){void realm?.stop().catch(()=>{});}else void finish(error);};
  async function finish(error) {
    if(terminal||finishing)return;finishing=true;
    let encodedBytes;
    try {
      if(!error&&begin.canvasEncoding){realm.finalizing=true;encodedBytes=await videoBrowserHooks.finishCanvas(realm,next);}
      if(controller.signal.aborted)throw controller.signal.reason;
      broker?.close(error);await realm?.rgbaTransport?.close();await realm?.stop();monitor?.stop();
      if(controller.signal.aborted)throw controller.signal.reason;
      terminal=true;
      if(writer)await writer.send(error?{type:error.code==="CANCELLED"?"cancelled":"failed",error:{code:error.code??"VISUAL_HOST_FAILED"},quota:monitor?.receipt()}:{type:"complete",frameCount:next,...(encodedBytes===undefined?{}:{encodedBytes}),quota:monitor.receipt()});
    } catch(failure) {
      broker?.close(failure);await realm?.rgbaTransport?.close();await realm?.stop();monitor?.stop();
      terminal=true;
      if(writer)await writer.send({type:failure.code==="CANCELLED"?"cancelled":"failed",error:{code:failure.code??"VISUAL_HOST_FAILED"},quota:monitor?.receipt()});
    } finally {if(realm)realm.finalizing=false;input.destroy();}
  }
  async function start() {
    // Admission retains the original per-frame limits. The one persistent
    // browser has the captured total or progress lifetime,not one frame's TTL.
    monitor=new QuotaMonitor({...begin.admission,deadlineMs:begin.executionLimit.milliseconds},abort,undefined,value=>validateAdmission(value,begin.executionLimit.milliseconds),begin.executionLimit.kind==="progress");await monitor.start();
    // Tag every callback request with the active frame. Replies retain the
    // existing bounded broker wire and monotonically increasing callback IDs.
    const tagged={send:packet=>writer.send({...packet,...(packet.type==="request"?{frameIndex:realm.currentFrame?.frameIndex}:{})})};
    broker=new CallbackBroker(tagged);realm=new BrowserRealm(begin,broker,writer,monitor,controller.signal,videoBrowserHooks);
    await realm.start();ready=true;
  }
  reader.on("packet",packet=>{
    try {
      if(!begin){begin=validateVideoBegin(packet);writer=new PacketWriter(output,begin.executionId);const send=writer.send.bind(writer);writer.send=packet=>{if(packet.type==="ready")ready=true;return send(packet);};starting=start().catch(abort);return;}
      validateEnvelope(packet,begin.executionId);if(terminal)reject("INVALID_HOST_PROTOCOL","Video execution ended");
      if(packet.type==="cancel"){abort(new VisualExecutionError("CANCELLED","Host cancelled video"));return;}
      if(finishing)reject("INVALID_HOST_PROTOCOL","Video is finalizing");
      if(packet.type==="reply"||packet.type==="reply_chunk"){if(!busy)reject("INVALID_HOST_PROTOCOL","Reply outside frame");broker.receive(packet);return;}
      if(packet.type==="endVideo") {if(!ready||busy||next!==begin.frameCount)reject("INVALID_HOST_PROTOCOL","Premature video completion");work=finish();return;}
      if(packet.type!=="frame"||!ready||busy||packet.frameIndex!==next||next>=begin.frameCount)reject("INVALID_HOST_PROTOCOL","Video frames require ordered single-frame admission");
      const [validateFrame,type]={beginTextVideo:[validateTextBegin,"beginTextFrame"],beginCaptionVideo:[validateCaptionBegin,"beginCaptionFrame"],beginGraphicVideo:[validateGraphicBegin,"beginGraphicFrame"],beginBlendVideo:[validateBlendBegin,"beginBlendFrame"],beginMaskVideo:[validateMaskBegin,"beginMaskFrame"],beginSceneVideo:[validateSceneBegin,"beginSceneFrame"]}[begin.type];
      const frame=validateFrame({...packet,type,admission:begin.admission});
      if(next===0)begin.identity=structuredClone(frame.plan.identity);
      else for(const key of ["projectId","revision","sequenceId","width","height","profile"])if(frame.plan.identity[key]!==begin.identity[key])reject("INVALID_HOST_PROTOCOL","Fixed video identity changed");
      realm.currentFrame=frame;busy=true;
      work=(async()=>{const frameTimer=setTimeout(()=>abort(new VisualExecutionError("VIDEO_TIME_LIMIT","Video frame exceeded its captured deadline")),begin.admission.deadlineMs);
        try {const receipt=await realm.render();if(controller.signal.aborted)throw controller.signal.reason;
        realm.currentFrame=undefined;busy=false;next++;
        await writer.send({type:"frameComplete",frameIndex:frame.frameIndex,receipt});
        } finally {clearTimeout(frameTimer);}
      })().catch(abort);
    } catch(error){abort(error);}
  });
  reader.on("failure",abort);reader.on("end",()=>{if(!terminal)abort(new VisualExecutionError("HOST_PIPE_CLOSED","Host closed video input"));});
  const signal=()=>abort(new VisualExecutionError("CANCELLED","Supervisor stopped video"));process.on("SIGTERM",signal);process.on("SIGINT",signal);
  try {await new Promise(resolve=>input.once("close",resolve));await starting;await work;}finally{process.off("SIGTERM",signal);process.off("SIGINT",signal);}
}
if(process.argv[1]&&await fs.realpath(process.argv[1]).catch(()=>undefined)===fileURLToPath(import.meta.url))await runVideoHost();
