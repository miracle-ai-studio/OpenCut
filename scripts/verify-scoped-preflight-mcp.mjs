import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
const [sdkRoot,binary,database,base,runner,reader,caseFile]=process.argv.slice(2);
const directory=path.join(sdkRoot,'node_modules/@modelcontextprotocol/sdk/dist/esm/client');
const {Client}=await import(pathToFileURL(path.join(directory,'index.js')).href);
const {StdioClientTransport}=await import(pathToFileURL(path.join(directory,'stdio.js')).href);
const {StreamableHTTPClientTransport}=await import(pathToFileURL(path.join(directory,'streamableHttp.js')).href);
const fixture=JSON.parse(fs.readFileSync(caseFile,'utf8')),checks=[];
for(const transportName of ['stdio','http'])for(const readonly of [false,true]){
 const credential=readonly?reader:runner;
 const transport=transportName==='stdio'?new StdioClientTransport({command:binary,args:['--database',database,'--credential-file',credential,'mcp','serve'],stderr:'pipe'}):new StreamableHTTPClientTransport(new URL(base+'/mcp'),{requestInit:{headers:{Authorization:'Bearer '+fs.readFileSync(credential,'utf8').trim()}}});
 const client=new Client({name:'scoped-preflight-qa',version:'1'});const errors=[];client.onerror=e=>errors.push(String(e));
 try{
  await client.connect(transport);
  for(const kind of ['healthy','damaged','legacy','foreign']){
   const result=await client.callTool({name:'opencut_render_preflight',arguments:fixture[kind]});
   if(readonly||kind==='foreign'){assert.equal(result.isError,true);assert.equal(result.structuredContent.error.code,'NOT_AUTHORIZED');continue;}
   assert.equal(result.structuredContent.error,null);const data=result.structuredContent.data;
   if(kind==='healthy')assert.deepEqual(data,fixture.expected);
   else{assert.equal(data.ready,false);assert.ok(data.issues.length>0);if(kind==='legacy')assert.equal(data.issues[0].code,'ASSET_LOCK_REQUIRED');}
   assert.equal(JSON.stringify(data).includes(fixture.root),false);
  }
  if(transportName==='http')await transport.terminateSession();assert.deepEqual(errors,[]);checks.push(`${transportName}: ${readonly?'read-only denial':'fixed revision, diagnostic isolation, cross-project denial'}`);
 }finally{await client.close();}
}
console.log(JSON.stringify({status:'passed',checks},null,2));
