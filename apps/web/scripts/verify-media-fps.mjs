import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { importProductMedia, placeProductMedia } from "./verify-editor.mjs";

export async function verifySceneMediaFps(browser, endpoint, directory, check) {
  const fixture = JSON.parse(await fs.readFile(path.join(directory, "fps-v4-fixture.json"), "utf8"));
  const { filename, expected_fps: fps } = fixture;
  expect(path.basename(filename)).toBe(filename);
  expect(fps.denominator).toBeGreaterThan(0);
  expect(fps.numerator / fps.denominator).toBeGreaterThan(24);
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
  page.setDefaultTimeout(60000);
  const writes = [], errors = [];
  const time = (ticks, timebase = 1) => ({ ticks: String(ticks), timebase });
  // Three visible source seconds fit Classic's 120000 tick rounding exactly;
  // the independent 1/7 trim remains deliberately off the project frame grid.
  const clocks = { start: time(1, 3), duration: time(3), trim_start: time(1, 7), trim_end: time(6, 7), key_end: time(2, 3) };
  let projectId;
  page.on("pageerror", error => errors.push(error.message));
  page.on("request", request => {
    if (request.url().endsWith("/api/tools/opencut_edit_apply")) writes.push(request.postDataJSON());
  });
  const tool = async (name, input) => {
    const envelope = await page.evaluate(async ({ name, input }) => (await fetch(`/api/tools/${name}`, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input),
    })).json(), { name, input });
    expect(envelope.error).toBeNull();
    return envelope.data;
  };
  const inspect = () => tool("opencut_project_inspect", { project_id: projectId, revision: null });
  const playhead = page.getByRole("slider", { name: "Scene playhead", exact: true });
  const assertPlayhead = () => expect(playhead).toHaveAttribute("aria-valuenow", "0.5005");
  try {
    await page.goto(`${endpoint}/editor`);
    await expect(page.getByRole("button", { name: "New project", exact: true })).toBeEnabled();
    projectId = `fps-v4-${crypto.randomUUID()}`;
    await tool("opencut_project_create", { project: { id: projectId, name: "F08 V4 global FPS",
      sequence_id: "main", width: 320, height: 180, fps: { numerator: 24, denominator: 1 } },
      idempotency_key: `${projectId}-create` });
    await page.goto(`${endpoint}/editor/${projectId}`);
    await expect(page.getByRole("main", { name: "OpenCut editor", exact: true })).toBeVisible();
    const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
    if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
    await importProductMedia(page, path.join(directory, "sample.mp4"), writes);
    await placeProductMedia(page, "sample.mp4", { track: "new", sourceAudio: false,
      fields: { "Timeline start (seconds)": "0", "Source start (seconds)": "0", "Source duration (seconds)": "4" } }, writes);
    const state = await inspect();
    const sourceTrack = state.project.sequences[0].tracks.find(track => track.elements.length);
    expect(sourceTrack).toBeTruthy();
    // Both scenes contain a real bound video. Precise clocks are authored via
    // the ordinary public API, without quantizing them to either frame grid.
    const scenes = ["main", "other"].map(id => {
      const scene = structuredClone(state.project.sequences[0]);
      scene.id = id; scene.name = id;
      scene.tracks = [structuredClone(sourceTrack)];
      scene.main_track_id = scene.tracks[0].id = `fps-${id}-main`;
      const element = scene.tracks[0].elements[0];
      element.id = `fps-${id}-video`;
      element.timeline_range = { start: clocks.start, duration: clocks.duration };
      element.source_span = { profile: "classic_trimmed", trim_start: clocks.trim_start, trim_end: clocks.trim_end, source_duration: time(4) };
      element.channels = [{ kind: "scalar", id: `fps-${id}-opacity`, target: { kind: "element", element_id: element.id },
        binding: { kind: "property", parameter: "opacity" }, extrapolation: { before: "hold", after: "hold" },
        keys: [0, 1].map((value, index) => ({ id: `fps-${id}-key-${index}`, time: index ? clocks.key_end : time(0), value,
          left_handle: null, right_handle: null, segment_to_next: { kind: "linear" }, tangent_mode: "auto" })) }];
      scene.bookmarks = [{ id: `fps-${id}-bookmark`, sequence_id: id, time: time(1, 3), duration: time(2, 3), color: null, note: "Independent clocks" }];
      return scene;
    });
    await tool("opencut_edit_apply", { project_id: projectId, expected_revision: state.revision,
      idempotency_key: `${projectId}-clocks`, label: "Two nonempty scenes with independent clocks",
      operations: [
        ...scenes.map(sequence => ({ type: "sequence_put", sequence: { ...sequence, bookmarks: [] } })),
        ...scenes.flatMap(sequence => sequence.bookmarks.map(bookmark => ({ type: "bookmark", operation: { kind: "put", bookmark } }))),
      ] });
    await page.reload();
    await expect(page.getByRole("main", { name: "OpenCut editor", exact: true })).toBeVisible();
    await page.getByLabel("Scene", { exact: true }).selectOption("other");
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill("0.5005");
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
    await assertPlayhead();
    const before = await inspect();
    expect(before.project.sequences).toEqual(scenes);
    expect(before.project.settings.settings.fps).toEqual({ numerator: 24, denominator: 1 });
    const writeCount = writes.length;
    const imported = await importProductMedia(page, path.join(directory, filename), writes);
    expect(writes.slice(writeCount)).toEqual([imported.plan.transaction]);
    expect(imported.plan.decisions).toMatchObject({ settings_scope: { profile: "classic_global" },
      previous_fps: { numerator: 24, denominator: 1 }, raised_fps: fps, first_visual_fps: null });
    expect(imported.plan.transaction.expected_revision).toBe(before.revision);
    expect(imported.receipt.base_revision).toBe(before.revision);
    const raised = await inspect();
    expect(raised.project.settings).toEqual({ ...before.project.settings, settings: { ...before.project.settings.settings, fps } });
    expect(raised.project.sequences).toEqual(scenes);
    expect(raised.project.assets).toHaveLength(before.project.assets.length + 1);
    expect(raised.project.assets.filter(asset => asset.name !== filename)).toEqual(before.project.assets);
    expect({ ...raised.project, settings: before.project.settings, assets: before.project.assets }).toEqual(before.project);
    expect(raised.project.assets.find(asset => asset.name === filename).metadata.frame_rate).toEqual(fps);
    await assertPlayhead();
    // Observe existing product sync (context.tsx); there is no project refresh
    // button to invent, and no production controller global to inject.
    const refreshed = await Promise.all([
      page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_project_inspect")),
      page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_project_history_navigation")),
    ]);
    for (const response of refreshed) expect((await response.json()).error).toBeNull();
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await assertPlayhead();
    for (const id of ["main", "other"]) {
      await page.getByLabel("Scene", { exact: true }).selectOption(id);
      await expect(page.locator(".editor-scene-bar")).toContainText(`${fps.numerator / fps.denominator} fps`);
      await expect(page.locator(".editor-scene-bar")).toContainText("Global settings");
    }
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill("0.5005");
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await page.getByRole("button", { name: "Apply undo", exact: true }).click();
    await expect.poll(async () => (await inspect()).project).toEqual(before.project);
    await assertPlayhead();
    await page.reload();
    await expect(page.getByRole("main", { name: "OpenCut editor", exact: true })).toBeVisible();
    expect((await inspect()).project).toEqual(before.project);
    await expect(page.locator(".editor-scene-bar")).toContainText("24 fps");
    await expect(playhead).toHaveAttribute("aria-valuenow", "0");
    expect(errors).toEqual([]);
    check("F08 current V4: real VFR import raises global FPS in a nonempty second scene through one original media transaction; exact complete scenes and live playhead survive apply/refresh/Undo, and Undo contents/settings survive reload", {
      fps_v4: { project_id: projectId, fixture, before_revision: before.revision, raised_revision: raised.revision,
        clocks, playhead: time(1001, 2000), reload_playhead: time(0), sequence_ids: ["main", "other"] },
    });
  } catch (error) {
    await page.screenshot({ path: path.join(directory, "fps-v4-failure.png"), fullPage: true });
    throw error;
  } finally {
    await fs.writeFile(path.join(directory, "fps-v4-calls.json"), JSON.stringify({ projectId, fixture, writes, errors }, null, 2));
    await page.close();
  }
}
