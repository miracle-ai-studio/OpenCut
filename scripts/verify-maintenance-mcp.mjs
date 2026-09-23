// Independent official MCP SDK consumers of publication query/reconciliation.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
const [sdkRoot,binary,database,base,ownerFile,scopedFile,hash,expectedFile,family]=process.argv.slice(2);
const directory=path.join(sdkRoot,'node_modules/@modelcontextprotocol/sdk/dist/esm/client');
const {Client}=await import(pathToFileURL(path.join(directory,'index.js')).href);
const {StdioClientTransport}=await import(pathToFileURL(path.join(directory,'stdio.js')).href);
const {StreamableHTTPClientTransport}=await import(pathToFileURL(path.join(directory,'streamableHttp.js')).href);
const queryTool=family==='plugin'?'opencut_plugin_publication_query':'opencut_maintenance_query';
const reconcileTool=family==='plugin'?'opencut_plugin_publication_reconcile':'opencut_maintenance_reconcile';
const object=family==='plugin'?JSON.parse(hash):{kind:'media_metadata',content_hash:hash};
const expected=JSON.parse(fs.readFileSync(expectedFile,'utf8'));const checks=[];
for(const transportName of ['stdio','http'])for(const scoped of [false,true]){
 const transport=transportName==='stdio'
  ?new StdioClientTransport({command:binary,args:['--database',database,...(scoped?['--credential-file',scopedFile]:[]),'mcp','serve'],stderr:'pipe'})
  :new StreamableHTTPClientTransport(new URL(base+'/mcp'),{requestInit:{headers:{Authorization:'Bearer '+fs.readFileSync(scoped?scopedFile:ownerFile,'utf8').trim()}}});
 const client=new Client({name:'publication-qa',version:'1'});const errors=[];client.onerror=e=>errors.push(String(e));
 try{
  await client.connect(transport);
  const read=await client.callTool({name:queryTool,arguments:{target:{kind:'workspace'},limit:100}});
  const reconcile=await client.callTool({name:reconcileTool,arguments:{object}});
  if(scoped){
   for(const result of [read,reconcile]){assert.equal(result.isError,true);assert.equal(result.structuredContent.error.code,'NOT_AUTHORIZED');}
  }else{
   assert.equal(read.structuredContent.error,null);assert.deepEqual(read.structuredContent.data.events,expected);
   assert.equal(reconcile.structuredContent.error,null);assert.deepEqual(reconcile.structuredContent.data.resolved_publication_ids,[]);
  }
  if(transportName==='http')await transport.terminateSession();
  assert.deepEqual(errors,[]);checks.push(`${transportName}: ${scoped?'scoped denial':'owner exact history and locked no-op reconciliation'}`);
 }finally{await client.close();}
}
console.log(JSON.stringify({status:'passed',checks},null,2));
