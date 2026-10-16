import { expect } from "@playwright/test";

// This helper consumes the integrator's existing synthetic project. It never
// creates/copies media, calls a controller through window, or supplies hit pixels.
// Run serially in the same candidate/browser context as the other V4 scenarios.
export const requiredFixture = {
  locale:
    "English UI; authenticated same-origin page and an editable V4 project",
  sequenceId: "Existing scene ID",
  transform:
    "{ elementId, additionalElementIds:[id,...], timeSeconds, deltaCanvas:{x,y}, pixelProbes:[{x,y,before:[r,g,b,a],during:[r,g,b,a],tolerance?:1}] }: visible static unlocked group, paused Scene preview, mandatory independent canvas probes for each member",
  alpha:
    "{ timeSeconds, width, height, probes:[{x,y,elementId:string|null}] }: independently authored opaque and transparent probes (at least one null), paused live scene preview available; probes must avoid transform handles, start with a non-null hit and alternate expected selections",
  gain: "{ elementId, channelId, timeSeconds, dy?:-8 }: audio/video with an existing nonempty scalar gain channel and native samples; two or more visible keys with nonzero tangents",
  keyframes:
    "{ elementId, channelId, keyId, valueDelta?:1 }: existing scalar key in the selected element, no pending edit",
  split:
    "{ timeSeconds, elements:[{elementId,channelId,probeTimes:[{ticks,timebase}]}] }: at least two unlocked elements on distinct tracks crossing the split; scalar element channel per item, absolute probe times on both sides; no transition at the cut",
  animatedScale:
    "{elementId,timeSeconds,xChannelId,yChannelId,deltaCanvas:12}: optional V2 horizontal resize; removes only scale_x animation, preserves authored scale_y and all unrelated channels; absent is explicitly pending",
  checkpoint:
    "Optional mutable checkpoint plus save(checkpoint) callback. Original UI write requests are saved before dispatch and reused after interruption.",
};
const T = (time) => Number(time.ticks) / time.timebase;
const cmp = (a, b) => {
  const value =
    BigInt(a.ticks) * BigInt(b.timebase) - BigInt(b.ticks) * BigInt(a.timebase);
  return value < 0n ? -1 : value > 0n ? 1 : 0;
};
const rational = (numerator, denominator) => {
  let a = numerator < 0n ? -numerator : numerator,
    b = denominator;
  while (b) [a, b] = [b, a % b];
  const divisor = a || 1n,
    timebase = Number(denominator / divisor);
  expect(
    Number.isSafeInteger(timebase) && timebase > 0 && timebase <= 0xffffffff,
  ).toBe(true);
  return { ticks: String(numerator / divisor), timebase };
};
const sub = (a, b) =>
  rational(
    BigInt(a.ticks) * BigInt(b.timebase) - BigInt(b.ticks) * BigInt(a.timebase),
    BigInt(a.timebase) * BigInt(b.timebase),
  );
const add = (a, b) =>
  rational(
    BigInt(a.ticks) * BigInt(b.timebase) + BigInt(b.ticks) * BigInt(a.timebase),
    BigInt(a.timebase) * BigInt(b.timebase),
  );
const element = (project, scene, id) =>
  project.sequences
    .find((s) => s.id === scene)
    ?.tracks.flatMap((track) => track.elements)
    .find((item) => item.id === id);
const channel = (project, scene, elementId, id) =>
  element(project, scene, elementId)?.channels.find((item) => item.id === id);
const selector = (id) => `[data-element-id=${JSON.stringify(id)}]`;

export async function verifyParallelEditing({
  page,
  base,
  projectId,
  fixture,
  checkpoint,
  save = async () => {},
  check = async (_name, run) => run(),
}) {
  if (
    !fixture ||
    !["sequenceId", "transform", "alpha", "gain", "keyframes", "split"].every(
      (key) => fixture[key],
    )
  )
    return { status: "fixture_required", requiredFixture };
  const facts = checkpoint ?? {
    schema_version: 1,
    project_id: projectId,
    sequence_id: fixture.sequenceId,
    stages: {},
    operations: {},
  };
  expect(facts.project_id).toBe(projectId);
  expect(facts.sequence_id).toBe(fixture.sequenceId);
  facts.stages ??= {};
  facts.operations ??= {};
  const persist = () => save(facts);
  const tool = async (name, input) => {
    const reply = await page.evaluate(
      async ({ name, input }) => {
        const response = await fetch(`/v1/tools/${name}`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(input),
        });
        return response.json();
      },
      { name, input },
    );
    expect(reply.error, JSON.stringify(reply.error)).toBeNull();
    return reply.data;
  };
  const inspect = () =>
    tool("opencut_project_v4_inspect", {
      project_id: projectId,
      revision: null,
    });
  const open = async () => {
    await page.goto(
      `${base.replace(/\/$/u, "")}/editor/${encodeURIComponent(projectId)}?scene=${encodeURIComponent(fixture.sequenceId)}`,
    );
    await expect(
      page.getByRole("main", { name: "V4 editor", exact: true }),
    ).toBeVisible();
    const welcome = page.getByRole("dialog", {
      name: "Welcome to OpenCut",
      exact: true,
    });
    if (await welcome.isVisible())
      await welcome.getByRole("button", { name: "Close", exact: true }).click();
  };
  const seek = async (seconds) => {
    await page
      .getByRole("textbox", { name: /^Playhead \(seconds\)/iu })
      .fill(String(seconds));
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
  };
  const select = async (id, modifiers = []) => {
    await page
      .locator(`${selector(id)} .v4-element-select`)
      .click({ position: { x: 10, y: 10 }, modifiers });
    await expect(page.locator(selector(id))).toHaveAttribute(
      "data-selected",
      "true",
    );
  };
  const transformIds = [
    fixture.transform.elementId,
    ...(fixture.transform.additionalElementIds ?? []),
  ];
  expect(new Set(transformIds).size).toBe(transformIds.length);
  expect(transformIds.length, "Actual group transform requires multiple elements").toBeGreaterThanOrEqual(2);
  expect(fixture.transform.pixelProbes?.length, "Independent transform pixels are mandatory").toBeGreaterThanOrEqual(transformIds.length * 2);
  expect(fixture.transform.pixelProbes.some(p => JSON.stringify(p.before) !== JSON.stringify(p.during))).toBe(true);
  const selectTransform = async () => {
    await select(transformIds[0]);
    for (const id of transformIds.slice(1)) await select(id, ["ControlOrMeta"]);
  };
  const pausedCanvas = async (seconds) => {
    await seek(seconds);
    await page
      .getByRole("button", { name: "Play scene preview", exact: true })
      .click();
    await expect(page.locator("canvas.v4-scene-frame")).toBeVisible();
    await page
      .getByRole("button", { name: "Pause scene audio", exact: true })
      .click();
    await expect(
      page.getByText("Scene sound paused", { exact: true }),
    ).toBeVisible();
    await seek(seconds);
    await expect
      .poll(async () =>
        Number(
          await page.locator("canvas.v4-scene-frame").getAttribute("data-time"),
        ),
      )
      .toBeCloseTo(seconds, 4);
  };
  // Draw only the rendered canvas into a private scratch canvas. This observes
  // bitmaprenderer and 2D frames equally; SVG handles and DOM overlays never
  // participate in the digest or independent expected-pixel checks.
  const canvasPixels = (probes = fixture.transform.pixelProbes) =>
    page.locator("canvas.v4-scene-frame").evaluate(async (canvas, probes) => {
      const copy = document.createElement("canvas");
      copy.width = canvas.width;
      copy.height = canvas.height;
      if (!copy.width || !copy.height) throw Error("Preview canvas is empty");
      const context = copy.getContext("2d", { willReadFrequently: true });
      if (!context) throw Error("Canvas pixel readback is unavailable");
      context.drawImage(canvas, 0, 0);
      const bytes = context.getImageData(0, 0, copy.width, copy.height).data,
        hash = Array.from(
          new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)),
          (byte) => byte.toString(16).padStart(2, "0"),
        ).join("");
      return {
        width: copy.width,
        height: copy.height,
        time: Number(canvas.dataset.time),
        generation: canvas.dataset.generation,
        sha256: hash,
        probes: probes.map(({ x, y }) => {
          if (
            !Number.isInteger(x) ||
            !Number.isInteger(y) ||
            x < 0 ||
            y < 0 ||
            x >= copy.width ||
            y >= copy.height
          )
            throw Error("Independent canvas probe is out of bounds");
          const start = 4 * (y * copy.width + x);
          return { x, y, rgba: Array.from(bytes.slice(start, start + 4)) };
        }),
      };
    }, probes);
  const expectedPixels = (pixels, phase) => {
    for (const [index, probe] of (
      fixture.transform.pixelProbes ?? []
    ).entries()) {
      expect(probe[phase]).toHaveLength(4);
      for (let component = 0; component < 4; component++)
        expect(
          Math.abs(
            pixels.probes[index].rgba[component] - probe[phase][component],
          ),
        ).toBeLessThanOrEqual(probe.tolerance ?? 1);
    }
  };
  const awaitExpectedPixels = async phase => {
    await expect.poll(async () => {
      try { expectedPixels(await canvasPixels(), phase); return true; }
      catch { return false; }
    }).toBe(true);
    const pixels = await canvasPixels();
    expectedPixels(pixels, phase);
    return pixels;
  };
  const exchange = async (id, name, invoke) => {
    const saved = facts.operations[id];
    if (saved) {
      expect(saved.name).toBe(name);
      const result = await tool(name, saved.input); // Same original key after a lost reply.
      saved.reply = result;
      await persist();
      return result;
    }
    const pattern = `**/v1/tools/${name}`;
    const route = async (r) => {
      facts.operations[id] = { name, input: r.request().postDataJSON() };
      await persist();
      await r.continue();
    };
    await page.route(pattern, route, { times: 1 });
    try {
      const response = page.waitForResponse((r) =>
        r.url().endsWith(`/v1/tools/${name}`),
      );
      response.catch(() => {});
      await invoke();
      const reply = await (await response).json();
      expect(reply.error, JSON.stringify(reply.error)).toBeNull();
      facts.operations[id].reply = reply.data;
      await persist();
      return reply.data;
    } finally {
      await page.unroute(pattern, route);
    }
  };
  const undo = async (id) =>
    exchange(`${id}:undo`, "opencut_edit_v4_undo_apply", async () => {
      const history = page.locator(".v4-history");
      await history.getByRole("button", { name: "Undo", exact: true }).click();
      await history
        .getByRole("button", { name: "Apply undo", exact: true })
        .click();
    });
  const stage = async (id, run) => {
    if (facts.stages[id]?.verified) return;
    await check(`parallel-editing:${id}`, async () => {
      const record = (facts.stages[id] ??= { before: await inspect() });
      await persist();
      await run(record);
      record.verified = true;
      await persist();
    });
  };
  const editAndUndo = async (id, prepare, assertChange) =>
    stage(id, async (record) => {
      if (!record.after) {
        const commit = facts.operations[`${id}:edit`]
          ? () => {}
          : await prepare(record);
        const count = writes.length;
        await exchange(`${id}:edit`, "opencut_edit_v4_apply", commit);
        expect(writes).toHaveLength(count + 1);
        record.observed_apply_requests = writes.slice(count);
        const after = await inspect();
        expect(after.revision).not.toBe(record.before.revision);
        await assertChange(record.before, after, record);
        record.after = after;
        await persist();
      }
      // Exercise the durable original request even after a successful response.
      // A lost reply must never allocate a replacement key or second edit.
      if (!record.original_request_replay) {
        const original = facts.operations[`${id}:edit`];
        const replay = await tool(original.name, original.input);
        expect(replay.revision).toBe(record.after.revision);
        expect(await inspect()).toEqual(record.after);
        record.original_request_replay = { input: original.input, reply: replay };
        await persist();
      }
      await open();
      await undo(id);
      const restored = await inspect();
      expect(restored.project).toEqual(record.before.project);
      record.restored_revision = restored.revision;
    });
  const writes = [];
  const observe = (request) => {
    if (request.url().endsWith("/v1/tools/opencut_edit_v4_apply"))
      writes.push(request.postDataJSON());
  };
  const settings = async () => {
    await page
      .getByRole("button", { name: "Shortcuts and settings", exact: true })
      .click();
    const dialog = page.getByRole("dialog", {
      name: "Keyboard shortcuts",
      exact: true,
    });
    await expect(dialog).toBeVisible();
    return dialog;
  };
  const exportPreferences = async (dialog) => {
    const details = dialog.locator("details.shortcut-transfer");
    if ((await details.getAttribute("open")) === null)
      await details.locator("summary").click();
    await dialog
      .getByRole("button", { name: "Show current configuration", exact: true })
      .click();
    return dialog
      .getByRole("textbox", {
        name: "Shortcut configuration JSON",
        exact: true,
      })
      .inputValue();
  };
  const pointerStart = async (locator, dx, dy) => {
    await expect(locator).toBeVisible();
    const bounds = await locator.boundingBox();
    expect(bounds).not.toBeNull();
    const from = {
        x: bounds.x + bounds.width / 2,
        y: bounds.y + bounds.height / 2,
      },
      to = { x: from.x + dx, y: from.y + dy };
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 5 });
    return { from, to };
  };
  const groupMove = async () => {
    const bounds = await page.locator(".v4-content-hit-surface").boundingBox();
    expect(bounds).not.toBeNull();
    const scale = bounds.width / fixture.alpha.width;
    return pointerStart(page.getByLabel("Move selected element", { exact: true }),
      fixture.transform.deltaCanvas.x * scale, fixture.transform.deltaCanvas.y * scale);
  };
  await open();
  facts.initial ??= await inspect();
  await persist();
  page.on("request", observe);
  try {
    await stage("shortcuts", async (record) => {
      record.start_write_count = writes.length;
      let dialog = await settings();
      record.original_preferences ??= await exportPreferences(dialog);
      await persist();
      const row = dialog.locator('[data-shortcut-action="toggle-snapping"]');
      // Start from the action's Classic default, then replace it through the recorder.
      await row
        .getByRole("button", {
          name: "Toggle snapping: Reset action",
          exact: true,
        })
        .click();
      await row
        .getByRole("button", { name: /^Toggle snapping: Change /u })
        .click();
      await dialog
        .getByRole("textbox", { name: "Record a shortcut", exact: true })
        .press("Alt+Shift+N");
      await dialog
        .getByRole("button", { name: "Save shortcut", exact: true })
        .click();
      record.rebound_preferences = JSON.parse(await exportPreferences(dialog));
      expect(record.rebound_preferences.bindings["toggle-snapping"]).toEqual([
        "alt+shift+n",
      ]);
      await dialog.getByRole("button", { name: "Close", exact: true }).click();
      await open(); // Preference persistence must survive a real reload.
      dialog = await settings();
      expect(JSON.parse(await exportPreferences(dialog))).toEqual(
        record.rebound_preferences,
      );
      await dialog.getByRole("button", { name: "Close", exact: true }).click();
      const snap = page.getByRole("checkbox", {
          name: "Snap to element edges",
          exact: true,
        }),
        before = await snap.isChecked();
      await page.locator('[data-shortcut-scope="timeline"]').focus();
      await page.keyboard.press("Alt+Shift+N");
      await expect.poll(() => snap.isChecked()).toBe(!before);
      dialog = await settings();
      await dialog
        .locator('[data-shortcut-action="toggle-snapping"]')
        .getByRole("button", { name: /^Toggle snapping: Clear /u })
        .click();
      record.cleared_preferences = JSON.parse(await exportPreferences(dialog));
      expect(record.cleared_preferences.bindings["toggle-snapping"]).toEqual(
        [],
      );
      await dialog.getByRole("button", { name: "Close", exact: true }).click();
      await page.locator('[data-shortcut-scope="timeline"]').focus();
      await page.keyboard.press("Alt+Shift+N");
      await expect.poll(() => snap.isChecked()).toBe(!before);
      dialog = await settings();
      await exportPreferences(dialog);
      await dialog
        .getByRole("textbox", {
          name: "Shortcut configuration JSON",
          exact: true,
        })
        .fill(record.original_preferences);
      await dialog
        .getByRole("button", { name: "Import configuration", exact: true })
        .click();
      expect(JSON.parse(await exportPreferences(dialog))).toEqual(
        JSON.parse(record.original_preferences),
      );
      await dialog.getByRole("button", { name: "Close", exact: true }).click();
      expect(await inspect()).toEqual(record.before);
      expect(writes.slice(record.start_write_count)).toHaveLength(0);
    });

    await stage("transform-cancel", async (record) => {
      await open();
      await selectTransform();
      await pausedCanvas(fixture.transform.timeSeconds);
      record.canvas_before = await canvasPixels();
      expectedPixels(record.canvas_before, "before");
      const count = writes.length;
      record.movement = await groupMove();
      await expect
        .poll(async () => (await canvasPixels()).sha256)
        .not.toBe(record.canvas_before.sha256);
      record.canvas_during = await canvasPixels();
      expect(record.canvas_during.time).toBeCloseTo(
        fixture.transform.timeSeconds,
        4,
      );
      record.canvas_during = await awaitExpectedPixels("during");
      expect(await inspect()).toEqual(record.before);
      expect(writes).toHaveLength(count);
      await page.keyboard.press("Escape");
      await page.mouse.up();
      await expect
        .poll(async () => (await canvasPixels()).sha256)
        .toBe(record.canvas_before.sha256);
      record.canvas_restored = await canvasPixels();
      expectedPixels(record.canvas_restored, "before");
      expect(await inspect()).toEqual(record.before);
      expect(writes).toHaveLength(count);
    });
    await editAndUndo(
      "transform-commit",
      async (record) => {
        await open();
        await selectTransform();
        await pausedCanvas(fixture.transform.timeSeconds);
        record.canvas_before = await canvasPixels();
        expectedPixels(record.canvas_before, "before");
        const overlay = page.locator(".v4-content-hit-surface"),
          box = await overlay.boundingBox();
        expect(box).not.toBeNull();
        record.scale =
          box.width / record.before.project.settings.settings.canvas_size.width;
        record.write_count_before = writes.length;
        record.movement = await groupMove();
        await expect
          .poll(async () => (await canvasPixels()).sha256)
          .not.toBe(record.canvas_before.sha256);
        record.canvas_during = await awaitExpectedPixels("during");
        expect(await inspect()).toEqual(record.before);
        expect(writes).toHaveLength(record.write_count_before);
        return () => page.mouse.up();
      },
      async (before, after, record) => {
        expect(record.observed_apply_requests).toHaveLength(1);
        expect(record.observed_apply_requests[0].operations).toHaveLength(
          transformIds.length,
        );
        const expected = structuredClone(before.project);
        for (const targetId of transformIds) {
          const actual = element(after.project, fixture.sequenceId, targetId)
              .visual.transform,
            original = element(expected, fixture.sequenceId, targetId).visual
              .transform;
          expect(
            Math.abs(
              actual.position_x_px -
                original.position_x_px -
                (record.movement.to.x - record.movement.from.x) / record.scale,
            ),
          ).toBeLessThanOrEqual(1 / record.scale + 1e-8);
          expect(
            Math.abs(
              actual.position_y_px -
                original.position_y_px -
                (record.movement.to.y - record.movement.from.y) / record.scale,
            ),
          ).toBeLessThanOrEqual(1 / record.scale + 1e-8);
          element(expected, fixture.sequenceId, targetId).visual.transform = {
            ...original,
            position_x_px: actual.position_x_px,
            position_y_px: actual.position_y_px,
          };
        }
        expect(after.project).toEqual(expected);
      },
    );

    await stage("transform-final-cancel", async (record) => {
      await open();
      await selectTransform();
      await pausedCanvas(fixture.transform.timeSeconds);
      const before = await canvasPixels(), count = writes.length;
      record.movement = await groupMove();
      await expect.poll(async () => (await canvasPixels()).sha256).not.toBe(before.sha256);
      record.canvas_during = await awaitExpectedPixels("during");
      const pattern = "**/v1/tools/opencut_visual_preview_v4_transform_draft_v2_frame";
      let release, captured, terminal;
      const gate = new Promise(resolve => { release = resolve; });
      const requestSeen = new Promise(resolve => { captured = resolve; });
      const route = async r => {
        const request = r.request();
        terminal = Promise.race([
          page.waitForEvent("requestfailed", { predicate: event => event === request }),
          page.waitForEvent("requestfinished", { predicate: event => event === request }),
        ]);
        terminal.catch(() => {});
        record.original_final_request = r.request().postDataJSON();
        await persist();
        captured();
        await gate;
        // Escape aborts the transport. Continuing a canceled read is harmless;
        // it must not become permission to submit the stored transaction.
        await r.continue().catch(() => {});
      };
      await page.route(pattern, route, { times: 1 });
      try {
        await page.mouse.up();
        await Promise.race([requestSeen, new Promise((_, reject) => {
          const timer = setTimeout(() => reject(Error("No final V2 admission request")), 30000);
          requestSeen.finally(() => clearTimeout(timer));
        })]);
        const request = record.original_final_request;
        expect(request.gesture_intent).toBe("translate");
        expect(request.transaction.expected_revision).toBe(record.before.revision);
        expect(request.transaction.idempotency_key).toBe(request.draft_id);
        expect(request.transaction.operations).toHaveLength(transformIds.length);
        expect(writes).toHaveLength(count);
        await page.keyboard.press("Escape");
        release();
        await terminal;
        await expect.poll(async () => (await canvasPixels()).sha256).toBe(before.sha256);
        expectedPixels(await canvasPixels(), "before");
        // The exact intercepted transport has now terminated. Restore the
        // canvas and inspect the head; no arbitrary sleep is counted as proof.
        expect(await inspect()).toEqual(record.before);
        expect(writes).toHaveLength(count);
        record.canvas_restored = await canvasPixels();
        record.apply_request_count = 0;
      } finally {
        release();
        await page.unroute(pattern, route);
      }
    });

    if (fixture.animatedScale) await editAndUndo(
      "animated-scale-x",
      async record => {
        await open();
        await select(fixture.animatedScale.elementId);
        await pausedCanvas(fixture.animatedScale.timeSeconds);
        record.canvas_before = await canvasPixels();
        const box = await page.locator(".v4-content-hit-surface").boundingBox();
        expect(box).not.toBeNull();
        record.movement = await pointerStart(page.locator('[data-transform-handle="right"]'),
          fixture.animatedScale.deltaCanvas * box.width / fixture.alpha.width, 0);
        await expect.poll(async () => (await canvasPixels()).sha256).not.toBe(record.canvas_before.sha256);
        expect(await inspect()).toEqual(record.before);
        return () => page.mouse.up();
      },
      async (before, after, record) => {
        const f = fixture.animatedScale, expected = structuredClone(before.project),
          source = element(before.project, fixture.sequenceId, f.elementId),
          target = element(expected, fixture.sequenceId, f.elementId),
          actual = element(after.project, fixture.sequenceId, f.elementId);
        expect(source.channels.find(c => c.id === f.xChannelId).binding.parameter).toBe("scale_x");
        expect(source.channels.find(c => c.id === f.yChannelId).binding.parameter).toBe("scale_y");
        expect(actual.channels.some(c => c.id === f.xChannelId)).toBe(false);
        expect(actual.visual.transform.scale_x).not.toBe(source.visual.transform.scale_x);
        expect(actual.visual.transform.scale_y).toBe(source.visual.transform.scale_y);
        target.channels = target.channels.filter(c => c.id !== f.xChannelId);
        target.visual.transform.scale_x = actual.visual.transform.scale_x;
        target.visual.transform.position_x_px = actual.visual.transform.position_x_px;
        target.visual.transform.position_y_px = actual.visual.transform.position_y_px;
        expect(after.project).toEqual(expected);
        expect(record.observed_apply_requests).toHaveLength(1);
      },
    );

    await stage("alpha-selection", async (record) => {
      expect(fixture.alpha.probes.some((p) => p.elementId === null)).toBe(true);
      expect(fixture.alpha.probes.some((p) => p.elementId !== null)).toBe(true);
      await open();
      await seek(fixture.alpha.timeSeconds);
      await page
        .getByRole("button", { name: "Play scene preview", exact: true })
        .click();
      await expect(page.locator("canvas.v4-scene-frame")).toBeVisible();
      await page
        .getByRole("button", { name: "Pause scene audio", exact: true })
        .click();
      await seek(fixture.alpha.timeSeconds);
      await expect
        .poll(async () =>
          Number(
            await page
              .locator("canvas.v4-scene-frame")
              .getAttribute("data-time"),
          ),
        )
        .toBeCloseTo(fixture.alpha.timeSeconds, 4);
      const surface = page.getByLabel("Select visible picture content", {
          exact: true,
        }),
        bounds = await surface.boundingBox();
      expect(bounds).not.toBeNull();
      record.probes = [];
      record.pixels = await canvasPixels(fixture.alpha.probes);
      for (const [index, probe] of fixture.alpha.probes.entries()) {
        expect(probe.expected, "Alpha hit probes require independent canvas colors").toHaveLength(4);
        record.pixels.probes[index].rgba.forEach((value, component) =>
          expect(Math.abs(value - probe.expected[component])).toBeLessThanOrEqual(1));
      }
      expect(fixture.alpha.probes[0].elementId).not.toBeNull();
      for (const probe of fixture.alpha.probes) {
        if (probe.elementId === null)
          expect(
            await page
              .locator('.v4-timeline-element[data-selected="true"]')
              .count(),
          ).toBeGreaterThan(0);
        else
          await expect(
            page.locator(selector(probe.elementId)),
          ).not.toHaveAttribute("data-selected", "true");
        expect(Number.isInteger(probe.x) && Number.isInteger(probe.y)).toBe(
          true,
        );
        expect(probe.x).toBeGreaterThanOrEqual(0);
        expect(probe.x).toBeLessThan(fixture.alpha.width);
        expect(probe.y).toBeGreaterThanOrEqual(0);
        expect(probe.y).toBeLessThan(fixture.alpha.height);
        await page.mouse.click(
          bounds.x + ((probe.x + 0.5) * bounds.width) / fixture.alpha.width,
          bounds.y + ((probe.y + 0.5) * bounds.height) / fixture.alpha.height,
        );
        if (probe.elementId === null)
          await expect(
            page.locator('.v4-timeline-element[data-selected="true"]'),
          ).toHaveCount(0);
        else
          await expect(page.locator(selector(probe.elementId))).toHaveAttribute(
            "data-selected",
            "true",
          );
        record.probes.push(probe);
      }
      expect((await inspect()).project).toEqual(record.before.project);
    });

    await editAndUndo(
      "keyframe-ui",
      async (record) => {
        await open();
        await select(fixture.keyframes.elementId);
        const editor = page.getByRole("region", {
          name: "Keyframes",
          exact: true,
        });
        await editor
          .getByRole("combobox", { name: "Animation channel", exact: true })
          .selectOption(fixture.keyframes.channelId);
        const source = channel(
          record.before.project,
          fixture.sequenceId,
          fixture.keyframes.elementId,
          fixture.keyframes.channelId,
        );
        expect(source.kind).toBe("scalar");
        const key = source.keys.find((k) => k.id === fixture.keyframes.keyId);
        expect(key).toBeTruthy();
        record.changed_value = key.value + (fixture.keyframes.valueDelta ?? 1);
        // Keyboard activation selects a key without generating a pointer move.
        const selectedLabel = await editor
          .getByRole("combobox", { name: "Animation channel", exact: true })
          .evaluate((select) => select.selectedOptions[0].textContent);
        const marker = editor.getByRole("button", {
          name: `${selectedLabel} keyframe ${T(key.time)} seconds`,
          exact: true,
        });
        await marker.focus();
        await page.keyboard.press("Enter");
        await editor
          .getByRole("spinbutton", { name: "Keyframe value", exact: true })
          .fill(String(record.changed_value));
        return () =>
          editor
            .getByRole("button", { name: "Set selected values", exact: true })
            .click();
      },
      async (before, after, record) => {
        const expected = structuredClone(before.project);
        channel(
          expected,
          fixture.sequenceId,
          fixture.keyframes.elementId,
          fixture.keyframes.channelId,
        ).keys.find((k) => k.id === fixture.keyframes.keyId).value =
          record.changed_value;
        expect(after.project).toEqual(expected);
      },
    );

    await editAndUndo(
      "gain-curve",
      async (record) => {
        await open();
        await select(fixture.gain.elementId);
        await seek(fixture.gain.timeSeconds);
        const clip = page.locator(selector(fixture.gain.elementId)),
          curve = clip.getByRole("button", {
            name: "Move volume automation",
            exact: true,
          });
        await expect(curve).toBeVisible();
        // Read the displayed sampled polyline solely to hit the UI, never as an expected curve value.
        const at = await curve.evaluate((node) => {
          const points = node.points,
            p = points.getItem(Math.floor(points.numberOfItems / 2)),
            svg = node.ownerSVGElement,
            point = svg.createSVGPoint();
          point.x = p.x;
          point.y = p.y;
          const result = point.matrixTransform(node.getScreenCTM());
          return { x: result.x, y: result.y };
        });
        record.write_count_before = writes.length;
        await page.mouse.move(at.x, at.y);
        await page.mouse.down();
        await page.mouse.move(at.x, at.y + (fixture.gain.dy ?? -8), {
          steps: 5,
        });
        expect(await inspect()).toEqual(record.before);
        expect(writes).toHaveLength(record.write_count_before);
        return () => page.mouse.up();
      },
      async (before, after, record) => {
        expect(record.observed_apply_requests).toHaveLength(1);
        const original = channel(
            before.project,
            fixture.sequenceId,
            fixture.gain.elementId,
            fixture.gain.channelId,
          ),
          changed = channel(
            after.project,
            fixture.sequenceId,
            fixture.gain.elementId,
            fixture.gain.channelId,
          );
        expect(original.keys.length).toBeGreaterThanOrEqual(2);
        const delta = changed.keys[0].value - original.keys[0].value;
        expect(Math.abs(delta)).toBeGreaterThan(0.01);
        const expected = structuredClone(before.project),
          target = channel(
            expected,
            fixture.sequenceId,
            fixture.gain.elementId,
            fixture.gain.channelId,
          );
        target.keys.forEach((key, i) => {
          expect(changed.keys[i].value - key.value).toBeCloseTo(delta, 8);
          key.value = changed.keys[i].value;
        });
        expect(after.project).toEqual(expected);
      },
    );

    await editAndUndo(
      "multi-split-curves",
      async (record) => {
        expect(fixture.split.elements.length).toBeGreaterThanOrEqual(2);
        await open();
        const modifier = await page.evaluate(() =>
          /Mac|iPhone|iPad/u.test(navigator.platform) ? "Meta" : "Control",
        );
        record.samples_before ??= {};
        for (const [index, item] of fixture.split.elements.entries()) {
          await select(item.elementId, index ? [modifier] : []);
          const source = element(
              record.before.project,
              fixture.sequenceId,
              item.elementId,
            ),
            c = source.channels.find((c) => c.id === item.channelId);
          expect(c.kind).toBe("scalar");
          expect(c.target).toEqual({ kind: "element", element_id: source.id });
          expect(
            item.probeTimes.some((t) => T(t) < fixture.split.timeSeconds),
          ).toBe(true);
          expect(
            item.probeTimes.some((t) => T(t) > fixture.split.timeSeconds),
          ).toBe(true);
          record.samples_before[item.elementId] = await tool(
            "opencut_keyframe_v1_sample",
            {
              project_id: projectId,
              revision: record.before.revision,
              sequence_id: fixture.sequenceId,
              channel_id: c.id,
              times: item.probeTimes.map((t) =>
                sub(t, source.timeline_range.start),
              ),
            },
          );
          if (item.expectedValues) {
            expect(item.expectedValues).toHaveLength(item.probeTimes.length);
            record.samples_before[item.elementId].samples.forEach((sample, i) =>
              expect(sample.value).toBeCloseTo(item.expectedValues[i], 7));
          }
        }
        for (const item of fixture.split.elements)
          await expect(page.locator(selector(item.elementId))).toHaveAttribute(
            "data-selected",
            "true",
          );
        await seek(fixture.split.timeSeconds);
        const dialog = await settings();
        await dialog
          .locator('[data-shortcut-action="split"]')
          .getByRole("button", {
            name: "Run: Split elements at playhead",
            exact: true,
          })
          .click();
        const review = page.getByRole("dialog", {
          name: "Review timeline changes",
          exact: true,
        });
        await expect(review).toBeVisible();
        return () =>
          review
            .getByRole("button", {
              name: "Apply timeline changes",
              exact: true,
            })
            .click();
      },
      async (before, after, record) => {
        const oldScene = before.project.sequences.find(
            (s) => s.id === fixture.sequenceId,
          ),
          newScene = after.project.sequences.find(
            (s) => s.id === fixture.sequenceId,
          );
        const trackIds = fixture.split.elements.map(
          (item) =>
            oldScene.tracks.find((t) =>
              t.elements.some((e) => e.id === item.elementId),
            ).id,
        );
        expect(new Set(trackIds).size).toBe(trackIds.length);
        for (const item of fixture.split.elements) {
          const oldTrack = oldScene.tracks.find((t) =>
              t.elements.some((e) => e.id === item.elementId),
            ),
            old = oldTrack.elements.find((e) => e.id === item.elementId),
            oldChannel = old.channels.find((c) => c.id === item.channelId),
            next = newScene.tracks.find((t) => t.id === oldTrack.id),
            end = add(old.timeline_range.start, old.timeline_range.duration),
            fragments = next.elements.filter(
              (e) =>
                cmp(e.timeline_range.start, old.timeline_range.start) >= 0 &&
                cmp(e.timeline_range.start, end) < 0 &&
                (e.id === old.id ||
                  !oldTrack.elements.some((old) => old.id === e.id)),
            );
          expect(fragments).toHaveLength(2);
          expect(next.elements).toHaveLength(oldTrack.elements.length + 1);
          for (const [index, time] of item.probeTimes.entries()) {
            const fragment = fragments.find(
              (e) =>
                cmp(time, e.timeline_range.start) >= 0 &&
                cmp(
                  time,
                  add(e.timeline_range.start, e.timeline_range.duration),
                ) < 0,
            );
            expect(fragment).toBeTruthy();
            const candidates = fragment.channels.filter(
              (c) =>
                c.kind === "scalar" &&
                c.target.kind === "element" &&
                c.target.element_id === fragment.id &&
                JSON.stringify(c.binding) ===
                  JSON.stringify(oldChannel.binding),
            );
            expect(candidates).toHaveLength(1);
            const observed = await tool("opencut_keyframe_v1_sample", {
              project_id: projectId,
              revision: after.revision,
              sequence_id: fixture.sequenceId,
              channel_id: candidates[0].id,
              times: [sub(time, fragment.timeline_range.start)],
            });
            expect(observed.samples[0].value).toBeCloseTo(
              record.samples_before[item.elementId].samples[index].value,
              7,
            );
            expect(observed.samples[0].physical_value).toBeCloseTo(
              record.samples_before[item.elementId].samples[index]
                .physical_value,
              7,
            );
          }
        }
        // Structural comparison outside the changed tracks prevents hidden edits.
        const expected = structuredClone(before.project);
        expected.sequences.find((s) => s.id === fixture.sequenceId).tracks =
          oldScene.tracks.map((t) =>
            trackIds.includes(t.id)
              ? newScene.tracks.find((n) => n.id === t.id)
              : t,
          );
        expect(after.project).toEqual(expected);
      },
    );
    const final = await inspect();
    expect(final.project).toEqual(facts.initial.project);
    facts.final_revision = final.revision;
    facts.status = "passed";
    facts.pending_scopes = fixture.animatedScale ? [] : ["V2 animated scale axis fixture absent"];
    if (facts.pending_scopes.length) facts.status = "passed_scoped";
    facts.scope =
      "UI shortcut persistence/clear, group transform with mandatory independently authored pixel probes, exact Escape and pending-final-admission cancel, original-request idempotency, observed-alpha selection, scalar key value UI, gain curve offset, multi-split sample preservation and exact graph Undo. V2 animated scale_x only when its explicit stage is verified; no independent PCM or OS IME claim.";
    await persist();
    return facts;
  } catch (error) {
    facts.status = "failed";
    facts.failure = String(error);
    await persist();
    throw error;
  } finally {
    page.off("request", observe);
    await page.keyboard.press("Escape").catch(() => {});
    await page.mouse.up().catch(() => {});
  }
}
