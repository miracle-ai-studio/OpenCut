import { editorSnapshot } from "./project-inspection.mjs";
/** F33 one retained-source scene: objects, enable/stack, history, preview/output. */
import {chromium,expect} from '@playwright/test';
import fs from 'node:fs/promises';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {OpenCutClient} from '../../../packages/sdk-ts/dist/index.js';
import {compositeClassicRgba8} from './blend-oracle.mjs';
const args=Object.fromEntries(process.argv.slice(2).reduce((a,v,i,s)=>i%2?a:[...a,[v.slice(2),s[i+1]]],[]));
const {workspace,endpoint,report}=args,client=new OpenCutClient({baseUrl:endpoint,token:(await fs.readFile(args.credential,'utf8')).trim()});
const checkpoint=path.join(workspace,'agent-object-blur-v4-checkpoint.json'),sha=b=>createHash('sha256').update(b).digest('hex'),time=n=>({ticks:String(Math.round(n*120000)),timebase:120000});
let state;try{state=JSON.parse(await fs.readFile(checkpoint,'utf8'))}catch(e){if(e.code!=='ENOENT')throw e;state={stages:{},requests:{}}}
const save=async()=>{await fs.writeFile(checkpoint+'.pending',JSON.stringify(state,null,2)+'\n');await fs.rename(checkpoint+'.pending',checkpoint)};
const stage=async(name,action)=>{if(state.stages[name])return state.stages[name];console.log('Begin '+name);const value=await action();state.stages[name]=value;await save();console.log('Passed '+name);return value};
const call=(name,input)=>client.call(name,input);
const request=async(name,tool,input)=>{if(state.requests[name])expect(input).toEqual(state.requests[name].input);else{state.requests[name]={tool,input};await save()}return call(tool,input)};
const inspect=(project_id,revision=null)=>editorSnapshot(call('opencut_project_inspect',{project_id,revision}));
const project='agent-object-blur-v4-acceptance',key=s=>'agent-object-blur-v4:'+s;
const source=JSON.parse(await fs.readFile(args.source,'utf8')),sourceProject=source.project_id,sourceRevision=source.revision;
const centers=[[48,45],[160,45],[272,45],[96,135],[224,135]],kinds=['video','image','sticker','graphic','text'],color=(n,a=1)=>({encoding:'srgb',red:n/255,green:n/255,blue:n/255,alpha:a});
const baseline=await stage('prepare-one-scene',async()=>{
 const old=await inspect(sourceProject,sourceRevision);state.source_head_before=await inspect(sourceProject);await save();
 await request('copy','opencut_project_copy',{source_project_id:sourceProject,source_revision:sourceRevision,project_id:project,name:'F33 combined Classic object blur',idempotency_key:key('copy')});
 const s=await inspect(project),q=structuredClone(s.project.sequences[0]),elements=old.project.sequences[0].tracks.flatMap(t=>t.elements),bases={};
 for(const k of ['video','image','graphic','text'])bases[k]=elements.find(e=>e.content.kind===k);for(const x of Object.values(bases))expect(x).toBeTruthy();
 const removeCaptions=q.captions.map(c=>({type:'caption_delete',sequence_id:q.id,caption_id:c.id}));
 q.captions=[];q.transitions=[];q.tracks=[];
 for(const [index,kind]of kinds.entries()){
  const track={id:kind==='video'?q.main_track_id:'f33-'+kind+'-track',name:'F33 '+kind,kind:kind==='text'?'text':kind==='graphic'?'graphic':'video',enabled:true,muted:true,locked:false,opacity:1,gain:{unit:'decibels',value:0},elements:[],channels:[]};
  for(let phase=0;phase<3;phase++){
   const e=structuredClone(bases[kind==='sticker'?'image':kind]);e.id=`f33-${kind}-${phase}`;e.name=`F33 ${kind} ${phase}`;e.channels=[];e.timeline_range={start:time(phase*.5),duration:time(.5)};
   e.source_span={profile:'classic_trimmed',trim_start:time(0),trim_end:time(0),source_duration:null};
   if(kind==='video'){e.content.source_audio_enabled=false;e.content.audio.muted=true;e.content.retime={rate:{numerator:1,denominator:1},maintain_pitch:false,mapping:{kind:'classic_rounded_ticks',timebase:120000}}}
   if(kind==='sticker')e.content={kind:'sticker',asset_id:bases.image.content.asset_id,intrinsic_size:{width:320,height:180}};
   if(kind==='graphic'){e.content.graphic.source_size={width:320,height:180};e.content.graphic.geometry={kind:'rectangle',corner_radius_percent:0};e.content.graphic.fill=color(224,.5);e.content.graphic.stroke.width_px=0}
   if(kind==='text'){const t=e.content.text;t.content='H';t.font_size={unit:'canvas_pixels',value:48};t.color=color(255);t.background.enabled=false;t.letter_spacing_px=0;t.decoration='none';t.line_height_multiplier=1.2}
   const effects=[{id:`blur-${kind}-${phase}`,enabled:phase!==1,effect:{kind:'classic_blur',intensity:phase===0&&kind==='graphic'?45:100}}];
   if(phase===2)effects.push({id:'second-'+kind,enabled:true,effect:{kind:'classic_blur',intensity:100}},{id:'disabled-'+kind,enabled:false,effect:{kind:'classic_blur',intensity:100}},{id:'zero-'+kind,enabled:true,effect:{kind:'classic_blur',intensity:0}});
   const [x,y]=centers[index];e.visual={profile:'classic_canvas_v31',opacity:1,blend_mode:'normal',masks:[],effects,transform:{position_x_px:x-160,position_y_px:y-90,scale_x:kind==='text'?1:(kind==='sticker'?-1:1)*64/320,scale_y:kind==='text'?1:48/180,rotation_degrees:0}};track.elements.push(e);
  }q.tracks.push(track);
 }
 const effectBase=elements.find(e=>e.content.kind==='effect_layer');expect(effectBase).toBeTruthy();const adjustment=structuredClone(effectBase);adjustment.id='f33-adjustment';adjustment.name='F33 canvas adjustment';adjustment.channels=[];adjustment.timeline_range={start:time(1),duration:time(.5)};adjustment.content.effect={id:'canvas-default',enabled:true,effect:{kind:'classic_blur',intensity:15}};
 q.tracks.push({id:'f33-adjustment-track',name:'F33 canvas effect',kind:'effect',enabled:true,muted:false,locked:false,opacity:1,gain:{unit:'decibels',value:0},elements:[adjustment],channels:[]});
 const settings=structuredClone(s.project.settings);settings.settings.background={kind:'color',color:color(32)};
 await request('setup-annotations-v1','opencut_edit_apply',{project_id:project,expected_revision:s.revision,idempotency_key:key('setup-annotations-v1'),label:'F33 existing-source object/enable/stack scene',operations:[...removeCaptions,{type:'settings_set',settings},{type:'sequence_put',sequence:q}]});return inspect(project);
});
const browser=await chromium.launch({headless:true,executablePath:process.env.OPENCUT_CHROME}),page=await browser.newPage({locale:'en-US',viewport:{width:1440,height:1100}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
try{
 const open=async()=>{await page.goto(`${endpoint}/editor/${project}`);await expect(page.getByRole('main',{name:'OpenCut editor',exact:true})).toBeVisible();const guide=page.getByRole('dialog',{name:'Welcome to OpenCut',exact:true});if(await guide.isVisible())await guide.getByRole('button',{name:'Close',exact:true}).click()};
 const uiAction=async(name,tool,click)=>stage(name,async()=>{
  if(state.requests[name]){const receipt=await call(tool,state.requests[name].input),s=await inspect(project);expect(s.revision).toBe(receipt.transaction?.revision??receipt.revision);return s}
  let receipt;await page.route('**/api/tools/'+tool,async route=>{const input=route.request().postDataJSON();state.requests[name]={tool,input};await save();const response=await route.fetch(),body=await response.json();expect(body.error).toBeNull();receipt=body.data;await route.fulfill({response})});
  try{await click();await expect.poll(()=>!!receipt).toBe(true);const revision=receipt.transaction?.revision??receipt.revision;await expect.poll(async()=>(await inspect(project)).revision).toBe(revision);return inspect(project)}finally{await page.unroute('**/api/tools/'+tool)}
 });
 await open();
 const selectGraphic=async()=>{await page.locator('[data-element-id="f33-graphic-0"] .editor-element-select').click();const panel=page.getByRole('region',{name:'Visual stack',exact:true});await panel.getByRole('button',{name:'Edit visual stack',exact:true}).click();return panel.getByRole('form',{name:'Edit visual stack',exact:true})};
 const edited=await uiAction('actual-ui-blur-save','opencut_edit_apply',async()=>{const form=await selectGraphic();await form.getByLabel('Blur intensity',{exact:true}).fill('100');await form.getByRole('button',{name:'Save stack',exact:true}).click()});
 await stage('actual-ui-reopen',async()=>{await open();const form=await selectGraphic();await expect(form.getByLabel('Blur intensity',{exact:true})).toHaveValue('100');return {passed:true}});
 await open();const undone=await uiAction('actual-ui-undo','opencut_edit_undo_apply',async()=>{await page.getByRole('region',{name:'History',exact:true}).getByRole('button',{name:'Undo',exact:true}).click();await page.getByRole('region',{name:'History plan',exact:true}).getByRole('button',{name:'Apply undo',exact:true}).click()});expect(undone.project).toEqual(baseline.project);
 await open();const fixed=await uiAction('actual-ui-redo','opencut_edit_redo_apply',async()=>{await page.getByRole('region',{name:'History',exact:true}).getByRole('button',{name:'Redo',exact:true}).click();await page.getByRole('region',{name:'History plan',exact:true}).getByRole('button',{name:'Apply redo',exact:true}).click()});expect(fixed.project).toEqual(edited.project);
 // Independent authored source rasters and public font/source bytes, then
 // numeric full-texture Gaussian with Classic object-size sigma. No product
 // plan, shader, rasterizer or observed pixels supplies an expected value.
 const q=fixed.project.sequences[0],first=kind=>q.tracks.flatMap(t=>t.elements).find(e=>e.id===`f33-${kind}-0`),font=fixed.project.fonts.find(f=>f.id===first('text').content.text.font.font_id),asset=kind=>fixed.project.assets.find(a=>a.id===first(kind).content.asset_id);
 const cas=hash=>path.join(workspace,'asset-store',hash.slice(0,2),hash),fontBytes=await fs.readFile(cas(font.content.content_hash));expect(sha(fontBytes)).toBe(font.content.content_hash);
 const raw=kind=>execFileSync('ffmpeg',['-v','error','-i',cas(asset(kind).version.content_hash),'-frames:v','1','-f','rawvideo','-pix_fmt','rgba','pipe:1'],{maxBuffer:32*1024*1024});
 const video=raw('video'),image=raw('image');expect(video.length).toBe(1280*720*4);expect(image.length).toBe(320*180*4);
 const layers=await page.evaluate(async({video,image,font,centers})=>{
  const face=new FontFace('F33Independent',Uint8Array.from(atob(font),c=>c.charCodeAt(0)),{weight:'400',style:'normal'});await face.load();document.fonts.add(face);
  const source=(raw,w,h)=>{const c=new OffscreenCanvas(w,h);c.getContext('2d').putImageData(new ImageData(Uint8ClampedArray.from(raw),w,h),0,0);return c},v=source(video,1280,720),im=source(image,320,180),result=[];
  for(let index=0;index<5;index++){
   const c=new OffscreenCanvas(320,180),ctx=c.getContext('2d'),[x,y]=centers[index];
   if(index<3){ctx.save();ctx.translate(x,y);if(index===2)ctx.scale(-1,1);ctx.drawImage(index===0?v:im,-32,-24,64,48);ctx.restore()}
   else if(index===3){ctx.fillStyle='rgba(224,224,224,0.5)';ctx.fillRect(x-32,y-24,64,48)}
   else{ctx.font='normal 400 48px F33Independent';ctx.textAlign='center';ctx.textBaseline='middle';ctx.letterSpacing='0px';ctx.fillStyle='white';ctx.fillText('H',x,y)}
   result.push(Array.from(ctx.getImageData(0,0,320,180).data));
  }document.fonts.delete(face);return result;
 },{video:Array.from(video),image:Array.from(image),font:fontBytes.toString('base64'),centers});
 const gaussian=(raw,intensity,w,h)=>{
  let input=Uint8Array.from(raw);for(const [axis,sigma]of [[0,intensity/5*w/1920],[1,intensity/5*h/1080]]){
   if(sigma<.001)continue;expect(sigma).toBeLessThanOrEqual(10);const weights=Array.from({length:61},(_,i)=>Math.exp(-((i-30)**2)/(2*sigma*sigma))),sum=weights.reduce((a,b)=>a+b,0),out=new Uint8Array(input.length);
   for(let y=0;y<180;y++)for(let x=0;x<320;x++)for(let c=0;c<4;c++){let value=0;for(let i=0;i<61;i++){const xx=Math.min(319,Math.max(0,x+(axis===0?i-30:0))),yy=Math.min(179,Math.max(0,y+(axis===1?i-30:0)));value+=input[(yy*320+xx)*4+c]*weights[i]}out[(y*320+x)*4+c]=Math.round(value/sum)}input=out;
  }return input;
 };
 const reference=(phase,wrongCanvas=false)=>{
  let scene=new Uint8Array(320*180*4);for(let p=0;p<scene.length;p+=4)scene.set([32,32,32,255],p);
  for(let i=0;i<5;i++){let layer=Uint8Array.from(layers[i]);if(phase!==1)for(let n=0;n<(phase===2?2:1);n++)layer=gaussian(layer,100,wrongCanvas||i===4?320:64,wrongCanvas||i===4?180:48);scene=compositeClassicRgba8(scene,layer,'normal')}
  return phase===2?gaussian(scene,15,320,180):scene;
 };
 const references=[reference(0),reference(1),reference(2)],wrong=reference(0,true);
 const artifact=async(job,name)=>{expect(job.state).toBe('succeeded');const response=await client.downloadArtifact(job.artifacts[0]),bytes=Buffer.from(await response.arrayBuffer());expect(sha(bytes)).toBe(job.artifacts[0].content_hash);const file=path.join(workspace,`f33-${job.job_id}.${name}`);await fs.writeFile(file,bytes);return file};
 const submit=async(name,tool,input)=>stage(name,async()=>{const accepted=await request(name,tool,input);let job=await call('opencut_job_view',{job_id:accepted.job_id});const deadline=Date.now()+180000;while(!['succeeded','failed','cancelled'].includes(job.state)){if(Date.now()>deadline)throw Error('Observe existing live job '+job.job_id+'; timeout is not termination');await new Promise(r=>setTimeout(r,400));job=await call('opencut_job_view',{job_id:accepted.job_id})}expect(job.state,JSON.stringify(job.failure)).toBe('succeeded');return job});
 const observations=[];let pngModule;
 for(const phase of [0,2]){
  const job=await submit('png-phase-'+phase,"opencut_render_submit",{ kind: "frame", request: {project_id:project,revision:fixed.revision,sequence_id:q.id,time:time(phase*.5+.25),idempotency_key:key('png-phase-'+phase)} }),file=await artifact(job,'png'),pixels=execFileSync('ffmpeg',['-v','error','-i',file,'-f','rawvideo','-pix_fmt','rgba','pipe:1']);pngModule=job.result.data.execution;
  const comparisons=[];for(let i=0;i<5;i++){const [cx,cy]=centers[i];let max=0,sum=0,count=0,negative=0;for(let y=cy-32;y<Math.min(180,cy+33);y++)for(let x=cx-42;x<Math.min(320,cx+43);x++)for(let c=0;c<3;c++){if(x<0||y<0)continue;const p=(y*320+x)*4+c;max=Math.max(max,Math.abs(pixels[p]-references[phase][p]));sum+=Math.abs(pixels[p]-references[phase][p]);count++;if(phase===0)negative=Math.max(negative,Math.abs(pixels[p]-wrong[p]))}comparisons.push({kind:kinds[i],max_channel_error:max,mean_absolute_error:sum/count,wrong_canvas_negative_max_delta:negative});expect(max,JSON.stringify({phase,kind:kinds[i],comparisons})).toBeLessThanOrEqual(2);if(phase===0&&i<4)expect(negative).toBeGreaterThan(12)}
  observations.push({phase,job_id:job.job_id,path:file,comparisons});
 }
 const videoJob=await submit('one-video',"opencut_render_submit",{ kind: "video", request: {project_id:project,revision:fixed.revision,sequence_id:q.id,idempotency_key:key('one-video'),options:{format:'mp4',quality:'very_high',include_audio:false}} }),videoPath=await artifact(videoJob,'mp4'),frames=execFileSync('ffmpeg',['-v','error','-i',videoPath,'-f','rawvideo','-pix_fmt','rgba','pipe:1'],{maxBuffer:32*1024*1024});expect(frames.length).toBe(45*320*180*4);
 const encoded=[];for(const phase of [0,1,2]){const f=phase*15+8,pixels=frames.subarray(f*320*180*4,(f+1)*320*180*4);let error=0;for(let p=0;p<pixels.length;p+=4)for(let c=0;c<3;c++)error+=Math.abs(pixels[p+c]-references[phase][p+c]);const mean=error/(320*180*3);expect(mean).toBeLessThan(5);encoded.push({phase,frame:f,independent_rgb_mean_absolute_error:mean,threshold_exclusive:5})}
 await stage('actual-combined-preview',async()=>{
  await page.addInitScript(()=>{window.__f33Pictures=[];const slots=new Set(),original=ImageBitmapRenderingContext.prototype.transferFromImageBitmap;ImageBitmapRenderingContext.prototype.transferFromImageBitmap=function(bitmap){const t=Number(this.canvas.dataset.time),phase=Math.floor(t/.5);if(bitmap&&window.__f33Capture&&this.canvas.classList.contains('editor-scene-frame')&&phase>=0&&phase<3&&t-phase*.5>.07&&!slots.has(phase)){slots.add(phase);window.__f33Pictures.push({phase,time:t,generation:this.canvas.dataset.generation,bitmap:createImageBitmap(bitmap)})}return original.call(this,bitmap)}});
  await open();await page.evaluate(()=>{window.__f33Capture=true});await page.getByRole('button',{name:'Play scene preview',exact:true}).click();await expect.poll(()=>page.evaluate(()=>window.__f33Pictures.length),{timeout:30000}).toBe(3);const pause=page.getByRole('button',{name:'Pause scene audio',exact:true});if(await pause.isEnabled())await pause.click();
  const pictures=await page.evaluate(async()=>Promise.all(window.__f33Pictures.map(async p=>{const b=await p.bitmap,c=new OffscreenCanvas(b.width,b.height),ctx=c.getContext('2d');ctx.drawImage(b,0,0);const raw=Array.from(ctx.getImageData(0,0,b.width,b.height).data);b.close();return {phase:p.phase,time:p.time,generation:p.generation,raw}})));
  const facts=[];for(const p of pictures){let max=0;for(let k=0;k<p.raw.length;k++)max=Math.max(max,Math.abs(p.raw[k]-references[p.phase][k]));expect(max,JSON.stringify({phase:p.phase,time:p.time})).toBeLessThanOrEqual(2);facts.push({phase:p.phase,time:p.time,generation:p.generation,independent_max_channel_error:max})}return facts;
 });
 expect((await inspect(project)).project).toEqual(fixed.project);expect(await inspect(sourceProject)).toEqual(state.source_head_before);expect(errors).toEqual([]);
 await fs.writeFile(report,JSON.stringify({status:'passed_scoped_f33_object_blur',project_id:project,revision:fixed.revision,source:{project_id:sourceProject,revision:sourceRevision,head_unchanged:true},types:kinds,phase_semantics:['enabled100','disabled100','two100+disabled+zero,full-canvas adjustment15'],actual_ui:{save:true,reopen:true,undo_full_project:true,redo_full_project:true},png:observations,encoded,video_job_id:videoJob.job_id,video_path:videoPath,preview:state.stages['actual-combined-preview'],runtime:pngModule,page_errors:errors,new_sources:0,new_jobs:3,new_rust_builds:0,scope:'F33 existing V4 object classes/parameter/enable/stack/alpha, actual UI history and one combined preview/output. Independent authored source/font rasters and Gaussian model; wrong-canvas negative controls. Whole Classic sticker import mapping and F34 animation/ordering remain separate open contracts; no full migration/performance/OS/D8 claim.'},null,2)+'\n');console.log(JSON.stringify({status:'passed_scoped_f33_object_blur',project_id:project,video_job_id:videoJob.job_id}));
}catch(error){await fs.writeFile(path.join(workspace,'f33-object-failure.json'),JSON.stringify({error:String(error),stack:error.stack,page_errors:errors},null,2)+'\n');await page.screenshot({path:path.join(workspace,'f33-object-failure.png'),fullPage:true});throw error}finally{await browser.close()}
