#!/usr/bin/env node
// Reuse one accepted transcription receipt; no model inference is repeated.
import assert from 'node:assert/strict';
import {execFileSync, spawn} from 'node:child_process';
import {createHash} from 'node:crypto';
import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {setTimeout as delay} from 'node:timers/promises';

function option(name) {
  const at = process.argv.indexOf(name);
  assert(at > 0 && process.argv[at + 1], `Missing ${name}`);
  return path.resolve(process.argv[at + 1]);
}
const binary = option('--binary');
const root = option('--artifacts-dir');
const mcpRoot = option('--mcp-sdk-root');
const database = path.join(root, 'workspace.sqlite');
const prior = JSON.parse(fs.readFileSync(path.join(root, 'report.json'), 'utf8'));
assert.equal(prior.status, 'passed');
const binaryHash = createHash('sha256').update(fs.readFileSync(binary)).digest('hex');
const sdkBuild = path.resolve(path.dirname(fileURLToPath(import.meta.url)),
  '../packages/sdk-ts/dist/index.js');
const {OpenCutClient, jobResult} = await import(pathToFileURL(sdkBuild).href);
const mcpClientDir = path.join(mcpRoot, 'node_modules/@modelcontextprotocol/sdk/dist/esm/client');
const {Client} = await import(pathToFileURL(path.join(mcpClientDir, 'index.js')).href);
const {StdioClientTransport} = await import(pathToFileURL(path.join(mcpClientDir, 'stdio.js')).href);
const {StreamableHTTPClientTransport} = await import(pathToFileURL(path.join(mcpClientDir, 'streamableHttp.js')).href);
const runnerFile = path.join(root, 'runner.secret');
const readerFile = path.join(root, 'reader.secret');
const ownerFile = path.join(root, 'clients-owner.secret');
const checks = [];
const runKey = Date.now().toString(36);
const evidence = {binary_sha256: binaryHash, journey_binary_sha256: prior.binary_sha256,
  accepted_job_id: prior.job_id, scope: 'Existing transcription receipt and this binary version only; no model inference repeated'};
let host;
let hostLog;
const childEnv = {...process.env};
delete childEnv.OPENCUT_WHISPER_MODEL;

function cli(name, input) {
  const raw = execFileSync(binary, ['--database', database, '--json', 'call', name, '--input', '-'],
    {input: JSON.stringify(input), encoding: 'utf8', timeout: 15000, env: childEnv});
  const response = JSON.parse(raw);
  assert.equal(response.error, null, JSON.stringify(response));
  return response.data;
}
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') return `{${Object.keys(value).sort().map(
    key => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}`;
  return JSON.stringify(value);
}
function token(file) { return fs.readFileSync(file, 'utf8').trim(); }
async function port() {
  const server = net.createServer();
  await new Promise((resolve, reject) => server.listen(0, '127.0.0.1', resolve).once('error', reject));
  const number = server.address().port;
  await new Promise(resolve => server.close(resolve));
  return number;
}
async function waitForHost(listen) {
  const deadline = Date.now() + 10000;
  while (Date.now() < deadline) {
    assert(host.exitCode === null && host.signalCode === null,
      'Temporary host exited during startup');
    if (fs.existsSync(ownerFile)) {
      const ready = await new Promise(resolve => {
        const socket = net.createConnection({host: '127.0.0.1', port: listen});
        socket.setTimeout(500);
        socket.once('connect', () => { socket.destroy(); resolve(true); });
        socket.once('error', () => { socket.destroy(); resolve(false); });
        socket.once('timeout', () => { socket.destroy(); resolve(false); });
      });
      if (ready) return;
    }
    await delay(50);
  }
  throw new Error('Temporary host did not start within 10 seconds');
}
async function mcp(mode, readOnly, base, request) {
  const credential = readOnly ? readerFile : runnerFile;
  const transport = mode === 'stdio'
    ? new StdioClientTransport({command: binary,
        args: ['--database', database, '--credential-file', credential, 'mcp', 'serve'],
        env: childEnv, stderr: 'pipe'})
    : new StreamableHTTPClientTransport(new URL(`${base}/mcp`),
        {requestInit: {headers: {Authorization: `Bearer ${token(credential)}`}}});
  const client = new Client({name: 'transcription-clients-acceptance', version: '1'});
  const errors = [];
  client.onerror = error => errors.push(String(error));
  try {
    await client.connect(transport, {timeout: 10000});
    const listed = await client.listTools(undefined, {timeout: 10000});
    const media = listed.tools.find(tool => tool.name === 'opencut_media');
    assert(media?.inputSchema, `${mode} media action was not discoverable`);
    const resource = await client.readResource({uri: 'opencut://schemas/domains/media'}, {timeout: 10000});
    const descriptor = JSON.parse(resource.contents[0].text).payload.data;
    const actions = Array.isArray(descriptor) ? descriptor.flatMap(item => item.actions ?? []) : descriptor.actions;
    const action = actions.find(item => item.action === "analysis.asset");
    assert(action?.input_schema && action?.output_schema, `${mode} media schema lacks analysis.transcribe`);
    const result = await client.callTool({name: 'opencut_media', arguments: {
      action: "analysis.asset", input: request}}, undefined, {timeout: 10000});
    assert(result.structuredContent, JSON.stringify(result));
    if (readOnly) {
      assert.equal(result.isError, true);
      assert.equal(result.structuredContent.error.code, 'NOT_AUTHORIZED');
    } else {
      assert.equal(result.structuredContent.error, null);
      assert.equal(result.structuredContent.data.job_id, prior.job_id);
    }
    if (mode === 'http') await transport.terminateSession();
    assert.deepEqual(errors, []);
    checks.push(`Official MCP ${mode} ${readOnly ? 'read-only denial' : 'media schema discovery and exact replay'}`);
  } finally {
    await client.close();
  }
}

try {
  assert(fs.existsSync(database) && fs.existsSync(runnerFile) && fs.existsSync(readerFile));
  const existing = cli('opencut_job_view', {job_id: prior.job_id});
  assert.equal(existing.kind, "analysis.asset");
  assert.equal(existing.state, 'succeeded');
  const request = {idempotency_key: 'transcribe-fixed-speech',
    project_id: existing.target.project_id, revision: existing.target.revision,
    asset_id: 'speech', language: 'zh'};
  const baseline = jobResult(existing, "analysis.asset");
  const baselineHash = createHash('sha256').update(canonical(baseline)).digest('hex');
  assert.equal(baselineHash, prior.transcription_sha256);
  evidence.transcription_sha256 = baselineHash;

  const queued = cli('opencut_analysis_transcribe', {...request,
    idempotency_key: `clients-queued-cancel-${runKey}`});
  assert.equal(queued.state, 'queued');
  const cancelled = cli('opencut_job_request_cancel', {job_id: queued.job_id});
  assert.equal(cancelled.state, 'cancelled');
  assert.equal(cli('opencut_job_view', {job_id: queued.job_id}).state, 'cancelled');
  evidence.cancelled_job_id = queued.job_id;
  checks.push('Owner CLI cancels a new queued transcription before any worker can claim it');

  const listen = await port();
  const base = `http://127.0.0.1:${listen}`;
  hostLog = fs.openSync(path.join(root, 'clients-host.log'), 'w');
  host = spawn(binary, ['--database', database, 'serve', '--listen', `127.0.0.1:${listen}`,
    '--token-file', ownerFile], {env: childEnv, stdio: ['ignore', hostLog, hostLog]});
  fs.closeSync(hostLog); hostLog = undefined;
  await waitForHost(listen);
  const sdk = new OpenCutClient({baseUrl: base, token: token(runnerFile)});
  const replay = await sdk.callDomain('media', "analysis.asset", request);
  assert.equal(replay.job_id, prior.job_id);
  const publicView = await sdk.call('opencut_job_view', {job_id: prior.job_id});
  assert.deepEqual(jobResult(publicView, "analysis.asset"), baseline);
  evidence.sdk_replay_job_id = replay.job_id;
  checks.push('Built TS SDK domain replay and jobResult preserve the accepted fixed transcript');

  const missing = await sdk.callDomain('media', "analysis.asset", {...request,
    idempotency_key: `clients-no-model-${runKey}`});
  assert.notEqual(missing.job_id, prior.job_id);
  let failed;
  const deadline = Date.now() + 45000;
  do {
    failed = await sdk.call('opencut_job_view', {job_id: missing.job_id});
    if (failed.state !== 'queued' && failed.state !== 'running') break;
    assert(Date.now() < deadline, 'No-model job did not finish within 45 seconds');
    await delay(100);
  } while (true);
  assert.equal(failed.state, 'failed');
  assert.equal(failed.failure.code, 'CAPABILITY_UNAVAILABLE');
  evidence.no_model_job_id = missing.job_id;
  checks.push('A new task on a host without OPENCUT_WHISPER_MODEL fails publicly with CAPABILITY_UNAVAILABLE');

  for (const mode of ['stdio', 'http']) {
    await mcp(mode, false, base, request);
    await mcp(mode, true, base, request);
  }
  const report = {status: 'passed', checks, evidence: root, ...evidence};
  fs.writeFileSync(path.join(root, 'clients-report.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify({status: 'passed', checks: checks.length,
    report: path.join(root, 'clients-report.json')}));
} catch (error) {
  fs.writeFileSync(path.join(root, 'clients-failure.json'), JSON.stringify({
    error: String(error), checks, ...evidence}, null, 2) + '\n');
  throw error;
} finally {
  if (host && host.exitCode === null) {
    host.kill('SIGINT');
    for (let attempt = 0; attempt < 120 && host.exitCode === null; attempt++) await delay(50);
    if (host.exitCode === null) host.kill('SIGKILL');
  }
  if (hostLog !== undefined) fs.closeSync(hostLog);
}
