import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import {createHash} from 'node:crypto';
const [sdkRoot,binary,database,base,runner,reader,source,mainUpload]=process.argv.slice(2);
const directory=path.join(sdkRoot,'node_modules/@modelcontextprotocol/sdk/dist/esm/client');
const {Client}=await import(pathToFileURL(path.join(directory,'index.js')).href);
const {StdioClientTransport}=await import(pathToFileURL(path.join(directory,'stdio.js')).href);
const {StreamableHTTPClientTransport}=await import(pathToFileURL(path.join(directory,'streamableHttp.js')).href);
const bytes=fs.readFileSync(source),hash=createHash('sha256').update(bytes).digest('hex'),jobs=[],checks=[];
for(const mode of ['stdio','http'])for(const readOnly of [false,true]){
 const credential=readOnly?reader:runner,token=fs.readFileSync(credential,'utf8').trim();
 const transport=mode==='stdio'?new StdioClientTransport({command:binary,args:['--database',database,'--credential-file',credential,'mcp','serve'],stderr:'pipe'}):new StreamableHTTPClientTransport(new URL(base+'/mcp'),{requestInit:{headers:{Authorization:'Bearer '+token}}});
 const client=new Client({name:'project-import-qa',version:'1'}),errors=[];client.onerror=e=>errors.push(String(e));
 async function call(name,args,error){const r=await client.callTool({name,arguments:args});const v=r.structuredContent;assert.ok(v);if(error){assert.equal(r.isError,true);assert.equal(v.error.code,error);return;}assert.equal(v.error,null);return v.data;}
 try{
  await client.connect(transport);
  const found=await client.listTools();assert.ok(found.tools.some(t=>t.name==='opencut_media'));
  assert.equal((await call('opencut_upload_inspect',{upload_id:mainUpload})).project_id,'a');
  const staging=found.tools.some(t=>t.name==='opencut_media');
  if(staging){assert.equal((await call('opencut_upload_retention',{upload_id:mainUpload})).project_id,'a');await call('opencut_upload_quota',{project_id:'a'});const page=await call('opencut_upload_query',{project_id:'a',limit:1});assert.ok(Array.isArray(page.uploads));if(readOnly)await call('opencut_upload_release',{upload_id:mainUpload},'NOT_AUTHORIZED');}
  const input={project_id:'a',name:`mcp-${mode}.png`,size_bytes:String(bytes.length),content_hash:hash,idempotency_key:`mcp-${mode}`};
  if(readOnly){await call('opencut_upload_begin',input,'NOT_AUTHORIZED');await call('opencut_upload_complete',{upload_id:mainUpload,content_hash:hash},'NOT_AUTHORIZED');}
  else{
   const upload=await call('opencut_upload_begin',input);
   const response=await fetch(`${base}/api/uploads/${upload.upload_id}/chunks/0?size_bytes=${bytes.length}&content_hash=${hash}`,{method:'PUT',headers:{Authorization:'Bearer '+token,'Content-Type':'application/octet-stream'},body:bytes});
   assert.equal(response.status,200);assert.equal((await response.json()).data.next_offset,String(bytes.length));
   const sealed=await call('opencut_upload_complete', { upload_id: upload.upload_id, content_hash: upload.content_hash });
   assert.equal((await call('opencut_upload_complete', { upload_id: upload.upload_id, content_hash: upload.content_hash })).job_id,sealed.job_id);jobs.push(sealed.job_id);
   if(staging){
    const deadline=Date.now()+30000;
    while((await call('opencut_job_view',{job_id:sealed.job_id})).state!=='succeeded'){assert.ok(Date.now()<deadline);await new Promise(r=>setTimeout(r,50));}
    const released=await call('opencut_upload_release',{upload_id:upload.upload_id});assert.equal(released.state,'released');assert.equal(released.reserved_bytes,'0');
    assert.deepEqual(await call('opencut_upload_release',{upload_id:upload.upload_id}),released);
   }

   await call('opencut_upload_begin',{...input,project_id:'b',idempotency_key:'foreign'},'NOT_AUTHORIZED');
  }
  if(mode==='http')await transport.terminateSession();assert.deepEqual(errors,[]);checks.push(`${mode}: ${readOnly?'read-only cannot mutate':'create, data handoff, seal replay, foreign denial'}`);
 }finally{await client.close();}
}
console.log(JSON.stringify({status:'passed',jobs,checks},null,2));
