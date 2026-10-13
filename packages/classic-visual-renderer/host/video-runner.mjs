import fs from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { BrowserRealm,QuotaMonitor,PacketReader,PacketWriter,CallbackBroker,validateEnvelope } from "@opencut/classic-audio-renderer/internal-host-runtime";
import { validateAdmission,validateTextBegin,validateCaptionBeginV3 } from "./protocol.mjs";
import { videoBrowserHooks } from "./video-browser.mjs";
import { integer,reject,VisualExecutionError } from "../src/validation.mjs";
export function validateVideoBegin(packet) {
  validateEnvelope(packet);if(!["beginTextVideoV1","beginCaptionVideoV2"].includes(packet.type))reject("INVALID_HOST_PROTOCOL","Expected persistent V1 video begin");
  return {...structuredClone(packet),frameCount:integer(packet.frameCount,"video frames",1,1800),admission:validateAdmission(packet.admission)};
}
export async function runVideoHost(input=process.stdin,output=process.stdout) {
  const controller=new AbortController(),reader=new PacketReader(input);
  let begin,writer,broker,monitor,realm,starting,work,terminal=false,busy=false,next=0,ready=false;
  const abort=error=>{if(terminal||controller.signal.aborted)return;controller.abort(error);broker?.close(error);void finish(error);};
  async function finish(error) {
    if(terminal)return;terminal=true;
    try {broker?.close(error);await realm?.stop();monitor?.stop();
      if(writer)await writer.send(error?{type:error.code==="CANCELLED"?"cancelled":"failed",error:{code:error.code??"VISUAL_HOST_FAILED"},quota:monitor?.receipt()}:{type:"complete",frameCount:next,quota:monitor.receipt()});
    } finally {input.destroy();}
  }
  async function start() {
    monitor=new QuotaMonitor(begin.admission,abort,undefined,validateAdmission);await monitor.start();
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
      if(packet.type==="reply"||packet.type==="reply_chunk"){if(!busy)reject("INVALID_HOST_PROTOCOL","Reply outside frame");broker.receive(packet);return;}
      if(packet.type==="endVideo") {if(!ready||busy||next!==begin.frameCount)reject("INVALID_HOST_PROTOCOL","Premature video completion");work=finish();return;}
      if(packet.type!=="frame"||!ready||busy||packet.frameIndex!==next||next>=begin.frameCount)reject("INVALID_HOST_PROTOCOL","Video frames require ordered single-frame admission");
      const frame=begin.type === "beginCaptionVideoV2"
        ? validateCaptionBeginV3({...packet,type:"beginCaptionFrameV3",admission:begin.admission})
        : validateTextBegin({...packet,type:"beginTextFrameV2",admission:begin.admission});
      if(next===0)begin.identity=structuredClone(frame.plan.identity);
      else for(const key of ["projectId","revision","sequenceId","width","height","profile"])if(frame.plan.identity[key]!==begin.identity[key])reject("INVALID_HOST_PROTOCOL","Fixed video identity changed");
      realm.currentFrame=frame;busy=true;
      work=(async()=>{const receipt=await realm.render();if(controller.signal.aborted)throw controller.signal.reason;
        realm.currentFrame=undefined;busy=false;next++;
        await writer.send({type:"frameComplete",frameIndex:frame.frameIndex,receipt});
      })().catch(abort);
    } catch(error){abort(error);}
  });
  reader.on("failure",abort);reader.on("end",()=>{if(!terminal)abort(new VisualExecutionError("HOST_PIPE_CLOSED","Host closed video input"));});
  const signal=()=>abort(new VisualExecutionError("CANCELLED","Supervisor stopped video"));process.on("SIGTERM",signal);process.on("SIGINT",signal);
  try {await new Promise(resolve=>input.once("close",resolve));await starting;await work;}finally{process.off("SIGTERM",signal);process.off("SIGINT",signal);}
}
if(process.argv[1]&&await fs.realpath(process.argv[1]).catch(()=>undefined)===fileURLToPath(import.meta.url))await runVideoHost();
