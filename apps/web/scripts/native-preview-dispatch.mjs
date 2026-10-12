// Verifier-only numeric witnesses. These top samples are deliberately not an
// exhaustive scheduler trace; original complete command accounting stays separate.
function greatest(items,value,key,limit=32){
  if(items.length<limit){items.push(value);return;}
  let least=0;for(let k=1;k<items.length;k++)if(items[k][key]<items[least][key])least=k;
  if(value[key]>items[least][key])items[least]=value;
}
export class NativePreviewDispatch {
  constructor(){this.clocks=[];this.tails=[];this.lags=[];this.lag_count=0;this.page_tasks=null;}
  clock(stage,before,page,after){
    if(!["prepare","terminal"].includes(stage)||this.clocks.length>=8||![before,page,after].every(Number.isFinite)||before<0||page<0||after<before)throw Error("Invalid clock calibration");
    this.clocks.push({stage,before_ms:before,page_ms:page,after_ms:after});
  }
  lag(start,end,late){
    if(![start,end,late].every(Number.isFinite)||start<0||end<start||late<0)throw Error("Invalid timer observation");
    if(late<1)return;this.lag_count++;greatest(this.lags,{start_ms:start,end_ms:end,late_ms:late},"late_ms");
  }
  complete(row,detail){
    const keys=["evaluate_enter_ms","evaluate_exit_ms","node_callback_enter_ms","node_callback_exit_ms","producer_enter_ms","hex_exit_ms","callback_enter_ms","callback_exit_ms"];
    if(!detail||detail.node_callback_calls!==1||keys.some(k=>!Number.isFinite(detail[k])||detail[k]<0)||detail.evaluate_exit_ms<detail.evaluate_enter_ms||detail.node_callback_exit_ms<detail.node_callback_enter_ms||detail.hex_exit_ms<detail.producer_enter_ms||detail.callback_exit_ms<detail.callback_enter_ms)throw Error("Incomplete dispatch command");
    greatest(this.tails,{command_id:row.command_id,start_sample:row.start_sample,frame_count:row.frame_count,total_ms:row.total_ms,node_callback_calls:1,...Object.fromEntries(keys.map(k=>[k,detail[k]]))},"total_ms");
  }
  snapshot(){return {schema_version:1,clock_samples:this.clocks,tails:[...this.tails].sort((a,b)=>b.total_ms-a.total_ms),
    timer_lag_count:this.lag_count,timer_lags:[...this.lags].sort((a,b)=>b.late_ms-a.late_ms),page_tasks:this.page_tasks,
    scope:"Greatest32 observed command totals,25ms timer tardiness>=1ms and isolated-page long tasks; selection, not exhaustive events or CPU/GC/OS cause. Clock calibration bounds include round-trip uncertainty."};}
}
export function summarizeNativePreviewDispatch(raw,rows){
  if(raw?.schema_version!==1||!Array.isArray(raw.clock_samples)||raw.clock_samples.length!==8||!Array.isArray(raw.tails)||raw.tails.length!==Math.min(32,rows.length)||!Array.isArray(raw.timer_lags)||raw.timer_lags.length>32||!Number.isSafeInteger(raw.timer_lag_count)||raw.timer_lag_count<raw.timer_lags.length)throw Error("Incomplete dispatch observation");
  const stages=["prepare","terminal"].map(stage=>{
    const probes=raw.clock_samples.filter(s=>s.stage===stage);
    if(probes.length!==4||probes.some(s=>![s.before_ms,s.page_ms,s.after_ms].every(Number.isFinite)||s.before_ms<0||s.page_ms<0||s.after_ms<s.before_ms))throw Error("Invalid calibrated clocks");
    const p=probes.reduce((a,b)=>a.after_ms-a.before_ms<b.after_ms-b.before_ms?a:b);
    return {stage,lower_ms:p.before_ms-p.page_ms,upper_ms:p.after_ms-p.page_ms,rtt_ms:p.after_ms-p.before_ms};
  });
  const lower=Math.min(...stages.map(p=>p.lower_ms)),upper=Math.max(...stages.map(p=>p.upper_ms));
  if(upper-lower>10)throw Error("Dispatch clock uncertainty exceeds10ms");
  const interval=(a,b)=>({lower_ms:a,upper_ms:b});
  const ids=new Set(),byId=new Map(rows.map(r=>[r.command_id,r]));
  const minimumSelected=[...rows].sort((a,b)=>b.total_ms-a.total_ms)[Math.min(32,rows.length)-1].total_ms;
  const tails=raw.tails.map(t=>{
    const row=byId.get(t.command_id);if(!row||ids.has(t.command_id)||row.start_sample!==t.start_sample||row.frame_count!==t.frame_count||row.total_ms!==t.total_ms||t.total_ms<minimumSelected)throw Error("Dispatch tail lost original command identity");ids.add(t.command_id);
    const detail=new NativePreviewDispatch();detail.complete(row,t);
    const nodeIntervals=raw.timer_lags.filter(l=>l.start_ms<=t.evaluate_exit_ms&&l.end_ms>=t.evaluate_enter_ms);
    // Include tasks before producer entry or after last hex join: those are
    // precisely the dispatch/serialization intervals this witness investigates.
    const pageTasks=raw.page_tasks?.long_tasks?.filter(l=>l.start_ms<=t.evaluate_exit_ms-lower&&l.start_ms+l.duration_ms>=t.evaluate_enter_ms-upper)??[];
    return {command_id:t.command_id,start_sample:t.start_sample,total_ms:t.total_ms,
      evaluate_ingress:interval(t.producer_enter_ms+lower-t.evaluate_enter_ms,t.producer_enter_ms+upper-t.evaluate_enter_ms),
      evaluate_return:interval(t.evaluate_exit_ms-t.hex_exit_ms-upper,t.evaluate_exit_ms-t.hex_exit_ms-lower),
      callback_ingress:interval(t.node_callback_enter_ms-t.callback_enter_ms-upper,t.node_callback_enter_ms-t.callback_enter_ms-lower),
      callback_return:interval(t.callback_exit_ms+lower-t.node_callback_exit_ms,t.callback_exit_ms+upper-t.node_callback_exit_ms),
      node_callback_ms:t.node_callback_exit_ms-t.node_callback_enter_ms,
      page_body_ms:t.hex_exit_ms-t.producer_enter_ms,
      overlapping_selected_node_timer_lags:nodeIntervals,overlapping_selected_page_long_tasks:pageTasks};
  });
  for(const l of raw.timer_lags)if(![l.start_ms,l.end_ms,l.late_ms].every(Number.isFinite)||l.start_ms<0||l.end_ms<l.start_ms||l.late_ms<1)throw Error("Invalid selected timer lag");
  if(typeof raw.page_tasks?.long_tasks_supported!=="boolean"||!Array.isArray(raw.page_tasks.long_tasks)||raw.page_tasks.long_tasks.length>32||!Number.isSafeInteger(raw.page_tasks.long_task_count)||raw.page_tasks.long_task_count<raw.page_tasks.long_tasks.length||!raw.page_tasks.long_tasks_supported&&raw.page_tasks.long_task_count!==0||raw.page_tasks.long_tasks.some(t=>![t.start_ms,t.duration_ms].every(Number.isFinite)||t.start_ms<0||t.duration_ms<0))throw Error("Incomplete page task observation");
  return {clock_bounds:stages,maximum_clock_uncertainty_ms:upper-lower,tails,timer_lag_count:raw.timer_lag_count,timer_lags:raw.timer_lags,page_tasks:raw.page_tasks,scope:raw.scope};
}
