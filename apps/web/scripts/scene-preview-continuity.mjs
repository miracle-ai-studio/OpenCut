import { summarizePreviewConsumption } from "./scene-preview-consumption.mjs";
const decimal = s => typeof s === "string" && /^(0|[1-9][0-9]{0,19})$/.test(s) && BigInt(s) <= 18446744073709551615n;

// A bounded carry point joins windows; every transition is checked, including
// the join. Raw receipt history never grows with playback duration.
export class PreviewContinuity {
  constructor(sampleRate, totalFrames, windowMs = 10000) {
    if (![44100,48000].includes(sampleRate) || !decimal(totalFrames) || !Number.isInteger(windowMs) || windowMs < 10 || windowMs > 20000) throw Error("Invalid observation range");
    this.rate=sampleRate; this.total=BigInt(totalFrames); this.windowFrames=sampleRate*windowMs/1000;
    this.points=[]; this.windows=[]; this.initial=null; this.terminal=null; this.maxRaw=0;
  }
  add(points) {
    for (const p of points) {
      if (this.terminal) break; // Post-end silence is outside the scene range.
      if (!this.initial) {
        if (!decimal(p.frames) || BigInt(p.frames)!==BigInt(p.copied) || p.copied<1) throw Error("Missing initial device cursor");
        this.initial=p;
      }
      this.points.push(p); this.maxRaw=Math.max(this.maxRaw,this.points.length);
      if (this.points.length>8192 || this.windows.length>=64) throw Error("Observation budget exceeded");
      if (BigInt(p.frames)>this.total) throw Error("Device advanced beyond scene range");
      if (this.points.length>1 && p.renderFrame+ p.quantumFrames-this.points[0].renderFrame-this.points[0].quantumFrames>=this.windowFrames) this.flush();
      if (BigInt(p.frames)===this.total) { this.terminal=p; this.flush(); }
    }
  }
  flush() {
    if (this.points.length<2) return;
    const device=summarizePreviewConsumption(this.points,[],[],this.rate);
    if (device.status!=="complete_observed_span") throw Error(device.reason);
    const first=this.points[0], last=this.points.at(-1), elapsed=last.at-first.at;
    let stalled=0;
    for(let i=1;i<this.points.length;i++) if(this.points[i-1].stalled) stalled+=this.points[i].at-this.points[i-1].at;
    const consumed=Number(BigInt(last.frames)-BigInt(first.frames));
    this.windows.push({start_frame:first.frames,end_frame:last.frames,elapsed_ms:elapsed,
      copied_frames:device.copied_frames,rendered_frames:device.rendered_frames,missing_frames:device.missing_frames,
      stalled_ratio:elapsed>0?stalled/elapsed:1,
      deficit_ratio:elapsed>0?Math.max(0,1-consumed/this.rate/(elapsed/1000)):1,
      render_quantum_deficit_ratio:device.render_quantum_deficit_ratio,
      maximum_message_lag_ms:device.quantum_delivery_lag.maximum_ms, reports:this.points.length});
    this.points=[last];
  }
  result() {
    this.flush();
    if(!this.initial || !this.terminal) throw Error("Scene did not reach its final device cursor");
    const sum=k=>this.windows.reduce((n,w)=>n+w[k],0);
    const rendered=sum("rendered_frames")+this.initial.quantumFrames, copied=sum("copied_frames")+this.initial.copied;
    const elapsed=this.terminal.at-this.initial.at;
    return {complete:true,initial_cursor:this.initial.frames,final_cursor:this.terminal.frames,
      rendered_frames:rendered,copied_frames:copied,missing_frames:rendered-copied,
      elapsed_ms:elapsed,render_quantum_deficit_ratio:(rendered-copied)/rendered,
      stalled_ratio:this.windows.reduce((n,w)=>n+w.stalled_ratio*w.elapsed_ms,0)/sum("elapsed_ms"),
      deficit_ratio:Math.max(0,1-Number(this.total-BigInt(this.initial.frames))/this.rate/(elapsed/1000)),
      max_raw_points:this.maxRaw,windows:this.windows};
  }
}

// Actual Worker receipt-entry timestamps precede its production handler.
// Bounded pending correlations survive drains; controls and supplies may have
// arrived in the adjacent page/worker observation batches.
export class PreviewCreditObservations {
  constructor() {this.previous=0;this.pending=new Map();this.controls=[];this.supplies=new Map();this.samples=[];this.tails=[];this.unmatched=0;}
  add(receipts,controls,supplies) {
    for(const p of receipts) {
      if(!decimal(p.frames)||!Number.isSafeInteger(p.completedBlocks)||p.completedBlocks<this.previous||!Number.isFinite(p.at)||p.at<0) throw Error("Invalid Worker receipt");
      if(p.completedBlocks>this.previous) this.pending.set(p.completedBlocks,{at:p.at,frames:p.frames});
      this.previous=p.completedBlocks;
    }
    for(const c of controls) {
      if(!["report_consumed","heartbeat"].includes(c.kind))continue;
      if(![c.began,c.ended].every(Number.isFinite)||c.began<0||c.ended<c.began||c.kind==="report_consumed"&&!decimal(c.frames))throw Error("Invalid control observation");
      if(c.headers_ended!==undefined&&(!Number.isFinite(c.headers_ended)||c.headers_ended<c.began||c.headers_ended>c.ended))throw Error("Invalid response body observation");
      this.controls.push(c);
    }
    this.controls=this.controls.slice(-512);
    for(const s of supplies) {
      if(!Number.isSafeInteger(s.ordinal)||s.ordinal<1||!Number.isSafeInteger(s.frames)||s.frames<1||s.frames>4096||!decimal(s.start_sample)||!Number.isFinite(s.at)||s.at<0)throw Error("Invalid supply observation");
      this.supplies.set(s.ordinal,s);
    }
    for(const [k,p] of this.pending) {
      const c=this.controls.find(c=>c.kind==="report_consumed"&&decimal(c.frames)&&BigInt(c.frames)>=BigInt(p.frames)&&c.began>=p.at);
      const s=this.supplies.get(k+2);
      if(!c||!s)continue;
      if(s.at<c.began || s.at<p.at) throw Error("Invalid two-slot credit observation order");
      this.samples.push({release:c.began-p.at,refill:s.at-p.at,turnaround:s.at-c.began});this.pending.delete(k);
      const overlapping=this.controls.filter(x=>x!==c&&x.began<=c.began&&x.ended>=p.at);
      this.tails.push({completed_block:k,consumed_frames:p.frames,supplied_block:s.ordinal,supplied_start_sample:s.start_sample,supplied_frames:s.frames,
        receipt_at_ms:p.at,report_began_ms:c.began,report_headers_ended_ms:c.headers_ended??c.ended,report_ended_ms:c.ended,supplied_at_ms:s.at,
        completion_to_report_ms:c.began-p.at,completion_to_supply_ms:s.at-p.at,
        preceding_control_count:overlapping.length,preceding_controls:overlapping.slice(0,8).map(x=>({kind:x.kind,began_ms:x.began,headers_ended_ms:x.headers_ended??x.ended,ended_ms:x.ended}))});
      this.tails.sort((a,b)=>b.completion_to_supply_ms-a.completion_to_supply_ms);this.tails.length=Math.min(32,this.tails.length);
    }
    for(const [k]of this.supplies)if(k<this.previous-64)this.supplies.delete(k);
    if(this.pending.size>64||this.supplies.size>512||this.samples.length>4096)throw Error("Credit observation budget exceeded");
  }
  result() {
    const stats=key=>{const a=this.samples.map(s=>s[key]).sort((a,b)=>a-b);return a.length?{count:a.length,mean_ms:a.reduce((a,b)=>a+b,0)/a.length,maximum_ms:a.at(-1),p95_ms:a[Math.ceil(a.length*.95)-1]}:{count:0};};
    return {completion_to_report:stats("release"),completion_to_next_supply:stats("refill"),report_to_next_supply:stats("turnaround"),
      slowest_refills:this.tails,
      completions_observed:this.previous,unmatched_terminal_completions:this.pending.size,
      method:"Actual Worker MessagePort receipt entry, covering serialized report and transfer block k+2; all timestamps converted with performance.timeOrigin. Not native-only or DAC latency."};
  }
}
