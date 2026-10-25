/** Accept one real MCP task, then close the real Web and MCP clients. */
import { chromium, expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { execFileSync, spawn } from "node:child_process";
import { createInterface } from "node:readline";
const args = Object.fromEntries(
  process.argv
    .slice(2)
    .reduce(
      (pairs, value, i, all) =>
        i % 2 ? pairs : [...pairs, [value.replace(/^--/, ""), all[i + 1]]],
      [],
    ),
);
const { binary, workspace, endpoint, progress } = args;
const request = JSON.parse(await fs.readFile(args.request, "utf8"));
let state;
try {
  state = JSON.parse(await fs.readFile(progress, "utf8"));
} catch (error) {
  if (error.code !== "ENOENT") throw error;
  state = { request };
}
expect(state.request).toEqual(request);
const save = async () => {
  await fs.writeFile(
    progress + ".pending",
    JSON.stringify(state, null, 2) + "\n",
  );
  await fs.rename(progress + ".pending", progress);
};
if (state.clients_closed) {
  console.log("Reuse original client-detachment receipt");
  process.exit(0);
}
const cli = (tool, input) => {
  const result = JSON.parse(
    execFileSync(
      binary,
      [
        "--json",
        "--database",
        path.join(workspace, "workspace.sqlite"),
        "call",
        tool,
        "--input",
        "-",
      ],
      { input: JSON.stringify(input), encoding: "utf8", timeout: 30000 },
    ),
  );
  expect(result.error).toBeNull();
  return result.data;
};
const browser = await chromium.launch({
  headless: true,
  executablePath: process.env.OPENCUT_CHROME,
});
const page = await browser.newPage({ locale: "en-US" });
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
const child = spawn(
  binary,
  ["--database", path.join(workspace, "workspace.sqlite"), "mcp", "serve"],
  { stdio: ["pipe", "pipe", "ignore"] },
);
const exited = new Promise((resolve) =>
  child.once("exit", (code, signal) => resolve({ code, signal })),
);
const lines = createInterface({ input: child.stdout }),
  pending = new Map();
let serial = 0;
lines.on("line", (line) => {
  const reply = JSON.parse(line),
    waiting = pending.get(reply.id);
  if (waiting) {
    pending.delete(reply.id);
    clearTimeout(waiting.timer);
    waiting.resolve(reply);
  }
});
async function rpc(method, params) {
  const id = ++serial;
  const response = new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(Error("MCP response timeout; retain original request")),
      30000,
    );
    pending.set(id, { timer, resolve });
  });
  child.stdin.write(
    JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n",
  );
  const value = await response;
  expect(value.error).toBeUndefined();
  return value.result;
}
try {
  // The already-qualified readonly view has no media/preview work. It is a
  // real connected Web client; the background task targets a separate fixed
  // V4 project, proving that no UI view owns its lifetime.
  await page.goto(
    `${endpoint}/editor/${encodeURIComponent(args["ui-project"])}`,
  );
  await expect(
    page.getByRole("main", { name: "Read-only project", exact: true }),
  ).toBeVisible();
  await rpc("initialize", {
    protocolVersion: "2025-11-25",
    capabilities: {},
    clientInfo: { name: "v4-client-detach-acceptance", version: "1" },
  });
  child.stdin.write('{"jsonrpc":"2.0","method":"notifications/initialized"}\n');
  const accepted = (
    await rpc("tools/call", {
      name: "opencut_render",
      arguments: { action: "submit", input: request.input },
    })
  ).structuredContent;
  expect(accepted.error).toBeNull();
  if (state.accepted) expect(state.accepted).toEqual(accepted.data);
  else state.accepted = accepted.data;
  state.before_close = cli("opencut_job_view", {
    job_id: accepted.data.job_id,
  });
  expect(["queued", "running"]).toContain(state.before_close.state);
  state.mcp_pid = child.pid;
  state.browser_version = browser.version();
  state.page_url = page.url();
  state.detachment_requested_at = new Date().toISOString();
  await save();
  await browser.close();
  expect(browser.isConnected()).toBe(false);
  expect(page.isClosed()).toBe(true);
  child.stdin.end();
  const exit = await exited;
  expect(exit).toEqual({ code: 0, signal: null });
  state.clients_closed = {
    web_page_closed: page.isClosed(),
    browser_connected: browser.isConnected(),
    mcp_exit: exit,
    at: new Date().toISOString(),
    page_errors: errors,
  };
  expect(errors).toEqual([]);
  state.first_cli_after_close = cli("opencut_job_view", {
    job_id: accepted.data.job_id,
  });
  expect(["queued", "running", "succeeded"]).toContain(
    state.first_cli_after_close.state,
  );
  await save();
  console.log(
    "Actual Web and MCP clients closed with an accepted nonterminal V4 task; CLI can still observe it",
  );
} finally {
  await browser.close();
  if (child.exitCode === null) child.stdin.end();
  lines.close();
  for (const waiting of pending.values()) clearTimeout(waiting.timer);
}
