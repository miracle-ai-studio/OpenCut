import { editorSnapshot } from "./project-inspection.mjs";
/** F20: new source classes on the retained project; one offline composite PNG. */
import { chromium, expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import {
  OpenCutClient,
  OpenCutApiError,
} from "../../../packages/sdk-ts/dist/index.js";
import {
  fixedFontCacheLocations,
  readFixedFontCache,
} from "./verify-font-synthesis.mjs";
const args = Object.fromEntries(
  process.argv
    .slice(2)
    .reduce((a, v, i, s) => (i % 2 ? a : [...a, [v.slice(2), s[i + 1]]]), []),
);
const project = "fb92adbb-b999-414e-b351-35f012b8e7b8",
  cache = "/private/tmp/opencut-b3-font-cache",
  W = 640,
  H = 480;
const file = path.join(
    args.workspace,
    "agent-font-paths-current-v4-checkpoint.json",
  ),
  read = async (p) => JSON.parse(await fs.readFile(p, "utf8")),
  sha = (b) => createHash("sha256").update(b).digest("hex");
const state = await read(file).catch((e) => {
  if (e.code !== "ENOENT") throw e;
  return {
    candidate: args.candidate,
    stages: {},
    requests: {},
    receipts: {},
    forward: [],
  };
});
expect(state.candidate).toBe(args.candidate);
const done = await read(args.report).catch((e) => {
  if (e.code !== "ENOENT") throw e;
  return null;
});
if (
  done?.status === "passed_scoped_new_font_paths" &&
  done.candidate === args.candidate
) {
  console.log("Reuse completed font paths");
  process.exit(0);
}
const budget = await read(args.budget),
  save = async () => {
    await fs.writeFile(
      file + ".pending",
      JSON.stringify(state, null, 2) + "\n",
    );
    await fs.rename(file + ".pending", file);
  };
const stage = async (name, run) => {
  if (state.stages[name]) return state.stages[name];
  console.log("Begin " + name);
  const v = await run();
  state.stages[name] = v;
  await save();
  console.log("Passed " + name);
  return v;
};
let client;
const reconnect = async () => {
  client = new OpenCutClient({
    baseUrl: args.endpoint,
    token: (await fs.readFile(args.credential, "utf8")).trim(),
  });
};
await reconnect();
const call = (tool, input) => client.call(tool, input),
  inspect = (revision = null) =>
    editorSnapshot(call("opencut_project_inspect", { project_id: project, revision })),
  time = (s) => ({ ticks: String(s * 120000), timebase: 120000 });
const original = await stage("original-current-project", () => inspect()),
  sequence = original.project.sequences[0].id;
const blueprint = [
  {
    id: "google-fjordone-regular-alias-v3",
    kind: "static_v3",
    family: "Fjord One",
    text: "Fjord AV",
    weight: 400,
    style: "normal",
    size: 24,
    ui: true,
  },
  {
    id: "google-dancingscript-bold-derived-v2",
    kind: "derived_v2",
    family: "Dancing Script",
    text: "Dance AV",
    weight: 700,
    style: "normal",
    size: 24,
    ui: true,
  },
  {
    id: "google-dancingscript-bold-derived-v2-derived-v2-synthetic-italic-v1",
    kind: "synthesis_v1",
    family: "Dancing Script",
    text: "Dance Italic",
    weight: 700,
    style: "italic",
    size: 24,
    ui: true,
  },
  {
    id: "google-fjordone-regular-alias-v3-static-v3-synthetic-italic-v1",
    kind: "synthesis_v1",
    family: "Fjord One",
    text: "Fjord Italic",
    weight: 400,
    style: "italic",
    size: 24,
  },
  {
    id: "google-chirongoroundtc-regular-derived-v2",
    kind: "derived_v2",
    family: "Chiron GoRound TC",
    text: "中文 AV",
    weight: 400,
    style: "normal",
    size: 24,
    cold: true,
  },
  {
    id: "google-anonymouspro-regular-color-v3",
    kind: "static_v3",
    family: "Anonymous Pro",
    text: "Bitmap AV",
    weight: 400,
    style: "normal",
    size: 12,
  },
  {
    id: "google-bungeetint-regular-color-v3",
    kind: "static_v3",
    family: "Bungee Tint",
    text: "TINT AV",
    weight: 400,
    style: "normal",
    size: 24,
  },
  {
    id: "google-arefruqaaink-regular-color-v3",
    kind: "static_v3",
    family: "Aref Ruqaa Ink",
    text: "سلام",
    weight: 400,
    style: "normal",
    size: 24,
  },
].map((c, i) => ({
  ...c,
  index: i,
  cx: i % 2 ? 480 : 160,
  cy: 60 + Math.floor(i / 2) * 120,
}));
expect(budget.catalog_ids).toEqual(blueprint.map((c) => c.id));
const staticPins = new Map(),
  derivedPins = new Map();
for (const name of [
  "static_source_pins.json",
  "color_source_pins.json",
  "supplemental_source_pins.json",
])
  for (const record of await read(
    new URL(
      "../../../crates/editor-application/src/font_catalog/" + name,
      import.meta.url,
    ),
  ))
    staticPins.set(record.source.catalog_id, record.source);
for (const record of await read(
  new URL(
    "../../../crates/editor-application/src/font_named/catalog_pins.json",
    import.meta.url,
  ),
))
  derivedPins.set(record.face.id, record.face);
const cases = await stage("exact-source-and-independent-cache", async () => {
  const values = [];
  for (const c of blueprint) {
    const source = await call("opencut_font_source", {
        catalog_id: c.id,
      }),
      face = source.face;
    expect(face.display_family).toBe(c.family);
    expect(face.weight).toBe(c.weight);
    expect(face.requested_style ?? face.style).toBe(c.style);
    const details = source.details.source;
    const baseId = c.kind === "synthesis_v1" ? details.binding.actual.id : c.id;
    const pinned = staticPins.get(baseId) ?? derivedPins.get(baseId);
    expect(pinned, baseId).toBeTruthy();
    expect(face.content_hash).toBe(pinned.content_hash);
    expect(String(face.size_bytes)).toBe(String(pinned.size_bytes));
    expect(face.family).toBe(pinned.family);
    let locations;
    if (
      c.kind === "derived_v2" ||
      (c.kind === "synthesis_v1" &&
        JSON.parse(details.source.proof_json).base?.route === "derived_v2")
    )
      locations = [path.join(cache, "derived", face.content_hash)];
    else {
      const actual =
        c.kind === "synthesis_v1" ? details.binding.actual : details;
      locations = fixedFontCacheLocations(
        cache,
        actual.source_url,
        actual.source_commit,
      );
    }
    const existing = [];
    for (const location of locations)
      if (
        await fs
          .lstat(location)
          .then(() => true)
          .catch((e) => {
            if (e.code === "ENOENT") return false;
            throw e;
          })
      )
        existing.push(location);
    const independent = existing.length
      ? await readFixedFontCache(
          cache,
          locations,
          face.content_hash,
          face.size_bytes,
        )
      : null;
    // A missing unmodified static input is acquired once by the real import.
    // Its exact repository-pinned input hash is checked before oracle use.
    if (!independent) expect(staticPins.has(baseId), baseId).toBe(true);
    const size = Number(face.size_bytes);
    expect(size).toBeLessThanOrEqual(budget.font_output_max_bytes);
    values.push({
      ...c,
      source,
      cache_file: independent?.file ?? locations.at(-1),
      cache_pending: !independent,
      expected_identity_from_repository_pin: true,
      hash: face.content_hash,
      bytes: size,
    });
  }
  return values;
});
const bytes = [];
for (const c of cases) {
  const p =
    c.cold && state.cold_cache?.moved ? state.cold_cache.backup : c.cache_file;
  const data = await fs.readFile(p).catch((e) => {
    if (e.code === "ENOENT" && c.cache_pending) return null;
    throw e;
  });
  if (data) expect(sha(data)).toBe(c.hash);
  bytes.push(data);
}
const service = await read(args.service),
  browserServer = await chromium.launchServer({
    headless: true,
    executablePath: process.env.OPENCUT_CHROME,
  }),
  browser = await chromium.connect(browserServer.wsEndpoint()),
  page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1300 },
  }),
  oracle = await browser.newPage();
page.setDefaultTimeout(25000);
let nativePid = state.active_service?.pid ?? service.data.pid;
const peaks = state.memory_peaks ?? {
    native_tree: 0,
    browser_tree: 0,
    driver: 0,
    cold_python: 0,
    samples: 0,
  },
  errors = [];
page.on("pageerror", (e) => errors.push(e.message));
const fontCalls = [];
const fontRequests = [];
const canvasNetwork = new Map();
page.on("request", (r) => {
  const u = new URL(r.url());
  if (!/_canvas_plan$|project_resource_inspect$|project-resources\/content/.test(u.pathname)) return;
  canvasNetwork.set(r, { path: u.pathname, started: Date.now(), done: false });
});
page.on("requestfinished", (r) => {
  const v = canvasNetwork.get(r);
  if (v) Object.assign(v, { done: true, elapsed_ms: Date.now() - v.started });
});
page.on("requestfailed", (r) => {
  const v = canvasNetwork.get(r);
  if (v) Object.assign(v, { done: true, elapsed_ms: Date.now() - v.started, error: r.failure()?.errorText });
});
page.on("request", (request) => {
  const tool = new URL(request.url()).pathname.split("/").at(-1);
  if (tool?.startsWith("opencut_font"))
    fontRequests.push({ tool, at: Date.now() });
});
page.on("response", async (response) => {
  const tool = new URL(response.url()).pathname.split("/").at(-1);
  if (
    !tool?.startsWith("opencut_font") &&
    !tool?.includes("_canvas_plan") &&
    !tool?.endsWith("_frame_capability")
  ) return;
  try {
    const input = response.request().postDataJSON(),
      body = await response.json();
    fontCalls.push({
      tool,
      request: input?.context?.request ?? input,
      error: body.error,
      recovery_id: body.data?.recovery_id,
    });
    if (body.error)
      console.log(
        JSON.stringify({
          font_tool: tool,
          error: body.error.code,
          message: body.error.message,
        }),
      );
    if (fontCalls.length > 24) fontCalls.shift();
  } catch {}
});
let sampling = false;
const sample = () => {
  if (sampling) return;
  sampling = true;
  try {
    const rows = execFileSync("ps", ["-axo", "pid=,ppid=,rss=,comm="], {
      encoding: "utf8",
    })
      .trim()
      .split("\n")
      .map((l) => /^\s*(\d+)\s+(\d+)\s+(\d+)\s+(.*)$/.exec(l))
      .filter(Boolean)
      .map((m) => ({ pid: +m[1], ppid: +m[2], rss: +m[3] * 1024, name: m[4] }));
    const tree = (root) => {
      const ids = new Set([root]);
      for (let changed = true; changed;) {
        changed = false;
        for (const p of rows)
          if (ids.has(p.ppid) && !ids.has(p.pid)) {
            ids.add(p.pid);
            changed = true;
          }
      }
      return rows.filter((p) => ids.has(p.pid));
    };
    const n = tree(nativePid),
      b = tree(browserServer.process().pid);
    peaks.native_tree = Math.max(
      peaks.native_tree,
      n.reduce((v, p) => v + p.rss, 0),
    );
    peaks.browser_tree = Math.max(
      peaks.browser_tree,
      b.reduce((v, p) => v + p.rss, 0),
    );
    peaks.driver = Math.max(peaks.driver, process.memoryUsage().rss);
    for (const p of n)
      if (/python/i.test(p.name))
        peaks.cold_python = Math.max(peaks.cold_python, p.rss);
    peaks.samples++;
  } finally {
    sampling = false;
  }
};
sample();
const timer = setInterval(sample, 750);
await page.addInitScript(() => {
  window.__f20FormEvents = [];
  for (const type of [
    "focusin",
    "pointerdown",
    "pointerup",
    "click",
    "submit",
    "compositionstart",
    "compositionend",
    "invalid",
  ])
    document.addEventListener(
      type,
      (event) => {
        if (
          event.target instanceof Element &&
          event.target.closest('form[aria-label="Import font"]')
        )
          window.__f20FormEvents.push({
            type,
            trusted: event.isTrusted,
            tag: event.target.tagName,
            buttonDisabled: document.querySelector(
              'form[aria-label="Import font"] button[type="submit"]',
            )?.disabled,
            at: Date.now(),
          });
      },
      true,
    );
  const original =
    ImageBitmapRenderingContext.prototype.transferFromImageBitmap;
  ImageBitmapRenderingContext.prototype.transferFromImageBitmap = function (
    bitmap,
  ) {
    if (
      bitmap &&
      window.__f20Capture &&
      !window.__f20Picture &&
      this.canvas.classList.contains("editor-scene-frame") &&
      this.canvas.dataset.time !== "" &&
      Number(this.canvas.dataset.time) >= 0
    ) {
      window.__f20Picture = {
        time: Number(this.canvas.dataset.time),
        generation: this.canvas.dataset.generation,
        bitmap: createImageBitmap(bitmap),
      };
    }
    return original.call(this, bitmap);
  };
});
const open = async () => {
  await page.goto(`${args.endpoint}/editor/${project}?scene=${sequence}`);
  await expect(page.locator(".save-status")).toHaveText("Saved");
  const g = page.getByRole("dialog", {
    name: "Welcome to OpenCut",
    exact: true,
  });
  if (await g.isVisible())
    await g.getByRole("button", { name: "Close", exact: true }).click();
};
const fontForm = async (family, style = "400:normal") => {
  await page.getByRole("button", { name: "Import font", exact: true }).click();
  const f = page.getByRole("form", { name: "Import font", exact: true });
  await f
    .getByRole("combobox", { name: /^Font family(?:\s|$)/ })
    .selectOption(family);
  await f
    .getByRole("combobox", { name: /^Font style(?:\s|$)/ })
    .selectOption(style);
  return f;
};
const record = async (name, tool, input, reply, forward = true) => {
  state.requests[name] ??= { tool, input };
  expect(state.requests[name]).toEqual({ tool, input });
  state.receipts[name] = reply;
  if (forward && !state.forward.includes(name)) state.forward.push(name);
  await save();
};
const mutate = async (name, tool, input) => {
  state.requests[name] ??= { tool, input };
  await save();
  const q = state.requests[name],
    start = performance.now(),
    reply = await call(q.tool, q.input);
  await record(name, q.tool, q.input, reply);
  return { receipt: reply, elapsed_ms: performance.now() - start };
};
const ui = async (name, tool, act, forward = true) => {
  if (state.requests[name]) {
    const q = state.requests[name],
      reply = await call(q.tool, q.input);
    await record(name, q.tool, q.input, reply, forward);
    await open();
    return reply;
  }
  let receipt;
  await page.route("**/api/tools/" + tool, async (route) => {
    const input = route.request().postDataJSON();
    state.requests[name] = { tool, input };
    await save();
    const response = await route.fetch(),
      body = await response.json();
    expect(body.error).toBeNull();
    receipt = body.data;
    await record(name, tool, input, receipt, forward);
    await route.fulfill({ response });
  });
  try {
    await act();
    await expect.poll(() => !!receipt).toBe(true);
    await expect(page.locator(".save-status")).toHaveText("Saved");
    return receipt;
  } finally {
    await page.unroute("**/api/tools/" + tool);
  }
};
const history = async (name, direction) =>
  stage(name, () =>
    ui(
      name,
      "opencut_edit_" + direction + "_apply",
      async () => {
        await page
          .getByRole("region", { name: "History", exact: true })
          .getByRole("button", {
            name: direction === "undo" ? "Undo" : "Redo",
            exact: true,
          })
          .click();
        await page
          .getByRole("region", { name: "History plan", exact: true })
          .getByRole("button", { name: "Apply " + direction, exact: true })
          .click();
      },
      false,
    ),
  );
const restart = async (mode) => {
  await page.goto("about:blank");
  const script = `import os,sys,json,subprocess\nos.environ['OPENCUT_ALLOW_PUBLIC_FONT_DOWNLOAD']=sys.argv[1]\nbinary=sys.argv[2];common=['--database',sys.argv[3],'--directory',sys.argv[4],'--json']\ns=json.loads(subprocess.check_output([binary,'server','stop',*common],text=True));assert s['data']['state']=='stopped'\nr=json.loads(subprocess.check_output([binary,'server','start',*common,'--config',sys.argv[5]],text=True));assert r['data']['ready'];print(json.dumps(r['data']))\n`;
  const data = JSON.parse(
    execFileSync(
      "python3",
      [
        args.runner,
        "python3",
        "-c",
        script,
        mode,
        args.binary,
        path.join(args.workspace, "workspace.sqlite"),
        path.join(args.workspace, "service"),
        path.join(args.workspace, "service/human-chinese-config.json"),
      ],
      { encoding: "utf8", timeout: 30000 },
    ),
  );
  nativePid = data.pid;
  state.active_service = data;
  state.offline = mode === "0";
  await save();
  await reconnect();
  return data;
};
const verifyImport = async (c) => {
  const name = "import-" + c.index;
  return stage(name, async () => {
    let result;
    const start = performance.now();
    if (c.ui && !state.requests[name]) {
      await open();
      const f = await fontForm(c.family, `${c.weight}:${c.style}`);
      await f
        .getByRole("textbox", { name: /^Text to check(?:\s|$)/ })
        .fill(c.text);
      await expect(
        f.getByRole("button", { name: "Import font", exact: true }),
      ).toBeEnabled();
      result = await ui(
        name,
        c.kind === "static_v3"
          ? "opencut_font_import"
          : c.kind === "derived_v2"
            ? "opencut_font_import"
            : "opencut_font_import",
        async () => {
          const started = fontRequests.length;
          await f
            .getByRole("button", { name: "Import font", exact: true })
            .click();
          await expect
            .poll(
              () =>
                fontRequests
                  .slice(started)
                  .some((r) => /recovery_stage|_import$/.test(r.tool)),
              { timeout: 5000 },
            )
            .toBe(true);
          await expect(f).toBeHidden({ timeout: budget.import_observation_ms });
        },
      );
    } else {
      const tool =
        c.kind === "static_v3"
          ? "opencut_font_import"
          : c.kind === "derived_v2"
            ? "opencut_font_import"
            : "opencut_font_import";
      const r = await mutate(name, tool, {
        project_id: project,
        expected_revision: (await inspect()).revision,
        idempotency_key: "f20-path-" + c.index,
        catalog_id: c.id,
        font_id: "f20-path-font-" + c.index,
        required_text: c.text,
      });
      result = r.receipt;
    }
    const elapsed_ms = performance.now() - start;
    expect(elapsed_ms).toBeLessThan(budget.import_observation_ms);
    const q = state.requests[name].input,
      head = await inspect(),
      font = head.project.fonts.find((f) => f.id === q.font_id);
    expect(font).toBeTruthy();
    expect(font.family).toBe(c.source.face.family);
    expect(font.weight).toBe(c.weight);
    expect(font.style).toBe(c.source.face.style);
    expect(font.content.content_hash).toBe(c.hash);
    expect(font.content.size).toBe(c.bytes);
    if (c.kind !== "static_v3") {
      const proof = await call(
        c.kind === "derived_v2"
          ? "opencut_font_derived_proof"
          : "opencut_font_synthesis_proof",
        { project_id: project, revision: head.revision, font_id: font.id },
      );
      expect(proof.source).toEqual(c.source.details.source);
    }
    const resource = await call("opencut_project_resource_inspect", {
        project_id: project,
        revision: head.revision,
        kind: "font",
        resource_id: font.id,
      }),
      downloaded = Buffer.from(
        await (await client.downloadProjectResource(resource)).arrayBuffer(),
      );
    expect(sha(downloaded)).toBe(c.hash);
    expect(downloaded.length).toBe(c.bytes);
    bytes[c.index] = downloaded;
    if (c.cache_pending) {
      await fs.mkdir(path.dirname(c.cache_file), { recursive: true });
      try {
        await fs.writeFile(c.cache_file, downloaded, { flag: "wx" });
      } catch (e) {
        if (e.code !== "EEXIST") throw e;
      }
      expect(sha(await fs.readFile(c.cache_file))).toBe(c.hash);
    }
    return {
      font,
      receipt: result,
      elapsed_ms,
      revision: head.revision,
      ui: !!c.ui,
      source_bytes_equal: true,
      candidate: args.candidate,
    };
  });
};
try {
  await stage("unavailable-original-families-ui", async () => {
    await open();
    const before = await inspect();
    for (const family of ["Pushster", "Material Symbols"]) {
      const f = await fontForm(family);
      await expect(
        f.getByRole("button", { name: "Import font", exact: true }),
      ).toBeDisabled();
      await expect(
        f.getByText("No fixed static source is available for this face.", {
          exact: true,
        }),
      ).toBeVisible();
      await f.getByRole("button", { name: "Cancel", exact: true }).click();
    }
    expect(await inspect()).toEqual(before);
    return {
      families: ["Pushster", "Material Symbols"],
      explicitly_unavailable: true,
      project_unchanged: true,
    };
  });
  await stage("new-static-profile-glyph-rejection", async () => {
    const before = await inspect(),
      input = state.requests.glyph_reject?.input ?? {
        project_id: project,
        expected_revision: before.revision,
        idempotency_key: "f20-v3-missing-cjk",
        catalog_id: cases[0].id,
        font_id: "f20-path-rejected-glyph",
        required_text: "中文",
      };
    state.requests.glyph_reject = { tool: "opencut_font_import", input };
    await save();
    let code;
    try {
      await call("opencut_font_import", input);
    } catch (e) {
      if (!(e instanceof OpenCutApiError)) throw e;
      code = e.code;
    }
    expect(code).toBe("FONT_GLYPH_MISSING");
    expect(await inspect()).toEqual(before);
    return { code, project_unchanged: true };
  });
  for (const i of [0, 1, 2, 3, 5, 6, 7]) await verifyImport(cases[i]);
  await stage("offline-service-and-replay", async () => {
    const data = await restart("0"),
      before = await inspect(),
      q = state.requests["import-0"];
    expect(await call(q.tool, q.input)).toEqual(state.receipts["import-0"]);
    const r = {
      project_id: project,
      expected_revision: before.revision,
      idempotency_key: "f20-offline-static-new",
      catalog_id: cases[0].id,
      font_id: "f20-offline-unbound",
      required_text: "Fjord AV",
    };
    state.requests.offline_denied = {
      tool: "opencut_font_import",
      input: r,
    };
    await save();
    let code;
    try {
      await call("opencut_font_import", r);
    } catch (e) {
      if (!(e instanceof OpenCutApiError)) throw e;
      code = e.code;
    }
    expect(code).toBe("FONT_NETWORK_DENIED");
    expect(await inspect()).toEqual(before);
    return {
      service: data,
      public_downloads: false,
      replay_exact_receipt: true,
      new_static_import_denied: code,
    };
  });
  await stage("largest-cold-cjk-native-runtime", async () => {
    const c = cases[4],
      target = path.join(cache, "derived", c.hash),
      backup = target + ".f20-original";
    state.cold_cache ??= {
      path: target,
      backup,
      hash: c.hash,
      size: c.bytes,
      moved: false,
    };
    await save();
    if (!state.cold_cache.moved) {
      await fs.rename(target, backup);
      state.cold_cache.moved = true;
      await save();
    }
    expect(
      await fs
        .stat(target)
        .then(() => true)
        .catch((e) => {
          if (e.code === "ENOENT") return false;
          throw e;
        }),
    ).toBe(false);
    const start = performance.now();
    const result = await verifyImport(c);
    const generated = await fs.readFile(target);
    expect(sha(generated)).toBe(c.hash);
    expect(generated.length).toBe(c.bytes);
    expect(peaks.cold_python).toBeGreaterThan(0);
    await fs.rename(backup, target);
    state.cold_cache.moved = false;
    await save();
    return {
      source_bytes: 51170304,
      output_bytes: c.bytes,
      output_sha256: c.hash,
      elapsed_ms: performance.now() - start,
      cached_output_absent_before: true,
      source_cache_reused: true,
      public_downloads: false,
      native_python_observed: true,
      original_output_cache_restored: true,
      import: result,
    };
  });
  const withFonts = await stage("all-eight-fonts-imported", () => inspect());
  const fixed = await stage("one-combined-scene", async () => {
    const q = structuredClone(withFonts.project.sequences[0]);
    expect(q.transitions).toHaveLength(0);
    expect(q.captions).toHaveLength(0);
    for (const t of q.tracks) t.enabled = false;
    for (const c of cases) {
      const t = {
        id: "f20-path-texts-" + c.index,
        name: "F20 " + c.family,
        kind: "text",
        enabled: true,
        muted: false,
        locked: false,
        opacity: 1,
        gain: { unit: "decibels", value: 0 },
        channels: [],
        elements: [],
      };
      const font = state.stages["import-" + c.index].font;
      t.elements.push({
        id: "f20-path-text-" + c.index,
        name: c.family,
        enabled: true,
        channels: [],
        timeline_range: { start: time(0), duration: time(3) },
        source_span: {
          profile: "classic_trimmed",
          source_duration: null,
          trim_start: time(0),
          trim_end: time(0),
        },
        visual: {
          profile: "classic_canvas_v31",
          opacity: 1,
          blend_mode: "normal",
          effects: [],
          masks: [],
          transform: {
            position_x_px: c.cx - W / 2,
            position_y_px: c.cy - H / 2,
            scale_x: 1,
            scale_y: 1,
            rotation_degrees: 0,
          },
        },
        content: {
          kind: "text",
          text: {
            content: c.text,
            font: { kind: "locked", font_id: font.id },
            font_weight: c.weight === 700 ? "bold" : "normal",
            font_style: c.style,
            decoration: "none",
            align: "center",
            font_size: { unit: "classic_height90", value: (c.size * 90) / H },
            color: { encoding: "srgb", red: 1, green: 1, blue: 1, alpha: 1 },
            letter_spacing_px: 0,
            line_height_multiplier: 1.2,
            background: {
              enabled: false,
              color: { encoding: "srgb", red: 0, green: 0, blue: 0, alpha: 0 },
              corner_radius_percent: 0,
              padding_x_at_size15: 0,
              padding_y_at_size15: 0,
              offset_x_px: 0,
              offset_y_px: 0,
            },
          },
        },
      });
      q.tracks.push(t);
    }
    const settings = structuredClone(withFonts.project.settings);
    settings.settings.canvas_size = { width: W, height: H };
    settings.settings.background = {
      kind: "color",
      color: {
        encoding: "srgb",
        red: 48 / 255,
        green: 48 / 255,
        blue: 48 / 255,
        alpha: 1,
      },
    };
    await mutate("scene", "opencut_edit_apply", {
      project_id: project,
      expected_revision: withFonts.revision,
      idempotency_key: "f20-path-one-scene-v2",
      label: "F20 bounded source classes and explicit synthesis",
      operations: [
        { type: "settings_set", settings },
        { type: "sequence_put", sequence: q },
      ],
    });
    return inspect();
  });
  if (!state.stages["redo-scene"]) {
    await open();
    await history("undo-scene", "undo");
    expect((await inspect()).project).toEqual(withFonts.project);
    await history("redo-scene", "redo");
    expect((await inspect()).project).toEqual(fixed.project);
  }
  const originalReference = await read(args.reference);
  const references = await stage(
    "independent-original-classic-pixels",
    async () => {
      const r = await oracle.evaluate(
        async ({ modules, cases, fonts, W, H }) => {
          const registry = {},
            normalize = (s) => {
              const a = [];
              for (const p of s.split("/"))
                if (p === "..") a.pop();
                else if (p && p !== ".") a.push(p);
              return a.join("/");
            },
            load = (id) => {
              if (registry[id]) return registry[id];
              const e = {};
              registry[id] = e;
              new Function("exports", "require", modules[id])(e, (s) =>
                load(
                  s.startsWith("@/")
                    ? s.slice(2)
                    : normalize(id.split("/").slice(0, -1).join("/") + "/" + s),
                ),
              );
              return e;
            },
            p = load("text/primitives");
          for (let i = 0; i < cases.length; i++) {
            const c = cases[i],
              bytes = Uint8Array.from(atob(fonts[i]), (x) => x.charCodeAt(0)),
              face = new FontFace("F20Oracle_" + i, bytes, {
                weight: String(c.weight),
                style: c.source.face.style,
              });
            await face.load();
            document.fonts.add(face);
          }
          const encode = (b) => {
              let s = "";
              for (let i = 0; i < b.length; i += 8192)
                s += String.fromCharCode(...b.subarray(i, i + 8192));
              return btoa(s);
            },
            draw = (variant) => {
              const canvas = document.createElement("canvas");
              canvas.width = W;
              canvas.height = H;
              const ctx = canvas.getContext("2d");
              ctx.fillStyle = "#303030";
              ctx.fillRect(0, 0, W, H);
              for (const c of cases) {
                if (
                  (variant === "omit-cjk" && c.cold) ||
                  (variant === "omit-color" &&
                    c.source.face.render_profile.startsWith("colr"))
                )
                  continue;
                const text = {
                    content: c.text,
                    fontFamily:
                      variant === "fallback"
                        ? "sans-serif"
                        : "F20Oracle_" + c.index,
                    fontSize: (c.size * 90) / H,
                    fontWeight: c.weight === 700 ? "bold" : "normal",
                    fontStyle:
                      variant === "normal-synthesis" &&
                      c.kind === "synthesis_v1"
                        ? "normal"
                        : c.style,
                    textAlign: "center",
                    textDecoration: "none",
                    letterSpacing: 0,
                    lineHeight: 1.2,
                  },
                  layout = p.measureTextLayout({ text, canvasHeight: H, ctx });
                ctx.save();
                ctx.translate(c.cx, c.cy);
                p.drawMeasuredTextLayout({
                  ctx,
                  layout,
                  textColor: "#ffffff",
                  background: {
                    enabled: false,
                    cornerRadius: 0,
                    paddingX: 0,
                    paddingY: 0,
                    offsetX: 0,
                    offsetY: 0,
                  },
                  backgroundColor: "#000000",
                });
                ctx.restore();
              }
              return {
                rgba: encode(ctx.getImageData(0, 0, W, H).data),
                png: canvas.toDataURL("image/png").split(",")[1],
              };
            };
          return {
            normal: draw("normal"),
            negative: Object.fromEntries(
              ["fallback", "normal-synthesis", "omit-cjk", "omit-color"].map(
                (v) => [v, draw(v).rgba],
              ),
            ),
          };
        },
        {
          modules: originalReference.modules,
          cases,
          fonts: bytes.map((b) => b.toString("base64")),
          W,
          H,
        },
      );
      const p = path.join(args.workspace, "f20-original-reference.rgba");
      await fs.writeFile(p, Buffer.from(r.normal.rgba, "base64"));
      await fs.writeFile(
        path.join(args.workspace, "f20-original-reference.png"),
        Buffer.from(r.normal.png, "base64"),
      );
      const negatives = {};
      for (const [name, b64] of Object.entries(r.negative)) {
        const file = path.join(
          args.workspace,
          "f20-negative-" + name + ".rgba",
        );
        await fs.writeFile(file, Buffer.from(b64, "base64"));
        negatives[name] = file;
      }
      return {
        path: p,
        sha256: sha(Buffer.from(r.normal.rgba, "base64")),
        negative: negatives,
        original_modules: originalReference.original_modules,
        scope:
          "Original Classic text primitives/layout/typography/defaults and independently hash-locked font cache;no product layout/renderer called by expected pixels.",
      };
    },
  );
  const expected = await fs.readFile(references.path);
  expect(sha(expected)).toBe(references.sha256);
  const metrics = (raw, c) => {
    let union = 0,
      intersection = 0,
      error = 0,
      foreground = 0;
    const fromY = c ? Math.floor(c.cy - 60) : 0,
      toY = c ? Math.ceil(c.cy + 60) : H,
      fromX = c ? (c.index % 2) * 320 : 0,
      toX = c ? ((c.index % 2) + 1) * 320 : W;
    for (let y = fromY; y < toY; y++)
      for (let x = fromX; x < toX; x++) {
        const p = (y * W + x) * 4,
          a = Math.max(...[0, 1, 2].map((k) => Math.abs(raw[p + k] - 48))) > 16,
          e =
            Math.max(...[0, 1, 2].map((k) => Math.abs(expected[p + k] - 48))) >
            16;
        if (a || e) union++;
        if (a && e) intersection++;
        if (e) foreground++;
        for (let k = 0; k < 3; k++)
          error += Math.abs(raw[p + k] - expected[p + k]);
      }
    return {
      glyph_iou: intersection / Math.max(1, union),
      mean_rgb_error: error / ((toY - fromY) * (toX - fromX) * 3),
      foreground_pixels: foreground,
    };
  };
  const compare = (raw) => {
    expect(raw.length).toBe(W * H * 4);
    const rows = cases.map((c) => ({ id: c.id, ...metrics(raw, c) }));
    for (const r of rows) {
      expect(r.foreground_pixels, r.id).toBeGreaterThan(20);
      expect(r.glyph_iou, r.id).toBeGreaterThanOrEqual(budget.glyph_iou);
      expect(r.mean_rgb_error, r.id).toBeLessThanOrEqual(budget.mean_rgb_error);
    }
    return rows;
  };
  const negative = await stage(
    "frozen-original-negative-controls",
    async () => {
      const out = {};
      for (const [name, file] of Object.entries(references.negative)) {
        const m = metrics(await fs.readFile(file));
        expect(
          m.glyph_iou < budget.glyph_iou ||
            m.mean_rgb_error > budget.mean_rgb_error,
          name,
        ).toBe(true);
        out[name] = m;
      }
      return out;
    },
  );
  await stage("retained-old-policy-png-readable", async () => {
    const old = await call("opencut_job_view", {
      job_id: "job_666de950a5f64d5f99cbff3683b07d18",
    });
    expect(old.state).toBe("succeeded");
    const artifact = old.artifacts[0];
    const data = Buffer.from(
      await (await client.downloadArtifact(artifact)).arrayBuffer(),
    );
    expect(sha(data)).toBe(artifact.content_hash);
    return {
      job_id: old.job_id,
      artifact,
      original_candidate: "/private/tmp/opencut-a7-20261017/catalog-final/candidate.json",
      current_candidate: args.candidate,
      existing_bytes_verified_without_rerender: true,
    };
  });
  const preview = await stage("one-offline-composite-preview", async () => {
    if (state.preview_observation?.candidate === args.candidate) {
      const p = state.preview_observation;
      const raw = await fs.readFile(p.path);
      expect(sha(raw)).toBe(p.sha256);
      return { ...p, rows: compare(raw), observation_reused: true };
    }
    await open();
    await page.evaluate(() => {
      window.__f20Capture = true;
    });
    await page
      .getByRole("button", { name: "Play scene preview", exact: true })
      .click();
    await expect
      .poll(() => page.evaluate(() => !!window.__f20Picture), {
        timeout: 45000,
      })
      .toBe(true);
    await page
      .getByRole("button", { name: "Pause scene audio", exact: true })
      .click();
    const f = await page.evaluate(async () => {
      const p = window.__f20Picture,
        b = await p.bitmap,
        c = new OffscreenCanvas(b.width, b.height),
        ctx = c.getContext("2d");
      ctx.drawImage(b, 0, 0);
      b.close();
      const a = ctx.getImageData(0, 0, c.width, c.height).data;
      let s = "";
      for (let i = 0; i < a.length; i += 8192)
        s += String.fromCharCode(...a.subarray(i, i + 8192));
      return { time: p.time, generation: p.generation, rgba: btoa(s) };
    });
    const raw = Buffer.from(f.rgba, "base64");
    await fs.writeFile(
      path.join(args.workspace, "f20-actual-preview.rgba"),
      raw,
    );
    state.preview_observation = {
      candidate: args.candidate,
      path: path.join(args.workspace, "f20-actual-preview.rgba"),
      sha256: sha(raw),
      time: f.time,
      generation: f.generation,
      scope: "First actually presented paused static-font frame;not continuous-playback acceptance. Prior >0.15s observation timeout retained.",
    };
    await save();
    return {
      ...state.preview_observation,
      time: f.time,
      generation: f.generation,
      rows: compare(raw),
      public_downloads: false,
    };
  });
  const png = await stage("one-offline-native-worker-png", async () => {
    const current = await inspect();
    expect(current.project).toEqual(fixed.project);
    state.requests.png ??= {
      tool: "opencut_render_submit",
      input: {
        kind: "frame",
        request: {
          project_id: project,
          revision: current.revision,
          sequence_id: sequence,
          time: time(0.5),
          idempotency_key: "f20-one-font-paths-png",
        },
      },
    };
    await save();
    state.accepted ??= await call(
      state.requests.png.tool,
      state.requests.png.input,
    );
    await save();
    let job;
    const until = Date.now() + budget.output_job_observation_ms;
    do {
      job = await call("opencut_job_view", { job_id: state.accepted.job_id });
      if (["succeeded", "failed", "cancelled"].includes(job.state)) break;
      if (Date.now() > until)
        throw Error("Observation timeout;keep original job " + job.job_id);
      await new Promise((r) => setTimeout(r, 400));
    } while (true);
    expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
    const r = job.result.data;
    expect(r.identity).toMatchObject({
      project_id: project,
      revision: current.revision,
      sequence_id: sequence,
      width: W,
      height: H,
    });
    expect(new Set(r.font_lock.map((f) => f.font_id))).toEqual(
      new Set(cases.map((c) => state.stages["import-" + c.index].font.id)),
    );
    for (const c of cases) {
      const f = r.font_lock.find(
        (f) => f.font_id === state.stages["import-" + c.index].font.id,
      );
      expect(f.content_hash).toBe(c.hash);
      expect(f.size).toBe(c.bytes);
    }
    const data = Buffer.from(
      await (await client.downloadArtifact(job.artifacts[0])).arrayBuffer(),
    );
    expect(sha(data)).toBe(job.artifacts[0].content_hash);
    const file = path.join(args.workspace, "f20-" + job.job_id + ".png");
    await fs.writeFile(file, data);
    const raw = await oracle.evaluate(async (b64) => {
      const b = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0)),
        im = await createImageBitmap(new Blob([b], { type: "image/png" })),
        c = new OffscreenCanvas(im.width, im.height),
        ctx = c.getContext("2d");
      ctx.drawImage(im, 0, 0);
      im.close();
      const a = ctx.getImageData(0, 0, c.width, c.height).data;
      let s = "";
      for (let i = 0; i < a.length; i += 8192)
        s += String.fromCharCode(...a.subarray(i, i + 8192));
      return btoa(s);
    }, data.toString("base64"));
    return {
      job_id: job.job_id,
      path: file,
      artifact: job.artifacts[0],
      result: r,
      rows: compare(Buffer.from(raw, "base64")),
      public_downloads: false,
    };
  });
  await stage("actual-canvas-font-bindings-and-escape", async () => {
    // Actual-alpha selection requires a ready paused live frame. A checkpoint
    // may reuse its pixel evidence, but a new page still needs that live setup.
    if (await page.locator(".editor-scene-frame[data-time]").count() === 0) {
      await open();
      await page.getByRole("textbox", { name: "Playhead (seconds)", exact: true }).fill("0");
      await page.getByRole("button", { name: "Go to time", exact: true }).click();
      await page.evaluate(() => { window.__f20Capture = true; window.__f20Picture = null; });
      await page.getByRole("button", { name: "Play scene preview", exact: true }).click();
      await expect.poll(() => page.evaluate(() => !!window.__f20Picture), { timeout: 45000 }).toBe(true);
      const pause = page.getByRole("button", { name: "Pause scene audio", exact: true });
      if (await pause.isEnabled()) await pause.click();
    }
    await expect(page.getByRole("button", { name: "Pause scene audio", exact: true })).toBeDisabled();
    await expect(page.getByText("Scene sound paused", { exact: true })).toBeVisible();
    await expect(page.locator(".editor-scene-frame")).toHaveAttribute("data-time", /\d/, { timeout: 25000 });
    const current = await inspect();
    expect(current.project).toEqual(fixed.project);
    if (
      state.canvas_observations?.candidate !== args.candidate ||
      state.canvas_observations?.revision !== current.revision
    ) state.canvas_observations = { candidate: args.candidate, revision: current.revision, observations: [] };
    const observations = state.canvas_observations.observations;
    for (const c of cases) {
      if (observations.some((v) => v.id === c.id)) continue;
      // The actual-alpha content picker owns pointer input above legacy text
      // hit boxes. Choose visible ink from the independent original reference.
      const ink = (x, y) => {
        const p = (y * W + x) * 4;
        return Math.max(...[0, 1, 2].map((k) => Math.abs(expected[p + k] - 48))) > 64;
      };
      const candidates = [];
      for (let y = c.cy - 55; y < c.cy + 55; y++)
        for (let x = (c.index % 2) * 320 + 1; x < ((c.index % 2) + 1) * 320 - 1; x++) {
          if (!ink(x, y)) continue;
          let neighbors = 0;
          for (let dy = -1; dy <= 1; dy++)
            for (let dx = -1; dx <= 1; dx++) neighbors += Number(ink(x + dx, y + dy));
          candidates.push({ x, y, neighbors, distance: (x - c.cx) ** 2 + (y - c.cy) ** 2 });
        }
      candidates.sort((a, b) => b.neighbors - a.neighbors || a.distance - b.distance);
      const pixel = candidates[0];
      expect(pixel).toBeTruthy();
      const surface = page.locator(".editor-content-hit-surface");
      await expect(surface).toBeVisible();
      const rect = await surface.boundingBox();
      expect(rect).toBeTruthy();
      const started = performance.now();
      const root = page.locator(".editor-canvas-transform");
      const bounds = await root.boundingBox();
      expect(bounds).toBeTruthy();
      const position = {
        x: rect.x - bounds.x + (pixel.x + 0.5) * rect.width / W,
        y: rect.y - bounds.y + (pixel.y + 0.5) * rect.height / H,
      };
      await root.click({ position });
      await expect(page.getByLabel("Selected element transform handles", { exact: true })).toBeVisible({ timeout: 25000 });
      await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      const selected = await surface.boundingBox();
      expect(selected).toBeTruthy();
      await page.mouse.dblclick(
        selected.x + (pixel.x + 0.5) * selected.width / W,
        selected.y + (pixel.y + 0.5) * selected.height / H,
      );
      const input = page.getByRole("textbox", {
        name: "Edit text on canvas",
        exact: true,
      });
      await expect(input).toHaveValue(c.text, { timeout: 25000 });
      await expect.poll(
        () => input.evaluate((node) => getComputedStyle(node).fontFamily.replaceAll('"', "")),
        { timeout: 25000 },
      ).toBe("OpenCutCanvas_" + c.hash);
      const data = await input.evaluate((node) => {
        const s = getComputedStyle(node),
          family = s.fontFamily.replaceAll('"', "");
        return {
          family,
          style: s.fontStyle,
          weight: s.fontWeight,
          faces: Array.from(document.fonts)
            .filter((f) => f.family.replaceAll('"', "") === family)
            .map((f) => ({
              style: f.style,
              weight: f.weight,
              status: f.status,
            })),
        };
      });
      expect(data.family).toBe("OpenCutCanvas_" + c.hash);
      expect(data.style).toBe(c.style);
      expect(data.weight).toBe(String(c.weight));
      expect(data.faces.length).toBeGreaterThan(0);
      for (const f of data.faces)
        expect(f).toMatchObject({
          style: c.source.face.style,
          weight: String(c.weight),
          status: "loaded",
        });
      await input.press("Escape");
      await expect(input).toHaveCount(0);
      observations.push({ id: c.id, actual_alpha_doubleclick_pixel: pixel, selection_before_edit: true, edit_ready_ms: performance.now() - started, ...data });
      state.memory_peaks = peaks;
      await save();
    }
    expect((await inspect()).project).toEqual(fixed.project);
    return { observations, cancelled_without_project_write: true };
  });
  for (let i = 0; i < state.forward.length; i++) {
    await open();
    await history("restore-original-" + i, "undo");
  }
  expect((await inspect()).project).toEqual(original.project);
  await stage("restore-online-font-host", () => restart("1"));
  await open();
  expect((await inspect()).project).toEqual(original.project);
  sample();
  state.memory_peaks = peaks;
  await save();
  expect(peaks.native_tree).toBeLessThanOrEqual(
    budget.native_tree_rss_max_bytes,
  );
  expect(peaks.cold_python).toBeLessThanOrEqual(
    budget.derivation_child_rss_max_bytes,
  );
  expect(errors).toEqual([]);
  const report = {
    status: "passed_scoped_new_font_paths",
    candidate: args.candidate,
    project_id: project,
    requirements: {
      classes: blueprint,
      unavailable_families: state.stages["unavailable-original-families-ui"],
      glyph_rejection: state.stages["new-static-profile-glyph-rejection"],
      offline: state.stages["offline-service-and-replay"],
      largest_cold_cjk: state.stages["largest-cold-cjk-native-runtime"],
      ui: state.stages["actual-canvas-font-bindings-and-escape"],
      original_public_graph_restored: true,
      original_output_cache_restored: !state.cold_cache.moved,
    },
    imports: cases.map((c) => ({
      id: c.id,
      hash: c.hash,
      bytes: c.bytes,
      source: c.source,
      import: state.stages["import-" + c.index],
    })),
    reference: references,
    negative_controls: negative,
    preview,
    png,
    retained_memory_peaks: state.memory_peaks_history ?? [],
    memory_peaks: peaks,
    budget,
    builds_started_by_verifier: { native: 0, web: 0, unit_tests: 0 },
    new_project_copies: 0,
    completed_native_cold_derivations: 1,
    new_output_jobs: 1,
    scope:
      "New StaticV3 alias/bitmap/COLRv0/COLRv1,DerivedV2 renamed Latin/largest CJK and explicit static-V3/derived-V2 italic;one actual native cold largest-source derivation and one offline composite PNG. Retained3018 original faces are not rerun or rebound;this does not certify every font/browser,unavailable test labels,physical IME or whole migration.",
  };
  await fs.writeFile(args.report, JSON.stringify(report, null, 2) + "\n");
  console.log(
    JSON.stringify({
      status: report.status,
      job_id: png.job_id,
      original_restored: true,
      peak_native_rss: peaks.native_tree,
    }),
  );
} catch (error) {
  state.memory_peaks = peaks;
  await save();
  await fs.writeFile(
    path.join(args.workspace, "f20-paths-failure.json"),
    JSON.stringify(
      {
        error: String(error),
        stack: error.stack,
        peaks,
        stages: Object.keys(state.stages),
        font_calls: fontCalls,
        font_requests: fontRequests,
        form_events: await page.evaluate(() => window.__f20FormEvents),
        form_validity: await page
          .getByRole("form", { name: "Import font", exact: true })
          .evaluateAll((forms) =>
            forms.map((form) => ({
              valid: form.checkValidity(),
              controls: [...form.elements].map((e) => ({
                tag: e.tagName,
                type: e.type,
                disabled: e.disabled,
                valid: e.validity?.valid,
                message: e.validationMessage,
              })),
            })),
          ),
        alerts: await page.getByRole("alert").allTextContents(),
        status_texts: await page.getByRole("status").allTextContents(),
        canvas_network: [...canvasNetwork.values()].slice(-40),
        text_actions: await page.getByRole("button", { name: "Add text at playhead", exact: true }).evaluateAll((nodes) => nodes.map((n) => ({ disabled: n.disabled, title: n.title }))),
        form_selection: await page
          .getByRole("form", { name: "Import font", exact: true })
          .evaluateAll((forms) =>
            forms.flatMap((form) =>
              [...form.querySelectorAll("select")].map((select) => ({
                value: select.value,
                selected: select.selectedOptions[0]?.textContent,
                disabled: select.disabled,
              })),
            ),
          ),
      },
      null,
      2,
    ) + "\n",
  );
  await page
    .screenshot({
      path: path.join(args.workspace, "f20-paths-failure.png"),
      fullPage: true,
    })
    .catch(() => {});
  throw error;
} finally {
  clearInterval(timer);
  await browser.close();
  await browserServer.close();
}
