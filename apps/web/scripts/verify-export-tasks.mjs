/** One mixed export: actual durable progress/reload, terminal cancellation and downloads. */
import {chromium,expect} from '@playwright/test';
import fs from 'node:fs/promises';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {OpenCutClient} from '../../../packages/sdk-ts/dist/index.js';
const args=Object.fromEntries(process.argv.slice(2).reduce((a,v,i,s)=>i%2?a:[...a,[v.slice(2),s[i+1]]],[]));
const {workspace,endpoint,report}=args,input=JSON.parse(await fs.readFile(args.input,'utf8'));
const cp=path.join(workspace,'agent-export-tasks-ui-checkpoint.json');let state;
try{state=JSON.parse(await fs.readFile(cp,'utf8'))}catch(e){if(e.code!=='ENOENT')throw e;state={}}
const save=async()=>{await fs.writeFile(cp+'.pending',JSON.stringify(state,null,2)+'\n');await fs.rename(cp+'.pending',cp)};
const token=(await fs.readFile(args.credential,'utf8')).trim(),client=new OpenCutClient({baseUrl:endpoint,token});
const sha=b=>createHash('sha256').update(b).digest('hex');
const browser=await chromium.launch({headless:true,executablePath:process.env.OPENCUT_CHROME});const page=await browser.newPage({locale:'en-US',viewport:{width:1440,height:1100}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
try{
 const open=async()=>{await page.goto(`${endpoint}/editor/${encodeURIComponent(input.project_id)}`);await expect(page.getByRole('main',{name:'OpenCut editor',exact:true})).toBeVisible();const guide=page.getByRole('dialog',{name:'Welcome to OpenCut',exact:true});if(await guide.isVisible())await guide.getByRole('button',{name:'Close',exact:true}).click()};
 await open();
 if(!state.request){
  await page.route('**/api/tools/opencut_render_submit',async route=>{const value=route.request().postDataJSON();expect(value.kind).toBe('video');expect(value.request.project_id).toBe(input.project_id);expect(value.request.revision).toBe(input.revision);state.request=value;await save();const response=await route.fetch(),body=await response.json();expect(body.error).toBeNull();state.accepted_job_id=body.data.job_id;await save();await route.fulfill({response})});
  await page.getByRole('button',{name:'Export video',exact:true}).click();const dialog=page.getByRole('dialog',{name:'Export video',exact:true});await expect(dialog.getByRole('button',{name:'Start video export',exact:true})).toBeEnabled();await dialog.getByRole('button',{name:'Start video export',exact:true}).click();await expect(dialog.getByRole('button',{name:'View tasks',exact:true})).toBeVisible();await dialog.getByRole('button',{name:'View tasks',exact:true}).click();await page.unroute('**/api/tools/opencut_render_submit');
 }else{
  if(state.accepted_job_id){const receipt=await client.call('opencut_job_view',{job_id:state.accepted_job_id});expect(receipt.job_id).toBe(state.accepted_job_id)}
  else{if(state.request.kind!=='video')throw Error('Retained historical export has no observed job ID;resolve its original acceptance before submitting through the current contract. Do not rewrite the original request or guess encoding options.');const receipt=await client.call('opencut_render_submit',state.request);state.accepted_job_id=receipt.job_id;await save()}
  await page.getByRole('button',{name:'Tasks',exact:true}).click();
 }
 const tasks=page.getByRole('dialog',{name:'Tasks',exact:true});const card=tasks.locator('article.task-card').filter({has:page.getByText('Export',{exact:true})});await expect(card).toHaveCount(1);
 if(!state.progress_before_reload){
  await expect.poll(async()=>{const stage=(await card.locator('small').allTextContents()).join(' '),value=Number(await card.locator('progress').getAttribute('value'));return stage.includes('encoding')&&value>0&&value<1},{timeout:120000}).toBe(true);
  state.progress_before_reload={stage:(await card.locator('small').allTextContents()).join(' '),fraction:Number(await card.locator('progress').getAttribute('value')),job:await client.call('opencut_job_view',{job_id:state.accepted_job_id})};await save();
  await page.reload();await expect(page.getByRole('main',{name:'OpenCut editor',exact:true})).toBeVisible();await page.getByRole('button',{name:'Tasks',exact:true}).click();await expect(card).toHaveCount(1);
  const after=await client.call('opencut_job_view',{job_id:state.accepted_job_id});expect(['running','succeeded']).toContain(after.state);state.reload_same_job={job_id:after.job_id,state:after.state};await save();
 }
 await expect(card.locator('.state')).toHaveText('succeeded',{timeout:180000});const completed=await client.call('opencut_job_view',{job_id:state.accepted_job_id});expect(completed.artifacts).toHaveLength(1);
 // Headless Chromium cannot operate the native OS save picker. Exercise the
 // real product's bounded Blob fallback; the unchanged streaming-picker path
 // retains its existing WAV/large-artifact evidence. No response bytes mocked.
 await page.evaluate(()=>{delete window.showSaveFilePicker});expect(await page.evaluate(()=>typeof window.showSaveFilePicker)).toBe('undefined');
 const pending=page.waitForEvent('download');void pending.catch(()=>{});await card.getByRole('button',{name:'Download',exact:true}).click();const file=await pending,bytes=await fs.readFile(await file.path());expect(sha(bytes)).toBe(completed.artifacts[0].content_hash);
 const cancelled=await client.cancelJob(state.accepted_job_id);expect(cancelled).toEqual(completed);
 const response=await client.downloadArtifact(cancelled.artifacts[0]);expect(sha(Buffer.from(await response.arrayBuffer()))).toBe(sha(bytes));
 const no_download=[];
 for(const job_id of input.refused_jobs){const v=await client.call('opencut_job_view',{job_id});expect(['cancelled','failed']).toContain(v.state);expect(v.artifacts).toEqual([]);const response=await fetch(`${endpoint}/api/jobs/${job_id}/artifact`,{headers:{Authorization:`Bearer ${token}`}});expect(response.status).toBeGreaterThanOrEqual(400);expect(response.status).toBeLessThan(500);const body=await response.json();expect(body.data).toBeNull();expect(body.error).toBeTruthy();no_download.push({job_id,state:v.state,status:response.status,code:body.error.code})}
 expect(errors).toEqual([]);state.completed={status:'passed_ui_progress_task_recovery_and_downloads',project_id:input.project_id,revision:input.revision,request:state.request,job_id:state.accepted_job_id,progress_before_reload:state.progress_before_reload,reload_same_job:state.reload_same_job,completed,actual_ui_download_sha256:sha(bytes),download_save_path:'actual bounded Blob fallback; headless OS chooser disabled',postpublication_cancel_exact_terminal_result:true,post_cancel_download_exact:true,refused_downloads:no_download,page_errors:errors};await save();await fs.writeFile(report,JSON.stringify(state.completed,null,2)+'\n');console.log(JSON.stringify({status:state.completed.status,job_id:state.accepted_job_id}));
}catch(error){await fs.writeFile(path.join(workspace,'f52-ui-failure.json'),JSON.stringify({error:String(error),job_id:state.accepted_job_id,page_errors:errors},null,2)+'\n');await page.screenshot({path:path.join(workspace,'f52-ui-failure.png'),fullPage:true});throw error}finally{await browser.close()}
