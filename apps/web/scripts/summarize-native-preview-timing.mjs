import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { summarizeNativePreviewDispatch } from "./native-preview-dispatch.mjs";

const phases=["total_ms","evaluate_ms","broker_ms","page_callback_ms","page_producer_ms","page_hex_ms","observer_ms","pcm_write_ms","post_evaluate_to_first_pcm_ms"];
export function summarizeNativePreviewTiming(snapshot) {
  if(snapshot.schema_version!==1||snapshot.overflow||snapshot.incomplete_commands!==0||!Array.isArray(snapshot.rows)||snapshot.rows.length<1||snapshot.rows.length>4096)throw Error("Incomplete native phase observation");
  const measuredPhases=snapshot.dispatch?[...phases,"node_callback_ms","page_body_ms"]:phases;
  let cursor=0n;
  const rows=snapshot.rows.map((row,k)=>{
    if(row.command_id!==String(k+1)||row.start_sample!==String(cursor)||!Number.isInteger(row.frame_count)||row.frame_count<1||row.frame_count>4096||row.hex_calls!==2||!Number.isInteger(row.compiled_bytes)||row.compiled_bytes<1||row.compiled_bytes>16*1024*1024)throw Error("Native command observation is not contiguous");
    cursor+=BigInt(row.frame_count);
    for(const key of measuredPhases)if(!Number.isFinite(row[key])||row[key]<0)throw Error("Invalid phase duration");
    if(row.page_producer_ms+0.01<row.page_callback_ms)throw Error("Callback exceeds containing producer");
    return {command_id:row.command_id,start_sample:row.start_sample,frame_count:row.frame_count,
      compiled_bytes:row.compiled_bytes,hex_calls:row.hex_calls,
      ...Object.fromEntries(measuredPhases.map(key=>[key,row[key]])),
      page_unpack_validate_mix_ms:Math.max(0,row.page_producer_ms-row.page_callback_ms),
      evaluate_residual_ms:Math.max(0,row.evaluate_ms-row.page_producer_ms-row.page_hex_ms),
      callback_outside_broker_ms:Math.max(0,row.page_callback_ms-row.broker_ms)};
  });
  const statistics=key=>{
    const v=rows.map(r=>r[key]).sort((a,b)=>a-b);
    return {mean_ms:v.reduce((a,b)=>a+b,0)/v.length,p95_ms:v[Math.ceil(.95*v.length)-1],max_ms:v.at(-1)};
  };
  const over=rows.filter(r=>r.total_ms>r.frame_count/48);
  return {schema_version:1,commands:rows.length,contiguous_supplied_frames:String(cursor),
    phases:Object.fromEntries([...measuredPhases,"page_unpack_validate_mix_ms","evaluate_residual_ms","callback_outside_broker_ms"].map(k=>[k,statistics(k)])),
    commands_over_own_audio_duration:over.length,
    slowest_commands:[...rows].sort((a,b)=>b.total_ms-a.total_ms).slice(0,12),
    ...(snapshot.dispatch?{dispatch:summarizeNativePreviewDispatch(snapshot.dispatch,rows)}:{}),
    scope:"Node command entry to original block write; elapsed overlapping phases, callback includes Rust/IPC/scheduling; producer minus callback combines unpack/validation/mixing. Evaluate residual also includes DataView/output validation, transport and scheduling, not pure IPC. Callback outside broker includes dispatch/return/serialization/scheduling. Extra observation RPC is separately measured. No physical DAC, pure Rust CPU or GC causal attribution."};
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  const directory=process.argv[2],files=(await fs.readdir(directory)).filter(f=>/^native-[0-9]+\.json$/.test(f));
  if(files.length!==1)throw Error("Expected one owned native producer observation");
  const data=await fs.readFile(path.join(directory,files[0]));if(data.length>2*1024*1024)throw Error("Native timing artifact exceeds bound");
  console.log(JSON.stringify(summarizeNativePreviewTiming(JSON.parse(data))));
}
