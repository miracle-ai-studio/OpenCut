import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash, randomUUID } from "node:crypto";
import {
  composeGraphicOracle,
  measureGraphicOracle,
  readGraphicOracleInputs,
} from "./graphic-oracle.mjs";
const hash = (value) => createHash("sha256").update(value).digest("hex");
const prefix = (label) =>
  new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?:\\s|$)`);
const shapes = [
  {
    kind: "rectangle",
    name: "B4 rounded rectangle",
    x: -80,
    y: -45,
    scale_x: 0.32,
    scale_y: 0.27,
    rotation_degrees: 14,
    fill: "#e84050",
    fillAlpha: 0.8,
    stroke: "#fff080",
    strokeAlpha: 0.7,
    strokeWidth: 3,
    align: "inside",
    rounding: 20,
  },
  {
    kind: "ellipse",
    name: "B4 ellipse",
    x: 80,
    y: -45,
    scale_x: 0.31,
    scale_y: 0.28,
    rotation_degrees: -9,
    fill: "#40c090",
    fillAlpha: 0.65,
    stroke: "#80e0ff",
    strokeAlpha: 0.9,
    strokeWidth: 2,
    align: "center",
  },
  {
    kind: "polygon",
    name: "B4 pentagon",
    x: -80,
    y: 45,
    scale_x: 0.36,
    scale_y: 0.34,
    rotation_degrees: 11,
    fill: "#b060e0",
    fillAlpha: 0.75,
    stroke: "#ffb060",
    strokeAlpha: 0.8,
    strokeWidth: 3,
    align: "outside",
    sides: 5,
    rounding: 0,
  },
  {
    kind: "star",
    name: "B4 star",
    x: 80,
    y: 45,
    scale_x: 0.34,
    scale_y: 0.32,
    rotation_degrees: 8,
    fill: "#50a0f0",
    fillAlpha: 0.7,
    stroke: "#ffe060",
    strokeAlpha: 0.85,
    strokeWidth: 2,
    align: "outside",
    points: 5,
    depth: 42,
  },
];
async function candidateIdentity() {
  expect(process.env.OPENCUT_WEB_BINARY).toBeTruthy();
  expect(process.env.OPENCUT_WEB_ROOT).toBeTruthy();
  const entries = [];
  async function visit(relative = "") {
    for (const item of await fs.readdir(
      path.join(process.env.OPENCUT_WEB_ROOT, relative),
      { withFileTypes: true },
    )) {
      const name = path.join(relative, item.name);
      if (item.isSymbolicLink()) throw Error(`Candidate symlink ${name}`);
      if (item.isDirectory()) await visit(name);
      else if (/\.(js|mjs|cjs|css)$/u.test(name))
        entries.push([
          name,
          hash(
            await fs.readFile(path.join(process.env.OPENCUT_WEB_ROOT, name)),
          ),
        ]);
    }
  }
  await visit();
  entries.sort((a, b) => a[0].localeCompare(b[0]));
  expect(entries.length).toBeGreaterThan(0);
  return {
    binary_sha256: hash(await fs.readFile(process.env.OPENCUT_WEB_BINARY)),
    web_js_css_tree_sha256: hash(JSON.stringify(entries)),
  };
}
/** Only this existing owned fixture is modified. Failed stages retain their fixed
 * head, original requests and jobs; success restores the complete initial graph. */
export async function verifyGraphic(
  browser,
  endpoint,
  directory,
  check,
  options = {},
) {
  const fixture = JSON.parse(
    await fs.readFile(path.join(directory, "b1-text-project.json"), "utf8"),
  );
  const candidate = await candidateIdentity(),
    checkpointPath = path.join(directory, "b4-graphic-checkpoint.json");
  let facts;
  try {
    facts = JSON.parse(await fs.readFile(checkpointPath, "utf8"));
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  if (options.freshRun) facts = null;
  if (facts) {
    expect(facts.project_id).toBe(fixture.project_id);
    expect(
      facts.candidate,
      "Retained graphic evidence belongs to another candidate; root must explicitly select a fresh run",
    ).toEqual(candidate);
  } else
    facts = {
      project_id: fixture.project_id,
      candidate,
      stages: {},
      graphics: [],
      phase: "bootstrap",
    };
  if (facts.phase === "passed" && facts.variant_frame?.pixels_passed) {
    check("B4 graphic UI and independent PNG", facts);
    return;
  }
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
  const errors = [];
  page.on("pageerror", (error) => {
    if (errors.length < 32) errors.push(error.message.slice(0, 1024));
  });
  let phase = "open",
    queue = Promise.resolve();
  const save = (values) => {
    Object.assign(facts, values, { active_phase: phase });
    const encoded = JSON.stringify(facts, null, 2) + "\n";
    if (Buffer.byteLength(encoded) > 4 * 1024 * 1024)
      throw Error("Graphic checkpoint exceeds 4 MiB");
    queue = queue.then(() => fs.writeFile(checkpointPath, encoded));
    return queue;
  };
  const tool = async (name, input) => {
    const envelope = await page.evaluate(
      async ({ name, input }) =>
        (
          await fetch(`/api/tools/${name}`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(input),
          })
        ).json(),
      { name, input },
    );
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull();
    return envelope.data;
  };
  const inspect = (revision) =>
    editorSnapshot(tool("opencut_project_inspect", {
      project_id: facts.project_id,
      revision: revision ?? null,
    }));
  const scene = (snapshot) =>
    snapshot.project.sequences.find((s) => s.id === facts.sequence_id);
  const reopen = async () => {
    await page.goto(
      `${endpoint}/editor/${encodeURIComponent(facts.project_id)}${facts.sequence_id ? `?scene=${encodeURIComponent(facts.sequence_id)}` : ""}`,
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
    if (facts.sequence_id)
      await page
        .getByRole("combobox", { name: prefix("Scene") })
        .selectOption(facts.sequence_id);
  };
  const exchange = async (stage, operation, action) => {
    const pendingRequest = page.waitForRequest((request) =>
      request.url().endsWith(`/api/tools/${operation}`),
    );
    const pendingResponse = page.waitForResponse((response) =>
      response.url().endsWith(`/api/tools/${operation}`),
    );
    const acting = action();
    const request = (await pendingRequest).postDataJSON();
    facts.stages[stage] = { ...facts.stages[stage], operation, request };
    await save({ pending: { stage, operation, request } });
    await acting;
    const envelope = await (await pendingResponse).json();
    facts.stages[stage].envelope = envelope;
    await save({});
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull();
    return { request, data: envelope.data };
  };
  const replay = async (stage) => {
    const retained = facts.stages[stage];
    expect(retained?.request).toBeTruthy();
    const data =
      retained.envelope?.data ??
      (await tool(retained.operation, retained.request));
    retained.envelope = { error: null, data };
    await save({});
    return data;
  };
  const restore = async (stage) => {
    if (!facts.stages[stage]) {
      const current = await inspect();
      facts.stages[stage] = {
        operation: "opencut_project_restore",
        request: {
          project_id: facts.project_id,
          expected_revision: current.revision,
          target_revision: facts.original.revision,
          idempotency_key: randomUUID(),
          asset_versions: {},
        },
      };
      await save({ pending: { stage, ...facts.stages[stage] } });
    }
    return replay(stage);
  };
  const select = async (id) =>
    page
      .locator(`[data-element-id="${id}"] .editor-element-select`)
      .click({ position: { x: 10, y: 10 } });
  const renderAndVerify = async (record, graphics, framePrefix = "") => {
    if (record.pixels_passed) return;
    const saveFrame = async (values) => {
      Object.assign(record, values);
      if (framePrefix)
        await save({
          variant_frame: record,
          ...(Object.hasOwn(values, "pending")
            ? { pending: values.pending }
            : {}),
        });
      else await save(values);
    };
    phase = "one graphic PNG submission";
    if (!record.submission) {
      await reopen();
      await page
        .getByRole("textbox", { name: prefix("Playhead (seconds)") })
        .fill("0.5");
      await page
        .getByRole("button", { name: "Go to time", exact: true })
        .click();
      const current = await inspect();
      await saveFrame({ render_revision: current.revision });
      const submission = facts.stages[`${framePrefix}png`]?.request
        ? {
            request: facts.stages[`${framePrefix}png`].request,
            data: await replay(`${framePrefix}png`),
          }
        : await exchange(
            `${framePrefix}png`,
            "opencut_render_submit",
            () =>
              page
                .getByRole("button", {
                  name: "Render scene frame",
                  exact: true,
                })
                .click(),
          );
      await saveFrame({ submission, pending: null });
      expect(submission.request).toMatchObject({
        project_id: facts.project_id,
        revision: current.revision,
        sequence_id: facts.sequence_id,
      });
      expect(2n * BigInt(submission.request.time.ticks)).toBe(
        BigInt(submission.request.time.timebase),
      );
    }
    phase = "original PNG job/download";
    let job;
    await expect
      .poll(
        async () => {
          job = await tool("opencut_job_view", {
            job_id: record.submission.data.job_id,
          });
          return ["succeeded", "failed", "cancelled"].includes(job.state)
            ? job.state
            : "pending";
        },
        { timeout: 120000, intervals: [100, 250, 500] },
      )
      .not.toBe("pending");
    await saveFrame({ job });
    expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
    expect(job.result.kind).toBe("render.graphic_frame");
    const result = job.result.data;
    expect(result.identity).toMatchObject({
      project_id: facts.project_id,
      revision: record.render_revision,
      sequence_id: facts.sequence_id,
      width: 320,
      height: 180,
      profile: "classic-v31-gpu-normal-graphic-frame-v1",
    });
    expect(result.graphics.map((r) => r.elementId).sort()).toEqual(
      graphics.map((g) => g.id).sort(),
    );
    expect(result.texts).toEqual([]);
    expect(result.captions).toEqual([]);
    const artifact = job.artifacts.find((a) => a.mime_type === "image/png"),
      output = path.join(
        directory,
        `b4-graphic${framePrefix ? "-variant" : ""}.png`,
      );
    expect(artifact).toBeTruthy();
    let png;
    try {
      png = await fs.readFile(output);
      if (hash(png) !== artifact.content_hash) png = null;
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
    if (!png) {
      await page.getByRole("button", { name: "Tasks", exact: true }).click();
      await page.getByLabel("Refresh tasks", { exact: true }).click();
      const task = page
        .locator(".task-card")
        .filter({ hasText: record.render_revision });
      await expect(task).toHaveCount(1);
      const download = page.waitForEvent("download");
      await task.getByRole("button", { name: "Download", exact: true }).click();
      await (await download).saveAs(output);
      png = await fs.readFile(output);
    }
    await saveFrame({ output, png_sha256: hash(png) });
    expect(hash(png)).toBe(artifact.content_hash);
    phase = "independent Canvas geometry and straight-alpha sampling oracle";
    const inputs = await page.evaluate(readGraphicOracleInputs, {
      png: png.toString("base64"),
      specs: graphics.map((g) => g.spec),
    });
    expect(inputs.width).toBe(320);
    expect(inputs.height).toBe(180);
    const oracle = measureGraphicOracle(
      inputs.actual,
      composeGraphicOracle(inputs.layers, 320, 180),
      320,
      180,
      graphics.map((graphic) => graphic.spec),
    );
    await saveFrame({
      oracle,
      oracle_criteria: {
        iou_min_exclusive: 0.99,
        full_mean_channel_error_max_exclusive: 1,
        per_shape_mean_channel_error_max_exclusive: 2,
        expected_neighborhood_radius: 2,
        stable_neighborhood_channel_range: 2,
        inner_max_channel_error_max_inclusive: 3,
        boundary_mean_channel_error_max_exclusive: 12,
      },
    });
    expect(oracle.width).toBe(320);
    expect(oracle.height).toBe(180);
    expect(oracle.iou).toBeGreaterThan(0.99);
    expect(oracle.mean_channel_error).toBeLessThan(1);
    expect(oracle.inner_pixels).toBeGreaterThan(1000);
    expect(oracle.inner_max_channel_error).toBeLessThanOrEqual(3);
    expect(oracle.boundary_pixels).toBeGreaterThan(100);
    expect(oracle.boundary_mean_channel_error).toBeLessThan(12);
    for (const region of oracle.regions) {
      expect(region.expected_pixels).toBeGreaterThan(300);
      expect(region.mean_channel_error).toBeLessThan(2);
    }
    expect(errors).toEqual([]);
    await saveFrame({ pixels_passed: true });
  };
  try {
    await reopen();
    if (!facts.original) {
      const original = await inspect();
      await save({
        original,
        sequence_id: original.project.default_sequence_id,
      });
    }
    if (!facts.stages.isolate?.envelope) {
      phase = "isolate existing scene";
      if (!facts.stages.isolate) {
        const current = await inspect(),
          settings = structuredClone(current.project.settings);
        expect(settings.profile).toBe("classic_global");
        settings.settings.canvas_size = { width: 320, height: 180 };
        settings.settings.canvas_size_mode = "custom";
        settings.settings.last_custom_canvas_size = { width: 320, height: 180 };
        settings.settings.background = {
          kind: "color",
          color: { encoding: "srgb", red: 0, green: 0, blue: 0, alpha: 1 },
        };
        const operations = [{ type: "settings_set", settings }];
        for (const track of scene(current).tracks)
          if (track.enabled)
            operations.push({
              type: "track_put",
              sequence_id: facts.sequence_id,
              track: { ...track, enabled: false },
            });
        facts.stages.isolate = {
          operation: "opencut_edit_apply",
          request: {
            project_id: facts.project_id,
            expected_revision: current.revision,
            idempotency_key: randomUUID(),
            label: "B4 isolate four graphic sources",
            operations,
          },
        };
        await save({});
      }
      const receipt = await replay("isolate");
      await save({ active_revision: receipt.revision });
      await reopen();
    }
    // A lost write receipt always reuses the captured public request/key; no new
    // defaults, shape IDs or create transaction are generated on this branch.
    for (let index = 0; index < shapes.length; index++) {
      const spec = shapes[index],
        stage = `create-${index}`;
      phase = `create ${spec.kind}`;
      if (facts.graphics[index]) continue;
      let committed, planned;
      if (facts.stages[stage]?.request) {
        committed = await replay(stage);
        planned = facts.stages[`${stage}-plan`].envelope.data;
      } else {
        await reopen();
        const before = await inspect();
        await page
          .getByRole("combobox", { name: prefix("Graphic shape") })
          .selectOption(spec.kind);
        await exchange(`${stage}-defaults`, "opencut_graphic_plan", () =>
          page
            .getByRole("button", { name: "Add graphic", exact: true })
            .click(),
        );
        const form = page.getByRole("form", {
          name: "Edit graphic",
          exact: true,
        });
        await expect(form).toBeVisible();
        const fields = {
          "Graphic name": spec.name,
          "Start (seconds)": "0",
          "Duration (seconds)": "3",
          "Graphic width (pixels)": "60",
          "Graphic height (pixels)": "50",
          "Fill color": spec.fill,
          "Fill opacity (0–1)": String(spec.fillAlpha),
          "Stroke color": spec.stroke,
          "Stroke opacity (0–1)": String(spec.strokeAlpha),
          "Stroke width (pixels)": String(spec.strokeWidth),
          "Horizontal center offset (pixels)": String(spec.x),
          "Vertical center offset (pixels)": String(spec.y),
          "Horizontal scale (multiplier)": String(spec.scale_x),
          "Vertical scale (multiplier)": String(spec.scale_y),
          "Rotation (degrees)": String(spec.rotation_degrees),
          "Graphic opacity (0–1)": "0.9",
        };
        if (spec.rounding !== undefined)
          fields["Corner radius (percent)"] = String(spec.rounding);
        if (spec.sides) fields["Polygon sides"] = String(spec.sides);
        if (spec.points) fields["Star points"] = String(spec.points);
        if (spec.depth) fields["Star depth (percent)"] = String(spec.depth);
        for (const [label, value] of Object.entries(fields)) {
          const textbox = form.getByRole("textbox", { name: prefix(label) });
          if (await textbox.count()) await textbox.fill(value);
          else await form.getByLabel(label, { exact: true }).fill(value);
        }
        await form
          .getByRole("combobox", { name: prefix("Stroke alignment") })
          .selectOption(spec.align);
        const proposal = await exchange(
          `${stage}-plan`,
          "opencut_graphic_plan",
          () =>
            form
              .getByRole("button", {
                name: "Review graphic changes",
                exact: true,
              })
              .click(),
        );
        planned = proposal.data;
        await save({});
        expect(planned.revision).toBe(before.revision);
        expect((await inspect()).project).toEqual(before.project);
        expect(planned.transaction.idempotency_key).toBe(
          proposal.request.idempotency_key,
        );
        const write = await exchange(stage, "opencut_edit_apply", () =>
          page
            .getByRole("button", { name: "Apply graphic changes", exact: true })
            .click(),
        );
        expect(write.request).toEqual(planned.transaction);
        committed = write.data;
      }
      const observed = await inspect(committed.revision),
        track = scene(observed).tracks.find(
          (t) => t.id === planned.selection.track_id,
        ),
        element = track.elements.find(
          (e) => e.id === planned.selection.element_id,
        );
      await save({ active_revision: committed.revision, pending: null });
      expect(track.kind).toBe("graphic");
      expect(element.name).toBe(spec.name);
      expect(element.content.graphic).toEqual(planned.initial.graphic);
      expect(element.visual.transform).toEqual(planned.initial.transform);
      expect(element.visual.opacity).toBe(0.9);
      expect(element.timeline_range).toEqual(planned.initial.timeline_range);
      expect(BigInt(element.timeline_range.start.ticks)).toBe(0n);
      expect(BigInt(element.timeline_range.duration.ticks)).toBe(
        3n * BigInt(element.timeline_range.duration.timebase),
      );
      expect(element.visual.transform).toEqual({
        position_x_px: spec.x,
        position_y_px: spec.y,
        scale_x: spec.scale_x,
        scale_y: spec.scale_y,
        rotation_degrees: spec.rotation_degrees,
      });
      const rgb = (hex, alpha) => ({
        encoding: "srgb",
        red: parseInt(hex.slice(1, 3), 16) / 255,
        green: parseInt(hex.slice(3, 5), 16) / 255,
        blue: parseInt(hex.slice(5, 7), 16) / 255,
        alpha,
      });
      expect(element.content.graphic.source_size).toEqual({
        width: 60,
        height: 50,
      });
      expect(element.content.graphic.fill).toEqual(
        rgb(spec.fill, spec.fillAlpha),
      );
      expect(element.content.graphic.stroke).toEqual({
        color: rgb(spec.stroke, spec.strokeAlpha),
        width_px: spec.strokeWidth,
        align: spec.align,
      });
      facts.graphics[index] = {
        id: element.id,
        track_id: track.id,
        element,
        spec,
        revision: committed.revision,
      };
      await save({});
      await reopen();
      expect((await inspect()).project).toEqual(observed.project);
    }
    if (!facts.edit_passed) {
      phase = "edit graphic and reopen";
      await reopen();
      if (!facts.before_edit) await save({ before_edit: await inspect() });
      let committed;
      if (facts.stages.edit?.request) committed = await replay("edit");
      else {
        await select(facts.graphics[0].id);
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
        const written = await exchange("edit", "opencut_edit_apply", () =>
          page
            .getByRole("button", { name: "Apply graphic changes", exact: true })
            .click(),
        );
        expect(written.request).toEqual(proposal.data.transaction);
        committed = written.data;
      }
      const edited = await inspect(committed.revision),
        expected = structuredClone(facts.before_edit.project);
      const shape = expected.sequences
        .find((s) => s.id === facts.sequence_id)
        .tracks.flatMap((t) => t.elements)
        .find((e) => e.id === facts.graphics[0].id);
      shape.content.graphic.geometry.corner_radius_percent = 25;
      await save({ edited, active_revision: edited.revision });
      expect(edited.project).toEqual(expected);
      facts.graphics[0].spec.rounding = 25;
      facts.graphics[0].element = shape;
      await reopen();
      expect((await inspect()).project).toEqual(expected);
      await save({ edit_passed: true, pending: null });
    }
    for (const direction of ["undo", "redo"]) {
      const stage = `history-${direction}`;
      if (facts[`${stage}_passed`]) continue;
      phase = stage;
      await reopen();
      const history = page.locator(".editor-history");
      let receipt;
      if (facts.stages[stage]?.request) receipt = await replay(stage);
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
        const applied = await exchange(
          stage,
          `opencut_edit_${direction}_apply`,
          () =>
            history
              .getByRole("button", { name: `Apply ${direction}`, exact: true })
              .click(),
        );
        expect(applied.request).toEqual(proposal.request);
        receipt = applied.data;
      }
      const observed = await inspect(receipt.revision);
      await save({ active_revision: observed.revision });
      expect(observed.project).toEqual(
        direction === "undo" ? facts.before_edit.project : facts.edited.project,
      );
      await save({ [`${stage}_passed`]: true, pending: null });
    }
    if (!facts.copy_passed) {
      phase = "duplicate graphic through existing clipboard";
      await reopen();
      if (!facts.copy_baseline) await save({ copy_baseline: await inspect() });
      let receipt;
      if (facts.stages.copy?.request) receipt = await replay("copy");
      else {
        await select(facts.graphics[3].id);
        const proposal = await exchange(
          "copy-plan",
          "opencut_timeline_paste_plan",
          () =>
            page
              .locator('[aria-label="Timeline clipboard"]')
              .getByRole("button", { name: "Duplicate elements", exact: true })
              .click(),
        );
        expect(proposal.request.expected_revision).toBe(
          facts.copy_baseline.revision,
        );
        expect((await inspect()).project).toEqual(facts.copy_baseline.project);
        const review = page.getByRole("dialog", {
          name: "Review paste",
          exact: true,
        });
        const written = await exchange("copy", "opencut_edit_apply", () =>
          review
            .getByRole("button", { name: "Apply copied elements", exact: true })
            .click(),
        );
        expect(written.request).toEqual(proposal.data.transaction);
        receipt = written.data;
        await expect(review.getByRole("status")).toHaveText(
          "Pasted elements saved.",
        );
        await review.getByRole("button", { name: "Done", exact: true }).click();
      }
      const observed = await inspect(receipt.revision),
        copiedId =
          facts.stages["copy-plan"].envelope.data.decision.selection[0]
            .element_id;
      const copy = scene(observed)
        .tracks.flatMap((track) => track.elements)
        .find((element) => element.id === copiedId);
      await save({
        copied_revision: observed.revision,
        active_revision: observed.revision,
        copied_id: copiedId,
        observed_copy: copy,
      });
      expect(copiedId).not.toBe(facts.graphics[3].id);
      const source = scene(facts.copy_baseline)
        .tracks.flatMap((track) => track.elements)
        .find((element) => element.id === facts.graphics[3].id);
      expect(copy.content).toEqual(source.content);
      expect(copy.visual).toEqual(source.visual);
      expect(copy.source_span).toEqual(source.source_span);
      expect(copy.channels).toEqual(source.channels);
      expect(copy.timeline_range).toEqual(source.timeline_range);
      expect(copy.enabled).toBe(source.enabled);
      const withoutCopy = structuredClone(observed.project);
      const copiedScene = withoutCopy.sequences.find(
        (sequence) => sequence.id === facts.sequence_id,
      );
      const originalTrackIds = new Set(
        scene(facts.copy_baseline).tracks.map((track) => track.id),
      );
      const addedTracks = copiedScene.tracks.filter(
        (track) => !originalTrackIds.has(track.id),
      );
      // Duplicate at the original time creates a new compatible track to avoid
      // overlapping the source; undo must remove that complete transaction.
      expect(addedTracks).toHaveLength(1);
      expect(addedTracks[0].id).toBe(
        facts.stages["copy-plan"].envelope.data.decision.selection[0].track_id,
      );
      expect(addedTracks[0].kind).toBe("graphic");
      expect(addedTracks[0].elements.map((element) => element.id)).toEqual([
        copiedId,
      ]);
      copiedScene.tracks = copiedScene.tracks.filter((track) =>
        originalTrackIds.has(track.id),
      );
      expect(withoutCopy).toEqual(facts.copy_baseline.project);
      await reopen();
      expect((await inspect()).project).toEqual(observed.project);
      await save({ copy_passed: true, pending: null });
    }
    if (!facts.copy_undo_passed) {
      phase = "undo duplicate restores four-shape composition";
      await reopen();
      const history = page.locator(".editor-history");
      let receipt;
      if (facts.stages["copy-undo"]?.request)
        receipt = await replay("copy-undo");
      else {
        const proposal = await exchange(
          "copy-undo-plan",
          "opencut_edit_undo_plan",
          () =>
            history.getByRole("button", { name: "Undo", exact: true }).click(),
        );
        const written = await exchange(
          "copy-undo",
          "opencut_edit_undo_apply",
          () =>
            history
              .getByRole("button", { name: "Apply undo", exact: true })
              .click(),
        );
        expect(written.request).toEqual(proposal.request);
        receipt = written.data;
      }
      const observed = await inspect(receipt.revision);
      await save({ active_revision: observed.revision });
      expect(observed.project).toEqual(facts.copy_baseline.project);
      await save({ copy_undo_passed: true, pending: null });
    }
    await renderAndVerify(facts, facts.graphics);
    // Different side/point counts are separately edited through the same UI;
    // the first successful PNG and its original geometry remain immutable.
    if (!facts.variant_graphics)
      await save({
        variant_graphics: structuredClone(facts.graphics),
        variant_frame: {},
      });
    for (const variation of [
      {
        index: 2,
        stage: "variant-polygon",
        fields: { "Polygon sides": "3" },
        geometry: { sides: 3 },
        spec: { sides: 3 },
      },
      {
        index: 3,
        stage: "variant-star",
        fields: { "Star points": "7", "Star depth (percent)": "60" },
        geometry: { points: 7, depth_percent: 60 },
        spec: { points: 7, depth: 60 },
      },
    ]) {
      if (facts[`${variation.stage}_passed`]) continue;
      phase = variation.stage;
      await reopen();
      const priorKey = `${variation.stage}_before`;
      if (!facts[priorKey]) await save({ [priorKey]: await inspect() });
      let receipt;
      if (facts.stages[variation.stage]?.request)
        receipt = await replay(variation.stage);
      else {
        await select(facts.graphics[variation.index].id);
        await page
          .getByRole("button", { name: "Edit graphic", exact: true })
          .click();
        const form = page.getByRole("form", {
          name: "Edit graphic",
          exact: true,
        });
        for (const [label, value] of Object.entries(variation.fields))
          await form.getByRole("textbox", { name: prefix(label) }).fill(value);
        const proposal = await exchange(
          `${variation.stage}-plan`,
          "opencut_graphic_plan",
          () =>
            form
              .getByRole("button", {
                name: "Review graphic changes",
                exact: true,
              })
              .click(),
        );
        expect((await inspect()).project).toEqual(facts[priorKey].project);
        const written = await exchange(
          variation.stage,
          "opencut_edit_apply",
          () =>
            page
              .getByRole("button", {
                name: "Apply graphic changes",
                exact: true,
              })
              .click(),
        );
        expect(written.request).toEqual(proposal.data.transaction);
        receipt = written.data;
      }
      const observed = await inspect(receipt.revision),
        expected = structuredClone(facts[priorKey].project);
      const target = expected.sequences
        .find((sequence) => sequence.id === facts.sequence_id)
        .tracks.flatMap((track) => track.elements)
        .find((element) => element.id === facts.graphics[variation.index].id);
      Object.assign(target.content.graphic.geometry, variation.geometry);
      await save({
        active_revision: observed.revision,
        [`${variation.stage}_observed`]: observed,
      });
      expect(observed.project).toEqual(expected);
      await reopen();
      expect((await inspect()).project).toEqual(expected);
      facts.variant_graphics[variation.index].element = target;
      Object.assign(
        facts.variant_graphics[variation.index].spec,
        variation.spec,
      );
      await save({
        variant_graphics: facts.variant_graphics,
        [`${variation.stage}_passed`]: true,
        pending: null,
      });
    }
    await renderAndVerify(
      facts.variant_frame,
      facts.variant_graphics,
      "variant-",
    );
    phase = "restore original complete graph";
    await restore("restore-original");
    const restored = await inspect();
    await save({ restored_revision: restored.revision });
    expect(restored.project).toEqual(facts.original.project);
    await save({
      phase: "passed",
      pending: null,
      scope:
        "Four parametric graphics and three stroke alignments through original public UI transactions; reopen, edit, whole-project Undo/Redo, clipboard Duplicate plus Undo, two separately checkpointed native PNGs with different polygon sides/star points/depth and independent analytical Canvas geometry and T*R*S transform. No continuous preview/MP4 claim.",
    });
    await fs.writeFile(
      path.join(directory, "b4-graphic.json"),
      JSON.stringify(facts, null, 2) + "\n",
    );
    check("B4 graphic UI and independent PNG", facts);
  } catch (error) {
    await save({
      phase: "failed",
      failure: { phase, error: String(error).slice(0, 4096) },
      errors,
    });
    try {
      await save({
        active_revision: (await inspect()).revision,
        ui: await page.evaluate(() => document.body.innerText.slice(0, 8192)),
      });
    } catch {}
    const filename = `b4-graphic-failure-${Date.now()}.json`;
    await fs.writeFile(
      path.join(directory, filename),
      JSON.stringify(facts, null, 2) + "\n",
    );
    await page
      .screenshot({
        path: path.join(directory, filename.replace(/\.json$/u, ".png")),
      })
      .catch(() => {});
    throw error;
  } finally {
    await queue;
    await page.close();
  }
}
