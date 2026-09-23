// Independent official MCP TypeScript SDK consumer, installed outside this repo.
// node scripts/verify-mcp-client.mjs --binary <opencut> --sdk-root <npm prefix>
import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, writeFileSync, appendFileSync, openSync, closeSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import { pathToFileURL } from "node:url";
import { setTimeout as delay } from "node:timers/promises";
const args = process.argv.slice(2);
const binary = resolve(args[args.indexOf("--binary") + 1]);
const sdk = resolve(args[args.indexOf("--sdk-root") + 1], "node_modules/@modelcontextprotocol/sdk");
const { Client } = await import(pathToFileURL(join(sdk, "dist/esm/client/index.js")));
const { StreamableHTTPClientTransport } = await import(pathToFileURL(join(sdk, "dist/esm/client/streamableHttp.js")));
const { ResourceUpdatedNotificationSchema } = await import(pathToFileURL(join(sdk, "dist/esm/types.js")));
const root = mkdtempSync(join(tmpdir(), "opencut-mcp-interop-"));
const listener = createServer();
await new Promise(r => listener.listen(0, "127.0.0.1", r));
const port = listener.address().port;
await new Promise(r => listener.close(r));
const db = join(root, "workspace.sqlite"), tokenFile = join(root, "token");
const fd = openSync(join(root, "host.log"), "w");
const host = spawn(binary, ["--database", db, "serve", "--listen", `127.0.0.1:${port}`, "--token-file", tokenFile], { stdio: ["ignore", fd, fd] });
closeSync(fd);
let transport, client;
const checks = [], errors = [];
try {
  let token;
  for (let i = 0; i < 200; i++) {
    assert.equal(host.exitCode, null, readFileSync(join(root, "host.log"), "utf8"));
    try {
      token = readFileSync(tokenFile, "utf8").trim();
      await fetch(`http://127.0.0.1:${port}/mcp`, { headers: { Authorization: `Bearer ${token}` } });
      break;
    } catch { await delay(50); }
  }
  assert(token);
  let severNext = false, severed = false, resumed = false, resumeCount = 0;
  const fetchWithDisconnect = async (url, init) => {
    if (new Headers(init?.headers).has("last-event-id")) { resumed = true; resumeCount++; }
    const response = await fetch(url, init);
    const cursor = new Headers(init?.headers).get("last-event-id");
    appendFileSync(join(root, "transport.jsonl"), JSON.stringify({ time: Date.now(), method: init?.method, cursor, status: response.status }) + "\n");
    if (cursor) appendFileSync(join(root, "transport.jsonl"), JSON.stringify({ cursor, replay: await response.clone().text() }) + "\n");
    if (!severNext || init?.method !== "POST" || !String(init.body).includes('"tools/call"')) return response;
    severNext = false;
    // Deliver only the real priming event, cancelling the HTTP response before
    // the SDK sees a business result. Its own reconnection code must recover.
    const reader = response.body.getReader();
    let text = "", decoder = new TextDecoder();
    while (!text.includes("\n\n")) {
      const { value, done } = await reader.read();
      assert(!done); text += decoder.decode(value, { stream: true });
    }
    const first = text.slice(0, text.indexOf("\n\n") + 2);
    assert.match(first, /id: mcp-event:/);
    assert(!first.includes('"result"'));
    await reader.cancel(); severed = true;
    return new Response(new TextEncoder().encode(first), { status: response.status, headers: response.headers });
  };
  transport = new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${port}/mcp`), {
    requestInit: { headers: { Authorization: `Bearer ${token}` } }, fetch: fetchWithDisconnect,
  });
  client = new Client({ name: "opencut-independent-acceptance", version: "1" });
  client.onerror = e => errors.push(String(e));
  await client.connect(transport);
  assert(transport.sessionId);
  assert.equal(transport.protocolVersion, "2025-11-25");
  assert((await client.listTools()).tools.some(t => t.name === "opencut_observe"));
  assert.equal((await client.listResources()).resources.length, 4);
  assert.equal((await client.listResourceTemplates()).resourceTemplates.length, 7);
  assert.equal((await client.listPrompts()).prompts.length, 3);
  checks.push("Official SDK negotiates a session and discovers tools/resources/templates/prompts");
  // Force several genuinely empty replays before the write can commit. This
  // catches clients losing their cursor when an empty finite SSE stream closes.
  const lock = spawn("python3", ["-u", "-c", "import sqlite3,sys,time; c=sqlite3.connect(sys.argv[1]); c.execute('BEGIN IMMEDIATE'); print('locked',flush=True); time.sleep(3.2); c.commit()", db]);
  await new Promise((yes, no) => { lock.stdout.once("data", yes); lock.once("error", no); lock.once("exit", code => { if (code) no(new Error(`writer lock exited ${code}`)); }); });
  severNext = true;
  const created = await client.callTool({ name: "opencut_project_create", arguments: {
    project: { id: "sdk-interop", name: "SDK interop", sequence_id: "main", width: 64, height: 48, fps: { numerator: 30, denominator: 1 } },
    idempotency_key: "sdk-interop-create",
  } }, undefined, { timeout: 15000 });
  assert(!created.isError, JSON.stringify(created));
  assert(severed && resumed && resumeCount >= 2);
  const cli = (name, input) => JSON.parse(execFileSync(binary, ["--database", db, "--json", "call", name, "--input", "-"], { input: JSON.stringify(input), encoding: "utf8" })).data;
  const current = cli("opencut_project_inspect", { project_id: "sdk-interop" });
  const compact = await client.callTool({ name: "opencut_project", arguments: {
    action: "inspect", input: { project_id: "sdk-interop" },
  } });
  assert(!compact.isError, JSON.stringify(compact));
  assert.deepEqual(compact.structuredContent?.data, current);
  checks.push("Official SDK invokes a compact domain action with the same result as independent CLI readback");
  checks.push("Official SDK resumes a severed POST across empty replays while a real SQLite writer lock delays commit; CLI confirms persistence");
  const uri = "opencut://projects/sdk-interop/branches/main/summary";
  const resource = await client.readResource({ uri });
  assert.equal(JSON.parse(resource.contents[0].text).payload.data.revision, current.revision);
  const prompt = await client.getPrompt({ name: "caption_review", arguments: { project_id: "sdk-interop", revision: current.revision, sequence_id: "main" } });
  assert(prompt.messages.length > 0);
  let notified = false;
  client.setNotificationHandler(ResourceUpdatedNotificationSchema, notification => { if (notification.params.uri === uri) notified = true; });
  await client.subscribeResource({ uri });
  cli("opencut_edit_apply", { project_id: "sdk-interop", request: { expected_revision: current.revision, idempotency_key: "interop-edit", label: "SDK observation", operations: [{ type: "track.create", sequence_id: "main", track: { id: "video", name: "Video", kind: "video", enabled: true, muted: false, locked: false, clips: [], params: {} } }] } });
  for (let i = 0; i < 100 && !notified; i++) await delay(50);
  assert(notified);
  await client.unsubscribeResource({ uri });
  checks.push("Official SDK reads fixed context/prompts and receives a resource update caused by an independent CLI edit");
  const session = transport.sessionId;
  await transport.terminateSession();
  const closed = await fetch(`http://127.0.0.1:${port}/mcp`, { headers: { Authorization: `Bearer ${token}`, "Mcp-Session-Id": session, "Mcp-Protocol-Version": "2025-11-25", Accept: "text/event-stream" } });
  assert.equal(closed.status, 404);
  checks.push("Official SDK DELETE invalidates its session");
  assert.deepEqual(errors, [], "Official SDK must complete without transport or parse errors");
  const report = { status: "passed", sdk_version: JSON.parse(readFileSync(join(sdk, "package.json"), "utf8")).version, binary_sha256: createHash("sha256").update(readFileSync(binary)).digest("hex"), checks, errors, evidence: root };
  writeFileSync(join(root, "report.json"), JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
} finally {
  await client?.close();
  host.kill("SIGINT");
  for (let i = 0; i < 140 && host.exitCode === null; i++) await delay(50);
  if (host.exitCode === null) host.kill("SIGKILL");
}
