import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { installScenePreviewWitness, trackScenePreviewPumps } from "./verify-v4-scene-preview.mjs";
import { PreviewContinuity, PreviewCreditObservations } from "./scene-preview-continuity.mjs";

const budget=Object.freeze({seconds:300,window_ms:10000,deficit_ratio:.01,skew_ms:150,
  useful_pts_per_placement:144,page_heap_delta:64*1024*1024,worker_heap:64*1024*1024,
  native_tree_rss:2*1024*1024*1024,gpu:256*1024*1024,cold_ms:10000,deadline_ms:340000});
function processes() {
  return execFileSync("ps",["-axo","pid=,ppid=,rss=,lstart=,comm="],{encoding:"utf8",maxBuffer:8*1024*1024}).trim().split("\n").map(line=>{
    const p=line.trim().split(/\s+/);return {pid:Number(p[0]),ppid:Number(p[1]),rss:Number(p[2])*1024,start:p.slice(3,8).join(" "),
      role:/Chrome/.test(p.slice(8).join(" "))?"chromium":/node/.test(p.slice(8).join(" "))?"node":"host"};
  });
}
function nativeTree(root, rows) {
  const ids=new Set([root]);let changed=true;
  while(changed){changed=false;for(const r of rows)if(ids.has(r.ppid)&&!ids.has(r.pid)){ids.add(r.pid);changed=true;}}
  const selected=rows.filter(r=>ids.has(r.pid));if(!selected.some(r=>r.pid===root))throw Error("Owned native service is not live");
  return selected;
}
// Playwright cannot make a page CDP session for a Worker. Use the documented
// browser Target transport to query this actual dedicated target's V8 heap.
async function workerHeap(browser) {
  const cdp=await browser.newBrowserCDPSession();let session=null,id=0;const pending=new Map();
  cdp.on("Target.receivedMessageFromTarget",e=>{
    if(e.sessionId!==session)return;const r=JSON.parse(e.message),p=pending.get(r.id);if(!p)return;
    pending.delete(r.id);clearTimeout(p.timer);r.error?p.reject(Error(r.error.message)):p.resolve(r.result);
  });
  return {async sample(){
    const {targetInfos}=await cdp.send("Target.getTargets");
    const targets=targetInfos.filter(t=>t.type==="worker"&&/pump-worker/.test(t.url));
    if(!targets.length)return null;
    if(targets.length!==1)throw Error("Expected exactly one owned PCM Worker");
    if(!session)session=(await cdp.send("Target.attachToTarget",{targetId:targets[0].targetId,flatten:false})).sessionId;
    const key=++id;
    return new Promise((resolve,reject)=>{
      const timer=setTimeout(()=>{pending.delete(key);reject(Error("Worker heap observation timed out"));},3000);
      pending.set(key,{resolve,reject,timer});
      void cdp.send("Target.sendMessageToTarget",{sessionId:session,message:JSON.stringify({id:key,method:"Runtime.getHeapUsage"})}).catch(error=>{clearTimeout(timer);pending.delete(key);reject(error);});
    });
  },async close(){for(const p of pending.values()){clearTimeout(p.timer);p.reject(Error("Worker observation closed"));}pending.clear();await cdp.detach();}};
}

export async function verifyV4ScenePreviewContinuity(browser,endpoint,directory,check) {
  const fixture=JSON.parse(await fs.readFile(path.join(directory,"scene-preview-dense-fixture.json"),"utf8"));
  const state=JSON.parse(await fs.readFile(path.join(directory,"service/state.json"),"utf8"));
  const page=await browser.newPage({locale:"en-US",viewport:{width:1440,height:1100}});
  page.setDefaultTimeout(30000);await page.addInitScript(installScenePreviewWitness,{deferReadback:true,rolling:true,sampleSchedule:{starts:Array.from({length:20},(_,k)=>292*k/19),duration:8}});
  const witness=trackScenePreviewPumps(page,{rolling:true}), errors=[],writes=[];
  page.on("pageerror",e=>errors.push(e.message));
  page.on("request",r=>{if(r.url().endsWith("opencut_edit_v4_apply"))writes.push(r.postDataJSON());});
  const tool=async(name,input)=>{
    const r=await page.evaluate(async({name,input})=>(await fetch(`/v1/tools/${name}`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(input)})).json(),{name,input});
    expect(r.error).toBeNull();return r.data;
  };
  const cdp=await page.context().newCDPSession(page);await cdp.send("Performance.enable");
  const metrics=async()=>Object.fromEntries((await cdp.send("Performance.getMetrics")).metrics.map(m=>[m.name,m.value]));
  const worker=await workerHeap(browser), account=new PreviewContinuity(48000,"14400000",budget.window_ms),credit=new PreviewCreditObservations();
  const pts=Array.from({length:20},()=>new Set()),pixels=[],ownedChildren=new Map(),pageHeaps=[],workerHeaps=[],nativeSamples=[];
  let evidence={budget},last,initial,maximumSkew=-Infinity,minimumSkew=Infinity,maximumGpu=0,maximumAccounted=0;
  try {
    // This suite requires the prior retained fixture; it cannot silently import,
    // copy or seed another project to make sustained acceptance easier.
    await page.goto(`${endpoint}/editor`);await expect(page.getByRole("button",{name:"New project",exact:true})).toBeEnabled();
    const fixed=await tool("opencut_project_v4_inspect",{project_id:fixture.project_id,revision:null});
    const sequence=fixed.project.sequences.find(s=>s.id==="main");expect(sequence.tracks).toHaveLength(5);
    for(const [lane,t]of sequence.tracks.entries()){
      expect(t.kind).toBe(lane<2?"video":"audio");expect(t.elements).toHaveLength(20);
      for(const [k,e]of t.elements.entries()){
        expect(e.timeline_range).toEqual({start:{ticks:String(292*k),timebase:19},duration:{ticks:"8",timebase:1}});
        expect(e.content.retime.rate).toEqual({numerator:1,denominator:1});expect(e.content.retime.maintain_pitch).toBe(false);
        expect(e.content.audio.gain).toEqual({unit:"decibels",value:-18});if(lane<2)expect(e.content.source_audio_enabled).toBe(false);
      }
    }
    await cdp.send("HeapProfiler.collectGarbage");const before=await metrics();
    await page.goto(`${endpoint}/editor/${fixture.project_id}`);await expect(page.locator(".v4-timeline-element")).toHaveCount(100);
    const guide=page.getByRole("dialog",{name:"Welcome to OpenCut",exact:true});if(await guide.isVisible())await guide.getByRole("button",{name:"Close",exact:true}).click();
    const began=Date.now();await page.getByRole("button",{name:"Play scene preview",exact:true}).click();
    const warm=await metrics(),deadline=Date.now()+budget.deadline_ms;
    let nextNative=0;
    while(!last?.endedSession || !account.terminal){
      if(Date.now()>deadline)throw Error("Real300s scene did not reach terminal consumed cursor");
      last=await witness();expect(last.overflows).toBe(0);expect(last.workerOverflows??0).toBe(0);
      if(last.firstSoundAt&&!initial)initial={cold_ms:last.firstSoundAt-began,pcm:last.pcm,identity:last.session.identity};
      account.add(last.progresses);credit.add(last.workerProgresses??[],last.controls,last.supplies);
      for(const f of last.frames){
        const skew=(f.actual-f.time)*1000;maximumSkew=Math.max(maximumSkew,skew);minimumSkew=Math.min(minimumSkew,skew);
        const r=JSON.parse(f.runtime);maximumGpu=Math.max(maximumGpu,Number(r.peakGpuBytes));maximumAccounted=Math.max(maximumAccounted,Number(r.peakAccountedBytes));
        // Independent authored placement schedule, never the service frame plan.
        const k=Math.min(19,Math.floor((f.time+1e-9)*19/292)),local=f.time-292*k/19;
        if(local>=0&&local<8)for(const p of f.pts)pts[k].add(p);
        if(f.color){
          // Witness selected the independently authored interval interior.
          const expected=local>=0&&local<8?(Math.floor(local)%2?[0,255,0,255]:[255,0,0,255]):[0,0,0,255];
          f.color.forEach((v,c)=>expect(Math.abs(v-expected[c])).toBeLessThanOrEqual(4));
          pixels.push({placement:k,interval:local<8?Math.floor(local):"gap",time:f.time});
        }
      }
      const m=await metrics();pageHeaps.push(m.JSHeapUsedSize);
      const h=await worker.sample();if(h)workerHeaps.push(h);
      if(Date.now()>=nextNative){
        const rows=nativeTree(state.pid,processes());nativeSamples.push({rss:rows.reduce((n,r)=>n+r.rss,0),count:rows.length});
        for(const r of rows)if(r.pid!==state.pid)ownedChildren.set(r.pid,r.start);
        nextNative=Date.now()+5000;
      }
      await page.waitForTimeout(1000);
    }
    const end=await metrics(),consumption=account.result();
    await expect(page.getByText("Scene sound ended",{exact:true})).toBeVisible();
    await expect.poll(async()=>(await witness()).deviceStates,{timeout:5000}).toEqual(["closed"]);
    await expect.poll(async()=>await worker.sample(),{timeout:5000}).toBeNull();
    await expect.poll(()=>processes().filter(r=>ownedChildren.get(r.pid)===r.start).length,{timeout:10000}).toBe(0);
    const head=await tool("opencut_project_v4_inspect",{project_id:fixture.project_id,revision:null});
    expect(head.revision).toBe(fixed.revision);expect(writes).toHaveLength(0);expect(errors).toEqual([]);
    expect(last.endedSession.consumed_frames).toBe("14400000");expect(last.endedSession.total_frames).toBe("14400000");
    expect(last.endedSession.authoritative_time).toEqual({ticks:"14400000",timebase:48000});
    const sample=initial.pcm,crossings=[];for(let i=1;i<sample.length;i++)if(sample[i-1]<=0&&sample[i]>0)crossings.push(i-1-sample[i-1]/(sample[i]-sample[i-1]));
    const frequency=(crossings.length-1)*48000/(crossings.at(-1)-crossings[0]),rms=Math.sqrt(sample.reduce((n,v)=>n+v*v,0)/sample.length);
    evidence={budget,fixture:{project_id:fixture.project_id,revision:fixed.revision,seconds:300,source_seconds:8,placements:100,tracks:5},
      cold_ms:initial.cold_ms,consumption,source_pts_per_placement:pts.map(s=>s.size),max_skew_ms:maximumSkew,minimum_skew_ms:minimumSkew,
      independent_pixels:{observations:pixels.length,intervals:Array.from({length:20},(_,k)=>[...new Set(pixels.filter(p=>p.placement===k).map(p=>p.interval))])},
      independent_pcm:{frequency_hz:frequency,rms,expected_rms:.125/Math.sqrt(2)*3*10**(-18/20)},credit:credit.result(),
      resources:{page:{baseline:before.JSHeapUsedSize,peak:Math.max(...pageHeaps),delta:Math.max(...pageHeaps)-before.JSHeapUsedSize},
        worker:{samples:workerHeaps.length,peak_used:Math.max(...workerHeaps.map(h=>h.usedSize)),peak_backing:Math.max(...workerHeaps.map(h=>h.backingStorageSize??0)),method:"Actual dedicated Worker CDP Runtime.getHeapUsage; no steady-window GC"},
        native:{samples:nativeSamples.length,peak_tree_rss:Math.max(...nativeSamples.map(s=>s.rss)),peak_processes:Math.max(...nativeSamples.map(s=>s.count)),owned_children_after_end:0,method:"ps RSS of live owned Rust service and descendants, captured PID/start identity; separate from verification-browser RSS, not a kernel quota"},
        peak_gpu_accounted:maximumGpu,peak_execution_accounted:maximumAccounted,device_closed:true,worker_target_closed:true},
      main_thread:{task_ms:(end.TaskDuration-warm.TaskDuration)*1000,script_ms:(end.ScriptDuration-warm.ScriptDuration)*1000},readonly:true,errors};
    const gates={};const gate=(name,test)=>{try{test();gates[name]=true;}catch{gates[name]=false;}};
    gate("complete_300s",()=>{expect(consumption.complete).toBe(true);expect(consumption.copied_frames).toBe(14400000);expect(consumption.elapsed_ms).toBeGreaterThanOrEqual(299000);});
    gate("cold",()=>expect(initial.cold_ms).toBeLessThanOrEqual(budget.cold_ms));
    for(const metric of ["stalled_ratio","deficit_ratio","render_quantum_deficit_ratio"])gate(metric,()=>{expect(consumption[metric]).toBeLessThanOrEqual(budget.deficit_ratio);for(const w of consumption.windows)expect(w[metric]).toBeLessThanOrEqual(budget.deficit_ratio);});
    gate("source_pts",()=>pts.forEach(s=>expect(s.size).toBeGreaterThanOrEqual(budget.useful_pts_per_placement)));
    gate("synchronization",()=>{expect(maximumSkew).toBeLessThanOrEqual(budget.skew_ms);expect(minimumSkew).toBeGreaterThanOrEqual(-10);});
    gate("independent_pixels",()=>{for(let k=0;k<20;k++)for(let n=0;n<8;n++)expect(pixels.some(p=>p.placement===k&&p.interval===n)).toBe(true);for(let k=0;k<19;k++)expect(pixels.some(p=>p.placement===k&&p.interval==="gap")).toBe(true);});
    gate("independent_pcm",()=>{expect(Math.abs(frequency-440)).toBeLessThan(1);expect(Math.abs(rms-evidence.independent_pcm.expected_rms)).toBeLessThan(.001);});
    gate("page_heap",()=>expect(evidence.resources.page.delta).toBeLessThanOrEqual(budget.page_heap_delta));
    gate("worker_heap",()=>{expect(workerHeaps.length).toBeGreaterThan(250);expect(evidence.resources.worker.peak_used+evidence.resources.worker.peak_backing).toBeLessThanOrEqual(budget.worker_heap);});
    gate("native_rss",()=>{expect(nativeSamples.length).toBeGreaterThan(50);expect(evidence.resources.native.peak_tree_rss).toBeLessThanOrEqual(budget.native_tree_rss);});
    gate("gpu",()=>{expect(maximumGpu).toBeGreaterThan(0);expect(maximumGpu).toBeLessThanOrEqual(budget.gpu);});
    gate("worker_credit",()=>{expect(evidence.credit.completion_to_report.count).toBeGreaterThanOrEqual(3400);expect(evidence.credit.unmatched_terminal_completions).toBeLessThanOrEqual(2);});
    evidence.gates=gates;const failed=Object.entries(gates).filter(([,ok])=>!ok).map(([k])=>k);if(failed.length)throw Error(`Sustained scene gates unmet: ${failed.join(", ")}`);
    check("300s continuous dense scene: complete device quantum coverage, every source interval/gap, actual Worker credit/resources and terminal cleanup",{scene_preview_continuity:evidence});
  }catch(error){
    await fs.writeFile(path.join(directory,"scene-preview-continuity-failure.json"),JSON.stringify({error:String(error),evidence,
      progress:last?.progress,terminal:last?.endedSession,windows:account.windows,alerts:await page.getByRole("alert").allTextContents()},null,2));throw error;
  }finally{await worker.close();await page.close();}
}
