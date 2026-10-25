/** Retained configurable export UI recovery and built-SDK download/playback. */
import { chromium, expect } from '@playwright/test';
import fs from 'node:fs/promises';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {OpenCutClient} from '../../../packages/sdk-ts/dist/index.js';
const args=Object.fromEntries(process.argv.slice(2).reduce((a,v,i,s)=>i%2?a:[...a,[v.slice(2),s[i+1]]],[]));
const {workspace,endpoint,phase,report}=args;
const input=JSON.parse(await fs.readFile(args.input,'utf8'));
const sha=b=>createHash('sha256').update(b).digest('hex');
const browser=await chromium.launch({headless:true,executablePath:process.env.OPENCUT_CHROME});
const page=await browser.newPage({locale:'en-US',viewport:{width:1440,height:1100}});
const errors=[];page.on('pageerror',e=>errors.push(e.message));
try {
 if(phase==='submit') {
  const cp=path.join(workspace,'agent-export-config-ui-checkpoint.json');let state;
  try{state=JSON.parse(await fs.readFile(cp,'utf8'))}catch(e){if(e.code!=='ENOENT')throw e;state={}}
  const save=async()=>{await fs.writeFile(cp+'.pending',JSON.stringify(state,null,2)+'\n');await fs.rename(cp+'.pending',cp)};
  const scope={recovery_storage_key:'opencut.export-video-recovery',submit_tool:'opencut_render_submit',request_kind:'video'};
  if(state.acceptance_scope && JSON.stringify(state.acceptance_scope)!==JSON.stringify(scope))throw Error('Retained UI export scope differs from the current flow');
  if(!state.acceptance_scope){
   if(Object.keys(state).length && !state.historical_checkpoint)state.historical_checkpoint=structuredClone(state);
   delete state.completed;state.acceptance_scope=scope;await save();
  }
  if(state.completed){await fs.writeFile(report,JSON.stringify(state.completed,null,2)+'\n');console.log('Reuse completed original UI export');}
  else {
   if(state.recovery)await page.addInitScript(value=>{
    const envelope=JSON.parse(value),historical=Object.hasOwn(envelope,'version');
    if(historical ? envelope.version!==2 || Object.keys(envelope).sort().join()!=='entries,version' : Object.keys(envelope).join()!=='entries')throw Error('Invalid retained export recovery envelope');
    sessionStorage.setItem(historical?'opencut.export-video-recovery.v2':'opencut.export-video-recovery',value);
   },state.recovery);
   const open=async()=>{await page.goto(`${endpoint}/editor/${encodeURIComponent(input.project_id)}`);await expect(page.getByRole('main',{name:'OpenCut editor',exact:true})).toBeVisible();const guide=page.getByRole('dialog',{name:'Welcome to OpenCut',exact:true});if(await guide.isVisible())await guide.getByRole('button',{name:'Close',exact:true}).click();await page.getByRole('button',{name:'Export video',exact:true}).click();};
   await open();let dialog=page.getByRole('dialog',{name:'Export video',exact:true});
   await page.route('**/api/tools/opencut_render_submit',async route=>{
    const submission=route.request().postDataJSON();expect(submission.kind).toBe('video');expect(Object.keys(submission).sort()).toEqual(['kind','request']);
    const request=submission.request;
    expect(request.project_id).toBe(input.project_id);expect(request.revision).toBe(input.revision);expect(request.options).toEqual({format:'mp4',quality:'low',include_audio:true});
    if(state.request)expect(request).toEqual(state.request);else{state.request=request;state.recovery=await page.evaluate(()=>sessionStorage.getItem('opencut.export-video-recovery'));await save();}
    const response=await route.fetch(),body=await response.json();expect(body.error).toBeNull();
    if(!state.committed){state.committed=body.data;await save();await route.abort('failed');}else{expect(body.data.job_id).toBe(state.committed.job_id);await route.fulfill({response});}
   });
   if(!state.request){
    for(const quality of ['low','medium','high','very_high'])await dialog.getByRole('combobox',{name:/^Quality/}).selectOption(quality);
    for(const format of ['webm','mp4'])await dialog.getByRole('combobox',{name:/^Format/}).selectOption(format);
    await dialog.getByRole('checkbox',{name:'Include audio',exact:true}).setChecked(false);await dialog.getByRole('checkbox',{name:'Include audio',exact:true}).setChecked(true);
    await dialog.getByRole('combobox',{name:/^Quality/}).selectOption('low');
    await expect(dialog.getByRole('button',{name:'Start video export',exact:true})).toBeEnabled();await dialog.getByRole('button',{name:'Start video export',exact:true}).click();
    await expect(dialog.getByRole('button',{name:'Retry original export',exact:true})).toBeEnabled();
    await open();dialog=page.getByRole('dialog',{name:'Export video',exact:true});
   }
   await expect(dialog.getByRole('button',{name:'Retry original export',exact:true})).toBeEnabled();await dialog.getByRole('button',{name:'Retry original export',exact:true}).click();
   await expect(dialog.getByText('Export task accepted. Progress and downloads are in Tasks.',{exact:true})).toBeVisible();
   const stored=await page.evaluate(()=>JSON.parse(sessionStorage.getItem('opencut.export-video-recovery')));
   const entry=stored.entries.find(e=>e.project===input.project_id);expect(entry.attempt.jobId).toBe(state.committed.job_id);expect(entry.attempt.request).toEqual(state.request);
   await dialog.locator('.modal-footer').getByRole('button',{name:'Close',exact:true}).click();await page.reload();await expect(page.getByRole('main',{name:'OpenCut editor',exact:true})).toBeVisible();expect(errors).toEqual([]);
   state.completed={acceptance_scope:scope,request:state.request,accepted_job_id:state.committed.job_id,committed_response_dropped:true,full_reload_original_request_retry:true,accepted_job_survives_dialog_close_and_reload:true,ui_format_quality_audio_controls:true,page_errors:errors};await save();await fs.writeFile(report,JSON.stringify(state.completed,null,2)+'\n');console.log('Passed original UI export recovery');
  }
 } else if(phase==='download-play') {
  const client=new OpenCutClient({baseUrl:endpoint,token:(await fs.readFile(args.credential,'utf8')).trim()});const rows=[];
  for(const job of input.jobs){
   const response=await client.downloadArtifact(job.artifact);const bytes=Buffer.from(await response.arrayBuffer());expect(sha(bytes)).toBe(job.artifact.content_hash);
   const row={job_id:job.job_id,downloaded_sha256:sha(bytes),bytes:bytes.length};
   if(job.play){await page.goto('about:blank');await page.setContent('<video controls muted></video>');const playback=await page.locator('video').evaluate(async(el,value)=>{const data=Uint8Array.from(atob(value.base64),c=>c.charCodeAt(0));el.src=URL.createObjectURL(new Blob([data],{type:value.mime}));await new Promise((resolve,reject)=>{el.onloadedmetadata=resolve;el.onerror=()=>reject(Error('Browser media decode failed'))});await el.play();await new Promise((resolve,reject)=>{const t=setTimeout(()=>reject(Error('Playback did not end')),10000);el.onended=()=>{clearTimeout(t);resolve()};el.onerror=()=>reject(Error('Playback failed'))});const result={duration:el.duration,time:el.currentTime,width:el.videoWidth,height:el.videoHeight,ended:el.ended,ready_state:el.readyState};URL.revokeObjectURL(el.src);return result;},{base64:bytes.toString('base64'),mime:job.artifact.mime_type});expect(playback.ended).toBe(true);expect(playback.width).toBe(320);expect(playback.height).toBe(180);row.browser_playback=playback;}
   rows.push(row);
  }
  expect(errors).toEqual([]);await fs.writeFile(report,JSON.stringify({status:'passed',downloads:rows,page_errors:errors},null,2)+'\n');console.log('Passed SDK downloads and MP4/WebM browser playback');
 } else throw Error('Unknown phase');
} catch(e){await fs.writeFile(path.join(workspace,'export-config-ui-failure.json'),JSON.stringify({phase,error:String(e),page_errors:errors},null,2)+'\n');await page.screenshot({path:path.join(workspace,'export-config-ui-failure.png'),fullPage:true});throw e;}finally{await browser.close()}
