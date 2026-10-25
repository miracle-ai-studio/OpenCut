import fs from "node:fs/promises";
import path from "node:path";
import { PerformanceObserver } from "node:perf_hooks";
import { BrowserRealm } from "../../../packages/classic-audio-renderer/host/browser.mjs";
import { PacketReader, PacketWriter, CallbackBroker } from "../../../packages/classic-audio-renderer/host/protocol.mjs";
import { installNativePreviewTiming, NativePreviewTiming } from "./native-preview-timing.mjs";
import { NativePreviewDispatch } from "./native-preview-dispatch.mjs";

// Explicit local verifier assembly only. NODE_OPTIONS is removed by the
// existing BrowserRealm before launching Chromium; public SDK/API unchanged.
if (path.basename(process.argv[1]??"")==="preview-runner.mjs") {
  const directory=process.env.OPENCUT_PREVIEW_PROFILE_DIR;
  if(!directory||!path.isAbsolute(directory)||await fs.realpath(directory)!==directory||(await fs.lstat(directory)).isSymbolicLink())throw Error("Native timing needs an owned canonical directory");
  const file=await fs.open(path.join(directory,`native-${process.pid}.json`),"wx",0o600);
  const timing=new NativePreviewTiming(),dispatch=new NativePreviewDispatch();let current=null,saved=false;
  dispatch.enableGc(PerformanceObserver.supportedEntryTypes.includes("gc"));
  const collectGc=entries=>{for(const e of entries)dispatch.gcEntry(e.startTime,e.duration,e.detail.kind,e.detail.flags);};
  const gcObserver=dispatch.gc.supported?new PerformanceObserver(list=>collectGc(list.getEntries())):null;
  gcObserver?.observe({entryTypes:["gc"]});
  let expectedTimer=performance.now()+25;
  const timer=setInterval(()=>{const now=performance.now();dispatch.lag(expectedTimer-25,now,Math.max(0,now-expectedTimer));expectedTimer=now+25;},25);timer.unref();
  async function save(){if(saved)return;saved=true;clearInterval(timer);if(gcObserver){collectGc(gcObserver.takeRecords());gcObserver.disconnect();}const text=JSON.stringify({...timing.snapshot(),dispatch:dispatch.snapshot()});if(Buffer.byteLength(text)>2*1024*1024)throw Error("Native timing artifact budget exceeded");await file.writeFile(text);await file.close();}
  const emit=PacketReader.prototype.emit;
  PacketReader.prototype.emit=function(event,p,...args){
    if(event==="packet"&&p?.type==="command"&&p.op==="next"){timing.command(p);current=p.id;}
    return emit.call(this,event,p,...args);
  };
  const request=CallbackBroker.prototype.request;
  CallbackBroker.prototype.request=async function(op,args){
    const row=timing.row(current),t=performance.now();
    try{const value=await request.call(this,op,args);if(row&&op==="compilePreviewBlockPacked")row.compiled_bytes=typeof value==="string"?Buffer.byteLength(value):0;return value;}
    finally{if(row&&op==="compilePreviewBlockPacked")row.broker_ms+=performance.now()-t;}
  };
  const start=BrowserRealm.prototype.start;
  BrowserRealm.prototype.start=async function(){
    const configure=this.hooks.configurePage;
    this.hooks={...this.hooks,configurePage:async realm=>{
      const expose=realm.page.exposeFunction.bind(realm.page);
      realm.page.exposeFunction=async(name,handler)=>expose(name,name!=="__opencutPreviewCallback"?handler:async(...args)=>{
        const row=args[0]==="compilePreviewBlock"?timing.row(current):null;
        if(row){if(args[1]?.startSample!==row.start_sample||args[1].frameCount!==row.frame_count||!row.dispatch||row.dispatch.node_callback_calls)throw Error("Callback lost observed command");row.dispatch.node_callback_calls=1;row.dispatch.node_callback_enter_ms=performance.now();}
        try{return await handler(...args);}finally{if(row){row.dispatch.node_callback_exit_ms=performance.now();row.node_callback_ms=row.dispatch.node_callback_exit_ms-row.dispatch.node_callback_enter_ms;}}
      });
      try{return await configure(realm);}finally{realm.page.exposeFunction=expose;}
    }};
    await start.call(this);const evaluate=this.page.evaluate.bind(this.page);
    const calibrate=async stage=>{for(let k=0;k<4;k++){const before=performance.now(),page=await evaluate(()=>performance.now()),after=performance.now();dispatch.clock(stage,before,page,after);}};
    this.page.evaluate=async(fn,arg)=>{
      if(fn.name==="preparePreviewPage"){
        const value=await evaluate(fn,arg);if(value?.ok===true){await evaluate(installNativePreviewTiming,true);await calibrate("prepare");}return value;
      }
      if(fn.name!=="nextPreviewPage")return evaluate(fn,arg);
      const row=timing.row(arg.id),t=performance.now();
      if(row)row.dispatch={evaluate_enter_ms:t,node_callback_calls:0};
      const value=await evaluate(fn,arg),ended=performance.now();
      // Separate this observer-only extra round trip from original evaluate.
      const page=await evaluate(()=>globalThis.__opencutPreviewTiming.last);
      if(BigInt(arg.startSample)+BigInt(arg.frameCount)===BigInt(this.begin.plan.frameCount)){
        await calibrate("terminal");dispatch.page_tasks=await evaluate(()=>{
          const {long_tasks,long_task_count,long_tasks_supported}=globalThis.__opencutPreviewTiming;
          return {long_tasks,long_task_count,long_tasks_supported};
        });
      }
      if(row){
        if(page?.command_id!==arg.id||page.start_sample!==arg.startSample||page.frame_count!==arg.frameCount)throw Error("Native timing command changed");
        row.evaluate_ms=ended-t;row.page_callback_ms=page.callback_ms;row.page_producer_ms=page.producer_ms;
        row.page_encode_ms=page.encode_ms;row.encode_calls=page.encode_calls;row.observer_ms=performance.now()-ended;row.evaluate_ended=ended;
        row.page_body_ms=page.encode_exit_ms-page.producer_enter_ms;
        Object.assign(row.dispatch,{evaluate_exit_ms:ended,...Object.fromEntries(["producer_enter_ms","encode_exit_ms","callback_enter_ms","callback_exit_ms"].map(k=>[k,page[k]]))});
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
      if(packet.type==="block"){
        const detail=row?.dispatch;if(row)delete row.dispatch;timing.finish(packet.commandId);if(row)dispatch.complete(row,detail);
      }
      if(["stopped","failed","cancelled"].includes(packet.type))await save();
    }
  };
}
