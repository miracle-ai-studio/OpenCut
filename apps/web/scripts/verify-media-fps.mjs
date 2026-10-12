import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { importProductMedia, placeProductMedia } from "./verify-v4-editor.mjs";

// Retained legacy V3 CFR/export scope; current V4 acceptance is separate below.
export async function verifyMediaFps(browser, endpoint, directory, check) {
  const page = await browser.newPage({
      locale: "en-US",
      viewport: { width: 1440, height: 1000 },
    }),
    calls = [],
    errors = [],
    responses = [];
  page.on("response", (r) => {
    if (r.url().includes("/v1/artifacts/"))
      responses.push({ url: r.url(), status: r.status() });
  });
  page.on("pageerror", (e) => errors.push(e.message));
  page.setDefaultTimeout(60000);
  let projectId;
  const tool = async (name, input) => {
    const result = await page.evaluate(
      async ({ name, input }) => {
        const r = await fetch(`/v1/tools/${name}`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(input),
        });
        return { status: r.status, reply: await r.json() };
      },
      { name, input },
    );
    calls.push({ name, input, ...result });
    expect(result.reply.error).toBeNull();
    return result.reply.data;
  };
  const inspect = () =>
    tool("opencut_project_v3_inspect", { project_id: projectId });
  const readPlayhead = (sequenceId) =>
    page.evaluate(
      ({ projectId, sequenceId }) => {
        // Flush the existing debounced view persistence through its lifecycle
        // event; compare rational time rather than the rounded timecode label.
        window.dispatchEvent(new Event("pagehide"));
        return JSON.parse(localStorage.getItem("opencut.timeline") ?? "[]")
          .find(
            (entry) =>
              entry.projectId === projectId && entry.sequenceId === sequenceId,
          )?.playhead;
      },
      { projectId, sequenceId },
    );
  const base = (operation) => ({ type: "base.apply", operation });
  const history = async (name) => {
    await page.getByRole("button", { name: "History", exact: true }).click();
    await page.getByRole("button", { name, exact: true }).click();
  };
  const media = page.getByRole("region", { name: "Media", exact: true });
  const file = media.locator('input[type="file"]');
  const importFiles = async (names) => {
    await page.getByRole("button", { name: "Media", exact: true }).click();
    await file.setInputFiles(names.map((name) => path.join(directory, name)));
    for (const name of names)
      await expect(
        media.getByRole("button", {
          name: `Add ${name} to timeline`,
          exact: true,
        }),
      ).toBeVisible({ timeout: 120000 });
    await expect(page.getByRole("alert")).toHaveCount(0);
  };
  try {
    for (const [name, rate, color] of [
      ["fps-low.mp4", "12", "red"],
      ["fps-high.mp4", "60000/1001", "blue"],
      ["fps-fraction.mp4", "30000/1001", "green"],
    ])
      execFileSync("ffmpeg", [
        "-v",
        "error",
        "-f",
        "lavfi",
        "-i",
        `color=${color}:s=160x90:r=${rate}:d=2.002`,
        ...(name === "fps-low.mp4"
          ? ["-vf", "drawbox=color=blue:t=fill:enable='lt(t,0.5)'"]
          : []),
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        path.join(directory, name),
      ]);
    await page.goto(`${endpoint}/editor`);
    await page
      .getByRole("button", { name: "New project", exact: true })
      .click();
    await page
      .getByLabel("Project name")
      .fill("F08 native frame rate acceptance");
    await page
      .getByRole("button", { name: "Create project", exact: true })
      .click();
    await expect(
      page.getByRole("region", { name: "Timeline", exact: true }),
    ).toBeVisible();
    await page.waitForURL(/\/editor\/[^/]+/);
    projectId = new URL(page.url()).pathname.split("/")[2];
    const guide = page.locator(".editor-guide");
    if (await guide.count()) {
      await guide.getByRole("button", { name: "Next", exact: true }).click();
      await guide.getByRole("button", { name: "Next", exact: true }).click();
      await guide.getByRole("button", { name: "Finish", exact: true }).click();
    }
    if (
      await page
        .getByRole("button", { name: "Dismiss update", exact: true })
        .count()
    )
      await page
        .getByRole("button", { name: "Dismiss update", exact: true })
        .click();
    await importFiles(["fps-low.mp4"]);
    let initial = await inspect();
    expect(initial.project.base.sequences[0].fps).toEqual({
      numerator: 24,
      denominator: 1,
    });
    expect(initial.project.base.assets[0].metadata.frame_rate).toEqual({
      numerator: 12,
      denominator: 1,
    });
    await media
      .getByRole("button", { name: "Add fps-low.mp4 to timeline", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Clip fps-low.mp4", exact: true }),
    ).toBeVisible();
    initial = await inspect();
    const clip = initial.project.base.sequences[0].tracks.flatMap(
        (t) => t.clips,
      )[0],
      track = initial.project.base.sequences[0].tracks.find((t) =>
        t.clips.some((c) => c.id === clip.id),
      );
    const fixture = await tool("opencut_edit_v3_apply", {
      project_id: projectId,
      expected_revision: initial.revision,
      idempotency_key: crypto.randomUUID(),
      label: "F08 exact timing fixture",
      operations: [
        base({
          type: "sequence.update_settings",
          sequence_id: initial.project.base.sequences[0].id,
          width: 160,
          height: 90,
        }),
        base({
          type: "clip.trim",
          sequence_id: initial.project.base.sequences[0].id,
          track_id: track.id,
          clip_id: clip.id,
          timeline_range: {
            start: { ticks: "1001", timebase: 2000 },
            duration: { ticks: "3003", timebase: 2000 },
          },
          source_range: {
            start: { ticks: "1001", timebase: 2000 },
            duration: { ticks: "3003", timebase: 2000 },
          },
        }),
        base({
          type: "keyframe.channel_set",
          sequence_id: initial.project.base.sequences[0].id,
          track_id: track.id,
          clip_id: clip.id,
          channel: {
            parameter: "opacity",
            keyframes: [
              {
                id: "fps-key-start",
                time: { ticks: "0", timebase: 1 },
                value: 0,
                interpolation: "linear",
              },
              {
                id: "fps-key-end",
                time: { ticks: "1001", timebase: 1000 },
                value: 1,
                interpolation: "linear",
              },
            ],
          },
        }),
      ],
    });
    await page.reload();
    await expect(
      page.getByRole("region", { name: "Timeline", exact: true }),
    ).toBeVisible();
    const before = await inspect();
    expect(before.revision).toBe(fixture.revision);
    await page
      .locator(".timeline-ruler .ruler-content")
      .click({ position: { x: 40, y: 8 } });
    const sequenceId = before.project.base.sequences[0].id;
    const playhead = await readPlayhead(sequenceId);
    expect(BigInt(playhead.ticks)).toBeGreaterThan(0n);
    // View time is not rounded to a new frame grid during an import.
    check(
      "F08: native low-rate metadata is exposed and a 12fps import leaves the 24fps scene unchanged",
    );
    await importFiles(["fps-fraction.mp4", "fps-high.mp4"]);
    const raised = await inspect();
    expect(raised.project.base.sequences[0].fps).toEqual({
      numerator: 60000,
      denominator: 1001,
    });
    expect(raised.project.base.sequences[0].tracks).toEqual(
      before.project.base.sequences[0].tracks,
    );
    expect(await readPlayhead(sequenceId)).toEqual(playhead);
    expect(
      raised.project.base.assets.find((a) => a.name === "fps-fraction.mp4")
        .metadata.frame_rate,
    ).toEqual({ numerator: 30000, denominator: 1001 });
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await expect(
      page
        .getByRole("region", { name: "Settings", exact: true })
        .getByRole("combobox", { name: "Frame rate", exact: true }),
    ).toHaveValue("60000/1001");
    await page.reload();
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await expect(
      page
        .getByRole("region", { name: "Settings", exact: true })
        .getByRole("combobox", { name: "Frame rate", exact: true }),
    ).toHaveValue("60000/1001");
    expect(await readPlayhead(sequenceId)).toEqual(playhead);
    check(
      "F08: mixed 29.97/59.94 imports raise once from native metadata, preserve exact clip/channel time and survive reload",
    );
    await history("Undo latest action");
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(before.project);
    expect(await readPlayhead(sequenceId)).toEqual(playhead);
    await history("Redo action");
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(raised.project);
    expect(await readPlayhead(sequenceId)).toEqual(playhead);
    check(
      "F08: one import history step restores/removes both published bindings and FPS with complete document undo/redo",
    );
    // Compare the actual frozen binary's CLI/MCP plans to the browser HTTP response.
    const current = await inspect();
    const low = current.project.base.assets.find(
      (a) => a.name === "fps-low.mp4",
    );
    const request = {
      project_id: projectId,
      expected_revision: current.revision,
      sequence_id: raised.project.base.sequences[0].id,
      idempotency_key: "fps-transport-plan",
      label: "F08 transport plan",
      media: [
        {
          media_id: raised.project.base.assets.find(
            (a) => a.name === "fps-low.mp4",
          ).id,
          replace_asset_id: low.id,
        },
      ],
      operations: [
        base({
          type: "track.update",
          sequence_id: current.project.base.sequences[0].id,
          track_id: track.id,
          muted: true,
        }),
      ],
      ripple: false,
    };
    const http = await tool("opencut_media_import_plan_v1", request);

    const transport = JSON.parse(
      execFileSync(
        "python3",
        [
          path.resolve(
            import.meta.dirname,
            "../../../scripts/verify-fps-transports.py",
          ),
          process.env.OPENCUT_WEB_BINARY,
          path.join(directory, "workspace.sqlite"),
          directory,
          JSON.stringify(request),
        ],
        { encoding: "utf8" },
      ),
    );
    expect(transport.cli).toEqual(http);
    expect(transport.mcp).toEqual(http);
    expect(http.raised_fps).toBeNull();
    expect((await inspect()).revision).toBe(current.revision);
    check(
      "F08: CLI, HTTP and MCP return identical read-only fixed-revision import plans; lower-rate replacement planning does not lower FPS",
    );
    await page.getByRole("button", { name: "Export", exact: true }).click();
    const dialog = page.getByRole("dialog", {
      name: "Export project",
      exact: true,
    });
    await dialog
      .getByRole("button", { name: "Check project", exact: true })
      .click();
    await expect(
      dialog.getByRole("button", { name: "Export", exact: true }),
    ).toBeEnabled({ timeout: 60000 });
    await dialog.getByRole("button", { name: "Export", exact: true }).click();
    await page
      .getByRole("button", { name: "Tasks", exact: true })
      .first()
      .click();
    const download = page
      .locator(".task-card")
      .filter({ hasText: "Export" })
      .getByRole("button", { name: "Download", exact: true });
    await expect(download).toBeVisible({ timeout: 120000 });
    await page.evaluate(() => {
      window.showSaveFilePicker = async () => ({
        createWritable: async () => {
          const parts = [];
          return new WritableStream({
            write(c) {
              parts.push(c);
            },
            close() {
              window.fpsExport = new Blob(parts);
            },
          });
        },
      });
    });
    await download.click();
    await expect
      .poll(() => page.evaluate(() => window.fpsExport?.size ?? 0), {
        timeout: 30000,
      })
      .toBeGreaterThan(0);
    const bytes = await page.evaluate(async () =>
        Array.from(new Uint8Array(await window.fpsExport.arrayBuffer())),
      ),
      output = path.join(directory, "fps-export.mp4");
    await fs.writeFile(output, Buffer.from(bytes));
    const probe = JSON.parse(
      execFileSync(
        "ffprobe",
        ["-v", "error", "-show_streams", "-show_format", "-of", "json", output],
        { encoding: "utf8" },
      ),
    );
    const video = probe.streams.find((s) => s.codec_type === "video");
    expect(video.avg_frame_rate).toBe("60000/1001");
    expect(Number(video.nb_frames)).toBe(120);
    expect(Math.abs(Number(probe.format.duration) - 2.002)).toBeLessThan(0.04);
    // At 60000/1001 fps, the clip begins at frame 30. Its source crop
    // excludes the blue prefix. Linear opacity reaches 1 at frame 90:
    // frames 45/60 are independently 1/4 and 1/2 red over black.
    const samples = [
      { frame: 15, expected: [0, 0, 0] },
      { frame: 45, expected: [64, 0, 0] },
      { frame: 60, expected: [128, 0, 0] },
      { frame: 90, expected: [255, 0, 0] },
    ];
    const pixels = execFileSync("ffmpeg", [
      "-v", "error", "-i", output,
      "-vf", "select='eq(n,15)+eq(n,45)+eq(n,60)+eq(n,90)',scale=1:1",
      "-fps_mode", "passthrough", "-frames:v", "4",
      "-pix_fmt", "rgb24", "-f", "rawvideo", "-",
    ]);
    expect(pixels.length).toBe(samples.length * 3);
    for (const [index, sample] of samples.entries()) {
      sample.actual = [...pixels.subarray(index * 3, index * 3 + 3)];
      for (const [channel, expected] of sample.expected.entries())
        expect(
          Math.abs(sample.actual[channel] - expected),
          `frame ${sample.frame}, RGB channel ${channel}`,
        ).toBeLessThanOrEqual(18);
    }
    expect(errors).toEqual([]);
    await fs.writeFile(
      path.join(directory, "fps-export-probe.json"),
      JSON.stringify({ probe, samples, transport }, null, 2),
    );
    check(
      "F08: actual browser export is 60000/1001fps, 120 frames and 2.002 seconds; independent frame samples verify nonzero timeline/source starts and changing opacity",
    );
    await page.screenshot({
      path: path.join(directory, "fps-acceptance.png"),
      fullPage: true,
    });
  } catch (error) {
    await page.screenshot({
      path: path.join(directory, "fps-failure.png"),
      fullPage: true,
    });
    await fs.writeFile(
      path.join(directory, "fps-failure.json"),
      JSON.stringify(
        { alerts: await page.getByRole("alert").allTextContents(), responses },
        null,
        2,
      ),
    );
    throw error;
  } finally {
    await fs.writeFile(
      path.join(directory, "fps-calls.json"),
      JSON.stringify({ projectId, calls, errors }, null, 2),
    );
    await page.close();
  }
}

// The existing runner supplies the owned VFR file and an independent packet-rate
// oracle in fps-v4-fixture.json. This verifier never generates media or infers
// an expected rate from the service response. Existing V3/export checks stay.
export async function verifyV4MediaFps(browser, endpoint, directory, check) {
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
    if (request.url().endsWith("/v1/tools/opencut_edit_v4_apply")) writes.push(request.postDataJSON());
  });
  const tool = async (name, input) => {
    const envelope = await page.evaluate(async ({ name, input }) => (await fetch(`/v1/tools/${name}`, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input),
    })).json(), { name, input });
    expect(envelope.error).toBeNull();
    return envelope.data;
  };
  const inspect = () => tool("opencut_project_v4_inspect", { project_id: projectId, revision: null });
  const playhead = page.getByRole("slider", { name: "Scene playhead", exact: true });
  const assertPlayhead = () => expect(playhead).toHaveAttribute("aria-valuenow", "0.5005");
  try {
    await page.goto(`${endpoint}/editor`);
    await expect(page.getByRole("button", { name: "New project", exact: true })).toBeEnabled();
    projectId = `fps-v4-${crypto.randomUUID()}`;
    await tool("opencut_project_v4_create", { project: { id: projectId, name: "F08 V4 global FPS",
      sequence_id: "main", width: 320, height: 180, fps: { numerator: 24, denominator: 1 } },
      idempotency_key: `${projectId}-create` });
    await page.goto(`${endpoint}/editor/${projectId}`);
    await expect(page.getByRole("main", { name: "V4 editor", exact: true })).toBeVisible();
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
    await tool("opencut_edit_v4_apply", { project_id: projectId, expected_revision: state.revision,
      idempotency_key: `${projectId}-clocks`, label: "Two nonempty scenes with independent clocks",
      operations: [
        ...scenes.map(sequence => ({ type: "sequence_put", sequence: { ...sequence, bookmarks: [] } })),
        ...scenes.flatMap(sequence => sequence.bookmarks.map(bookmark => ({ type: "bookmark", operation: { kind: "put", bookmark } }))),
      ] });
    await page.reload();
    await expect(page.getByRole("main", { name: "V4 editor", exact: true })).toBeVisible();
    await page.getByLabel("V4 scene", { exact: true }).selectOption("other");
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
      page.waitForResponse(response => response.url().endsWith("/v1/tools/opencut_project_v4_inspect")),
      page.waitForResponse(response => response.url().endsWith("/v1/tools/opencut_project_v4_history_navigation")),
    ]);
    for (const response of refreshed) expect((await response.json()).error).toBeNull();
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await assertPlayhead();
    for (const id of ["main", "other"]) {
      await page.getByLabel("V4 scene", { exact: true }).selectOption(id);
      await expect(page.locator(".v4-scene-bar")).toContainText(`${fps.numerator / fps.denominator} fps`);
      await expect(page.locator(".v4-scene-bar")).toContainText("Global settings");
    }
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill("0.5005");
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await page.getByRole("button", { name: "Apply undo", exact: true }).click();
    await expect.poll(async () => (await inspect()).project).toEqual(before.project);
    await assertPlayhead();
    await page.reload();
    await expect(page.getByRole("main", { name: "V4 editor", exact: true })).toBeVisible();
    expect((await inspect()).project).toEqual(before.project);
    await expect(page.locator(".v4-scene-bar")).toContainText("24 fps");
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
