import { classicReference } from "./classic-reference.mjs";
import { editorSnapshot } from "./project-inspection.mjs";
/** F24: one retained-project catalog/search/acquisition/placement/alpha/export flow. */
import { chromium, expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import ts from "typescript";
import { OpenCutClient } from "../../../packages/sdk-ts/dist/index.js";
const args = Object.fromEntries(
  process.argv
    .slice(2)
    .reduce((a, v, i, s) => (i % 2 ? a : [...a, [v.slice(2), s[i + 1]]]), []),
);
const project = "fb92adbb-b999-414e-b351-35f012b8e7b8",
  file = path.join(args.workspace, "agent-catalog-images-v4-checkpoint.json");
const sha = (b) => createHash("sha256").update(b).digest("hex"),
  time = (s) => ({ ticks: String(s * 120000), timebase: 120000 });
const read = async (p) => JSON.parse(await fs.readFile(p, "utf8"));
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
  done?.status === "passed_scoped_f24_catalog_images" &&
  done.candidate === args.candidate
) {
  console.log("Reuse completed F24 flow");
  process.exit(0);
}
const save = async () => {
  await fs.writeFile(file + ".pending", JSON.stringify(state, null, 2) + "\n");
  await fs.rename(file + ".pending", file);
};
const stage = async (name, run) => {
  if (state.stages[name]) return state.stages[name];
  console.log("Begin " + name);
  const value = await run();
  state.stages[name] = value;
  await save();
  console.log("Passed " + name);
  return value;
};
const client = new OpenCutClient({
  baseUrl: args.endpoint,
  token: (await fs.readFile(args.credential, "utf8")).trim(),
});
const call = (tool, input) => client.call(tool, input),
  inspect = () =>
    editorSnapshot(call("opencut_project_inspect", { project_id: project, revision: null }));
const original = await stage("original", inspect),
  sequence = original.project.sequences[0].id;
const browser = await chromium.launch({
  headless: true,
  executablePath: process.env.OPENCUT_CHROME,
});
const page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1300 },
  }),
  oracle = await browser.newPage();
page.setDefaultTimeout(20000);
const errors = [],
  egress = [];
page.on("pageerror", (e) => errors.push(e.message));
await page.route("**/*", async (route) => {
  const u = new URL(route.request().url());
  if (["http:", "https:"].includes(u.protocol) && u.origin !== args.endpoint) {
    egress.push(u.origin);
    await route.abort();
  } else await route.continue();
});
await page.addInitScript(() => {
  const transfer =
    ImageBitmapRenderingContext.prototype.transferFromImageBitmap;
  ImageBitmapRenderingContext.prototype.transferFromImageBitmap = function (
    bitmap,
  ) {
    if (
      bitmap &&
      window.__f24Capture &&
      !window.__f24Picture &&
      this.canvas.classList.contains("editor-scene-frame") &&
      Number(this.canvas.dataset.time) > 0.15
    ) {
      window.__f24Picture = {
        time: Number(this.canvas.dataset.time),
        generation: this.canvas.dataset.generation,
        bitmap: createImageBitmap(bitmap),
      };
    }
    return transfer.call(this, bitmap);
  };
});
const panel = page.getByRole("region", {
  name: "Resource catalog",
  exact: true,
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
  await expect(panel).toBeVisible();
};
const closePreview = async () => {
  const p = page.getByRole("dialog", {
    name: "Source media preview",
    exact: true,
  });
  if (await p.isVisible())
    await p.getByRole("button", { name: "Close", exact: true }).click();
};
const record = async (name, tool, input, reply, forward = true) => {
  state.requests[name] ??= { tool, input };
  expect(state.requests[name]).toEqual({ tool, input });
  state.receipts[name] = reply;
  if (forward && !state.forward.includes(name)) state.forward.push(name);
  await save();
};
const mutation = async (name, tool, input) => {
  state.requests[name] ??= { tool, input };
  await save();
  const q = state.requests[name];
  expect(q.tool).toBe(tool);
  const reply = await call(tool, q.input);
  await record(name, tool, q.input, reply);
  return inspect();
};
const ui = async (name, tool, act, { forward = true } = {}) => {
  if (state.requests[name]) {
    const q = state.requests[name],
      reply = await call(q.tool, q.input);
    await record(name, q.tool, q.input, reply, forward);
    await open();
    return inspect();
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
    return inspect();
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
      { forward: false },
    ),
  );
const catalog = await stage("full-catalog-inventory", async () => {
  const value = await call("opencut_catalog_resources_list", {});
  expect(value.entries.filter((e) => e.category === "flags")).toHaveLength(254);
  expect(value.entries.filter((e) => e.category === "shapes")).toHaveLength(4);
  const all = [];
  let offset = 0,
    pages = 0;
  do {
    const p = await call("opencut_brand_resources_list", {
      query: null,
      offset,
      limit: 100,
    });
    expect(p.total).toBe(3466);
    expect(p.matched).toBe(3466);
    all.push(...p.entries);
    pages++;
    if (p.next_offset === null) break;
    offset = p.next_offset;
  } while (true);
  const local = await read(
    path.join(
      args.root,
      "crates/editor-application/src/brand_resources/inventory.json",
    ),
  );
  expect(all).toEqual(local.map((x) => x.entry));
  expect(all.filter((x) => x.import_available)).toHaveLength(80);
  return {
    entries: value.entries,
    brand_count: all.length,
    brand_pages: pages,
    admitted_brands: 80,
    unavailable_brands: 3386,
  };
});
const flagIds = ["flags:gb-eng", "flags:np"],
  brandId = "simple-icons:16.35.0:activitypub";
const sourceDefinitions = await stage(
  "independent-original-sources",
  async () => {
    const flags = await Promise.all(
      flagIds.map(async (id) => {
        const e = catalog.entries.find((e) => e.id === id),
          svg = await fs.readFile(
            path.join(
              args.root,
              "apps/web/src/editor/workspace/scene/catalog-flags-v1",
              id.slice(6) + ".svg",
            ),
            "utf8",
          );
        expect(sha(svg)).toBe(e.source_sha256);
        return {
          id,
          name: e.name,
          svg,
          width: e.width,
          height: e.height,
          hash: e.source_sha256,
        };
      }),
    );
    const b = await call("opencut_brand_resources_source", {
        catalog_id: brandId,
      }),
      local = (
        await read(
          path.join(
            args.root,
            "crates/editor-application/src/brand_resources/sources.json",
          ),
        )
      )[brandId];
    expect(b.svg).toBe(local);
    expect(sha(local)).toBe(b.entry.svg_sha256);
    expect(b.license.identifier).toBe("CC0-1.0");
    return [
      ...flags,
      {
        id: brandId,
        name: b.entry.name,
        svg: local,
        width: 512,
        height: 512,
        hash: b.entry.svg_sha256,
      },
    ];
  },
);
const rasterize = async (spec) =>
  oracle.evaluate(async ({ svg, width, height }) => {
    const image = new Image();
    image.src =
      "data:image/svg+xml;base64," + btoa(unescape(encodeURIComponent(svg)));
    await image.decode();
    const c = new OffscreenCanvas(width, height),
      ctx = c.getContext("2d");
    ctx.drawImage(image, 0, 0, width, height);
    return Array.from(ctx.getImageData(0, 0, width, height).data);
  }, spec);
const referenceSources = [];
for (const s of sourceDefinitions)
  referenceSources.push({ ...s, raw: Uint8Array.from(await rasterize(s)) });
const ellipsePath = classicReference("apps/web/src/graphics/definitions/ellipse.ts"),
  ellipseSource = await fs.readFile(ellipsePath, "utf8"),
  ellipseJs = ts.transpileModule(ellipseSource, {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022,
    },
  }).outputText;
const ellipseRaw = await oracle.evaluate((code) => {
  const exports = {},
    require = (id) => {
      if (id === "../stroke")
        return {
          applyAlignedStroke() {
            throw Error("Unexpected stroke in no-stroke reference");
          },
        };
      if (id === "./shared") return { STROKE_ALIGN_PARAM: {} };
      throw Error("Unexpected original source dependency " + id);
    };
  new Function("exports", "require", code)(exports, require);
  const c = new OffscreenCanvas(80, 50),
    ctx = c.getContext("2d");
  exports.ellipseGraphicDefinition.render({
    ctx,
    params: {
      fill: "rgba(112,192,64,0.7)",
      stroke: "#000000",
      strokeWidth: 0,
      strokeAlign: "center",
    },
    width: 80,
    height: 50,
  });
  return Array.from(ctx.getImageData(0, 0, 80, 50).data);
}, ellipseJs);
referenceSources.push({
  id: "original-classic-ellipse",
  width: 80,
  height: 50,
  raw: Uint8Array.from(ellipseRaw),
});
const decode = async (bytes) =>
  Uint8Array.from(
    await oracle.evaluate(async (b64) => {
      const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0)),
        im = await createImageBitmap(new Blob([bytes], { type: "image/png" })),
        c = new OffscreenCanvas(im.width, im.height),
        ctx = c.getContext("2d");
      ctx.drawImage(im, 0, 0);
      return Array.from(ctx.getImageData(0, 0, im.width, im.height).data);
    }, bytes.toString("base64")),
  );
try {
  await stage("isolate-original-tracks-once", async () => {
    const before = await inspect(),
      q = structuredClone(before.project.sequences[0]);
    expect(q.transitions).toHaveLength(0);
    expect(q.captions).toHaveLength(0);
    for (const t of q.tracks) t.enabled = false;
    const settings = structuredClone(before.project.settings);
    settings.settings.background = {
      kind: "color",
      color: {
        encoding: "srgb",
        red: 32 / 255,
        green: 48 / 255,
        blue: 64 / 255,
        alpha: 1,
      },
    };
    return mutation("isolate", "opencut_edit_apply", {
      project_id: project,
      expected_revision: before.revision,
      idempotency_key: "f24-isolate",
      label: "F24 combined resource acceptance",
      operations: [
        { type: "sequence_put", sequence: q },
        { type: "settings_set", settings },
      ],
    });
  });
  await open();
  await stage("three-category-search-and-unavailable", async () => {
    await panel
      .getByRole("combobox", { name: /^Category(?:\s|$)/ })
      .selectOption("flags");
    await panel
      .getByLabel("Search catalog", { exact: true })
      .fill("english red");
    await expect(
      panel
        .locator("article")
        .filter({ has: page.getByText("England", { exact: true }) }),
    ).toHaveCount(1);
    await panel
      .getByLabel("Search catalog", { exact: true })
      .fill("zz-absent-resource");
    await expect(panel.locator("article")).toHaveCount(0);
    await panel
      .getByRole("combobox", { name: /^Category(?:\s|$)/ })
      .selectOption("shapes");
    await panel.getByLabel("Search catalog", { exact: true }).fill("oval");
    await expect(
      panel
        .getByRole("combobox", { name: /^Graphic shape(?:\s|$)/ })
        .locator("option"),
    ).toHaveCount(1);
    await expect(
      panel.getByRole("combobox", { name: /^Graphic shape(?:\s|$)/ }),
    ).toHaveValue("ellipse");
    await panel
      .getByLabel("Search catalog", { exact: true })
      .fill("absent-shape");
    await expect(
      panel.getByText("No matching shapes.", { exact: true }),
    ).toBeVisible();
    await panel
      .getByRole("combobox", { name: /^Category(?:\s|$)/ })
      .selectOption("brands");
    await panel
      .getByLabel("Search catalog", { exact: true })
      .fill("1001Tracklists");
    const brands = panel.getByRole("region", { name: "Brands", exact: true });
    await expect(
      brands.getByRole("button", {
        name: "Acquire brand · 1001Tracklists",
        exact: true,
      }),
    ).toBeDisabled();
    await expect(
      brands.getByText(/No per-icon license is recorded/),
    ).toBeVisible();
    let blocked;
    try {
      await call("opencut_brand_resources_source", {
        catalog_id: "simple-icons:16.35.0:1001tracklists",
      });
    } catch (e) {
      blocked = e.code;
    }
    expect(blocked).toBe("RESOURCE_LICENSE_UNAVAILABLE");
    let missing;
    try {
      await call("opencut_brand_resources_source", {
        catalog_id: "simple-icons:16.35.0:missing_f24",
      });
    } catch (e) {
      missing = e.code;
    }
    expect(missing).toBe("INVALID_ARGUMENT");
    await panel
      .getByLabel("Search catalog", { exact: true })
      .fill("ActivityPub");
    await expect(
      brands.getByRole("button", {
        name: "Acquire brand · ActivityPub",
        exact: true,
      }),
    ).toBeEnabled();
    expect((await inspect()).project).toEqual(
      state.stages["isolate-original-tracks-once"].project,
    );
    return {
      flags_keyword_and_absent: true,
      shapes_alias_and_absent: true,
      brand_search: true,
      unavailable_ui_and_native_code: blocked,
      unknown_id_code: missing,
      project_unchanged: true,
    };
  });
  const assets = [];
  for (const [index, source] of sourceDefinitions.entries()) {
    const asset = await stage("acquire-" + index, async () => {
      await open();
      await panel
        .getByRole("combobox", { name: /^Category(?:\s|$)/ })
        .selectOption(index === 2 ? "brands" : "flags");
      await panel
        .getByLabel("Search catalog", { exact: true })
        .fill(source.name);
      const tool =
          index === 2
            ? "opencut_brand_resources_import"
            : "opencut_catalog_resources_import",
        name = "import-" + index;
      if (index === 0) {
        await page.route("**/api/tools/" + tool, async (route) => {
          const input = route.request().postDataJSON();
          if (state.requests[name])
            expect(input).toEqual(state.requests[name].input);
          else {
            state.requests[name] = { tool, input };
            await save();
          }
          const response = await route.fetch(),
            body = await response.json();
          expect(body.error).toBeNull();
          await record(name, tool, input, body.data);
          if (!state.lostFlagResponse) {
            state.lostFlagResponse = { request: input, receipt: body.data };
            await save();
            await route.abort("failed");
          } else {
            expect(body.data).toEqual(state.lostFlagResponse.receipt);
            await route.fulfill({ response });
          }
        });
        try {
          if (!state.lostFlagResponse)
            await panel
              .locator("article")
              .filter({ has: page.getByText(source.name, { exact: true }) })
              .getByRole("button", { name: "Acquire and preview", exact: true })
              .click();
          await expect(
            panel.getByRole("button", {
              name: "Retry original acquisition",
              exact: true,
            }),
          ).toBeEnabled();
          await open();
          await panel
            .getByRole("button", {
              name: "Retry original acquisition",
              exact: true,
            })
            .click();
          await expect(
            page.getByRole("dialog", {
              name: "Source media preview",
              exact: true,
            }),
          ).toBeVisible();
          expect((await inspect()).revision).toBe(
            state.lostFlagResponse.receipt.revision,
          );
        } finally {
          await page.unroute("**/api/tools/" + tool);
        }
      } else
        await ui(name, tool, async () => {
          const button =
            index === 2
              ? panel.getByRole("button", {
                  name: "Acquire brand · ActivityPub",
                  exact: true,
                })
              : panel
                  .locator("article")
                  .filter({ has: page.getByText(source.name, { exact: true }) })
                  .getByRole("button", {
                    name: "Acquire and preview",
                    exact: true,
                  });
          await button.click();
        });
      const current = await inspect(),
        a = current.project.assets.find(
          (a) => a.id === state.requests[name].input.asset_id,
        );
      expect(a).toBeTruthy();
      expect(a.kind).toBe("image");
      expect(a.license.identifier).toBe(index === 2 ? "CC0-1.0" : "MIT");
      await closePreview();
      return a;
    });
    assets.push(asset);
    await stage("original-source-pixels-" + index, async () => {
      const head = await inspect(),
        resource = await call("opencut_project_resource_inspect", {
          project_id: project,
          revision: head.revision,
          kind: "asset",
          resource_id: asset.id,
        }),
        bytes = Buffer.from(
          await (
            await client.downloadProjectResource(resource)
          ).arrayBuffer(),
        );
      expect(sha(bytes)).toBe(asset.version.content_hash);
      expect(bytes.length).toBe(asset.version.size);
      const raw = await decode(bytes),
        expected = referenceSources[index].raw;
      expect(raw.length).toBe(expected.length);
      let max = 0,
        transparent = 0,
        opaque = 0;
      for (let i = 0; i < raw.length; i++) {
        max = Math.max(max, Math.abs(raw[i] - expected[i]));
        if (i % 4 === 3) {
          if (raw[i] === 0) transparent++;
          if (raw[i] === 255) opaque++;
        }
      }
      expect(max).toBe(0);
      expect(opaque).toBeGreaterThan(0);
      if (index > 0) expect(transparent).toBeGreaterThan(100);
      return {
        source_sha256: source.hash,
        raster_sha256: asset.version.content_hash,
        bytes: bytes.length,
        full_rgba_max_error: max,
        transparent_pixels: transparent,
        opaque_pixels: opaque,
      };
    });
    await stage("place-" + index, async () => {
      await open();
      await panel
        .getByRole("combobox", { name: /^Category(?:\s|$)/ })
        .selectOption(index === 2 ? "brands" : "flags");
      const row =
        index === 2
          ? panel
              .getByRole("region", { name: "Brands", exact: true })
              .locator("div")
              .filter({
                has: page.getByRole("button", {
                  name: "Download PNG",
                  exact: true,
                }),
              })
              .filter({ has: page.getByText(asset.name, { exact: true }) })
          : panel
              .locator("article")
              .filter({
                has: page.getByRole("button", {
                  name: "Download source",
                  exact: true,
                }),
              })
              .filter({ has: page.getByText(asset.name, { exact: true }) });
      return ui("place-" + index, "opencut_edit_apply", async () => {
        await row
          .getByRole("button", {
            name: index === 2 ? "Place" : "Place media",
            exact: true,
          })
          .click();
        const d = page.getByRole("dialog", {
          name: "Place media",
          exact: true,
        });
        await d
          .getByRole("combobox", { name: /^Target track(?:\s|$)/ })
          .selectOption("new_graphic");
        await d
          .getByLabel("New track name", { exact: true })
          .fill("F24 " + source.name);
        await d
          .getByLabel("Timeline start (seconds)", { exact: true })
          .fill("0");
        await d
          .getByLabel("Image duration (seconds)", { exact: true })
          .fill("1");
        await d
          .getByRole("button", { name: "Review placement", exact: true })
          .click();
        const r = page.getByRole("dialog", {
          name: "Review media changes",
          exact: true,
        });
        await r
          .getByRole("button", { name: "Apply media changes", exact: true })
          .click();
        await expect(
          r.getByText("Media changes saved.", { exact: true }),
        ).toBeVisible();
        await r.getByRole("button", { name: "Done", exact: true }).click();
      });
    });
  }
  await stage("catalog-shape-authoring", async () => {
    await open();
    await panel
      .getByRole("combobox", { name: /^Category(?:\s|$)/ })
      .selectOption("shapes");
    await panel.getByLabel("Search catalog", { exact: true }).fill("oval");
    await panel
      .getByRole("button", { name: "Add graphic", exact: true })
      .click();
    const f = page.getByRole("form", { name: "Edit graphic", exact: true });
    for (const [label, value] of Object.entries({
      "Graphic name": "F24 ellipse",
      "Start (seconds)": "0",
      "Duration (seconds)": "1",
      "Graphic width (pixels)": "80",
      "Graphic height (pixels)": "50",
      "Fill color": "#70c040",
      "Fill opacity (0–1)": "0.7",
      "Stroke width (pixels)": "0",
      "Horizontal center offset (pixels)": "75",
      "Vertical center offset (pixels)": "45",
      "Horizontal scale (multiplier)": "1.2",
      "Vertical scale (multiplier)": "0.8",
      "Graphic opacity (0–1)": "0.8",
    }))
      await f.getByLabel(label, { exact: true }).fill(value);
    await f
      .getByRole("button", { name: "Review graphic changes", exact: true })
      .click();
    return ui("shape", "opencut_edit_apply", () =>
      page
        .getByRole("region", { name: "Review graphic changes", exact: true })
        .getByRole("button", { name: "Apply graphic changes", exact: true })
        .click(),
    );
  });
  const beforeTransforms = await stage("before-resource-transforms", inspect);
  const fixed = await stage("one-atomic-resource-transform", async () => {
    const q = beforeTransforms.project.sequences[0],
      centers = [
        [80, 45],
        [240, 45],
        [80, 135],
      ],
      ops = [];
    for (const [i, asset] of assets.entries()) {
      const track = q.tracks.find((t) =>
          t.elements.some((e) => e.content.asset_id === asset.id),
        ),
        e = structuredClone(
          track.elements.find((e) => e.content.asset_id === asset.id),
        );
      e.visual.transform = {
        position_x_px: centers[i][0] - 160,
        position_y_px: centers[i][1] - 90,
        scale_x: 0.4,
        scale_y: i === 0 ? 0.35 : 0.4,
        rotation_degrees: 0,
      };
      e.visual.opacity = i === 2 ? 0.65 : 1;
      ops.push({
        type: "element_put",
        sequence_id: sequence,
        track_id: track.id,
        element: e,
      });
    }
    return mutation("resource-transform", "opencut_edit_apply", {
      project_id: project,
      expected_revision: beforeTransforms.revision,
      idempotency_key: "f24-one-transform",
      label: "F24 scaled transparent catalog resources",
      operations: ops,
    });
  });
  await open();
  await stage("reopen-all-resource-metadata", async () => {
    expect((await inspect()).project).toEqual(fixed.project);
    for (const a of assets) {
      expect(fixed.project.assets.find((x) => x.id === a.id)).toEqual(a);
    }
    return {
      all_three_assets_and_four_elements: true,
      licenses_and_provenance_preserved: true,
    };
  });
  const undone = await history("undo-resource-transform", "undo");
  expect(undone.project).toEqual(beforeTransforms.project);
  const redone = await history("redo-resource-transform", "redo");
  expect(redone.project).toEqual(fixed.project);
  const geometry = [
    { cx: 80, cy: 45, sx: 0.4, sy: 0.35, opacity: 1 },
    { cx: 240, cy: 45, sx: 0.4, sy: 0.4, opacity: 1 },
    { cx: 80, cy: 135, sx: 0.4, sy: 0.4, opacity: 0.65 },
    { cx: 235, cy: 135, sx: 1.2, sy: 0.8, opacity: 0.8 },
  ];
  const reference = (variant = "normal") => {
    const result = new Uint8Array(320 * 180 * 4);
    for (let p = 0; p < result.length; p += 4) result.set([32, 48, 64, 255], p);
    for (let i = 0; i < referenceSources.length; i++) {
      const s = referenceSources[i],
        g = geometry[i],
        contain = Math.min(320 / s.width, 180 / s.height),
        w = s.width * contain * (variant === "wrong-scale" ? 1 : g.sx),
        h = s.height * contain * (variant === "wrong-scale" ? 1 : g.sy);
      for (let y = 0; y < 180; y++)
        for (let x = 0; x < 320; x++) {
          const u = (x + 0.5 - g.cx) / w + 0.5,
            v = (y + 0.5 - g.cy) / h + 0.5;
          if (u < 0 || u >= 1 || v < 0 || v >= 1) continue;
          const tx = Math.max(0, Math.min(s.width - 1, u * s.width - 0.5)),
            ty = Math.max(0, Math.min(s.height - 1, v * s.height - 0.5)),
            x0 = Math.floor(tx),
            y0 = Math.floor(ty),
            dx = tx - x0,
            dy = ty - y0,
            rgba = [0, 0, 0, 0];
          for (const [xx, yy, f] of [
            [x0, y0, (1 - dx) * (1 - dy)],
            [Math.min(x0 + 1, s.width - 1), y0, dx * (1 - dy)],
            [x0, Math.min(y0 + 1, s.height - 1), (1 - dx) * dy],
            [
              Math.min(x0 + 1, s.width - 1),
              Math.min(y0 + 1, s.height - 1),
              dx * dy,
            ],
          ]) {
            const p = (yy * s.width + xx) * 4,
              a = s.raw[p + 3] / 255;
            rgba[3] += a * f;
            for (let c = 0; c < 3; c++) rgba[c] += s.raw[p + c] * f;
          }
          let a =
            rgba[3] * (variant === "opaque-brand" && i === 2 ? 1 : g.opacity);
          if (variant === "opaque-nepal" && i === 1) {
            a = 1;
            rgba[3] = 1;
          }
          if (variant === "drop-brand" && i === 2) continue;
          const p = (y * 320 + x) * 4;
          for (let c = 0; c < 3; c++)
            result[p + c] = Math.round(rgba[c] * a + result[p + c] * (1 - a));
        }
    }
    return result;
  };
  const expected = reference(),
    probesFile = path.join(args.workspace, "f24-frozen-probes.json"),
    probes = await read(probesFile).catch(async (error) => {
      if (error.code !== "ENOENT") throw error;
      // Freeze from independent expected pixels before any output observation.
      const selected = [];
      for (let y = 3; y < 177; y += 4)
        for (let x = 3; x < 317; x += 4) {
          let stable = true;
          for (let dy = -2; dy <= 2; dy++)
            for (let dx = -2; dx <= 2; dx++)
              for (let c = 0; c < 3; c++)
                if (
                  Math.abs(
                    expected[((y + dy) * 320 + x + dx) * 4 + c] -
                      expected[(y * 320 + x) * 4 + c],
                  ) > 2
                )
                  stable = false;
          if (stable) selected.push([x, y]);
        }
      await fs.writeFile(probesFile, JSON.stringify(selected) + "\n");
      return selected;
    });
  expect(probes.length).toBeGreaterThan(2000);
  const measure = (raw) => {
    expect(raw.length).toBe(320 * 180 * 4);
    let max = 0,
      sum = 0,
      worst = null;
    const quadrants = {};
    for (const [x, y] of probes) {
      const key = (y < 90 ? "top-" : "bottom-") + (x < 160 ? "left" : "right");
      quadrants[key] ??= { max: 0, sum: 0, count: 0 };
      for (let c = 0; c < 3; c++) {
        const diff = Math.abs(
          raw[(y * 320 + x) * 4 + c] - expected[(y * 320 + x) * 4 + c],
        );
        if (diff > max) {
          max = diff;
          worst = {
            x,
            y,
            actual: Array.from(
              raw.slice((y * 320 + x) * 4, (y * 320 + x) * 4 + 4),
            ),
            expected: Array.from(
              expected.slice((y * 320 + x) * 4, (y * 320 + x) * 4 + 4),
            ),
          };
        }
        sum += diff;
        quadrants[key].max = Math.max(quadrants[key].max, diff);
        quadrants[key].sum += diff;
        quadrants[key].count++;
      }
    }
    return {
      probes: probes.length,
      max_channel_error: max,
      mean_channel_error: sum / (probes.length * 3),
      quadrants,
      worst,
      threshold: 3,
    };
  };
  const controls = Object.fromEntries(
    ["wrong-scale", "opaque-brand", "opaque-nepal", "drop-brand"].map((v) => [
      v,
      measure(reference(v)),
    ]),
  );
  for (const v of Object.values(controls))
    expect(v.max_channel_error).toBeGreaterThan(3);
  await stage("freeze-independent-output-probes", async () => {
    const p = path.join(args.workspace, "f24-independent-reference.rgba");
    await fs.writeFile(p, expected);
    return {
      reference_file: p,
      reference_sha256: sha(expected),
      probes,
      negative_controls: controls,
      model:
        "Original pinned SVG raster at declared size; independent contain/scale/position and Classic straight-RGBA bilinear sampling then source-over; actual original Classic ellipse definition raster at80x50; contain then authored scale. Same frozen probe positions and threshold retained; no sample dropped after comparison.",
      threshold: 3,
      original_ellipse_source: ellipsePath,
      original_ellipse_sha256: sha(ellipseSource),
    };
  });
  const preview = await stage("one-continuous-preview", async () => {
    const observed = await read(
      path.join(args.workspace, "f24-preview-observed.json"),
    ).catch((e) => {
      if (e.code !== "ENOENT") throw e;
      return null;
    });
    if (observed?.candidate === args.candidate) {
      const raw = await fs.readFile(
        path.join(args.workspace, "f24-preview-observed.rgba"),
      );
      expect(sha(raw)).toBe(observed.raw_sha256);
      expect((await inspect()).revision).toBe(observed.revision);
      const m = measure(raw);
      expect(m.max_channel_error).toBeLessThanOrEqual(3);
      return {
        time: observed.time,
        generation: observed.generation,
        revision: observed.revision,
        observation_reused: true,
        reference_correction: path.join(
          path.dirname(args.candidate),
          "reference-correction.json",
        ),
        ...m,
      };
    }
    await open();
    await page.evaluate(() => {
      window.__f24Capture = true;
    });
    await page
      .getByRole("button", { name: "Play scene preview", exact: true })
      .click();
    await expect
      .poll(() => page.evaluate(() => !!window.__f24Picture), {
        timeout: 30000,
      })
      .toBe(true);
    const pause = page.getByRole("button", {
      name: "Pause scene audio",
      exact: true,
    });
    if (await pause.isEnabled()) await pause.click();
    const f = await page.evaluate(async () => {
        const p = window.__f24Picture,
          b = await p.bitmap,
          c = new OffscreenCanvas(b.width, b.height),
          ctx = c.getContext("2d");
        ctx.drawImage(b, 0, 0);
        const raw = Array.from(ctx.getImageData(0, 0, b.width, b.height).data);
        b.close();
        return { time: p.time, generation: p.generation, raw };
      }),
      m = measure(Uint8Array.from(f.raw));
    await fs.writeFile(
      path.join(args.workspace, "f24-preview-observed.rgba"),
      Uint8Array.from(f.raw),
    );
    await fs.writeFile(
      path.join(args.workspace, "f24-preview-observed.json"),
      JSON.stringify(
        { time: f.time, generation: f.generation, ...m },
        null,
        2,
      ) + "\n",
    );
    expect(m.max_channel_error).toBeLessThanOrEqual(3);
    return { time: f.time, generation: f.generation, ...m };
  });
  const output = await stage("one-fixed-revision-png", async () => {
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
          idempotency_key: "f24-one-combined-png",
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
    const deadline = Date.now() + 120000;
    do {
      job = await call("opencut_job_view", { job_id: state.accepted.job_id });
      if (["succeeded", "failed", "cancelled"].includes(job.state)) break;
      if (Date.now() > deadline)
        throw Error(
          "Observation timeout; retain and observe original Job " + job.job_id,
        );
      await new Promise((r) => setTimeout(r, 400));
    } while (true);
    expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
    const bytes = Buffer.from(
      await (await client.downloadArtifact(job.artifacts[0])).arrayBuffer(),
    );
    expect(sha(bytes)).toBe(job.artifacts[0].content_hash);
    const p = path.join(args.workspace, "f24-" + job.job_id + ".png");
    await fs.writeFile(p, bytes);
    const m = measure(await decode(bytes));
    expect(m.max_channel_error).toBeLessThanOrEqual(3);
    return {
      job_id: job.job_id,
      revision: current.revision,
      path: p,
      artifact: job.artifacts[0],
      result: job.result.data,
      ...m,
    };
  });
  for (let i = 0; i < state.forward.length; i++) {
    await open();
    await history("restore-original-" + i, "undo");
  }
  await open();
  const restored = await inspect();
  expect(restored.project).toEqual(original.project);
  expect(errors).toEqual([]);
  expect(egress).toEqual([]);
  const report = {
    status: "passed_scoped_f24_catalog_images",
    candidate: args.candidate,
    project_id: project,
    inventory: {
      flags: 254,
      shapes: 4,
      brands: 3466,
      brand_pages: catalog.brand_pages,
      admitted: 80,
      unavailable: 3386,
    },
    search: state.stages["three-category-search-and-unavailable"],
    sources: sourceDefinitions.map(({ svg, ...s }) => s),
    source_pixels: [0, 1, 2].map(
      (i) => state.stages["original-source-pixels-" + i],
    ),
    regional_flag_original_response_recovery: {
      request: state.lostFlagResponse.request,
      receipt: state.lostFlagResponse.receipt,
    },
    four_catalog_objects: true,
    history: {
      undo_redo_full_graph: true,
      original_restored: true,
      forward_transactions: state.forward.length,
    },
    reference: state.stages["freeze-independent-output-probes"],
    preview,
    output,
    page_errors: errors,
    external_requests: egress,
    new_builds: 0,
    new_unit_tests: 0,
    new_project_copies: 0,
    new_image_acquisitions: 3,
    new_output_jobs: 1,
    scope:
      "F24 declared catalog workflow and reviewed availability policy, actual3 image acquisitions+parametric ellipse and one PNG; not all3466 brands usable or broader graphics/codec/whole-migration acceptance.",
  };
  await fs.writeFile(args.report, JSON.stringify(report, null, 2) + "\n");
  console.log(
    JSON.stringify({
      status: report.status,
      job_id: output.job_id,
      preview_max: preview.max_channel_error,
      png_max: output.max_channel_error,
      restored: true,
    }),
  );
} catch (e) {
  await fs.writeFile(
    path.join(args.workspace, "f24-catalog-failure.json"),
    JSON.stringify(
      { error: String(e), stack: e.stack, page_errors: errors },
      null,
      2,
    ) + "\n",
  );
  await page.screenshot({
    path: path.join(args.workspace, "f24-catalog-failure.png"),
    fullPage: true,
  });
  throw e;
} finally {
  await browser.close();
}
