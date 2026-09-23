import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
const [sdk,binary,database,base,agent,owner]=process.argv.slice(2);
const dir=path.join(sdk,'node_modules/@modelcontextprotocol/sdk/dist/esm/client');
const {Client}=await import(pathToFileURL(path.join(dir,'index.js')));
const {StdioClientTransport}=await import(pathToFileURL(path.join(dir,'stdio.js')));
const {StreamableHTTPClientTransport}=await import(pathToFileURL(path.join(dir,'streamableHttp.js')));
const checks=[];
for(const mode of ['stdio','http']) for(const restricted of [false,true]) {
 const secret=restricted?agent:owner;
 const transport=mode==='stdio'?new StdioClientTransport({command:binary,args:['--database',database,...(restricted?['--credential-file',agent]:[]),'mcp','serve'],stderr:'pipe'}):new StreamableHTTPClientTransport(new URL(base+'/mcp'),{requestInit:{headers:{Authorization:'Bearer '+fs.readFileSync(secret,'utf8').trim()}}});
 const client=new Client({name:'storage-qa',version:'1'});
 try {
  await client.connect(transport);
  const list=await client.listTools();
  const tool=list.tools.find(t=>t.name==='opencut_workspace');assert.ok(tool);assert.equal(tool.annotations.readOnlyHint,false);
  const response=await client.callTool({name:'opencut_storage_inspect',arguments:{max_entries:1}});
  if(restricted){assert.equal(response.isError,true);assert.equal(response.structuredContent.error.code,'NOT_AUTHORIZED');}
  else {assert.ok(!response.isError);assert.equal(response.structuredContent.data.consistency,'live_non_atomic');assert.equal(response.structuredContent.data.stop_reason,'entry_limit');assert.equal(response.structuredContent.data.complete,false);assert.equal(response.structuredContent.data.visited_entries,1);}
  if(mode==='http') await transport.terminateSession();
  checks.push(`${mode} ${restricted?'restricted refusal':'owner partial observation'}`);
 } finally {await client.close();}
}
console.log(JSON.stringify({status:'passed',checks},null,2));
