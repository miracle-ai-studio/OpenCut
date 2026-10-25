// Verifier-only injection, serialized into the already-owned isolated page.
// No source clock/gain/PCM computation, DTO changes or authorized byte copies.
export function installNativePreviewTiming(traceDispatch=false) {
  const producer=globalThis.__opencutPreviewProducer, callback=globalThis.__opencutPreviewCallback;
  if (!producer || typeof producer.next!=="function" || typeof callback!=="function" || globalThis.__opencutPreviewTiming) throw Error("No unique prepared native producer");
  const next=producer.next, encode=globalThis.btoa;
  let current=null;
  globalThis.__opencutPreviewTiming={last:null};
  let longTaskObserver;
  if(traceDispatch){
    const meta=globalThis.__opencutPreviewTiming;
    meta.long_tasks=[];meta.long_task_count=0;
    meta.long_tasks_supported=typeof PerformanceObserver!=="undefined"&&PerformanceObserver.supportedEntryTypes.includes("longtask");
    if(meta.long_tasks_supported){
      longTaskObserver=new PerformanceObserver(list=>{
        for(const e of list.getEntries()){
          meta.long_task_count++;meta.long_tasks.push({start_ms:e.startTime,duration_ms:e.duration});
          meta.long_tasks.sort((a,b)=>b.duration_ms-a.duration_ms);meta.long_tasks.length=Math.min(32,meta.long_tasks.length);
        }
      });longTaskObserver.observe({type:"longtask"});
    }
  }
  globalThis.__opencutPreviewCallback=async function(op,args) {
    const t=performance.now();if(traceDispatch&&current&&op==="compilePreviewBlock")current.callback_enter_ms=t;
    try{return await callback(op,args);}
    finally{if(current&&op==="compilePreviewBlock"){const end=performance.now();current.callback_ms+=end-t;if(traceDispatch)current.callback_exit_ms=end;}}
  };
  producer.next=async function(command) {
    current={command_id:command.id,start_sample:command.startSample,frame_count:command.frameCount,
      callback_ms:0,producer_ms:0,encode_ms:0,encode_calls:0,after_next:false};
    globalThis.__opencutPreviewTiming.last=current;
    const t=performance.now();
    if(traceDispatch)current.producer_enter_ms=t;
    try{return await next.call(this,command);}
    finally{current.producer_ms=performance.now()-t;current.encode_after=performance.now();current.after_next=true;}
  };
  globalThis.btoa=function(value) {
    const eligible=current?.after_next&&typeof value==="string"&&value.length===current.frame_count*4;
    try{return encode.call(this,value);}finally{if(eligible){
      const end=performance.now();current.encode_ms+=end-current.encode_after;current.encode_after=end;
      current.encode_calls++;if(traceDispatch)current.encode_exit_ms=end;
    }}
  };
  globalThis.__restoreNativePreviewTiming=()=>{
    producer.next=next;globalThis.__opencutPreviewCallback=callback;globalThis.btoa=encode;
    longTaskObserver?.disconnect();
    delete globalThis.__opencutPreviewTiming;delete globalThis.__restoreNativePreviewTiming;
  };
}

const decimal=s=>typeof s==="string"&&/^(0|[1-9][0-9]{0,19})$/.test(s);
export class NativePreviewTiming {
  constructor(now=()=>performance.now()) {this.now=now;this.active=new Map();this.rows=[];this.overflow=false;}
  command(c) {
    if(!decimal(c.id)||!decimal(c.startSample)||!Number.isInteger(c.frameCount)||c.frameCount<1||c.frameCount>4096)throw Error("Invalid observed command");
    if(this.active.size>=4||this.rows.length>=4096){this.overflow=true;return;}
    this.active.set(c.id,{command_id:c.id,start_sample:c.startSample,frame_count:c.frameCount,
      began:this.now(),broker_ms:0,evaluate_ms:0,page_callback_ms:0,page_producer_ms:0,page_encode_ms:0,
      pcm_write_ms:0,post_evaluate_to_first_pcm_ms:0,encode_calls:0,compiled_bytes:0});
  }
  row(id) {return this.active.get(id);}
  finish(id) {
    const row=this.row(id);if(!row)return;
    row.total_ms=this.now()-row.began;
    delete row.began;delete row.evaluate_ended;this.active.delete(id);
    if(Object.values(row).some(x=>typeof x==="number"&&(!Number.isFinite(x)||x<0)))throw Error("Invalid timing observation");
    this.rows.push(row);
  }
  snapshot() {return {schema_version:1,overflow:this.overflow,incomplete_commands:this.active.size,rows:this.rows};}
}
