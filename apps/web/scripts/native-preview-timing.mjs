// Verifier-only injection, serialized into the already-owned isolated page.
// No source clock/gain/PCM computation, DTO changes or authorized byte copies.
export function installNativePreviewTiming() {
  const producer=globalThis.__opencutPreviewProducer, callback=globalThis.__opencutPreviewCallback;
  if (!producer || typeof producer.next!=="function" || typeof callback!=="function" || globalThis.__opencutPreviewTiming) throw Error("No unique prepared native producer");
  const next=producer.next, from=Array.from, join=Array.prototype.join, arrays=new WeakSet();
  let current=null;
  globalThis.__opencutPreviewTiming={last:null};
  globalThis.__opencutPreviewCallback=async function(op,args) {
    const t=performance.now();try{return await callback(op,args);}
    finally{if(current&&op==="compilePreviewBlock")current.callback_ms+=performance.now()-t;}
  };
  producer.next=async function(command) {
    current={command_id:command.id,start_sample:command.startSample,frame_count:command.frameCount,
      callback_ms:0,producer_ms:0,hex_ms:0,hex_calls:0,after_next:false};
    globalThis.__opencutPreviewTiming.last=current;
    const t=performance.now();
    try{return await next.call(this,command);}
    finally{current.producer_ms=performance.now()-t;current.after_next=true;}
  };
  Array.from=function(...args) {
    const eligible=current?.after_next&&args[0] instanceof Uint8Array&&args[0].length===current.frame_count*4;
    const t=eligible?performance.now():0,result=from.apply(this,args);
    if(eligible){current.hex_ms+=performance.now()-t;current.hex_calls++;arrays.add(result);}return result;
  };
  Array.prototype.join=function(...args) {
    if(!arrays.has(this))return join.apply(this,args);
    const t=performance.now();try{return join.apply(this,args);}finally{current.hex_ms+=performance.now()-t;arrays.delete(this);}
  };
  globalThis.__restoreNativePreviewTiming=()=>{
    producer.next=next;globalThis.__opencutPreviewCallback=callback;Array.from=from;Array.prototype.join=join;
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
      began:this.now(),broker_ms:0,evaluate_ms:0,page_callback_ms:0,page_producer_ms:0,page_hex_ms:0,
      pcm_write_ms:0,post_evaluate_to_first_pcm_ms:0,hex_calls:0,compiled_bytes:0});
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
