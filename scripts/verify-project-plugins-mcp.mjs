import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
const [sdk,binary,database,base,agent,reader,input]=process.argv.slice(2);
const dir=path.join(sdk,'node_modules/@modelcontextprotocol/sdk/dist/esm/client');
const {Client}=await import(pathToFileURL(path.join(dir,'index.js')));
const {StdioClientTransport}=await import(pathToFileURL(path.join(dir,'stdio.js')));
const {StreamableHTTPClientTransport}=await import(pathToFileURL(path.join(dir,'streamableHttp.js')));
const args=JSON.parse(fs.readFileSync(input,'utf8')),checks=[];
for(const mode of ['stdio','http'])for(const readonly of [false,true]){
 const credential=readonly?reader:agent;
 const transport=mode==='stdio'?new StdioClientTransport({command:binary,args:['--database',database,'--credential-file',credential,'mcp','serve'],stderr:'pipe'}):new StreamableHTTPClientTransport(new URL(base+'/mcp'),{requestInit:{headers:{Authorization:'Bearer '+fs.readFileSync(credential,'utf8').trim()}}});
 const client=new Client({name:'project-plugin-acceptance',version:'1'});
 try{
  await client.connect(transport);
  assert.ok((await client.listTools()).tools.some(t=>t.name==='opencut_plugin'));
  const inspected=await client.callTool({name:'opencut_project_plugin_inspect',arguments:args.inspection});assert.equal(inspected.structuredContent.error,null);assert.deepEqual(inspected.structuredContent.data.input_schema,{type:'object'});
  const planned=await client.callTool({name:'opencut_project_plugin_plan',arguments:args.plan});
  if(readonly){assert.equal(planned.isError,true);assert.equal(planned.structuredContent.error.code,'NOT_AUTHORIZED');}
  else{assert.equal(planned.structuredContent.error,null);assert.equal(planned.structuredContent.data.result.operations[0].track.id,'plugin-track');}
  if(mode==='http')await transport.terminateSession();checks.push(`${mode}: ${readonly?'inspect with plan denied':'inspect and real WASM plan'}`);
 }finally{await client.close();}
}
console.log(JSON.stringify({status:'passed',checks},null,2));
