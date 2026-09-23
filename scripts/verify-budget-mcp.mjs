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
 const client=new Client({name:'budget-qa',version:'1'});
 try {
  await client.connect(transport);const list=await client.listTools();
  const domain=list.tools.find(t=>t.name==='opencut_workspace');assert.ok(domain);assert.equal(domain.annotations.readOnlyHint,false);
  const call=async(suffix,args={})=>await client.callTool({name:'opencut_storage_budget_'+suffix,arguments:args});
  if(restricted){for(const suffix of ['inspect','recover']){const result=await call(suffix);assert.equal(result.isError,true);assert.equal(result.structuredContent.error.code,'NOT_AUTHORIZED');}
   const result=await call('configure',{expected_revision:0,idempotency_key:'refused',payload_limit_bytes:'1',minimum_available_bytes:'0'});assert.equal(result.structuredContent.error.code,'NOT_AUTHORIZED');
  }else{
   const result=await call('inspect');assert.ok(!result.isError);const old=result.structuredContent.data.policy;
   const input={expected_revision:old.revision,idempotency_key:'mcp-'+mode,payload_limit_bytes:old.payload_limit_bytes,minimum_available_bytes:old.minimum_available_bytes};
   const changed=await call('configure',input);assert.ok(!changed.isError);assert.equal(changed.structuredContent.data.actor.source,mode==='http'?'mcp_http':'mcp_stdio');assert.equal(changed.structuredContent.data.policy.revision,old.revision+1);assert.deepEqual((await call('configure',input)).structuredContent.data,changed.structuredContent.data);
   const recovered=await call('recover');assert.ok(!recovered.isError);assert.equal(recovered.structuredContent.data.reserved_bytes,'0');assert.equal(recovered.structuredContent.data.enforces_all_workspace_writes,false);
  }
  if(mode==='http')await transport.terminateSession();checks.push(`${mode} ${restricted?'scoped refusal':'owner policy replay and recovery'}`);
 }finally{await client.close();}
}
console.log(JSON.stringify({status:'passed',checks},null,2));
