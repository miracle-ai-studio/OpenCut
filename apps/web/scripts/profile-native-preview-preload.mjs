import fs from "node:fs/promises";
import path from "node:path";
import { BrowserRealm } from "../../../packages/classic-audio-renderer/host/browser.mjs";
import { PacketReader, PacketWriter, CallbackBroker } from "../../../packages/classic-audio-renderer/host/protocol.mjs";
import { installNativePreviewTiming, NativePreviewTiming } from "./native-preview-timing.mjs";

// Explicit local verifier assembly only. NODE_OPTIONS is removed by the
// existing BrowserRealm before launching Chromium; public SDK/API unchanged.
if (path.basename(process.argv[1]??"")==="preview-runner.mjs") {
  const directory=process.env.OPENCUT_PREVIEW_PROFILE_DIR;
  if(!directory||!path.isAbsolute(directory)||await fs.realpath(directory)!==directory||(await fs.lstat(directory)).isSymbolicLink())throw Error("Native timing needs an owned canonical directory");
  const file=await fs.open(path.join(directory,`native-${process.pid}.json`),"wx",0o600);
  const timing=new NativePreviewTiming();let current=null,saved=false;
  async function save(){if(saved)return;saved=true;const text=JSON.stringify(timing.snapshot());if(Buffer.byteLength(text)>2*1024*1024)throw Error("Native timing artifact budget exceeded");await file.writeFile(text);await file.close();}
  const emit=PacketReader.prototype.emit;
  PacketReader.prototype.emit=function(event,p,...args){
    if(event==="packet"&&p?.type==="command"&&p.op==="next"){timing.command(p);current=p.id;}
    return emit.call(this,event,p,...args);
  };
  const request=CallbackBroker.prototype.request;
  CallbackBroker.prototype.request=async function(op,args){
    const row=timing.row(current),t=performance.now();
    try{const value=await request.call(this,op,args);if(row&&op==="compilePreviewBlockPackedV2")row.compiled_bytes=typeof value==="string"?Buffer.byteLength(value):0;return value;}
    finally{if(row&&op==="compilePreviewBlockPackedV2")row.broker_ms+=performance.now()-t;}
  };
  const start=BrowserRealm.prototype.start;
  BrowserRealm.prototype.start=async function(){
    await start.call(this);const evaluate=this.page.evaluate.bind(this.page);
    this.page.evaluate=async(fn,arg)=>{
      if(fn.name==="preparePreviewPage"){
        const value=await evaluate(fn,arg);if(value?.ok===true)await evaluate(installNativePreviewTiming);return value;
      }
      if(fn.name!=="nextPreviewPage")return evaluate(fn,arg);
      const row=timing.row(arg.id),t=performance.now();
      const value=await evaluate(fn,arg),ended=performance.now();
      // Separate this observer-only extra round trip from original evaluate.
      const page=await evaluate(()=>globalThis.__opencutPreviewTiming.last);
      if(row){
        if(page?.command_id!==arg.id||page.start_sample!==arg.startSample||page.frame_count!==arg.frameCount)throw Error("Native timing command changed");
        row.evaluate_ms=ended-t;row.page_callback_ms=page.callback_ms;row.page_producer_ms=page.producer_ms;
        row.page_hex_ms=page.hex_ms;row.hex_calls=page.hex_calls;row.observer_ms=performance.now()-ended;row.evaluate_ended=ended;
      }
      return value;
    };
    const stop=this.stop.bind(this);this.stop=async()=>{try{return await stop();}finally{await save();}};
  };
  const send=PacketWriter.prototype.send;
  PacketWriter.prototype.send=async function(packet){
    const row=timing.row(packet.commandId),t=performance.now();
    if(row&&packet.type==="pcm"&&packet.channel===0)row.post_evaluate_to_first_pcm_ms=t-row.evaluate_ended;
    try{return await send.call(this,packet);}
    finally{
      if(row&&packet.type==="pcm")row.pcm_write_ms+=performance.now()-t;
      if(packet.type==="block")timing.finish(packet.commandId);
      if(["stopped","failed","cancelled"].includes(packet.type))await save();
    }
  };
}
