import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {randomUUID} from 'node:crypto';
import {pathToFileURL} from 'node:url';
const [sdk,binary,database,base,agent,owner]=process.argv.slice(2);
const dir=path.join(sdk,'node_modules/@modelcontextprotocol/sdk/dist/esm/client');
const {Client}=await import(pathToFileURL(path.join(dir,'index.js')));
const {StdioClientTransport}=await import(pathToFileURL(path.join(dir,'stdio.js')));
const {StreamableHTTPClientTransport}=await import(pathToFileURL(path.join(dir,'streamableHttp.js')));
const checks=[];
for(const mode of ['stdio','http']) for(const restricted of [false,true]) {
 const transport=mode==='stdio'?new StdioClientTransport({command:binary,args:['--database',database,...(restricted?['--credential-file',agent]:[]),'mcp','serve'],stderr:'pipe'}):new StreamableHTTPClientTransport(new URL(base+'/mcp'),{requestInit:{headers:{Authorization:'Bearer '+fs.readFileSync(restricted?agent:owner,'utf8').trim()}}});
 const client=new Client({name:'cas-collection-qa',version:'1'});
 try {
  await client.connect(transport);
  const tools=await client.listTools();assert.equal(tools.tools.find(t=>t.name==='opencut_workspace').annotations.readOnlyHint,false);
  const capture=path.join(path.dirname(database),'asset-store','.capture-'+randomUUID());
  if(!restricted)fs.writeFileSync(capture,'abandoned fixture');
  const response=await client.callTool({name:'opencut_cas_gc_plan',arguments:{idempotency_key:`${mode}-${restricted}`}});
  if(restricted){assert.equal(response.isError,true);assert.equal(response.structuredContent.error.code,'NOT_AUTHORIZED');}
  else {
   const plan=response.structuredContent.data;assert.equal(plan.state,'planned');assert.equal(plan.items.length,1);assert.equal(plan.items[0].object.kind,'capture');
   const listed=await client.callTool({name:'opencut_cas_gc_query',arguments:{}});assert.ok(listed.structuredContent.data.plans.some(p=>p.plan_id===plan.plan_id));
   const seen=await client.callTool({name:'opencut_cas_gc_inspect',arguments:{plan_id:plan.plan_id}});assert.deepEqual(seen.structuredContent.data,plan);
   const result=await client.callTool({name:'opencut_cas_gc_execute',arguments:{plan_id:plan.plan_id,expected_plan_hash:plan.plan_hash}});assert.equal(result.structuredContent.data.state,'complete');assert.equal(result.structuredContent.data.items[0].state,'removed');assert.ok(!fs.existsSync(capture));
  }
  if(mode==='http') await transport.terminateSession();checks.push(`${mode} ${restricted?'restricted refusal':'reviewed capture deletion'}`);
 }finally{await client.close();}
}
console.log(JSON.stringify({status:'passed',checks},null,2));
