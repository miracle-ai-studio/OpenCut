import {expect} from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import {correlateHotWindow, retainAcousticWindow, installAcousticDeliveryGate} from "./scene-preview-acoustic-diagnosis.mjs";
import {installScenePreviewWitness, trackScenePreviewPumps} from "./verify-v4-scene-preview.mjs";
import {importProductMedia, placeProductMedia} from "./verify-v4-editor.mjs";
import {assertTaggedStereo, measureTaggedStereo, acousticCaptureMatches, masteringWitnessProcessor, installMasteringWitness} from "./scene-preview-acoustics.mjs";

const toolFor = page => async (name, input) => {
  const reply = await page.evaluate(async ({name,input}) => (await fetch(`/v1/tools/${name}`, {
    method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(input),
  })).json(), {name,input});
  expect(reply.error).toBeNull(); return reply.data;
};
async function ready(page, endpoint, id) {
  await page.goto(`${endpoint}/editor${id ? `/${id}` : ""}`);
  if (id) await expect(page.getByRole("main",{name:"V4 editor",exact:true})).toBeVisible();
  else await expect(page.getByRole("button",{name:"New project",exact:true})).toBeEnabled();
  const guide=page.getByRole("dialog",{name:"Welcome to OpenCut",exact:true});
  if(await guide.isVisible()) await guide.getByRole("button",{name:"Close",exact:true}).click();
}
// One additional authored stereo file is necessary: the existing scene/dense
// fixtures only duplicate the same mono440 voice. The copied project's old
// assets retain their original versions/CAS. Persist the project id before any
// import so setup can resume without copying/reimporting an existing fixture.
async function fixtureFor(page, endpoint, directory, writes) {
  const tool=toolFor(page);
  const original=JSON.parse(await fs.readFile(path.join(directory,"scene-preview-fixture.json"),"utf8"));
  const inspect=id=>tool("opencut_project_v4_inspect",{project_id:id,revision:null});
  const source=await inspect(original.project_id);expect(source.revision).toBe(original.revision);
  const file=path.join(directory,"scene-preview-acoustics-fixture.json");
  let fixture=await fs.readFile(file,"utf8").then(JSON.parse).catch(e=>{if(e.code==="ENOENT")return null;throw e;});
  if(!fixture){
    const id="scene-preview-acoustics-"+crypto.randomUUID();
    const copied=await tool("opencut_project_v4_copy",{
      source_project_id:source.project.id,source_revision:source.revision,project_id:id,
      name:"Distinct voices, stereo and real preview mastering",idempotency_key:crypto.randomUUID(),
    });
    fixture={project_id:id,source_project_id:source.project.id,source_revision:source.revision,
      sequence_id:copied.project.default_sequence_id,revision:null};
    await fs.writeFile(file,JSON.stringify(fixture)+"\n",{flag:"wx"});
  }
  if(!fixture.revision){
    await ready(page,endpoint,fixture.project_id);
    let fixed=await inspect(fixture.project_id);
    const filename="preview-tagged-stereo.wav";
    if(!fixed.project.assets.some(a=>a.name===filename)){
      const frames=8*48000, bytes=Buffer.alloc(44+frames*8);
      bytes.write("RIFF",0);bytes.writeUInt32LE(bytes.length-8,4);bytes.write("WAVEfmt ",8);
      bytes.writeUInt32LE(16,16);bytes.writeUInt16LE(3,20);bytes.writeUInt16LE(2,22);
      bytes.writeUInt32LE(48000,24);bytes.writeUInt32LE(48000*8,28);
      bytes.writeUInt16LE(8,32);bytes.writeUInt16LE(32,34);bytes.write("data",36);bytes.writeUInt32LE(frames*8,40);
      for(let i=0;i<frames;i++){
        bytes.writeFloatLE(i<6*48000 ? .1*Math.sin(2*Math.PI*880*i/48000) : .5,44+i*8);
        bytes.writeFloatLE(i<6*48000 ? .05*Math.cos(2*Math.PI*660*i/48000+.37) : .25,48+i*8);
      }
      const wave=path.join(directory,filename);
      await fs.writeFile(wave,bytes,{flag:"wx"}).catch(e=>{if(e.code!=="EEXIST")throw e;});
      expect(await fs.readFile(wave)).toEqual(bytes);
      await importProductMedia(page,wave,writes);
      fixed=await inspect(fixture.project_id);
    }
    const asset=fixed.project.assets.find(a=>a.name===filename);
    let scene=fixed.project.sequences.find(s=>s.id===fixture.sequence_id);
    if(!scene.tracks.some(t=>t.elements.some(e=>e.content.asset_id===asset.id))){
      await placeProductMedia(page,filename,{track:"new",keepPitch:false,
        fields:{"Timeline start (seconds)":"0","Source start (seconds)":"0","Source duration (seconds)":"8"}},writes);
      fixed=await inspect(fixture.project_id);scene=fixed.project.sequences.find(s=>s.id===fixture.sequence_id);
    }
    const referenceTrack=scene.tracks.find(t=>t.elements.some(e=>e.content.asset_id===asset.id));
    const low=structuredClone(referenceTrack.elements.find(e=>e.content.asset_id===asset.id));
    low.name="Stereo reference";
    low.timeline_range={start:{ticks:"0",timebase:1},duration:{ticks:"6",timebase:1}};
    low.source_span={...low.source_span,trim_start:{ticks:"0",timebase:1},trim_end:{ticks:"2",timebase:1}};
    low.content.audio.gain={unit:"decibels",value:0};
    const hot=structuredClone(low);hot.id=crypto.randomUUID();hot.name="Linked hot DC";
    hot.timeline_range={start:{ticks:"6",timebase:1},duration:{ticks:"2",timebase:1}};
    hot.source_span={...hot.source_span,trim_start:{ticks:"6",timebase:1},trim_end:{ticks:"0",timebase:1}};
    hot.content.audio.gain={unit:"decibels",value:20*Math.log10(4)};
    const targetTrack=scene.tracks.find(t=>t.elements.some(e=>e.content.kind==="audio"&&e.content.asset_id!==asset.id));
    const target=structuredClone(targetTrack.elements.find(e=>e.content.kind==="audio"));
    target.timeline_range={...target.timeline_range,duration:{ticks:"6",timebase:1}};
    target.source_span={...target.source_span,trim_end:{ticks:"2",timebase:1}};
    const changed=await tool("opencut_edit_v4_apply",{
      project_id:fixture.project_id,expected_revision:fixed.revision,idempotency_key:crypto.randomUUID(),
      label:"Author distinct stereo and linked hot mastering intervals",
      operations:[{type:"track_put",sequence_id:scene.id,track:{...targetTrack,elements:[target]}},
        {type:"track_put",sequence_id:scene.id,track:{...referenceTrack,elements:[low,hot]}}],
    });
    fixture={...fixture,revision:changed.revision,target_track_id:targetTrack.id,target_element_id:target.id,
      target_name:target.name,stereo_asset_id:asset.id};
    await fs.writeFile(file,JSON.stringify(fixture)+"\n");
  }
  const fixed=await inspect(fixture.project_id);expect(fixed.revision).toBe(fixture.revision);
  for(const asset of source.project.assets)
    expect(fixed.project.assets.find(a=>a.id===asset.id)?.version).toEqual(asset.version);
  expect((await inspect(original.project_id)).revision).toBe(original.revision);
  return fixture;
}

export async function verifyV4ScenePreviewAcoustics(browser,endpoint,directory,check,{hotOnly=false,controlledGap=false}={}) {
  if(controlledGap && !hotOnly)throw Error("Controlled gap requires the retained hot fixture only");
  const page=await browser.newPage({locale:"en-US",viewport:{width:1440,height:1100}});
  const errors=[],writes=[],controls=[];
  page.on("pageerror",e=>{if(errors.length<8)errors.push(e.message);});
  page.on("request",r=>{
    if(/opencut_edit_v4_apply$/.test(r.url()))writes.push(r.postDataJSON());
    if(/opencut_audio_preview_v4_control$/.test(r.url())&&controls.length<2048)controls.push(r.postDataJSON());
  });
  await page.addInitScript(installScenePreviewWitness);
  await page.addInitScript(installMasteringWitness,{processorSource:`(${masteringWitnessProcessor.toString()})()`});
  const witness=trackScenePreviewPumps(page,{pcmWindows:[{id:"interior",start_sample:36000,frames:4096}],
    installDeliveryGate:controlledGap?installAcousticDeliveryGate:null});
  const tool=toolFor(page);
  const results=[];
  let failure=null;
  let observedSignal=null;
  const retained=[];
  try{
    await ready(page,endpoint);
    const fixture=await fixtureFor(page,endpoint,directory,writes);
    await ready(page,endpoint,fixture.project_id);writes.length=0;
    if(hotOnly){
      const field=page.getByLabel("Playhead (seconds)",{exact:true});await field.fill("6.25");await field.press("Enter");
    }
    await page.getByRole("button",{name:"Play scene preview",exact:true}).click();
    let original;
    const capture=async (name,db,hot=false)=>{
      let sampled,post;
      await expect.poll(async()=>{
        const matching=await page.evaluate(({db,hot})=>{
          const w=window.__scenePreviewWitness,s=w.session;
          return !!s && s.state==="playing" && (hot || (s.temporary_gain?.decibels_tenths??0)===db*10) &&
            !!w.progress?.token && BigInt(w.progress.consumedFrames)>0n;
        },{db,hot});
        if(!matching)return false;
        sampled=await witness();
        post=await page.evaluate(()=>{
          const token=window.__scenePreviewWitness.progress?.token;
          return window.__sceneMasteringWitness.records.find(r=>r.token===token)??null;
        });
        return sampled.selectedPcm?.length===1 && acousticCaptureMatches(sampled.session,sampled.progress,post,{db,hot});
      },{timeout:15000,intervals:[50,100,100]}).toBe(true);
      original??=sampled.session;
      expect(sampled.overflows).toBe(0);expect(sampled.pcmCapture.completed).toBe(1);
      expect(post.sample_rate).toBe(48000);expect(post.window.frames).toBe(4096);
      expect(Math.abs(post.output_gain-.98)).toBeLessThan(1e-6);
      expect(post.window.token).toBe(sampled.progress.token);
      expect(post.session.identity).toEqual(sampled.session.identity);
      const start=Number(sampled.session.actual_start_sample)/48000;
      expect(start+.75+4096/48000).toBeLessThan(hot?8:6);
      if(hot)expect(start).toBeGreaterThanOrEqual(6);
      observedSignal={name,actual_start_sample:sampled.session.actual_start_sample,
        pre_master:measureTaggedStereo(sampled.selectedPcm[0].values),post_master:measureTaggedStereo(post.window.values),
        post_context_first_nonzero_frame:post.window.first_nonzero_frame,post_context_window_start_frame:post.window.start_context_frame,
        post_ranges:[0,1].map(c=>{
          const values=post.window.values.filter((_,i)=>i%2===c);
          return {min:Math.min(...values),max:Math.max(...values),first:values[0],last:values.at(-1),zeros:values.filter(v=>v===0).length};
        })};
      let pre,mastered,correlation=null,acousticFailure=null;
      try{
        pre=assertTaggedStereo(sampled.selectedPcm[0].values,{db,hot});
        mastered=assertTaggedStereo(post.window.values,{db,hot,postMaster:true});
      }catch(error){
        acousticFailure=error.message;
        try{if(hot)correlation=correlateHotWindow(post);}catch(cause){correlation={error:cause.message};}
        retained.push(await retainAcousticWindow(directory,{pre:sampled.selectedPcm[0],post,
          session:post.session,correlation,gate:sampled.deliveryGate,failure:acousticFailure}));
        if(!controlledGap)throw error;
      }
      if(controlledGap){
        expect(acousticFailure).toBe("Unexpected acoustic residual");
        expect(pre).toHaveLength(2);
        expect(sampled.deliveryGate).toMatchObject({released:true,timed_out:false,cancelled:false,zero_quanta:30});
        expect(sampled.deliveryGate.max_held).toBeLessThanOrEqual(2);
        expect(correlation.error).toBeUndefined();
        expect(correlation.missing_frames).toBeGreaterThan(0);
        expect(correlation.post_zero_frames).toBe(correlation.missing_frames);
        expect(correlation.zero_mask_mismatches).toBe(0);
        mastered=measureTaggedStereo(post.window.values);
      }
      const frames=sampled.frames.filter(f=>f.generation===sampled.session.generation);
      expect(frames.length).toBeGreaterThanOrEqual(4);
      for(const f of frames){
        const expected=Math.floor(f.time)%2?[0,255,0,255]:[255,0,0,255];
        f.color.forEach((v,i)=>expect(Math.abs(v-expected[i])).toBeLessThanOrEqual(4));
        expect(f.actual-f.time).toBeGreaterThanOrEqual(-.01);expect(f.actual-f.time).toBeLessThanOrEqual(.15);
      }
      const result={name,generation:sampled.session.generation,actual_start_sample:sampled.session.actual_start_sample,
        pre_master:pre,post_master:mastered,frames_per_point:4096,
        controlled_gap:controlledGap,expected_acoustic_failure:acousticFailure,correlation,
        post_context_first_nonzero_frame:post.window.first_nonzero_frame,
        post_context_window_start_frame:post.window.start_context_frame,
        pixel_frames:frames.length,max_skew_ms:Math.max(...frames.map(f=>(f.actual-f.time)*1000))};
      results.push(result);
      check(controlledGap?"controlled delivery gap: failed post-master PCM retained and matched to actual missing quanta":
        `actual stereo/${name}: independent voices and PCM at native supply and real mastering output`,{scene_preview_acoustics:result});
    };
    if(!hotOnly){
      await capture("baseline",0);
      const slider=page.getByRole("slider",{name:`Volume ${fixture.target_name}`,exact:true});
      const surface=page.locator(".v4-timeline-element").filter({has:slider}).locator(".v4-gain-surface");
      await slider.scrollIntoViewIfNeeded();const line=await slider.boundingBox();expect(line.height).toBe(14);
      await page.mouse.move(line.x+line.width/2,line.y+line.height/2);await page.mouse.down();
      const box=await surface.boundingBox();const percent=1-Math.sqrt((10**(-12/20)-.001)/9.999);
      await page.mouse.move(box.x+box.width/2,box.y+box.height*percent);
      await expect(slider).toHaveAttribute("aria-valuenow","-12");await capture("target_gain",-12);
      await page.keyboard.press("Escape");await page.mouse.up();
      await expect(slider).toHaveAttribute("aria-valuenow","0");await capture("restored",0);
      const field=page.getByLabel("Playhead (seconds)",{exact:true});await field.fill("6.25");await field.press("Enter");
    }
    await capture("linked_hot_mastering",0,true);
    await page.getByRole("button",{name:"Pause scene audio",exact:true}).click();
    await expect(page.getByText("Scene sound paused",{exact:true})).toBeVisible();
    const final=await tool("opencut_audio_preview_v4_inspect",{session_id:original.session_id});
    expect(final.state).toBe("paused");expect(final.temporary_gain).toBeNull();
    expect(final.owner_id).toBe(original.owner_id);expect(final.lease_id).toBe(original.lease_id);
    expect(final.identity).toEqual(original.identity);
    const commands=controls.filter(r=>!["heartbeat","report_consumed"].includes(r.command.kind));
    expect(commands.map(r=>r.command.kind)).toEqual(hotOnly?["play","pause"]:["play","set_temporary_gain","clear_temporary_gain","seek","pause"]);
    const gain=commands.find(r=>r.command.kind==="set_temporary_gain");
    if(gain)expect(gain.command.gain).toMatchObject({track_id:fixture.target_track_id,element_id:fixture.target_element_id,decibels_tenths:-120});
    expect((await tool("opencut_project_v4_inspect",{project_id:fixture.project_id,revision:null})).revision).toBe(fixture.revision);
    expect(writes).toEqual([]);expect(errors).toEqual([]);
    expect(await page.evaluate(()=>window.__sceneMasteringWitness.overflows)).toBe(0);
    expect((await witness()).deviceStates.every(s=>s==="closed")).toBe(true);
    const cdp=await browser.newBrowserCDPSession();
    try{expect((await cdp.send("Target.getTargets")).targetInfos.filter(t=>t.type==="worker"&&/pump-worker/.test(t.url))).toHaveLength(0);}
    finally{await cdp.detach();}
    check("actual stereo preview closes its device/Worker and preserves original authority and head",{
      scene_preview_acoustics_summary:{project_id:fixture.project_id,revision:fixture.revision,
        cases:results.map(r=>r.name),commands:commands.map(r=>r.command.kind),device_closed:true,worker_targets:0,
        physical_dac_verified:false,phase_boundary_history_verified:false,performance_matrix_repeated:false},
    });
  }catch(error){
    // Even a polling/identity assertion failure must preserve the actual raw
    // window if it arrived. Do not lose it because the short stream has ended.
    if(!retained.length){
      const raw=await witness().catch(()=>null);
      const post=await page.evaluate(()=>window.__sceneMasteringWitness?.records.find(r=>r.window)??null).catch(()=>null);
      if(raw?.selectedPcm?.length===1 && post?.window){
        let correlation;
        try{correlation=hotOnly?correlateHotWindow(post):null;}catch(cause){correlation={error:cause.message};}
        retained.push(await retainAcousticWindow(directory,{pre:raw.selectedPcm[0],post,session:post.session,
          correlation,gate:raw.deliveryGate,failure:error.message}));
      }
    }
    failure=await page.evaluate(()=>{
      const w=window.__scenePreviewWitness,s=w?.session;
      return {session_state:s?.state??null,generation:s?.generation??null,progress:w?.progress??null,
        mastering:window.__sceneMasteringWitness?.records.map(r=>({token:r.token,sample_rate:r.sample_rate,
          window_received:!!r.window}))??null,
        player_text:document.querySelector('.v4-audio-preview')?.textContent.slice(0,1024)??null,
        alerts:Array.from(document.querySelectorAll('[role="alert"]')).slice(0,8).map(e=>e.textContent.slice(0,512))};
    }).catch(()=>({page_closed:true}));
    throw error;
  }finally{
    await fs.writeFile(path.join(directory,controlledGap?"scene-preview-acoustics-gap-evidence.json":hotOnly?"scene-preview-acoustics-hot-evidence.json":"scene-preview-acoustics-evidence.json"),JSON.stringify({cases:results,errors,failure,retained_windows:retained,failed_signal:failure?observedSignal:null},null,2));
    await page.close();
  }
}
