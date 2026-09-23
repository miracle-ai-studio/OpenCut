// Consume installed SDKs against an already-running, isolated scoped host.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import {createHash} from 'node:crypto';
const [base, tokenPath, readerPath, referencePath, sdkRoot, mcpRoot] = process.argv.slice(2);
const token=fs.readFileSync(tokenPath,'utf8').trim();
const readerToken=fs.readFileSync(readerPath,'utf8').trim();
const reference=JSON.parse(fs.readFileSync(referencePath,'utf8'));
const checks=[];
if (sdkRoot && sdkRoot !== '-') {
  const {OpenCutClient,OpenCutApiError}=await import(pathToFileURL(path.join(sdkRoot,'node_modules/@opencut/sdk/dist/index.js')).href);
  const client=new OpenCutClient({baseUrl:base,token});
  assert.equal((await client.call('opencut_access_inspect',{})).principal_id,'editor-a');
  assert.deepEqual((await client.call('opencut_project_list',{})).map(p=>p.project.id),['a']);
  await assert.rejects(client.call('opencut_project_inspect',{project_id:'b'}),e=>e instanceof OpenCutApiError && e.code==='NOT_AUTHORIZED' && e.status===403);
  const response=await client.downloadArtifact(reference);
  const bytes=Buffer.from(await response.arrayBuffer());
  assert.equal(createHash('sha256').update(bytes).digest('hex'),reference.content_hash);
  const restricted=new OpenCutClient({baseUrl:base,token:readerToken});
  await assert.rejects(restricted.downloadArtifact(reference),e=>e instanceof OpenCutApiError && e.code==='NOT_AUTHORIZED' && e.status===403);
  checks.push('Installed OpenCut SDK retains scoped identity, typed rejection and verified artifact bytes');
}
if (mcpRoot && mcpRoot !== '-') {
  const directory=path.join(mcpRoot,'node_modules/@modelcontextprotocol/sdk/dist/esm/client');
  const {Client}=await import(pathToFileURL(path.join(directory,'index.js')).href);
  const {StreamableHTTPClientTransport}=await import(pathToFileURL(path.join(directory,'streamableHttp.js')).href);
  const client=new Client({name:'scoped-client-acceptance',version:'1'});
  const transport=new StreamableHTTPClientTransport(new URL(base+'/mcp'),{requestInit:{headers:{Authorization:'Bearer '+token}}});
  const errors=[];client.onerror=error=>errors.push(String(error));
  try {
    await client.connect(transport);
    const result=await client.callTool({name:'opencut_project_list',arguments:{}});
    assert.deepEqual(result.structuredContent.data.map(p=>p.project.id),['a']);
    const denied=await client.callTool({name:'opencut_project_inspect',arguments:{project_id:'b'}});
    assert.equal(denied.isError,true);assert.equal(denied.structuredContent.error.code,'NOT_AUTHORIZED');
    await client.readResource({uri:'opencut://projects/a/branches/main/summary'});
    await assert.rejects(client.readResource({uri:'opencut://projects/b/branches/main/summary'}),e=>e.data?.code==='NOT_AUTHORIZED');
    await transport.terminateSession();
    assert.deepEqual(errors,[]);
  } finally {await client.close();}
  checks.push('Official MCP SDK negotiates a scoped HTTP session, reads authorized context, rejects foreign resources and terminates its own session');
}
assert.ok(checks.length);
console.log(JSON.stringify({status:'passed',checks},null,2));
