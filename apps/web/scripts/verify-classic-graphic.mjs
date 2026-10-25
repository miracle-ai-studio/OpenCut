import { editorSnapshot } from "./project-inspection.mjs";
// Original v31 static parameter shapes -> direct V4 -> one independently checked PNG.
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import {
  composeGraphicOracle,
  measureGraphicOracle,
  GRAPHIC_ORACLE_MODEL,
} from "./graphic-oracle.mjs";
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const prefix = (label) =>
  new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?:\\s|$)`);
const specs = [
  {
    kind: "rectangle",
    x: -128,
    y: -128,
    scale_x: 0.2,
    scale_y: 0.18,
    rotation_degrees: 14,
    fill: "#e84050",
    stroke: "#fff080",
    strokeWidth: 14,
    align: "inside",
    rounding: 20,
  },
  {
    kind: "ellipse",
    x: 128,
    y: -128,
    scale_x: 0.19,
    scale_y: 0.17,
    rotation_degrees: -9,
    fill: "#40c090",
    stroke: "#80e0ff",
    strokeWidth: 12,
    align: "center",
  },
  {
    kind: "polygon",
    x: -128,
    y: 128,
    scale_x: 0.2,
    scale_y: 0.2,
    rotation_degrees: 11,
    fill: "#b060e0",
    stroke: "#ffb060",
    strokeWidth: 14,
    align: "outside",
    sides: 5,
    rounding: 0,
  },
  {
    kind: "star",
    x: 128,
    y: 128,
    scale_x: 0.22,
    scale_y: 0.2,
    rotation_degrees: 8,
    fill: "#50a0f0",
    stroke: "#ffe060",
    strokeWidth: 12,
    align: "outside",
    points: 5,
    depth: 42,
  },
].map((spec) => ({ ...spec, fillAlpha: 204 / 255, strokeAlpha: 221 / 255 }));

async function candidateIdentity() {
  expect(process.env.OPENCUT_WEB_BINARY).toBeTruthy();
  expect(process.env.OPENCUT_WEB_ROOT).toBeTruthy();
  const files = [];
  async function visit(relative = "") {
    for (const item of await fs.readdir(
      path.join(process.env.OPENCUT_WEB_ROOT, relative),
      { withFileTypes: true },
    )) {
      const file = path.join(relative, item.name);
      if (item.isSymbolicLink())
        throw Error(`Unhashed candidate symlink ${file}`);
      if (item.isDirectory()) await visit(file);
      else if (/\.(js|css|mjs|cjs)$/u.test(file))
        files.push([
          file,
          sha(await fs.readFile(path.join(process.env.OPENCUT_WEB_ROOT, file))),
        ]);
    }
  }
  await visit();
  files.sort((a, b) => a[0].localeCompare(b[0]));
  expect(files.length).toBeGreaterThan(0);
  return {
    binary_sha256: sha(await fs.readFile(process.env.OPENCUT_WEB_BINARY)),
    web_js_css_tree_sha256: sha(JSON.stringify(files)),
  };
}

// The original Classic shapes have a 512-square source. Build their elementary
// paths independently here; only the acceptance CPU sampling/composition math is
// shared with the existing oracle. No production path, plan or receipt is used.
async function readStaticClassicInputs({ png, specs }) {
  const image = new Image();
  image.src = `data:image/png;base64,${png}`;
  await image.decode();
  const layers = [];
  const color = (hex, alpha) =>
    `rgba(${parseInt(hex.slice(1, 3), 16)},${parseInt(hex.slice(3, 5), 16)},${parseInt(hex.slice(5, 7), 16)},${alpha})`;
  for (const spec of specs) {
    const pad = spec.align === "inside" ? 0 : 128;
    const source = new OffscreenCanvas(512 + 2 * pad, 512 + 2 * pad),
      context = source.getContext("2d"),
      shape = new Path2D();
    if (spec.kind === "rectangle")
      shape.roundRect(pad, pad, 512, 512, (512 * spec.rounding) / 100);
    else if (spec.kind === "ellipse") {
      const radius = 256 - (spec.align === "center" ? spec.strokeWidth / 2 : 0);
      shape.ellipse(pad + 256, pad + 256, radius, radius, 0, 0, Math.PI * 2);
    } else {
      const count = spec.kind === "star" ? spec.points * 2 : spec.sides;
      for (let i = 0; i < count; i++) {
        const radius =
            256 * (spec.kind === "star" && i % 2 ? spec.depth / 100 : 1),
          angle = -Math.PI / 2 + (i * 2 * Math.PI) / count;
        const x = pad + 256 + radius * Math.cos(angle),
          y = pad + 256 + radius * Math.sin(angle);
        if (!i) shape.moveTo(x, y);
        else shape.lineTo(x, y);
      }
      shape.closePath();
    }
    context.fillStyle = color(spec.fill, spec.fillAlpha);
    context.fill(shape);
    context.strokeStyle = color(spec.stroke, spec.strokeAlpha);
    context.lineWidth = spec.strokeWidth;
    if (spec.align === "inside") {
      context.save();
      context.clip(shape);
      context.lineWidth *= 2;
      context.stroke(shape);
      context.restore();
    } else if (spec.align === "outside") {
      const border = new OffscreenCanvas(source.width, source.height),
        stroke = border.getContext("2d");
      stroke.strokeStyle = color(spec.stroke, spec.strokeAlpha);
      stroke.lineWidth = 2 * spec.strokeWidth;
      stroke.stroke(shape);
      stroke.globalCompositeOperation = "destination-out";
      stroke.fill(shape);
      context.drawImage(border, 0, 0);
    } else context.stroke(shape);
    layers.push({
      source: {
        width: source.width,
        height: source.height,
        data: Array.from(
          context.getImageData(0, 0, source.width, source.height).data,
        ),
      },
      centerX: 256 + spec.x,
      centerY: 256 + spec.y,
      scaleX: spec.scale_x,
      scaleY: spec.scale_y,
      rotation: (spec.rotation_degrees * Math.PI) / 180,
      opacity: 0.9,
    });
  }
  const actual = new OffscreenCanvas(512, 512),
    context = actual.getContext("2d");
  context.drawImage(image, 0, 0);
  return {
    width: image.naturalWidth,
    height: image.naturalHeight,
    actual: Array.from(context.getImageData(0, 0, 512, 512).data),
    layers,
  };
}

export async function verifyClassicGraphic(
  browser,
  endpoint,
  directory,
  check,
) {
  const checkpoint = path.join(directory, "classic-graphic-v4-checkpoint.json"),
    candidate = await candidateIdentity();
  let facts = { stages: {}, specs, candidates: [] };
  try {
    facts = JSON.parse(await fs.readFile(checkpoint, "utf8"));
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  // This slice has its own source and candidate evidence. It does not require
  // an old text/graphic acceptance candidate to match the current binary/Web.
  facts.candidates ??= [];
  if (
    !facts.candidates.some(
      (value) => JSON.stringify(value) === JSON.stringify(candidate),
    )
  )
    facts.candidates.push(candidate);
  facts.active_candidate = candidate;
  expect(facts.specs).toEqual(specs);
  const page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1100 },
    acceptDownloads: true,
  });
  page.setDefaultTimeout(30000);
  await page.addInitScript(() =>
    Object.defineProperty(window, "showSaveFilePicker", {
      value: undefined,
      configurable: true,
    }),
  );
  const errors = [],
    calls = [];
  page.on("pageerror", (error) => {
    if (errors.length < 20) errors.push(error.message);
  });
  page.on("request", (request) => {
    const operation = new URL(request.url()).pathname.split("/").at(-1);
    if (operation.startsWith("opencut_") && calls.length < 256)
      calls.push(operation);
  });
  let phase = "prepare original static source",
    queue = Promise.resolve();
  const save = (values) => {
    Object.assign(facts, values, { phase });
    const bytes = JSON.stringify(facts, null, 2) + "\n";
    if (Buffer.byteLength(bytes) > 4 * 1024 * 1024)
      throw Error("Classic graphic checkpoint exceeds 4 MiB");
    queue = queue.then(async () => {
      await fs.writeFile(`${checkpoint}.pending`, bytes);
      await fs.rename(`${checkpoint}.pending`, checkpoint);
    });
    return queue;
  };
  const tool = async (operation, input) => {
    const reply = await page.evaluate(
      async ({ operation, input }) =>
        (
          await fetch(`/api/tools/${operation}`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(input),
          })
        ).json(),
      { operation, input },
    );
    expect(reply.error, JSON.stringify(reply.error)).toBeNull();
    return reply.data;
  };
  const replay = async (stage) => {
    const record = facts.stages[stage];
    expect(record?.request).toBeTruthy();
    if (record.envelope?.error === null && record.envelope.data)
      return { request: record.request, data: record.envelope.data };
    const data = await tool(record.operation, record.request);
    record.envelope = { error: null, data };
    await save({});
    return { request: record.request, data };
  };
  const exchange = async (stage, operation, action) => {
    const matcher = `**/api/tools/${operation}`;
    const handler = async (route) => {
      facts.stages[stage] = {
        ...facts.stages[stage],
        operation,
        request: route.request().postDataJSON(),
        candidate,
      };
      await save({ pending: stage });
      await route.continue();
    };
    await page.route(matcher, handler);
    try {
      const waiting = page.waitForResponse((response) =>
        response.url().endsWith(`/api/tools/${operation}`),
      );
      await action();
      const envelope = await (await waiting).json();
      facts.stages[stage].envelope = envelope;
      await save({ pending: null });
      expect(envelope.error, JSON.stringify(envelope.error)).toBeNull();
      return { request: facts.stages[stage].request, data: envelope.data };
    } finally {
      await page.unroute(matcher, handler);
    }
  };
  const inspect = (revision = null) =>
    editorSnapshot(tool("opencut_project_inspect", {
      project_id: facts.imported.target_project_id,
      revision,
    }));
  const reopen = async () => {
    await page.goto(
      `${endpoint}/editor/${encodeURIComponent(facts.imported.target_project_id)}?scene=scene`,
    );
    await expect(
      page.getByRole("main", { name: "OpenCut editor", exact: true }),
    ).toBeVisible();
    const welcome = page.getByRole("dialog", {
      name: "Welcome to OpenCut",
      exact: true,
    });
    if (await welcome.isVisible())
      await welcome.getByRole("button", { name: "Close", exact: true }).click();
  };
  try {
    if (!facts.original_source) {
      const raw = JSON.parse(
        await fs.readFile(
          new URL(
            "../../../crates/editor-application/src/classic_import_v4/fixture.json",
            import.meta.url,
          ),
          "utf8",
        ),
      );
      raw.metadata = {
        id: "classic-v4-static-graphics-512-v1",
        name: "Classic direct V4 static graphics",
      };
      raw.settings.canvasSize = { width: 512, height: 512 };
      raw.settings.lastCustomCanvasSize = { width: 512, height: 512 };
      raw.settings.background.color = "#000000";
      raw.scenes[0].tracks.overlay = specs.map((spec) => ({
        id: `track-${spec.kind}`,
        name: `Imported ${spec.kind}`,
        type: "graphic",
        hidden: false,
        elements: [
          {
            id: `shape-${spec.kind}`,
            name: `Classic ${spec.kind}`,
            type: "graphic",
            definitionId: spec.kind,
            startTime: 0,
            duration: 360000,
            trimStart: 12000,
            trimEnd: 24000,
            sourceDuration: 396000,
            params: {
              fill: `${spec.fill}cc`,
              stroke: `${spec.stroke}dd`,
              strokeWidth: spec.strokeWidth,
              strokeAlign: spec.align,
              ...(spec.rounding !== undefined
                ? { cornerRadius: spec.rounding }
                : {}),
              ...(spec.sides ? { sides: spec.sides } : {}),
              ...(spec.points
                ? { points: spec.points, depth: spec.depth }
                : {}),
              "transform.positionX": spec.x,
              "transform.positionY": spec.y,
              "transform.scaleX": spec.scale_x,
              "transform.scaleY": spec.scale_y,
              "transform.rotate": spec.rotation_degrees,
              opacity: 0.9,
              blendMode: "normal",
            },
            effects: [],
            masks: [],
            animations: {},
          },
        ],
      }));
      const original = JSON.stringify(raw);
      await fs.writeFile(
        path.join(directory, "classic-graphic-v4-source.json"),
        original + "\n",
      );
      const snapshot = {
        origin: "http://classic.invalid",
        sourceId: raw.metadata.id,
        raw,
        context: { legacyTracksBySceneId: {}, mediaTypesById: {} },
        media: [],
      };
      const header = Buffer.from(
          JSON.stringify({
            format: "opencut.classic-recovery",
            snapshot,
            media: [],
          }),
        ),
        length = Buffer.alloc(4);
      length.writeUInt32LE(header.length);
      const bundle = path.join(directory, "classic-graphic-v4.occlassic"),
        bytes = Buffer.concat([
          Buffer.from("OPENCUT-CLASSIC-RECOVERY\n"),
          length,
          header,
        ]);
      await fs.writeFile(bundle, bytes);
      await save({
        original_source: raw,
        source_sha256: sha(original),
        bundle,
        bundle_sha256: sha(bytes),
      });
    }
    expect(sha(JSON.stringify(facts.original_source))).toBe(
      facts.source_sha256,
    );
    expect(sha(await fs.readFile(facts.bundle))).toBe(facts.bundle_sha256);
    await page.goto(`${endpoint}/editor`);
    if (!facts.imported) {
      phase = "real recovery analysis and direct V4 import";
      if (facts.stages.import?.request)
        await save({ imported: (await replay("import")).data });
      else {
        await page
          .getByRole("button", {
            name: "Recover Classic projects",
            exact: true,
          })
          .click();
        await page
          .getByLabel("Classic recovery bundle", { exact: true })
          .setInputFiles(facts.bundle);
        const analysis = await exchange(
          "analysis",
          "opencut_classic_analyze",
          () =>
            page
              .getByRole("button", { name: "Prepare recovery", exact: true })
              .click(),
        );
        expect(JSON.parse(analysis.request.content)).toEqual(
          facts.original_source,
        );
        expect(analysis.data.blockers).toEqual([]);
        expect(analysis.data.losses).toEqual([]);
        expect(analysis.data.unresolved_fonts).toEqual([]);
        await save({ analysis: analysis.data });
        const imported = await exchange(
          "import",
          "opencut_classic_import",
          () =>
            page
              .getByRole("button", { name: "Recover project", exact: true })
              .click(),
        );
        await save({ imported: imported.data });
        await page
          .getByRole("button", { name: "Open recovered project", exact: true })
          .click();
      }
    }
    await reopen();
    const original = await inspect(facts.imported.revision);
    expect(original.project).toEqual(facts.analysis.proposed_project);
    expect(original.project.assets).toEqual([]);
    expect(original.project.fonts).toEqual([]);
    expect(original.project.settings.settings.canvas_size).toEqual({
      width: 512,
      height: 512,
    });
    const rgb = (hex, alpha) => ({
      encoding: "srgb",
      red: parseInt(hex.slice(1, 3), 16) / 255,
      green: parseInt(hex.slice(3, 5), 16) / 255,
      blue: parseInt(hex.slice(5, 7), 16) / 255,
      alpha,
    });
    for (const spec of specs) {
      const shape = original.project.sequences[0].tracks.find(
        (track) => track.id === `track-${spec.kind}`,
      ).elements[0];
      expect(shape.id).toBe(`shape-${spec.kind}`);
      expect(shape.channels).toEqual([]);
      expect(shape.content.graphic).toEqual({
        source_size: { width: 512, height: 512 },
        geometry: {
          kind: spec.kind,
          ...(spec.rounding !== undefined
            ? { corner_radius_percent: spec.rounding }
            : {}),
          ...(spec.sides ? { sides: spec.sides } : {}),
          ...(spec.points
            ? { points: spec.points, depth_percent: spec.depth }
            : {}),
        },
        fill: rgb(spec.fill, spec.fillAlpha),
        stroke: {
          color: rgb(spec.stroke, spec.strokeAlpha),
          width_px: spec.strokeWidth,
          align: spec.align,
        },
      });
      expect(shape.visual.transform).toEqual({
        position_x_px: spec.x,
        position_y_px: spec.y,
        scale_x: spec.scale_x,
        scale_y: spec.scale_y,
        rotation_degrees: spec.rotation_degrees,
      });
      expect(shape.visual.opacity).toBe(0.9);
      expect(
        Number(shape.source_span.trim_start.ticks) /
          shape.source_span.trim_start.timebase,
      ).toBe(0.1);
      expect(
        Number(shape.source_span.trim_end.ticks) /
          shape.source_span.trim_end.timebase,
      ).toBe(0.2);
    }
    await save({ import_verified: true });
    if (!facts.edit_passed) {
      phase = "imported graphic edit and reopen";
      if (!facts.before_edit) await save({ before_edit: await inspect() });
      let committed;
      if (facts.stages.edit?.request) committed = await replay("edit");
      else {
        await page
          .locator('[data-element-id="shape-rectangle"] .editor-element-select')
          .click({ position: { x: 10, y: 10 } });
        await page
          .getByRole("button", { name: "Edit graphic", exact: true })
          .click();
        const form = page.getByRole("form", {
          name: "Edit graphic",
          exact: true,
        });
        await form
          .getByRole("textbox", { name: prefix("Corner radius (percent)") })
          .fill("25");
        const proposal = await exchange(
          "edit-plan",
          "opencut_graphic_plan",
          () =>
            form
              .getByRole("button", {
                name: "Review graphic changes",
                exact: true,
              })
              .click(),
        );
        expect((await inspect()).project).toEqual(facts.before_edit.project);
        committed = await exchange("edit", "opencut_edit_apply", () =>
          page
            .getByRole("button", { name: "Apply graphic changes", exact: true })
            .click(),
        );
        expect(committed.request).toEqual(proposal.data.transaction);
      }
      const edited = await inspect(committed.data.revision),
        expected = structuredClone(facts.before_edit.project);
      expected.sequences[0].tracks.find(
        (track) => track.id === "track-rectangle",
      ).elements[0].content.graphic.geometry.corner_radius_percent = 25;
      expect(edited.project).toEqual(expected);
      await reopen();
      expect((await inspect()).project).toEqual(expected);
      await save({ edited, edit_passed: true });
    }
    for (const [stage, direction, expected] of [
      ["undo", "undo", facts.before_edit.project],
      ["redo", "redo", facts.edited.project],
      ["restore-original", "undo", facts.before_edit.project],
    ]) {
      if (facts[`${stage}_passed`]) {
        const revision = facts.stages[stage].envelope.data.transaction.revision;
        expect(typeof revision).toBe("string");
        expect((await inspect(revision)).project).toEqual(expected);
        await save({ render_revision: revision });
        continue;
      }
      phase = `graphic history ${stage}`;
      await reopen();
      const history = page.locator(".editor-history");
      let applied;
      if (facts.stages[stage]?.request) applied = await replay(stage);
      else {
        const proposal = await exchange(
          `${stage}-plan`,
          `opencut_edit_${direction}_plan`,
          () =>
            history
              .getByRole("button", {
                name: direction === "undo" ? "Undo" : "Redo",
                exact: true,
              })
              .click(),
        );
        applied = await exchange(
          stage,
          `opencut_edit_${direction}_apply`,
          () =>
            history
              .getByRole("button", { name: `Apply ${direction}`, exact: true })
              .click(),
        );
        expect(applied.request).toEqual(proposal.request);
      }
      expect(
        (await inspect(applied.data.transaction.revision)).project,
      ).toEqual(expected);
      await reopen();
      expect((await inspect()).project).toEqual(expected);
      await save({
        [`${stage}_passed`]: true,
        render_revision: applied.data.transaction.revision,
      });
    }
    if (!facts.submission) {
      phase = "single static graphic PNG submission";
      if (facts.stages.png?.request)
        await save({ submission: await replay("png") });
      else {
        await page
          .getByRole("textbox", { name: prefix("Playhead (seconds)") })
          .fill("0.5");
        await page
          .getByRole("button", { name: "Go to time", exact: true })
          .click();
        await save({
          submission: await exchange(
            "png",
            "opencut_render_submit",
            () =>
              page
                .getByRole("button", {
                  name: "Render scene frame",
                  exact: true,
                })
                .click(),
          ),
        });
      }
    }
    expect(facts.submission.request.revision).toBe(facts.render_revision);
    phase = "original PNG job and actual UI download";
    let job = facts.job;
    if (job?.state !== "succeeded") {
      await expect
        .poll(
          async () => {
            job = await tool("opencut_job_view", {
              job_id: facts.submission.data.job_id,
            });
            return ["succeeded", "failed", "cancelled"].includes(job.state)
              ? job.state
              : "pending";
          },
          { timeout: 120000, intervals: [200, 500] },
        )
        .not.toBe("pending");
      await save({ job });
    }
    expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
    expect(job.result.kind).toBe("render.graphic_frame");
    expect(job.result.data.identity).toMatchObject({
      project_id: facts.imported.target_project_id,
      revision: facts.render_revision,
      sequence_id: "scene",
      width: 512,
      height: 512,
      profile: "classic-v31-gpu-normal-graphic-frame-v1",
    });
    expect(
      job.result.data.graphics
        .map((graphic) => graphic.elementId)
        .sort(),
    ).toEqual(specs.map((spec) => `shape-${spec.kind}`).sort());
    const artifact = job.artifacts.find(
        (item) => item.mime_type === "image/png",
      ),
      output = path.join(directory, "classic-graphic-v4.png");
    expect(artifact).toBeTruthy();
    let png;
    try {
      png = await fs.readFile(output);
      if (sha(png) !== artifact.content_hash) png = null;
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
    if (!png) {
      await page.getByRole("button", { name: "Tasks", exact: true }).click();
      await page.getByLabel("Refresh tasks", { exact: true }).click();
      const task = page
        .locator(".task-card")
        .filter({ hasText: facts.render_revision });
      await expect(task).toHaveCount(1);
      const waiting = page.waitForEvent("download");
      await task.getByRole("button", { name: "Download", exact: true }).click();
      await (await waiting).saveAs(output);
      png = await fs.readFile(output);
    }
    expect(sha(png)).toBe(artifact.content_hash);
    await save({ output, png_sha256: sha(png) });
    if (!facts.pixels_passed) {
      phase = "independent static 512-square geometry and sampling";
      const inputs = await page.evaluate(readStaticClassicInputs, {
        png: png.toString("base64"),
        specs,
      });
      expect(inputs.width).toBe(512);
      expect(inputs.height).toBe(512);
      const oracle = measureGraphicOracle(
        inputs.actual,
        composeGraphicOracle(inputs.layers, 512, 512),
        512,
        512,
        specs,
      );
      await save({
        oracle,
        oracle_model: GRAPHIC_ORACLE_MODEL,
        oracle_criteria: {
          iou_min_exclusive: 0.99,
          mean_max_exclusive: 1,
          inner_max_inclusive: 2,
          boundary_mean_max_exclusive: 2,
          per_region_mean_max_exclusive: 2,
          blank_max_inclusive: 2,
        },
      });
      expect(oracle.iou).toBeGreaterThan(0.99);
      expect(oracle.mean_channel_error).toBeLessThan(1);
      expect(oracle.inner_max_channel_error).toBeLessThanOrEqual(2);
      expect(oracle.boundary_mean_channel_error).toBeLessThan(2);
      expect(oracle.blank_error).toBeLessThanOrEqual(2);
      for (const region of oracle.regions) {
        expect(region.expected_pixels).toBeGreaterThan(100);
        expect(region.mean_channel_error).toBeLessThan(2);
      }
      await save({ pixels_passed: true });
    }
    expect((await inspect()).project).toEqual(facts.before_edit.project);
    expect(sha(JSON.stringify(facts.original_source))).toBe(
      facts.source_sha256,
    );
    expect(calls).not.toContain("opencut_project_promote");
    expect(calls).not.toContain("opencut_classic_import");
    expect(calls).not.toContain("opencut_font_import");
    expect(errors).toEqual([]);
    await save({
      status: "passed",
      scope:
        "Original v31 static rectangle/ellipse/polygon/star, 512-square source, explicit transforms and three stroke alignments; real direct V4 import/edit/reopen/Undo/Redo, original imported graph restored, one independently checked PNG. Does not prove animated graphic rendering, other Classic features, project-package or full migration parity.",
    });
    await fs.writeFile(
      path.join(directory, "classic-graphic-v4.json"),
      JSON.stringify(facts, null, 2) + "\n",
    );
    check(
      "Classic direct V4 static four-graphic import and independent PNG",
      facts,
    );
  } catch (error) {
    await queue.catch(() => {});
    await fs.writeFile(
      path.join(directory, `classic-graphic-v4-failure-${Date.now()}.json`),
      JSON.stringify(
        {
          phase,
          error: String(error),
          facts,
          errors,
          calls,
          ui: await page
            .locator("body")
            .innerText()
            .catch(() => ""),
        },
        null,
        2,
      ),
    );
    throw error;
  } finally {
    await page.close();
  }
}
