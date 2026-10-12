#!/usr/bin/env node
// External package consumer + authoritative HTTP host + separate CLI readback.
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, mkdtempSync, openSync, closeSync, readFileSync, writeFileSync } from "node:fs";
import net from "node:net";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";

const root = resolve(import.meta.dirname, "..");
const sdk = resolve(root, "packages/sdk-ts");
const binaryArg = process.argv.indexOf("--binary");
if (binaryArg !== -1 && !process.argv[binaryArg + 1]) throw new Error("--binary requires a path");
const binary = binaryArg === -1 ? resolve(root, "target/debug/opencut") : resolve(process.argv[binaryArg + 1]);
const evidence = mkdtempSync(resolve(tmpdir(), "opencut-sdk-"));
const sha = path => createHash("sha256").update(readFileSync(path)).digest("hex");
const run = (exe, args, cwd = root) => execFileSync(exe, args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
console.log(`SDK acceptance evidence: ${evidence}`);
if (binaryArg === -1) run("cargo", ["build", "-p", "opencut-cli"]);
run("npm", ["run", "build"], sdk);
run("node", ["scripts/generate.mjs", "--check", "--skip-build", "--binary", binary], sdk);
const packed = JSON.parse(run("npm", ["pack", "--ignore-scripts", "--json", "--pack-destination", evidence], sdk));
const tarball = resolve(evidence, packed[0].filename);
writeFileSync(resolve(evidence, "package.json"), JSON.stringify({ name: "opencut-independent-consumer", private: true, type: "module" }));
run("npm", ["install", "--ignore-scripts", "--no-audit", "--no-fund", tarball], evidence);
const fixture = resolve(evidence, "edit-journey.json");
copyFileSync(resolve(root, "packages/contract-fixtures/edit-journey.json"), fixture);
copyFileSync(resolve(sdk, "tests/live-consumer.mjs"), resolve(evidence, "consumer.mjs"));
writeFileSync(resolve(evidence, "consumer.ts"), `
import { OpenCutClient, jobResult, type Job, type ToolInput, type ToolOutput } from '@opencut/sdk';
const client = new OpenCutClient({ baseUrl: 'http://localhost', token: '' });
const input: ToolInput<'opencut_project_inspect'> = { project_id: 'demo' };
const result: Promise<ToolOutput<'opencut_project_inspect'>> = client.call('opencut_project_inspect', input);
void result;
const domainResult: Promise<ToolOutput<'opencut_project_inspect'>> = client.callDomain('project', 'inspect', input);
void domainResult;
declare const job: Job;
const media = jobResult(job, 'media.probe');
const codec: string = media.streams[0].codec_type;
void codec;
// @ts-expect-error known tool requires project_id
client.call('opencut_project_inspect', {});
// @ts-expect-error unknown tool cannot be invoked
client.call('opencut_unknown', {});
// @ts-expect-error domain action requires project_id
client.callDomain('project', 'inspect', {});
// @ts-expect-error unknown domain action cannot be invoked
client.callDomain('project', 'missing', {});
// @ts-expect-error job discriminant and input must agree
client.call('opencut_job_start', { kind: 'media.probe', idempotency_key: 'k', input: { project_id: 'p' } });
`);
run("node", [resolve(sdk, "node_modules/typescript/bin/tsc"), "--noEmit", "--strict", "--module", "NodeNext", "--target", "ES2022", "consumer.ts"], evidence);

const openapi = JSON.parse(readFileSync(resolve(root, "packages/contract-fixtures/openapi.generated.json"), "utf8"));
let refs = 0;
function checkRefs(value) {
  if (!value || typeof value !== "object") return;
  if (value.$ref) {
    assert.ok(value.$ref.startsWith("#/components/schemas/"));
    let node = openapi;
    for (const key of value.$ref.slice(2).split("/")) node = node?.[key.replaceAll("~1", "/").replaceAll("~0", "~")];
    assert.ok(node !== undefined, value.$ref); refs++;
  }
  for (const child of Object.values(value)) checkRefs(child);
}
checkRefs(openapi);
const schemas = JSON.parse(run(binary, ["system", "schema", "--json"])).data;
const domains = JSON.parse(run(binary, ["system", "domains", "--json"])).data;
const expectedPaths = [
  ...schemas.map(t => `/v1/tools/${t.name}`),
  ...domains.map(d => `/v1/tools/opencut_${d.domain}`),
  "/v1/domains", "/v1/domains/{domain}",
  "/v1/uploads/{upload_id}/chunks/{offset}",
  "/v1/stream-uploads/{upload_id}/chunks/{offset}",
  "/v1/projects/{project}/media/{media_id}/content",
  "/v1/project-v4-resources/content",
  "/v1/preview/v4/audio/stream",
  "/v1/artifacts/{artifact}",
  "/v1/changes/stream",
  "/v1/preview/stream",
];
assert.deepEqual(Object.keys(openapi.paths).sort(), expectedPaths.sort());
assert.equal(openapi.paths["/v1/stream-uploads/{upload_id}/chunks/{offset}"].put.operationId, "opencut_upload_stream_chunk");
assert.equal(openapi.paths["/v1/project-v4-resources/content"].get.operationId, "opencut_project_v4_resource_content");
assert.equal(openapi.paths["/v1/preview/v4/audio/stream"].post.operationId, "opencut_audio_preview_v4_stream");
assert.equal(openapi.paths["/v1/uploads/{upload_id}/chunks/{offset}"].put.operationId, "opencut_upload_chunk");
assert.equal(openapi.paths["/v1/uploads/{upload_id}/chunks/{offset}"].put.requestBody.content["application/octet-stream"].schema.format, "binary");
assert.equal(openapi.paths["/v1/projects/{project}/media/{media_id}/content"].get.operationId, "opencut_project_media_content");
assert.equal(openapi.paths["/v1/artifacts/{artifact}"].get.operationId, "opencut_artifact_download");
assert.equal(openapi.paths["/v1/changes/stream"].post.operationId, "opencut_changes_stream");
assert.deepEqual(openapi.paths["/v1/changes/stream"].post.responses["200"].content["text/event-stream"]["x-opencut-data-schema"],
  { $ref: "#/components/schemas/OpenCutEventStreamRecord" });
for (const domain of domains) {
  const path = openapi.paths[`/v1/tools/opencut_${domain.domain}`].post;
  assert.deepEqual(path["x-opencut-action-operations"], Object.fromEntries(domain.actions.map(a => [a.action, a.operation])));
  assert.equal(path.requestBody.content["application/json"].schema.oneOf.length, domain.actions.length);
}

const database = resolve(evidence, "editor.sqlite"), tokenFile = resolve(evidence, "token");
const seed = resolve(evidence, "seed.json");
writeFileSync(seed, JSON.stringify({ kind: "media.probe", idempotency_key: "sdk-cancelled", input: { input: resolve(evidence, "never-read") } }));
const queued = JSON.parse(run(binary, ["--database", database, "--json", "job", "start", "--input", seed])).data;
run(binary, ["--database", database, "--json", "job", "cancel", queued.job_id]);
const listener = net.createServer();
await new Promise(resolve => listener.listen(0, "127.0.0.1", resolve));
const port = listener.address().port;
await new Promise(resolve => listener.close(resolve));
const log = openSync(resolve(evidence, "host.log"), "w");
const host = spawn(binary, ["--database", database, "serve", "--listen", `127.0.0.1:${port}`, "--token-file", tokenFile], { stdio: ["ignore", log, log] });
closeSync(log);
try {
  const base = `http://127.0.0.1:${port}`;
  const deadline = Date.now() + 15_000;
  for (;;) {
    if (host.exitCode !== null) throw new Error("Host exited during startup");
    try { const response = await fetch(`${base}/v1/capabilities`); if (response.status === 403) break; }
    catch { /* wait for actual socket */ }
    if (Date.now() > deadline) throw new Error("Host startup timeout");
    await delay(50);
  }
  const report = resolve(evidence, "report.json");
  run("node", ["consumer.mjs", base, tokenFile, binary, database, fixture, queued.job_id, report], evidence);
  const result = JSON.parse(readFileSync(report, "utf8"));
  result.package = { name: packed[0].name, version: packed[0].version, sha256: sha(tarball) };
  result.binary_sha256 = sha(binary);
  result.node = process.version;
  result.platform = `${process.platform}/${process.arch}`;
  result.openapi = { endpoints: expectedPaths.length, checked_local_references: refs, sha256: sha(resolve(root, "packages/contract-fixtures/openapi.generated.json")) };
  result.checks.unshift("npm tarball installs in an external consumer with no runtime dependencies", "installed declaration files accept valid calls and reject invalid tool inputs", "OpenAPI control/data routes match binary and every embedded local ref resolves");
  writeFileSync(report, JSON.stringify(result, null, 2));
  console.log(JSON.stringify({ status: result.status, checks: result.checks.length, report }, null, 2));
} finally {
  host.kill("SIGTERM");
  await Promise.race([new Promise(resolve => host.once("exit", resolve)), delay(5_000)]);
  if (host.exitCode === null && host.signalCode === null) host.kill("SIGKILL");
}
