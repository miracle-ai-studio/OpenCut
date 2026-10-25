import { classicReference } from "./classic-reference.mjs";
import { editorSnapshot } from "./project-inspection.mjs";
/** One pinned font cohort, one composite, durable original requests. */
import { chromium, expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { createRequire } from "node:module";
import { OpenCutClient } from "../../../packages/sdk-ts/dist/index.js";
const args = Object.fromEntries(
  process.argv
    .slice(2)
    .reduce((a, v, i, s) => (i % 2 ? a : [...a, [v.slice(2), s[i + 1]]]), []),
);
const colorStatic = args.cohort === "color_static_v1";
if (args.cohort && !colorStatic) throw Error("Unknown font cohort");
const namespace = colorStatic ? "color-static-v1" : "distribution";
// Verify the actual service target; configuration may override CLI flags.
const runtimeCandidate = await JSON.parse(
  await fs.readFile(args.candidate, "utf8"),
);
const runtimeState = JSON.parse(
  await fs.readFile(path.join(args.workspace, "service/state.json"), "utf8"),
);
expect(await fs.realpath(runtimeState.web_root)).toBe(
  await fs.realpath(runtimeCandidate.web_root),
);
expect(await fs.realpath(runtimeState.executable)).toBe(
  await fs.realpath(args.binary),
);
const project = "fb92adbb-b999-414e-b351-35f012b8e7b8",
  W = colorStatic ? 540 : 640,
  H = colorStatic ? 300 : 480;
const read = async (p) => JSON.parse(await fs.readFile(p, "utf8")),
  sha = (b) => createHash("sha256").update(b).digest("hex");
const checkpoint = path.join(
  args.workspace,
  colorStatic
    ? "agent-font-color-static-v1-checkpoint.json"
    : "agent-font-distribution-v1-checkpoint.json",
);
const state = await read(checkpoint).catch((e) => {
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
const binaryHash = sha(await fs.readFile(args.binary));
expect(binaryHash).toBe(runtimeCandidate.binary_sha256);
if (state.binary_sha256) expect(state.binary_sha256).toBe(binaryHash);
state.binary_sha256 = binaryHash;
const save = async () => {
  await fs.writeFile(
    checkpoint + ".pending",
    JSON.stringify(state, null, 2) + "\n",
  );
  await fs.rename(checkpoint + ".pending", checkpoint);
};
const stage = async (name, run) => {
  if (state.stages[name]) {
    console.log("Reuse " + name);
    return state.stages[name];
  }
  console.log("Begin " + name);
  const value = await run();
  state.stages[name] = value;
  await save();
  console.log("Passed " + name);
  return value;
};
const layoutStage = (name, run) =>
  stage(colorStatic ? "grid-" + name : name, run);
const client = new OpenCutClient({
  baseUrl: args.endpoint,
  token: (await fs.readFile(args.credential, "utf8")).trim(),
});
const call = (name, input) => client.call(name, input),
  inspect = () =>
    editorSnapshot(call("opencut_project_inspect", { project_id: project, revision: null }));
const cli = (name, input) => {
  const r = JSON.parse(
    execFileSync(
      args.binary,
      [
        "--json",
        "--database",
        path.join(args.workspace, "workspace.sqlite"),
        "call",
        name,
        "--input",
        "-",
      ],
      { input: JSON.stringify(input), encoding: "utf8", timeout: 45000 },
    ),
  );
  expect(r.error).toBeNull();
  return r.data;
};
const privateGraph = (revision) =>
  JSON.parse(
    execFileSync(
      "python3",
      [
        "-c",
        "import sqlite3,json,sys; c=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro',uri=True); print(c.execute('select document_json from revisions where revision=?',(sys.argv[2],)).fetchone()[0])",
        path.join(args.workspace, "workspace.sqlite"),
        revision,
      ],
      { encoding: "utf8" },
    ),
  );
const time = (n) => ({ ticks: String(n * 120000), timebase: 120000 });
const original = await stage("original-project", async () => {
  const s = await inspect();
  return { snapshot: s, private: privateGraph(s.revision) };
});
const sequence = original.snapshot.project.default_sequence_id;
const pins = await read(
  path.join(
    process.cwd(),
    colorStatic
      ? "crates/editor-application/src/font_color_static/catalog_pins.json"
      : "crates/editor-application/src/font_catalog/distribution_source_pins.json",
  ),
);
const cases = pins.map((p, index) =>
  colorStatic
    ? (() => {
        const proof = JSON.parse(p.proof_json);
        return {
          index,
          id: p.face.id,
          family: proof.source.source_family,
          weight: p.face.weight,
          style: "normal",
          text: "HAMBURG 39",
          cx: 90 + (index % 3) * 180,
          cy: 25 + Math.floor(index / 3) * 50,
          size: 24,
          importTool: "opencut_font_import",
          proofTool: "opencut_font_derived_proof",
          native: p,
          axes: p.axes,
          oracle_path: proof.source.source_path,
          pin: {
            source: {
              ...p.face,
              size_bytes: Number(p.face.size_bytes),
              catalog_id: p.face.id,
              license_notice: p.license_notice,
            },
            source_path: "derived/" + p.face.content_hash,
          },
        };
      })()
    : {
        index,
        id: p.source.catalog_id,
        family: p.display_family,
        weight: p.source.weight,
        text: index < 2 ? "\ue88a \ue8b8" : "Pushster AV",
        cx: 320,
        cy: 100 + index * 140,
        size: index < 2 ? 48 : 42,
        pin: p,
      },
);
if (colorStatic) {
  const catalog = await call("opencut_font_catalog", {});
  const face = catalog.fonts.find(
    (f) =>
      f.kind === "synthesis" &&
      f.display_family === "Foldit" &&
      f.weight === 700 &&
      f.id.includes("color-static-v1"),
  );
  expect(face?.import_available).toBe(true);
  const selectedSource = await call("opencut_font_source", {catalog_id:face.id});
  expect(selectedSource.details.kind).toBe("synthesis");
  const source=selectedSource.details.source;
  const base = cases.find((c) => c.id === source.binding.actual.id);
  expect(base).toBeDefined();
  cases.push({
    ...base,
    index: cases.length,
    id: face.id,
    style: "italic",
    cx: 90 + (cases.length % 3) * 180,
    cy: 25 + Math.floor(cases.length / 3) * 50,
    importTool: "opencut_font_import",
    proofTool: "opencut_font_synthesis_proof",
    synthesis: source,
  });
}
for (const c of cases) c.importTool ??= "opencut_font_import";
const evidence = await stage("distribution-provenance", async () => {
  if (colorStatic) {
    const catalog = await call("opencut_font_catalog", {});
    expect(cases.filter(c=>!c.synthesis).every(c=>catalog.fonts.some(f=>f.id===c.id&&f.kind==="derived"))).toBe(true);
    const result = [];
    for (const c of cases) {
      const source =
        c.synthesis ??
        (await call("opencut_font_source", {
          catalog_id: c.id,
        }));
      if (!c.synthesis) {
        expect(source.details).toEqual({kind:"derived",source:c.native});
        expect(source.face).toEqual(catalog.fonts.find((f) => f.id === c.id));
        expect(source.face.import_available).toBe(true);
        expect(source.license_sha256).toBe(
          sha(Buffer.from(c.native.license_notice)),
        );
        if (c.index === 0)
          expect(
            cli("opencut_font_source", { catalog_id: c.id }),
          ).toEqual(source);
      }
      const bytes = await fs.readFile(
        path.join(args.sources, c.pin.source_path),
      );
      expect(sha(bytes)).toBe(c.pin.source.content_hash);
      expect(bytes.length).toBe(Number(c.pin.source.size_bytes));
      result.push(source);
    }
    return result;
  }
  const catalog = await call("opencut_font_catalog", {});
  expect(cases.every(c=>catalog.fonts.some(f=>f.id===c.id&&f.kind==="distribution"))).toBe(true);
  const result = [];
  for (const c of cases) {
    const source = await call("opencut_font_source", {
      catalog_id: c.id,
    });
    expect(
      cli("opencut_font_source", { catalog_id: c.id }),
    ).toEqual(source);
    expect(source.face).toEqual(catalog.fonts.find((f) => f.id === c.id));
    expect(source.face.family).toBe(c.pin.source.family);
    expect(source.details.source.provenance).toEqual(c.pin.distribution);
    expect(source.details.source.license_notice).toBe(c.pin.source.license_notice);
    expect(sha(Buffer.from(source.details.source.license_notice))).toBe(source.license_sha256);
    const bytes = await fs.readFile(path.join(args.sources, c.pin.source_path));
    expect(sha(bytes)).toBe(source.face.content_hash);
    expect(bytes.length).toBe(Number(source.face.size_bytes));
    result.push(source);
  }
  try {
    await call("opencut_font_source", {
      catalog_id: cases[0].id,
      kind: "static_v3",
    });
    throw Error("caller protocol selector accepted");
  } catch (e) {
    expect(e.code).toBe("INVALID_ARGUMENT");
  }
  return result;
});
const browser = await chromium.launch({
  headless: true,
  executablePath: process.env.OPENCUT_CHROME,
});
const page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1250 },
  }),
  oracle = await browser.newPage();
page.setDefaultTimeout(30000);
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
await page.addInitScript(() => {
  const original =
    ImageBitmapRenderingContext.prototype.transferFromImageBitmap;
  ImageBitmapRenderingContext.prototype.transferFromImageBitmap = function (
    bitmap,
  ) {
    if (
      bitmap &&
      window.__distributionCapture &&
      !window.__distributionPicture &&
      this.canvas.classList.contains("editor-scene-frame") &&
      this.canvas.dataset.time !== "" &&
      Number(this.canvas.dataset.time) >= 0
    ) {
      window.__distributionPicture = {
        time: Number(this.canvas.dataset.time),
        bitmap: createImageBitmap(bitmap),
      };
    }
    return original.call(this, bitmap);
  };
});
const open = async () => {
  await page.goto(`${args.endpoint}/editor/${project}?scene=${sequence}`);
  await expect(page.locator(".save-status")).toHaveText("Saved");
  const guide = page.getByRole("dialog", {
    name: "Welcome to OpenCut",
    exact: true,
  });
  if (await guide.isVisible())
    await guide.getByRole("button", { name: "Close", exact: true }).click();
};
const record = async (name, tool, input, receipt, forward = true) => {
  expect(state.requests[name] ?? { tool, input }).toEqual({ tool, input });
  state.requests[name] = { tool, input };
  state.receipts[name] = receipt;
  if (
    forward &&
    !state.forward.some((r) => r.transaction_id === receipt.transaction_id)
  )
    state.forward.push(receipt);
  await save();
  return receipt;
};
const mutate = async (name, tool, input, forward = true) => {
  const q = state.requests[name] ?? { tool, input };
  expect(q).toEqual({ tool, input });
  state.requests[name] = q;
  await save();
  return record(name, tool, input, await call(tool, input), forward);
};
try {
  for (const c of cases)
    await stage("import-" + c.index, async () => {
      let receipt, q;
      if (c.index === 0 && !state.requests["import-0"]) {
        await open();
        await page
          .getByRole("button", { name: "Import font", exact: true })
          .click();
        const form = page.getByRole("form", {
          name: "Import font",
          exact: true,
        });
        await form
          .getByRole("combobox", { name: /^Font family(?:\s|$)/ })
          .selectOption(c.family);
        await form
          .getByRole("combobox", { name: /^Font style(?:\s|$)/ })
          .selectOption(`${c.weight}:normal`);
        await form
          .getByRole("textbox", { name: /^Text to check(?:\s|$)/ })
          .fill(c.text);
        await expect(form).toContainText(
          colorStatic ? c.native.face.family : c.pin.distribution.release,
        );
        await expect(
          form.getByRole("button", { name: "Import font", exact: true }),
        ).toBeEnabled();
        await page.route(
          "**/api/tools/" + c.importTool,
          async (route) => {
            const input = route.request().postDataJSON();
            state.requests["import-0"] = {
              tool: c.importTool,
              input,
            };
            await save();
            const response = await route.fetch();
            const result = await response.json();
            expect(result.error).toBeNull();
            await record("import-0", c.importTool, input, result.data);
            await route.abort("failed");
          },
          { times: 1 },
        );
        await form
          .getByRole("button", { name: "Import font", exact: true })
          .click();
        await expect
          .poll(() => !!state.receipts["import-0"], { timeout: 60000 })
          .toBe(true);
        await page.reload();
        const recovery = page.getByRole("region", {
          name: "Recover original font import",
          exact: true,
        });
        await expect(recovery).toContainText("Original font import saved.");
        await recovery
          .getByRole("button", {
            name: "Release resolved font import",
            exact: true,
          })
          .click();
        await expect(
          recovery.getByRole("button", {
            name: "Release resolved font import",
            exact: true,
          }),
        ).toBeHidden();
        q = state.requests["import-0"].input;
        receipt = state.receipts["import-0"];
      } else {
        q = state.requests["import-" + c.index]?.input ?? {
          project_id: project,
          expected_revision: (await inspect()).revision,
          idempotency_key: namespace + "-font-" + c.index,
          catalog_id: c.id,
          font_id: namespace + "-font-" + c.index,
          required_text: c.text,
        };
        receipt = await mutate("import-" + c.index, c.importTool, q);
      }
      expect(await call(c.importTool, q)).toEqual(receipt);
      const font = (await inspect()).project.fonts.find(
        (f) => f.id === q.font_id,
      );
      expect(font.family).toBe(c.pin.source.family);
      expect(font.weight).toBe(c.weight);
      expect(font.content.content_hash).toBe(c.pin.source.content_hash);
      expect(font.content.size).toBe(c.pin.source.size_bytes);
      expect(font.license.notice).toBe(c.pin.source.license_notice);
      let proof;
      if (colorStatic) {
        proof = await call(c.proofTool, {
          project_id: project,
          revision: receipt.revision,
          font_id: q.font_id,
        });
        if(!c.synthesis)expect(JSON.parse(proof.source.proof_json).schema_version).toBe(4);
        if (c.synthesis)
          expect(
            JSON.parse(proof.source.source.proof_json).schema_version,
          ).toBe(3);
        expect(proof.source).toEqual(c.synthesis ?? c.native);
        expect(font.origin.provider).toBe(
          c.synthesis
            ? "opencut-chromium-font-synthesis-v1"
            : "opencut-fonttools-static-colr-v1",
        );
      }
      return { request: q, receipt, font, ...(proof ? { proof } : {}) };
    });
  if (!colorStatic)
    await stage("pushster-actual-family-ui", async () => {
      await open();
      await page
        .getByRole("button", { name: "Import font", exact: true })
        .click();
      const form = page.getByRole("form", { name: "Import font", exact: true });
      await form
        .getByRole("combobox", { name: /^Font family(?:\s|$)/ })
        .selectOption("Pushster");
      await expect(form).toContainText("Imported font family: Lobster");
      await expect(form).toContainText(
        "does not certify a separate Pushster design",
      );
      await form.getByRole("button", { name: "Cancel", exact: true }).click();
      return {
        display: "Pushster",
        native: "Lobster",
        explicit_source_notice: true,
      };
    });
  const fixed = await layoutStage("combined-scene", async () => {
    const head = await inspect(),
      q = structuredClone(
        head.project.sequences.find((s) => s.id === sequence),
      );
    if (colorStatic)
      q.tracks = q.tracks.filter(
        (t) => !t.id.startsWith(namespace + "-texts-"),
      );
    for (const t of q.tracks) t.enabled = false;
    for (const c of cases)
      q.tracks.push({
        id: namespace + "-texts-" + c.index,
        name: c.family,
        kind: "text",
        enabled: true,
        muted: false,
        locked: false,
        opacity: 1,
        gain: { unit: "decibels", value: 0 },
        channels: [],
        elements: [
          {
            id: namespace + "-text-" + c.index,
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
                font: {
                  kind: "locked",
                  font_id: state.stages["import-" + c.index].font.id,
                },
                font_weight: c.weight === 700 ? "bold" : "normal",
                font_style: c.style ?? "normal",
                decoration: "none",
                align: "center",
                font_size: {
                  unit: "classic_height90",
                  value: (c.size * 90) / H,
                },
                color: {
                  encoding: "srgb",
                  red: 1,
                  green: 1,
                  blue: 1,
                  alpha: 1,
                },
                letter_spacing_px: 0,
                line_height_multiplier: 1.2,
                background: {
                  enabled: false,
                  color: {
                    encoding: "srgb",
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 0,
                  },
                  corner_radius_percent: 0,
                  padding_x_at_size15: 0,
                  padding_y_at_size15: 0,
                  offset_x_px: 0,
                  offset_y_px: 0,
                },
              },
            },
          },
        ],
      });
    const settings = structuredClone(head.project.settings);
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
    await mutate(
      colorStatic ? "grid-combined-scene" : "combined-scene",
      "opencut_edit_apply",
      {
        project_id: project,
        expected_revision: head.revision,
        idempotency_key:
          namespace + "-one-scene" + (colorStatic ? "-grid" : ""),
        label: colorStatic
          ? "Seventeen fixed complete color fonts and explicit italic"
          : "Three fixed official font distributions",
        operations: [
          { type: "settings_set", settings },
          { type: "sequence_put", sequence: q },
        ],
      },
    );
    return inspect();
  });
  const reference = await layoutStage(
    "independent-classic-reference",
    async () => {
      const ts = createRequire(import.meta.url)("typescript"),
        modules = {},
        original_modules = [];
      for (const id of [
        "text/typography",
        "text/layout",
        "text/background",
        "text/primitives",
        "utils/math",
        "timeline/defaults",
        "subtitles/build-subtitle-text-element",
      ]) {
        const rel = "apps/web/src/" + id + ".ts",
          raw = await fs.readFile(classicReference(rel), "utf8");
        modules[id] = ts.transpileModule(raw, {
          compilerOptions: {
            module: ts.ModuleKind.CommonJS,
            target: ts.ScriptTarget.ES2022,
          },
        }).outputText;
        original_modules.push({ path: rel, sha256: sha(Buffer.from(raw)) });
      }
      const fonts = await Promise.all(
        cases.map(async (c) =>
          (
            await fs.readFile(
              path.join(args.sources, c.oracle_path ?? c.pin.source_path),
            )
          ).toString("base64"),
        ),
      );
      const rendered = await oracle.evaluate(
        async ({ modules, fonts, cases, W, H }) => {
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
            p = load("text/primitives"),
            encode = (b) => {
              let s = "";
              for (let i = 0; i < b.length; i += 8192)
                s += String.fromCharCode(...b.subarray(i, i + 8192));
              return btoa(s);
            };
          for (const c of cases) {
            const bytes = Uint8Array.from(atob(fonts[c.index]), (x) =>
                x.charCodeAt(0),
              ),
              face = new FontFace("DistributionOracle_" + c.index, bytes, {
                weight: String(c.weight),
                style: "normal",
                ...(c.axes
                  ? {
                      variationSettings: c.axes
                        .map((a) => `"${a.tag}" ${a.coordinate}`)
                        .join(", "),
                    }
                  : {}),
              });
            await face.load();
            document.fonts.add(face);
          }
          const draw = (fallback) => {
            const canvas = new OffscreenCanvas(W, H),
              ctx = canvas.getContext("2d");
            ctx.fillStyle = "#303030";
            ctx.fillRect(0, 0, W, H);
            for (const c of cases) {
              const text = {
                  content: c.text,
                  fontFamily: fallback
                    ? "sans-serif"
                    : "DistributionOracle_" + c.index,
                  fontSize: (c.size * 90) / H,
                  fontWeight: c.weight === 700 ? "bold" : "normal",
                  fontStyle: c.style ?? "normal",
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
            return encode(ctx.getImageData(0, 0, W, H).data);
          };
          return { expected: draw(false), fallback: draw(true) };
        },
        { modules, fonts, cases, W, H },
      );
      const file = path.join(
          args.artifacts,
          (colorStatic ? "grid-" : "") + "classic-reference.rgba",
        ),
        negative = path.join(
          args.artifacts,
          (colorStatic ? "grid-" : "") + "fallback-reference.rgba",
        );
      await fs.writeFile(file, Buffer.from(rendered.expected, "base64"));
      await fs.writeFile(negative, Buffer.from(rendered.fallback, "base64"));
      return {
        path: file,
        sha256: sha(Buffer.from(rendered.expected, "base64")),
        negative,
        original_modules,
      };
    },
  );
  const expected = await fs.readFile(reference.path);
  expect(sha(expected)).toBe(reference.sha256);
  const metrics = (raw, c) => {
    let union = 0,
      intersection = 0,
      error = 0,
      foreground = 0;
    const radius = colorStatic ? 25 : 65,
      y0 = c.cy - radius,
      y1 = c.cy + radius;
    const x0 = colorStatic ? c.cx - 90 : 0,
      x1 = colorStatic ? c.cx + 90 : W;
    for (let y = y0; y < y1; y++)
      for (let x = x0; x < x1; x++) {
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
      mean_rgb_error: error / ((y1 - y0) * (x1 - x0) * 3),
      foreground_pixels: foreground,
    };
  };
  const compare = async (raw, label) => {
    expect(raw.length).toBe(W * H * 4);
    const rows = cases.map((c) => ({ id: c.id, ...metrics(raw, c) }));
    await fs.writeFile(
      path.join(args.artifacts, label + "-metrics.json"),
      JSON.stringify(rows, null, 2) + "\n",
    );
    for (const m of rows) {
      expect(m.foreground_pixels).toBeGreaterThan(20);
      expect(m.glyph_iou, m.id).toBeGreaterThan(0.99);
      expect(m.mean_rgb_error, m.id).toBeLessThanOrEqual(2);
    }
    return rows;
  };
  await layoutStage("oracle-negative-control", async () => {
    const raw = await fs.readFile(reference.negative);
    const rows = cases.map((c) => metrics(raw, c));
    expect(rows.every((r) => r.glyph_iou < 0.99)).toBe(true);
    return rows;
  });
  const previewCapture =
    state.stages[colorStatic ? "grid-actual-preview" : "actual-preview"] ??
    (await layoutStage("actual-preview-capture", async () => {
      await open();
      await page.evaluate(() => {
        window.__distributionCapture = true;
      });
      await page
        .getByRole("button", { name: "Play scene preview", exact: true })
        .click();
      await expect
        .poll(() => page.evaluate(() => !!window.__distributionPicture), {
          timeout: 45000,
        })
        .toBe(true);
      await page
        .getByRole("button", { name: "Pause scene audio", exact: true })
        .click();
      const result = await page.evaluate(async () => {
        const p = window.__distributionPicture,
          b = await p.bitmap,
          c = new OffscreenCanvas(b.width, b.height),
          ctx = c.getContext("2d");
        ctx.drawImage(b, 0, 0);
        b.close();
        const a = ctx.getImageData(0, 0, c.width, c.height).data;
        let s = "";
        for (let i = 0; i < a.length; i += 8192)
          s += String.fromCharCode(...a.subarray(i, i + 8192));
        return { time: p.time, rgba: btoa(s) };
      });
      const raw = Buffer.from(result.rgba, "base64"),
        file = path.join(args.artifacts, "actual-preview.rgba");
      await fs.writeFile(file, raw);
      return {
        time: result.time,
        path: file,
        sha256: sha(raw),
      };
    }));
  const preview = await layoutStage("actual-preview", async () => {
    const raw = await fs.readFile(previewCapture.path);
    expect(sha(raw)).toBe(previewCapture.sha256);
    return { ...previewCapture, rows: await compare(raw, "preview") };
  });
  const png = await layoutStage("one-native-png", async () => {
    const request = state.requests.png ?? {
      tool: "opencut_render_submit",
      input: {
        kind: "frame",
        request: {
          project_id: project,
          revision: fixed.revision,
          sequence_id: sequence,
          time: time(0.5),
          idempotency_key: namespace + "-one-png",
        },
      },
    };
    state.requests.png = request;
    await save();
    state.accepted ??= await call(request.tool, request.input);
    await save();
    let job;
    const until = Date.now() + 180000;
    do {
      job = await call("opencut_job_view", { job_id: state.accepted.job_id });
      if (["succeeded", "failed", "cancelled"].includes(job.state)) break;
      expect(
        Date.now(),
        "Observation timeout;keep original " + job.job_id,
      ).toBeLessThan(until);
      await new Promise((r) => setTimeout(r, 400));
    } while (true);
    expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
    const result = job.result.data;
    expect(result.identity).toMatchObject({
      project_id: project,
      revision: fixed.revision,
      sequence_id: sequence,
      width: W,
      height: H,
    });
    expect(result.font_lock).toHaveLength(cases.length);
    for (const c of cases) {
      const lock = result.font_lock.find(
        (f) => f.font_id === state.stages["import-" + c.index].font.id,
      );
      expect(lock.content_hash).toBe(c.pin.source.content_hash);
      expect(lock.size).toBe(c.pin.source.size_bytes);
    }
    const bytes = Buffer.from(
      await (await client.downloadArtifact(job.artifacts[0])).arrayBuffer(),
    );
    expect(sha(bytes)).toBe(job.artifacts[0].content_hash);
    const file = path.join(args.artifacts, "distribution-fonts.png");
    await fs.writeFile(file, bytes);
    const raw = await oracle.evaluate(async (value) => {
      const b = Uint8Array.from(atob(value), (c) => c.charCodeAt(0)),
        bitmap = await createImageBitmap(new Blob([b], { type: "image/png" })),
        canvas = new OffscreenCanvas(bitmap.width, bitmap.height),
        ctx = canvas.getContext("2d");
      ctx.drawImage(bitmap, 0, 0);
      bitmap.close();
      const a = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
      let s = "";
      for (let i = 0; i < a.length; i += 8192)
        s += String.fromCharCode(...a.subarray(i, i + 8192));
      return btoa(s);
    }, bytes.toString("base64"));
    return {
      job_id: job.job_id,
      path: file,
      artifact: job.artifacts[0],
      result,
      rows: await compare(Buffer.from(raw, "base64"), "native-png"),
    };
  });
  await browser.close();
  await stage("restore-complete-project", async () => {
    for (const r of [...state.forward].reverse()) {
      const name = "undo-" + r.transaction_id;
      if (state.receipts[name]) continue;
      const input = state.requests[name]?.input ?? {
        project_id: project,
        expected_revision: (await inspect()).revision,
        target_transaction_id: r.transaction_id,
        idempotency_key: name,
      };
      const plan = await call("opencut_edit_undo_plan", input);
      expect(plan.conflicts).toEqual([]);
      await mutate(name, "opencut_edit_undo_apply", input, false);
    }
    const restored = await inspect();
    expect(restored.project).toEqual(original.snapshot.project);
    expect(privateGraph(restored.revision)).toEqual(original.private);
    return {
      revision: restored.revision,
      public_and_private_graph_equal: true,
    };
  });
  expect(errors).toEqual([]);
  const report = {
    status: colorStatic
      ? "passed_scoped_seventeen_static_colr_and_explicit_italic"
      : "passed_scoped_three_official_font_distributions",
    candidate: args.candidate,
    checkpoint,
    imports: cases.map((c) => ({
      source: evidence[c.index],
      ...state.stages["import-" + c.index],
    })),
    independent_reference: reference,
    preview,
    png,
    restoration: state.stages["restore-complete-project"],
    page_errors: errors,
    limits: colorStatic
      ? [
          "Seventeen complete static COLR outputs and one explicit Chromium italic only; source-derived expected pixels use original variable fonts and original Classic layout.",
          "Not full per-font/platform/runtime qualification, sustained FPS or full migration completion.",
          "Existing metadata/distribution/large-font/NASA/ASR and300s source evidence remain at their recorded candidates and are not rerun.",
        ]
      : [
          "Three official CSS distributions only;not full per-font/platform/native runtime qualification or sustainedFPS.",
          "Pushster is the explicitly labelled official Lobster distribution;no separate intended Pushster design is asserted.",
          "Existing font classes,large-font continuity,NASA/ASR and300s long-source evidence remain at their recorded candidates;not rerun.",
        ],
  };
  await fs.writeFile(args.report, JSON.stringify(report, null, 2) + "\n");
  console.log(
    JSON.stringify({
      status: report.status,
      png: png.job_id,
      report: args.report,
    }),
  );
} catch (error) {
  await fs.writeFile(
    args.report,
    JSON.stringify(
      {
        status: "failed",
        candidate: args.candidate,
        error: String(error.stack ?? error),
        checkpoint,
        stages: Object.keys(state.stages),
        page_errors: errors,
      },
      null,
      2,
    ) + "\n",
  );
  await browser.close().catch(() => {});
  throw error;
}
