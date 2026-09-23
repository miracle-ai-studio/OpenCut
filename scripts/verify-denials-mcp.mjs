import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import {execFileSync} from 'node:child_process';
const [sdk,binary,database,base,agent,owner]=process.argv.slice(2);
const dir=path.join(sdk,'node_modules/@modelcontextprotocol/sdk/dist/esm/client');
const {Client}=await import(pathToFileURL(path.join(dir,'index.js')));
const {StdioClientTransport}=await import(pathToFileURL(path.join(dir,'stdio.js')));
const {StreamableHTTPClientTransport}=await import(pathToFileURL(path.join(dir,'streamableHttp.js')));
const {ResourceUpdatedNotificationSchema}=await import(pathToFileURL(path.join(dir,'../types.js')));
const checks=[];
async function query(id){const r=await fetch(base+'/v1/tools/opencut_denial_query',{method:'POST',headers:{Authorization:'Bearer '+fs.readFileSync(owner,'utf8').trim(),'Content-Type':'application/json'},body:JSON.stringify({target:{kind:'request',id}})});assert.equal(r.status,200);return (await r.json()).data.records;}
for(const mode of ['stdio','http']){
 const credential=mode==='stdio'?path.join(path.dirname(owner),'mcp-revocable.secret'):agent;
 let credentialId;
 if(mode==='stdio')credentialId=JSON.parse(execFileSync(binary,['--database',database,'--json','access','issue','agent','--output',credential],{encoding:'utf8'})).data.credential_id;
 const transport=mode==='stdio'?new StdioClientTransport({command:binary,args:['--database',database,'--credential-file',credential,'mcp','serve'],stderr:'pipe'}):new StreamableHTTPClientTransport(new URL(base+'/mcp'),{requestInit:{headers:{Authorization:'Bearer '+fs.readFileSync(agent,'utf8').trim()}}});
 const client=new Client({name:'denial-audit-qa',version:'1'});let updates=0;client.setNotificationHandler(ResourceUpdatedNotificationSchema,()=>{updates++;});
 try{
  await client.connect(transport);
  const denied=await client.callTool({name:'opencut_denial_query',arguments:{target:{kind:'workspace'}}});assert.equal(denied.isError,true);assert.equal(denied.structuredContent.error.code,'NOT_AUTHORIZED');
  let rows=await query(denied.structuredContent.request_id);assert.equal(rows.length,1);assert.equal(rows[0].source,mode==='stdio'?'mcp_stdio':'mcp_http');
  let resourceError;try{await client.readResource({uri:'opencut://projects/b/branches/main/summary'});}catch(e){resourceError=e;}
  assert.equal(resourceError?.data?.code,'NOT_AUTHORIZED');rows=await query(resourceError.data.request_id);assert.equal(rows.length,1);assert.equal(rows[0].source,mode==='stdio'?'mcp_stdio':'mcp_http');assert.equal(rows[0].operation.name,'opencut_resource_read');
  if(mode==='stdio'){
   await client.subscribeResource({uri:'opencut://projects/a/branches/main/summary'});
   execFileSync(binary,['--database',database,'--json','access','revoke',credentialId]);
   let metadataError;try{await client.listTools();}catch(e){metadataError=e;}
   assert.equal(metadataError?.data?.code,'NOT_AUTHORIZED');assert.equal((await query(metadataError.data.request_id)).length,1);
   const readCredential=async()=>{const response=await fetch(base+'/v1/tools/opencut_denial_query',{method:'POST',headers:{Authorization:'Bearer '+fs.readFileSync(owner,'utf8').trim(),'Content-Type':'application/json'},body:JSON.stringify({target:{kind:'credential',id:credentialId}})});return (await response.json()).data.records.filter(r=>r.operation.name==='opencut_resource_read');};
   const deadline=Date.now()+5000;while(updates===0){assert.ok(Date.now()<deadline);await new Promise(r=>setTimeout(r,50));}
   const before=(await readCredential()).length;await new Promise(r=>setTimeout(r,800));assert.equal(before,2);assert.equal((await readCredential()).length,before);assert.equal(updates,1);
  }
  if(mode==='http')await transport.terminateSession();checks.push(`${mode}: tool and resource denials correlate without transport/source confusion`);
 }finally{await client.close();}
}
console.log(JSON.stringify({status:'passed',checks},null,2));
