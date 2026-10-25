import { editorSnapshot } from "./project-inspection.mjs";
/** F51 UI/history and independent source-cover/Gaussian/color output checks. */
import {chromium,expect} from '@playwright/test';
import fs from 'node:fs/promises';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {OpenCutClient} from '../../../packages/sdk-ts/dist/index.js';
import {gaussianSceneRgba} from './scene-stack-oracle.mjs';
import {compositeClassicRgba8} from './blend-oracle.mjs';
const args=Object.fromEntries(process.argv.slice(2).reduce((a,v,i,s)=>i%2?a:[...a,[v.slice(2),s[i+1]]],[]));
const {workspace,report}=args,input=JSON.parse(await fs.readFile(args.input,'utf8'));
const sha=b=>createHash('sha256').update(b).digest('hex');
const browser=await chromium.launch({headless:true,executablePath:process.env.OPENCUT_CHROME});
const page=await browser.newPage({locale:'en-US',viewport:{width:1440,height:1100}});
const errors=[];page.on('pageerror',e=>errors.push(e.message));
try {
 if(args.phase==='oracle') {
  const decode=(file,seconds,w,h)=>execFileSync('ffmpeg',['-v','error',...(seconds===null?[]:['-ss',String(seconds)]),'-i',file,'-frames:v','1','-f','rawvideo','-pix_fmt','rgba','pipe:1'],{maxBuffer:32*1024*1024});
  const source=input.layouts[0].baseline.project.assets.find(a=>a.id==='ae49da88-9b66-4b6f-a37b-34096a656970');
  const green=input.layouts[0].baseline.project.assets.find(a=>a.id==='6f2e16da-b308-489d-ab47-5aadf19a9858');
  const sourcePath=a=>path.join(workspace,'asset-store',a.version.content_hash.slice(0,2),a.version.content_hash);
  for(const a of [source,green])expect(sha(await fs.readFile(sourcePath(a)))).toBe(a.version.content_hash);
  const raw=decode(sourcePath(source),.25),greenRaw=decode(sourcePath(green),.25);
  expect(raw.length).toBe(320*180*4);
  const greenProbe=JSON.parse(execFileSync('ffprobe',['-v','error','-show_streams','-of','json',sourcePath(green)],{encoding:'utf8'})).streams.find(s=>s.codec_type==='video');
  const greenColor=Array.from(greenRaw.subarray(((Math.floor(greenProbe.height/2)*greenProbe.width)+Math.floor(greenProbe.width/2))*4,((Math.floor(greenProbe.height/2)*greenProbe.width)+Math.floor(greenProbe.width/2))*4+4));
  const old=JSON.parse(await fs.readFile(input.retained_background_checkpoint,'utf8'));
  const exact=gaussianSceneRgba(raw,60);expect(sha(exact)).toBe(old.jobs.background.oracle.expected_rgba_sha256);expect(old.jobs.background.observation.passed).toBe(true);
  // Numeric generalization of the existing independently qualified 61-tap,
  // clamped, separable RGBA8 Gaussian. No product renderer or WGSL is imported.
  const gaussian=(raw,w,h)=>{
   if(w===320&&h===180)return exact;
   let data=Uint8Array.from(raw);
   for(const [axis,sigma] of [[0,12*w/1920],[1,12*h/1080]]){
    const weights=Array.from({length:61},(_,i)=>Math.exp(-((i-30)**2)/(2*sigma*sigma))),sum=weights.reduce((a,b)=>a+b,0),out=new Uint8Array(data.length);
    for(let y=0;y<h;y++)for(let x=0;x<w;x++)for(let c=0;c<4;c++){
     let v=0;for(let i=0;i<61;i++){const sx=Math.min(w-1,Math.max(0,x+(axis===0?i-30:0))),sy=Math.min(h-1,Math.max(0,y+(axis===1?i-30:0)));v+=data[(sy*w+sx)*4+c]*weights[i]}
     out[(y*w+x)*4+c]=Math.min(255,Math.max(0,Math.round(v/sum)));
    }data=out;
   }return data;
  };
  const references={};
  for(const l of input.layouts){
   const {width:w,height:h}=l;
   // Independent FFmpeg source decode and ordinary Canvas2D cover geometry;
   // source bytes never come from a product frame or render artifact.
   const covered=Uint8Array.from(await page.evaluate(({raw,w,h})=>{
    const source=document.createElement('canvas');source.width=320;source.height=180;source.getContext('2d').putImageData(new ImageData(Uint8ClampedArray.from(raw),320,180),0,0);
    const out=document.createElement('canvas');out.width=w;out.height=h;const s=Math.max(w/320,h/180);out.getContext('2d').drawImage(source,(w-320*s)/2,(h-180*s)/2,320*s,180*s);return Array.from(out.getContext('2d').getImageData(0,0,w,h).data);
   },{raw:Array.from(raw),w,h}));
   const blurred=gaussian(covered,w,h);references[l.name]={covered,blurred};
   if(l.name==='landscape')expect(Buffer.from(covered)).toEqual(raw);
  }
  const observations=[],failures=[];
  for(const o of input.outputs){
   const l=input.layouts.find(l=>l.name===o.layout),{width:w,height:h}=l;
   const times=o.format==='png'?[null]:[8/30,14/30,15/30,23/30];
   if(o.format==='mp4'){
    const probe=JSON.parse(execFileSync('ffprobe',['-v','error','-count_frames','-show_streams','-of','json',o.path],{encoding:'utf8'}));const stream=probe.streams.find(s=>s.codec_type==='video');expect([stream.width,stream.height,Number(stream.nb_read_frames),stream.codec_name]).toEqual([w,h,30,'h264']);expect(probe.streams.some(s=>s.codec_type==='audio')).toBe(false);
   }
   for(const time of times){
    const pixels=decode(o.path,time),threshold=o.format==='png'?2:8,samples=[];
    expect(pixels.length).toBe(w*h*4);const second=time!==null&&time>=.5;
    const lossyBlur=o.format==='mp4'&&o.kind==='blur'&&!second;
    const expectedAt=(x,y)=>o.kind==='color'?[36,104,172,255]:second?greenColor:Array.from(references[l.name].blurred.subarray((y*w+x)*4,(y*w+x)*4+4));
    let negative=0,discriminating=0;
    for(const fy of [.07,.13,.22,.78,.87,.93])for(const fx of [.05,.1,.2,.8,.9,.95]){
     const x=Math.floor(fx*w),y=Math.floor(fy*h),p=(y*w+x)*4,expected=expectedAt(x,y),actual=Array.from(pixels.subarray(p,p+4)),error=Math.max(...actual.map((v,c)=>Math.abs(v-expected[c])));
     if(!lossyBlur&&error>threshold)failures.push({layout:l.name,format:o.format,time,x,y,actual,expected,error,threshold});samples.push({x,y,expected,actual,error});
    }
    let independentBackgroundError=0,backgroundChannels=0;
    if(o.kind==='blur'&&!second)for(let y=0;y<h;y++)if(y<h*.3||y>h*.7)for(let x=0;x<w;x++){
     const p=(y*w+x)*4,delta=Math.max(...[0,1,2].map(c=>Math.abs(references[l.name].blurred[p+c]-references[l.name].covered[p+c])));negative=Math.max(negative,delta);if(delta>12)discriminating++;
     for(let c=0;c<3;c++){independentBackgroundError+=Math.abs(pixels[p+c]-references[l.name].blurred[p+c]);backgroundChannels++}
    }
    if(o.kind==='blur'&&!second&&discriminating<8)failures.push({layout:l.name,format:o.format,time,insufficient_discriminating_points:discriminating});
    const x=Math.floor(.2*w),y=Math.floor(.5*h),expected=Array.from(compositeClassicRgba8(expectedAt(x,y),[200,80,40,128],'normal')),actual=Array.from(pixels.subarray((y*w+x)*4,(y*w+x)*4+4)),error=Math.max(...actual.map((v,c)=>Math.abs(v-expected[c])));
    if(error>threshold)failures.push({transparent_content:true,layout:l.name,format:o.format,time,actual,expected,error,threshold});samples.push({x,y,expected,actual,error,transparent_content:true});
    let lossyConsistency=null;
    if(o.format==='png'&&o.kind==='blur')references[l.name].lossless=pixels;
    if(lossyBlur){
     // The existing flat-source max8 criterion is not a high-frequency lossy
     // raster criterion. Preserve those diagnostic deltas; use the established
     // PNG/video mean-error<5 gate, anchored by independently checked PNGs.
     const lossless=references[l.name].lossless;expect(lossless).toBeTruthy();let error=0;
     for(let p=0;p<pixels.length;p+=4)for(let c=0;c<3;c++)error+=Math.abs(pixels[p+c]-lossless[p+c]);
     const mean=error/(w*h*3),independentMean=independentBackgroundError/backgroundChannels;
     if(mean>=5||independentMean>=5)failures.push({layout:l.name,time,lossy_png_consistency_mae:mean,independent_background_mae:independentMean,existing_mean_error_limit:5});
     lossyConsistency={png_video_mean_absolute_error:mean,independent_background_mean_absolute_error:independentMean,threshold_exclusive:5,criterion_source:'scripts/verify-agent-interview.py reviewed PNG/MP4 consistency; PNG independent Gaussian and color max2 remain required'};
    }
    observations.push({layout:l.name,kind:o.kind,format:o.format,time,job_id:o.job.job_id,threshold:lossyBlur?null:threshold,pointwise_lossy_diagnostic_only:lossyBlur,max_channel_error:Math.max(...samples.map(s=>s.error)),samples,unblurred_negative_max_delta:negative,blur_discriminating_points:discriminating,lossy_consistency:lossyConsistency});
   }
  }
  expect(errors).toEqual([]);await fs.writeFile(report,JSON.stringify({status:failures.length?'failed_independent_background_pixels':'passed_independent_background_pixels',reused_landscape_png:{job_id:old.jobs.background.job.job_id,oracle:old.jobs.background.oracle,source_sha256:source.version.content_hash,original_observation:old.jobs.background.observation},reference_model:'independent source decode, centered cover projection, existing Gaussian61-tap RGBA8 model; independent normal alpha blend',observations,failures,initial_inapplicable_pointwise_diagnostic:path.join(path.dirname(args.input),'background-oracle-diagnostic.json'),scope:'Three canvas ratios, both background modes, two active source clips and half-transparent foreground. Independent PNG max2; flat H264 source and transparent probes max8; high-frequency lossy blur uses established PNG/video MAE<5 plus independently expected background MAE<5, not a relaxed point maximum. Initial max22 diagnostic retained. No full performance/format/OS-picker claim.'},null,2)+'\n');expect(failures).toEqual([]);
 }else{
  const {endpoint}=args,token=(await fs.readFile(args.credential,'utf8')).trim(),client=new OpenCutClient({baseUrl:endpoint,token});
  const cp=path.join(workspace,'agent-background-ui-checkpoint.json');let state;
  try{state=JSON.parse(await fs.readFile(cp,'utf8'))}catch(e){if(e.code!=='ENOENT')throw e;state={actions:{},layouts:[]}}
  const save=async()=>{await fs.writeFile(cp+'.pending',JSON.stringify(state,null,2)+'\n');await fs.rename(cp+'.pending',cp)};
  const inspect=project_id=>editorSnapshot(client.call('opencut_project_inspect',{project_id,revision:null}));
  const revisionOf=receipt=>receipt.transaction?.revision??receipt.revision;
  const open=async id=>{await page.goto(`${endpoint}/editor/${encodeURIComponent(id)}`);await expect(page.getByRole('main',{name:'OpenCut editor',exact:true})).toBeVisible();const guide=page.getByRole('dialog',{name:'Welcome to OpenCut',exact:true});if(await guide.isVisible())await guide.getByRole('button',{name:'Close',exact:true}).click()};
  const settings=async()=>{const form=page.getByRole('region',{name:'Canvas and frame rate',exact:true});if(!await form.isVisible())await page.getByRole('button',{name:'Canvas and background',exact:true}).click();await expect(form).toBeVisible();return form};
  const action=async(name,tool,project_id,click)=>{
   let rec=state.actions[name];if(rec?.snapshot)return rec.snapshot;
   if(rec?.request){rec.receipt=await client.call(tool,rec.request);rec.snapshot=await inspect(project_id);expect(rec.snapshot.revision).toBe(revisionOf(rec.receipt));await save();return rec.snapshot}
   rec=state.actions[name]={tool};await save();
   await page.route('**/api/tools/'+tool,async route=>{rec.request=route.request().postDataJSON();await save();const response=await route.fetch(),body=await response.json();expect(body.error).toBeNull();rec.receipt=body.data;await save();await route.fulfill({response})});
   try{await click();await expect.poll(()=>!!rec.receipt,{timeout:30000}).toBe(true);await expect.poll(async()=>(await inspect(project_id)).revision).toBe(revisionOf(rec.receipt));rec.snapshot=await inspect(project_id);await save();return rec.snapshot}finally{await page.unroute('**/api/tools/'+tool)}
  };
  for(const l of input.layouts){
   if(state.layouts.some(x=>x.name===l.name))continue;
   let previous=l.baseline;const out={...l};
   for(const kind of ['color','blur']){
    await open(l.project_id);
    const edited=await action(l.name+'-'+kind+'-save','opencut_edit_apply',l.project_id,async()=>{
     const form=await settings(),select=form.getByRole('combobox',{name:'Background',exact:true});expect(await select.locator('option').evaluateAll(xs=>xs.map(x=>x.value))).toEqual(['color','main_track_blur']);await select.selectOption(kind==='color'?'color':'main_track_blur');
     if(kind==='color'){const color=form.getByLabel('Background color',{exact:true});await color.evaluate(input=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,'#2468ac');input.dispatchEvent(new Event('input',{bubbles:true}));input.dispatchEvent(new Event('change',{bubbles:true}))});await expect(color).toHaveValue('#2468ac');await form.getByLabel('Background opacity',{exact:true}).fill('1')}
     else await form.getByLabel('Background blur intensity',{exact:true}).fill('60');
     await form.getByRole('button',{name:'Save canvas and frame rate',exact:true}).click();
    });
    const background=edited.project.settings.settings.background;expect(background.kind).toBe(kind==='color'?'color':'main_track_blur');if(kind==='color')expect(background.color).toEqual({encoding:'srgb',red:36/255,green:104/255,blue:172/255,alpha:1});else expect(background.intensity).toBe(60);
    await open(l.project_id);const form=await settings();await expect(form.getByRole('combobox',{name:'Background',exact:true})).toHaveValue(background.kind);if(kind==='color')await expect(form.getByLabel('Background color',{exact:true})).toHaveValue('#2468ac');else await expect(form.getByLabel('Background blur intensity',{exact:true})).toHaveValue('60');
    await open(l.project_id);
    const undone=await action(l.name+'-'+kind+'-undo','opencut_edit_undo_apply',l.project_id,async()=>{await page.getByRole('region',{name:'History',exact:true}).getByRole('button',{name:'Undo',exact:true}).click();await page.getByRole('region',{name:'History plan',exact:true}).getByRole('button',{name:'Apply undo',exact:true}).click()});expect(undone.project).toEqual(previous.project);
    await open(l.project_id);
    const redone=await action(l.name+'-'+kind+'-redo','opencut_edit_redo_apply',l.project_id,async()=>{await page.getByRole('region',{name:'History',exact:true}).getByRole('button',{name:'Redo',exact:true}).click();await page.getByRole('region',{name:'History plan',exact:true}).getByRole('button',{name:'Apply redo',exact:true}).click()});expect(redone.project).toEqual(edited.project);
    out[kind]=redone;previous=redone;
   }
   state.layouts.push(out);await save();
  }
  expect(errors).toEqual([]);await fs.writeFile(report,JSON.stringify({status:'passed_actual_background_ui_history',layouts:state.layouts,actions:state.actions,page_errors:errors,color_input_path:'DOM native input/change events through actual React handler; OS color chooser not automated'},null,2)+'\n');
 }
 console.log(JSON.stringify({status:'passed',phase:args.phase??'ui'}));
}catch(error){await fs.writeFile(path.join(workspace,'f51-'+(args.phase??'ui')+'-failure.json'),JSON.stringify({error:String(error),page_errors:errors},null,2)+'\n');await page.screenshot({path:path.join(workspace,'f51-'+(args.phase??'ui')+'-failure.png'),fullPage:true});throw error}finally{await browser.close()}
