/** Current-candidate local collaboration session acceptance; no media jobs. */
import { chromium, expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { OpenCutClient } from "../../../packages/sdk-ts/dist/index.js";

const args = Object.fromEntries(
  process.argv
    .slice(2)
    .reduce(
      (all, value, i, source) =>
        i % 2 ? all : [...all, [value.slice(2), source[i + 1]]],
      [],
    ),
);
for (const key of ["workspace", "candidate", "endpoint", "chromium", "report"])
  if (!args[key]) throw Error(`Missing --${key}`);
if (!new URL(args.endpoint).hostname.match(/^(127\.0\.0\.1|localhost)$/))
  throw Error("Acceptance requires the existing loopback service");
const read = async (file) => JSON.parse(await fs.readFile(file, "utf8"));
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const candidate = await read(args.candidate);
const service = await read(path.join(args.workspace, "service/state.json"));
expect(await fs.realpath(service.executable)).toBe(
  await fs.realpath(candidate.binary),
);
expect(await fs.realpath(service.web_root)).toBe(
  await fs.realpath(candidate.web_root),
);
expect(sha(await fs.readFile(candidate.binary))).toBe(candidate.binary_sha256);
const identity = {
  candidate: args.candidate,
  native_sha256: candidate.binary_sha256,
  web_js_css_tree_sha256: candidate.web_js_css_tree_sha256,
  chromium_sha256: sha(await fs.readFile(args.chromium)),
};
const explicitCheckpoint = args["checkpoint-name"];
if (explicitCheckpoint && !/^[a-zA-Z][a-zA-Z0-9_-]{0,79}$/.test(explicitCheckpoint))
  throw Error("Invalid owned checkpoint name");
const checkpoint = path.join(
  args.workspace,
  explicitCheckpoint ? `${explicitCheckpoint}.json` : `agent-local-session-${identity.web_js_css_tree_sha256.slice(0, 12)}-checkpoint.json`,
);
const state = await read(checkpoint).catch((error) => {
  if (error.code !== "ENOENT") throw error;
  return { identity, stages: {}, limits: [] };
});
expect(state.identity).toEqual(identity);
const save = async () => {
  const text = JSON.stringify(state, null, 2) + "\n";
  await fs.writeFile(checkpoint + ".pending", text);
  await fs.rename(checkpoint + ".pending", checkpoint);
  await fs.writeFile(args.report, text);
};
const stage = async (name, run) => {
  if (state.stages[name]) {
    console.log("Reuse " + name);
    return;
  }
  console.log("Begin " + name);
  state.stages[name] = {
    ...(await run()),
    executed_verifier_sha256: sha(await fs.readFile(import.meta.filename)),
  };
  delete state.failure;
  await save();
  console.log("Completed " + name);
};
async function request(route, options = {}) {
  const response = await fetch(args.endpoint + route, {
    ...options,
    signal: AbortSignal.timeout(20000),
  });
  const body = await response.text();
  expect(Buffer.byteLength(body)).toBeLessThanOrEqual(4 * 1024 * 1024);
  return {
    status: response.status,
    headers: Object.fromEntries(response.headers),
    body,
  };
}
const baseline = () =>
  JSON.parse(
    execFileSync(
      "python3",
      [
        "-B",
        "-c",
        `
import hashlib,json,sqlite3,sys
c=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro',uri=True)
tables={'projects':'select project_id,name,head_revision from projects',
 'revisions':'select revision from revisions',
 'jobs':'select job_id,state,attempt,cancel_requested from jobs'}
out={}
for table,query in tables.items():
 rows=sorted(c.execute(query).fetchall(),key=repr)
 out[table]={'rows':len(rows),'sha256':hashlib.sha256(repr(rows).encode()).hexdigest()}
out['last_job_event']=c.execute('select coalesce(max(sequence),0) from job_events').fetchone()[0]
print(json.dumps(out))
`,
        path.join(args.workspace, "workspace.sqlite"),
      ],
      { encoding: "utf8" },
    ),
  );
state.baseline ??= baseline();
const wanted = args.phase ?? "all";
if (!["all", "core", "session"].includes(wanted)) throw Error("Unknown phase");
let browser;
try {
  browser = await chromium.launch({
    headless: true,
    executablePath: args.chromium,
  });
  state.browser = await browser.version();
  expect(state.browser).toBe(candidate.runtime_versions.chromium);
  if (wanted === "all" || wanted === "core")
    await stage("core-entry", async () => {
      const unsupported = [
        "/blog",
        "/blog/removed",
        "/app/public/blog?page=1",
        "/rss.xml",
        "/sitemap.xml",
        "/brand",
        "/contributors",
        "/sponsors",
        "/roadmap",
        "/changelog",
        "/changelog/0.3.0",
        "/privacy",
        "/terms",
        "/login",
        "/register",
        "/sign-in",
        "/sign-up",
      ];
      for (const route of unsupported)
        expect((await request(route)).status, route).toBe(404);
      const context = await browser.newContext({
        locale: "en-US",
        viewport: { width: 1440, height: 1000 },
      });
      const page = await context.newPage();
      page.setDefaultTimeout(30000);
      const errors = [],
        external = [];
      let releaseDiscovery;
      let holdingDiscovery = true;
      let pendingDiscovery = 0;
      const discoveryGate = new Promise((resolve) => {
        releaseDiscovery = resolve;
      });
      // Hold output discovery to reproduce the observed startup dependency,
      // then simulate an unavailable backend. Core reads still use real Rust.
      await context.route("**/api/tools/*_capability", async (route) => {
        pendingDiscovery++;
        if (holdingDiscovery) await discoveryGate;
        await route.abort("failed").catch(() => {});
      });
      page.on("pageerror", (error) => errors.push(error.message));
      page.on("request", (request) => {
        const url = request.url();
        if (url.startsWith("http") && new URL(url).origin !== args.endpoint)
          external.push(url);
      });
      try {
        await page.goto(args.endpoint + "/");
        await expect(
          page.getByRole("button", { name: "New project", exact: true }),
        ).toBeEnabled();
        await expect(page.locator(".site-root")).toHaveCount(0);
        const project = "fb92adbb-b999-414e-b351-35f012b8e7b8";
        const input = {
          project_id: project,
          revision: null,
          sequence_offset: 0,
          limit: 1,
        };
        const client = new OpenCutClient({
          baseUrl: args.endpoint,
          token: (
            await fs.readFile(
              path.join(args.workspace, "service/api-token"),
              "utf8",
            )
          ).trim(),
        });
        const summary = await client.call("opencut_project_summary", input);
        const envelope = JSON.parse(
          execFileSync(
            candidate.binary,
            [
              "--json",
              "--database",
              path.join(args.workspace, "workspace.sqlite"),
              "call",
              "opencut_project_summary",
              "--input",
              "-",
            ],
            { input: JSON.stringify(input), encoding: "utf8", timeout: 30000 },
          ),
        );
        expect(envelope.error).toBeNull();
        expect(envelope.data).toEqual(summary);
        const openingStarted = Date.now();
        await page.goto(args.endpoint + "/editor/" + project);
        await expect(
          // The first-run modal makes the surrounding editor AX-inert, while
          // its core DOM is ready. verifyGuide also checks the usable timeline
          // after completing the guide and reloading the same project.
          page.getByRole("region", {
            name: "Timeline",
            exact: true,
            includeHidden: true,
          }),
        ).toBeVisible();
        const opening_ms = Date.now() - openingStarted;
        await expect.poll(() => pendingDiscovery).toBeGreaterThan(0);
        state.core_open_observation = {
          opening_ms,
          output_discovery_still_held: true,
          project_id: project,
        };
        await save();
        // V4 has its own workspace; the legacy-only guide is not an entry
        // requirement. Reload the real V4 route while discovery is still held.
        await page.reload();
        await expect(
          page.getByRole("region", { name: "Timeline", exact: true }),
        ).toBeVisible();
        holdingDiscovery = false;
        releaseDiscovery();
        expect(errors).toEqual([]);
        expect(external).toEqual([]);
        return {
          root_opens_library: true,
          existing_editor_direct_reload: true,
          unsupported_routes: unsupported,
          agent_cli_http_sdk_summary_equal: true,
          opening_ms_with_output_discovery_held: opening_ms,
          output_discovery_scope:
            "Controlled pending then unavailable output backend;real Native project/history/recovery and Agent reads. Renderer acceptance is reused separately.",
          project_id: project,
          summary_sha256: sha(JSON.stringify(summary)),
          page_errors: errors,
          external_page_requests: external,
          new_projects: 0,
          new_jobs: 0,
        };
      } finally {
        holdingDiscovery = false;
        releaseDiscovery();
        await context.request
          .delete(args.endpoint + "/app/session", {
            headers: { Origin: args.endpoint, "X-OpenCut-App": "web" },
          })
          .catch(() => {});
        await context.close();
      }
    });
  if (wanted === "all" || wanted === "session")
    await stage("session", async () => {
      const first = await browser.newContext({ locale: "en-US" });
      const second = await browser.newContext({ locale: "en-US" });
      const headers = { Origin: args.endpoint, "X-OpenCut-App": "web" };
      const route = args.endpoint + "/app/session";
      try {
        for (const context of [first, second])
          expect(
            (await context.request.post(route, { headers })).status(),
          ).toBe(204);
        const cookieA = (await first.cookies(args.endpoint)).find(
          (cookie) => cookie.name === "opencut_session",
        );
        const cookieB = (await second.cookies(args.endpoint)).find(
          (cookie) => cookie.name === "opencut_session",
        );
        expect(cookieA.value).not.toBe(cookieB.value);
        expect(cookieA.httpOnly && cookieB.httpOnly).toBe(true);
        expect(cookieA.sameSite).toBe("Strict");
        const status = async (context) =>
          (await context.request.get(route)).json();
        const refused = async (response) => {
          // Authentication uses the shared NOT_AUTHORIZED/403 policy, including
          // malformed bearer headers; it must not fall back to the live cookie.
          expect(response.status()).toBe(403);
          expect((await response.json()).error?.code).toBe("NOT_AUTHORIZED");
        };
        const initial = await status(first);
        expect(initial.active).toBe(true);
        expect(initial.expires_in_seconds).toBeLessThanOrEqual(28800);
        const page = await first.newPage();
        await page.goto(args.endpoint + "/projects");
        await expect(
          page.getByRole("button", { name: "New project", exact: true }),
        ).toBeEnabled();
        const reused = (await first.cookies(args.endpoint)).find(
          (cookie) => cookie.name === "opencut_session",
        );
        expect(reused.value).toBe(cookieA.value);
        expect((await status(first)).expires_in_seconds).toBeLessThanOrEqual(
          initial.expires_in_seconds,
        );
        await refused(
          await first.request.get(args.endpoint + "/api/capabilities", {
            headers: { Authorization: "Bearer invalid-public-acceptance" },
          }),
        );
        expect(
          (
            await first.request.post(route, {
              headers: { ...headers, Origin: "https://untrusted.example" },
            })
          ).status(),
        ).toBe(403);
        expect(
          (
            await request("/api/capabilities", {
              headers: {
                Cookie: `opencut_session=${cookieA.value}; opencut_session=${cookieA.value}`,
              },
            })
          ).status,
        ).toBe(403);
        await page.evaluate((after) => {
          const observation = {
            status: null,
            bytes: 0,
            ended: false,
            error: null,
          };
          window.__sessionStream = observation;
          void (async () => {
            try {
              const reply = await fetch(
                `/api/job-events/stream?after_sequence=${Math.max(0, after - 1)}&limit=1`,
              );
              observation.status = reply.status;
              const reader = reply.body.getReader();
              for (;;) {
                const next = await reader.read();
                if (next.done) break;
                observation.bytes += next.value.length;
              }
            } catch (error) {
              observation.error = error.name;
            } finally {
              observation.ended = true;
            }
          })();
        }, state.baseline.last_job_event);
        await expect
          .poll(() => page.evaluate(() => window.__sessionStream.status))
          .toBe(200);
        await expect
          .poll(() => page.evaluate(() => window.__sessionStream.bytes))
          .toBeGreaterThan(0);
        await page
          .getByRole("button", { name: "Local session", exact: true })
          .click();
        await expect(
          page.getByText(/Cloud accounts are not used/),
        ).toBeVisible();
        const started = Date.now();
        await page
          .getByRole("button", { name: "End local session", exact: true })
          .click();
        await expect
          .poll(() => page.evaluate(() => window.__sessionStream.ended), {
            timeout: 5000,
          })
          .toBe(true);
        const stream = await page.evaluate(() => window.__sessionStream);
        const retirement_ms = Date.now() - started;
        expect((await status(first)).active).toBe(false);
        expect(
          (
            await request("/api/capabilities", {
              headers: { Cookie: `opencut_session=${cookieA.value}` },
            })
          ).status,
        ).toBe(403);
        expect((await status(second)).active).toBe(true);
        expect(
          (
            await second.request.get(args.endpoint + "/api/capabilities")
          ).status(),
        ).toBe(200);
        await page
          .getByRole("button", { name: "Local session", exact: true })
          .click();
        await page
          .getByRole("button", { name: "Start local session", exact: true })
          .click();
        await expect(
          page.getByRole("button", { name: "New project", exact: true }),
        ).toBeEnabled();
        const renewed = (await first.cookies(args.endpoint)).find(
          (cookie) => cookie.name === "opencut_session",
        );
        expect(renewed.value).not.toBe(cookieA.value);
        return {
          isolated_grants: true,
          httponly_strict: true,
          initial_expires_in_seconds: initial.expires_in_seconds,
          bootstrap_did_not_extend_or_replace: true,
          invalid_bearer_duplicate_cookie_and_cross_origin_rejected: true,
          actual_ui_logout: true,
          active_stream_retired: stream,
          retirement_ms,
          retired_cookie_rejected: true,
          other_browser_still_authorized: true,
          explicit_ui_reconnect_uses_new_grant: true,
          ttl_scope:
            "Production eight-hour expiry is not accelerated; reuse unchanged recorded clock/stream tests, not a claim of eight-hour wall-clock acceptance.",
        };
      } finally {
        for (const context of [first, second]) {
          await context.request.delete(route, { headers }).catch(() => {});
          await context.close();
        }
      }
    });
  expect(baseline()).toEqual(state.baseline);
  state.project_heads_history_identities_and_job_states_unchanged = true;
  await save();
} catch (error) {
  state.failure = { phase: wanted, message: String(error), stack: error.stack };
  await save();
  throw error;
} finally {
  await browser?.close();
}
