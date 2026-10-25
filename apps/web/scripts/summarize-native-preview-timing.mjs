import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { summarizeNativePreviewDispatch } from "./native-preview-dispatch.mjs";

const phases=["total_ms","evaluate_ms","broker_ms","page_callback_ms","page_producer_ms","page_encode_ms","observer_ms","pcm_write_ms","post_evaluate_to_first_pcm_ms"];
export function summarizeNativePreviewTiming(snapshot) {
  if(snapshot.schema_version!==1||snapshot.overflow||snapshot.incomplete_commands!==0||!Array.isArray(snapshot.rows)||snapshot.rows.length<1||snapshot.rows.length>4096)throw Error("Incomplete native phase observation");
  const measuredPhases=snapshot.dispatch?[...phases,"node_callback_ms","page_body_ms"]:phases;
  let cursor=0n;
  const rows=snapshot.rows.map((row,k)=>{
    if(row.command_id!==String(k+1)||row.start_sample!==String(cursor)||!Number.isInteger(row.frame_count)||row.frame_count<1||row.frame_count>4096||row.encode_calls!==2||!Number.isInteger(row.compiled_bytes)||row.compiled_bytes<1||row.compiled_bytes>16*1024*1024)throw Error("Native command observation is not contiguous");
    cursor+=BigInt(row.frame_count);
    for(const key of measuredPhases)if(!Number.isFinite(row[key])||row[key]<0)throw Error("Invalid phase duration");
    if(row.page_producer_ms+0.01<row.page_callback_ms)throw Error("Callback exceeds containing producer");
    return {command_id:row.command_id,start_sample:row.start_sample,frame_count:row.frame_count,
      compiled_bytes:row.compiled_bytes,encode_calls:row.encode_calls,
      ...Object.fromEntries(measuredPhases.map(key=>[key,row[key]])),
      page_unpack_validate_mix_ms:Math.max(0,row.page_producer_ms-row.page_callback_ms),
      evaluate_residual_ms:Math.max(0,row.evaluate_ms-row.page_producer_ms-row.page_encode_ms),
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
export function correlateNativePreviewCredit(snapshot,continuity) {
  // Independent Worker transfer counts identify the corresponding native
  // command. Sample/window association is not an exact wall-clock cause proof.
  summarizeNativePreviewTiming(snapshot);
  const tails=continuity?.credit?.slowest_refills,windows=continuity?.consumption?.windows;
  if(!Array.isArray(tails)||tails.length<1||tails.length>32||!Array.isArray(windows)||windows.length<1||windows.length>64)throw Error("Missing bounded credit/window observation");
  const decimal=s=>typeof s==="string"&&/^(0|[1-9][0-9]{0,19})$/.test(s)&&BigInt(s)<=18446744073709551615n;
  const rows=new Map(snapshot.rows.map(r=>[r.command_id,r])),seen=new Set();
  const samples=tails.map(t=>{
    const row=rows.get(String(t.supplied_block));
    if(seen.has(t.supplied_block))throw Error("Duplicate refill command");seen.add(t.supplied_block);
    if(!row||!Number.isSafeInteger(t.completed_block)||t.completed_block<1||t.supplied_block!==t.completed_block+2||row.start_sample!==t.supplied_start_sample||row.frame_count!==t.supplied_frames||!decimal(t.consumed_frames)||![t.receipt_at_ms,t.report_began_ms,t.report_ended_ms,t.supplied_at_ms,t.completion_to_report_ms,t.completion_to_supply_ms].every(Number.isFinite)||t.receipt_at_ms<0||t.report_began_ms<t.receipt_at_ms||t.report_ended_ms<t.report_began_ms||t.supplied_at_ms<t.report_began_ms||Math.abs(t.completion_to_report_ms-(t.report_began_ms-t.receipt_at_ms))>.001||Math.abs(t.completion_to_supply_ms-(t.supplied_at_ms-t.receipt_at_ms))>.001||!Array.isArray(t.preceding_controls)||t.preceding_controls.length>8||!Number.isSafeInteger(t.preceding_control_count)||t.preceding_control_count<t.preceding_controls.length)throw Error("Credit lost native command identity or interval");
    if(!Number.isFinite(t.report_headers_ended_ms)||t.report_headers_ended_ms<t.report_began_ms||t.report_headers_ended_ms>t.report_ended_ms)throw Error("Invalid report body interval");
    const controls=t.preceding_controls.map(c=>{
      if(!["heartbeat","report_consumed"].includes(c.kind)||![c.began_ms,c.ended_ms].every(Number.isFinite)||c.began_ms<0||c.ended_ms<c.began_ms||c.began_ms>t.report_began_ms||c.ended_ms<t.receipt_at_ms)throw Error("Invalid preceding control interval");
      if(!Number.isFinite(c.headers_ended_ms)||c.headers_ended_ms<c.began_ms||c.headers_ended_ms>c.ended_ms)throw Error("Invalid preceding body interval");
      return {kind:c.kind,began_ms:c.began_ms,headers_ended_ms:c.headers_ended_ms,ended_ms:c.ended_ms,overlap_ms:Math.max(0,Math.min(c.ended_ms,t.report_began_ms)-Math.max(c.began_ms,t.receipt_at_ms))};
    });
    const end=BigInt(t.supplied_start_sample)+BigInt(t.supplied_frames),begin=BigInt(t.consumed_frames);
    if(begin>end)throw Error("Refill preceded its consumed cursor");
    const associated=[];
    for(const [k,w] of windows.entries()) {
      if(!decimal(w.start_frame)||!decimal(w.end_frame)||BigInt(w.end_frame)<BigInt(w.start_frame)||!Number.isSafeInteger(w.missing_frames)||w.missing_frames<0)throw Error("Invalid consumption window");
      if(BigInt(w.start_frame)<=end&&BigInt(w.end_frame)>=begin)associated.push({one_based_index:k+1,missing_frames:w.missing_frames});
    }
    const keys=["completed_block","consumed_frames","supplied_block","supplied_start_sample","supplied_frames","receipt_at_ms","report_began_ms","report_headers_ended_ms","report_ended_ms","supplied_at_ms","completion_to_report_ms","completion_to_supply_ms","preceding_control_count"];
    return {...Object.fromEntries(keys.map(k=>[k,t[k]])),preceding_controls:controls,native_command:{command_id:row.command_id,start_sample:row.start_sample,frame_count:row.frame_count,total_ms:row.total_ms,evaluate_ms:row.evaluate_ms,broker_ms:row.broker_ms,page_producer_ms:row.page_producer_ms,page_callback_ms:row.page_callback_ms},associated_sample_windows:associated};
  });
  return {samples,scope:"Greatest32 Worker entry-to-refill samples; native ID/start/count checked against independent cumulative transfer bytes for this zero-start uninterrupted fixture. Control span includes original fetch and response text completion; subsequent SDK JSON/validation/scheduling is not separately timed. Preceding overlap is in converted Worker/page clock. Window association uses consumed/supplied sample ranges, not exact missing-quantum timing or sole-cause attribution; final Worker extraction may omit terminal supply tail."};
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  const directory=process.argv[2],files=(await fs.readdir(directory)).filter(f=>/^native-[0-9]+\.json$/.test(f));
  if(files.length!==1)throw Error("Expected one owned native producer observation");
  const data=await fs.readFile(path.join(directory,files[0]));if(data.length>2*1024*1024)throw Error("Native timing artifact exceeds bound");
  const snapshot=JSON.parse(data),summary=summarizeNativePreviewTiming(snapshot);
  if(process.argv[3]) {
    let continuity;
    try {continuity=JSON.parse(await fs.readFile(path.join(directory,"scene-preview-continuity-failure.json"),"utf8")).evidence;}
    catch(error){if(error.code!=="ENOENT")throw error;const report=JSON.parse(await fs.readFile(process.argv[3],"utf8"));continuity=report.evidence?.find(e=>e.scene_preview_continuity)?.scene_preview_continuity;}
    summary.credit_correlation=correlateNativePreviewCredit(snapshot,continuity);
  }
  console.log(JSON.stringify(summary));
}
