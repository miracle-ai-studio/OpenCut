import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { randomUUID, createHash } from "node:crypto";
import { importProductMedia, placeProductMedia } from "./verify-editor.mjs";
import { installScenePreviewWitness } from "./verify-scene-preview.mjs";

// The integrator owns the browser/service/candidate. Reuse the B1 project and
// its locked font history plus the existing synthetic media; never copy them.
export async function verifyVideo(browser, endpoint, directory, check) {
  const { project_id: projectId } = JSON.parse(await fs.readFile(path.join(directory, "b1-text-project.json"), "utf8"));
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 }, acceptDownloads: true });
  page.setDefaultTimeout(30000);
  const phase = process.env.OPENCUT_B2_PHASE ?? "all";
  expect(["preview", "export", "all"]).toContain(phase);
  const reuse = process.env.OPENCUT_B2_REUSE_COMPOSITION === "1";
  const recordPrefix = process.env.OPENCUT_B2_EVIDENCE_PREFIX ?? "b2";
  expect(recordPrefix).toMatch(/^[a-z0-9-]+$/u);
  const errors = [], writes = [], fontDownloads = [], fontAuthorizations = [], network = [];
  const beganRequests = new WeakMap(), requestRecords = new WeakMap(), pendingRequests = new Map();
  const diagnosticPending = new Set();
  const operation = request => {
    const pathname = new URL(request.url()).pathname;
    return pathname.startsWith("/api/tools/") ? pathname.slice("/api/tools/".length) :
      pathname.includes("/api/preview/") ? pathname : null;
  };
  page.on("request", request => {
    const name = operation(request); if (!name || network.length >= 2048) return;
    beganRequests.set(request, performance.now());
    const record = { operation: name, status: null, error: null, duration_ms: 0 };
    requestRecords.set(request, record); pendingRequests.set(request, record); network.push(record);
  });
  page.on("response", response => {
    const name = operation(response.request()), record = requestRecords.get(response.request()); if (!name || !record) return;
    record.status = response.status(); record.duration_ms = performance.now() - beganRequests.get(response.request());
    if (name.startsWith("opencut_")) {
      const reading = response.json().then(result => {
        if (result.error) record.error = { code: result.error.code, message: String(result.error.message ?? "").slice(0, 512) };
      }).catch(() => {}).finally(() => diagnosticPending.delete(reading));
      diagnosticPending.add(reading);
    }
  });
  page.on("requestfinished", request => {
    const record = requestRecords.get(request); if (record) record.duration_ms = performance.now() - beganRequests.get(request);
    pendingRequests.delete(request);
  });
  page.on("requestfailed", request => {
    const record = requestRecords.get(request); if (record) {
      record.error = String(request.failure()?.errorText ?? "request failed").slice(0, 512);
      record.duration_ms = performance.now() - beganRequests.get(request);
    }
    pendingRequests.delete(request);
  });
  page.on("pageerror", e => errors.push(e.message));
  page.on("request", r => {
    if (r.url().endsWith("opencut_edit_apply")) writes.push(r.postDataJSON());
    if (r.url().endsWith("/visual-font/stream")) fontDownloads.push(r.postDataJSON());
    if (r.url().endsWith("opencut_visual_preview_font_batch_authorize")) fontAuthorizations.push(r.postDataJSON());
  });
  // Headless acceptance exercises the existing browser-download fallback, not
  // the native Save File picker. No product download behavior is changed.
  await page.addInitScript(() => Object.defineProperty(window, "showSaveFilePicker", { value: undefined, configurable: true }));
  await page.addInitScript(installScenePreviewWitness, { deferReadback: true });
  await page.addInitScript(() => {
    const samples = window.__b2MixedSamples = [];
    const present = ImageBitmapRenderingContext.prototype.transferFromImageBitmap;
    ImageBitmapRenderingContext.prototype.transferFromImageBitmap = function (bitmap) {
      const time = Number(this.canvas.dataset.time);
      if (bitmap && this.canvas.classList.contains("editor-scene-frame") && samples.length < 2 &&
          ((samples.length === 0 && time >= .3 && time <= 1) || (samples.length === 1 && time >= 1.5 && time <= 2.5))) {
        samples.push({ time, generation: this.canvas.dataset.generation, bitmap: createImageBitmap(bitmap) });
      }
      return present.call(this, bitmap);
    };
    window.__readB2MixedSamples = async () => {
      const output = [];
      for (const sample of samples) {
        const bitmap = await sample.bitmap;
        try {
          const canvas = new OffscreenCanvas(bitmap.width, bitmap.height), ctx = canvas.getContext("2d");
          ctx.drawImage(bitmap, 0, 0);
          const blob = await canvas.convertToBlob({ type: "image/png" });
          const bytes = new Uint8Array(await blob.arrayBuffer());
          let binary = ""; for (const value of bytes) binary += String.fromCharCode(value);
          output.push({ time: sample.time, generation: sample.generation, width: bitmap.width, height: bitmap.height, png: btoa(binary) });
          canvas.width = 0; canvas.height = 0;
        } finally { bitmap.close(); }
      }
      return output;
    };
    const fetchOriginal = window.fetch;
    window.fetch = async (...args) => {
      const response = await fetchOriginal(...args);
      if (String(args[0]).endsWith("opencut_visual_preview_frame")) response.clone().json().then(r => {
        if (r.data?.bound) window.__scenePreviewWitness.bindings.push({ time: r.data.time_seconds,
          pts: r.data.bound_media.decoded.filter(d => d.kind === "readyVideo").map(d => d.actualTimestampSeconds),
          fontIds: r.data.plan.execution.execution.execution.execution.texts.map(t => t.font.fontId),
        });
      }).catch(() => {});
      return response;
    };
  });
  const tool = async (name, input) => {
    const result = await page.evaluate(async ({ name, input }) => (await fetch(`/api/tools/${name}`, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input),
    })).json(), { name, input });
    expect(result.error, JSON.stringify(result.error)).toBeNull(); return result.data;
  };
  const inspect = () => editorSnapshot(tool("opencut_project_inspect", { project_id: projectId, revision: null }));
  let fixed, artifact, job, titles, fonts, settings, preview = null, pictures = [], metadata, submission;
  try {
    await page.goto(`${endpoint}/editor/${encodeURIComponent(projectId)}`);
    const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
    if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
    await expect(page.getByRole("main", { name: "OpenCut editor", exact: true })).toBeVisible({ timeout: 30000 });
    let before = await inspect();
    const sequenceId = before.project.default_sequence_id;
    await page.getByRole("combobox", { name: "Scene", exact: true }).selectOption(sequenceId);
    if (!reuse) {
    for (const name of ["sample.mp4", "sample.wav"]) {
      if (!before.project.assets.some(a => a.name === name)) await importProductMedia(page, path.join(directory, name), writes);
      before = await inspect();
      const scene = before.project.sequences.find(s => s.id === sequenceId);
      if (!scene.tracks.some(t => t.elements.some(e => e.name === name))) await placeProductMedia(page, name, {
        track: "new", ...(name.endsWith("mp4") ? { sourceAudio: false } : { keepPitch: true }),
        fields: { "Timeline start (seconds)": "0", "Source start (seconds)": "0", "Source duration (seconds)": "3" },
      }, writes);
    }
    before = await inspect();
    const scene = before.project.sequences.find(s => s.id === sequenceId);
    const titleTrack = scene.tracks.find(t => t.elements.some(e => e.content.kind === "text"));
    const template = titleTrack?.elements.find(e => e.content.kind === "text");
    expect(template, "B1 owned title must exist").toBeTruthy();
    fonts = ["Lato", "Ma Shan Zheng"].map(family => before.project.fonts.find(f => f.family === family && f.weight === 400 && f.style === "normal"));
    expect(fonts.every(Boolean)).toBe(true);
    settings = structuredClone(before.project.settings);
    settings.settings.canvas_size = { width: 320, height: 180 };
    settings.settings.canvas_size_mode = "custom";
    settings.settings.last_custom_canvas_size = { width: 320, height: 180 };
    settings.settings.fps = { numerator: 30, denominator: 1 };
    settings.settings.background = { kind: "color", color: { encoding: "srgb", red: 0, green: 0, blue: 0, alpha: 1 } };
    const titleIds = [randomUUID(), randomUUID()];
    titles = ["OpenCut", "你好世界"].map((content, i) => {
      const element = structuredClone(template);
      element.id = titleIds[i]; element.name = `B2 independent ${i ? "Chinese" : "Latin"} title`; element.enabled = true; element.channels = [];
      element.timeline_range = { start: { ticks: "0", timebase: 1 }, duration: { ticks: "3", timebase: 1 } };
      Object.assign(element.content.text, { content, font: { kind: "locked", font_id: fonts[i].id }, font_weight: "normal", font_style: "normal",
        align: "center", decoration: "none", font_size: { unit: "classic_height90", value: 15 }, letter_spacing_px: 0, line_height_multiplier: 1.2,
        color: { encoding: "srgb", red: 1, green: 1, blue: 1, alpha: 1 }, background: { ...element.content.text.background, enabled: false } });
      Object.assign(element.visual, { opacity: 1, blend_mode: "normal", effects: [], masks: [], transform: { position_x_px: 0, position_y_px: i ? 45 : -45, scale_x: 1, scale_y: 1, rotation_degrees: 0 } });
      return element;
    });
    const bookmark = { id: randomUUID(), sequence_id: sequenceId, time: { ticks: "0", timebase: 1 }, duration: { ticks: "3", timebase: 1 }, note: "B2 EDITOR NOTE MUST NOT ENTER OUTPUT", color: "#facc15" };
    expect(scene.captions).toEqual([]); expect(scene.transitions).toEqual([]);
    const composed = structuredClone(scene);
    composed.bookmarks = structuredClone(scene.bookmarks);
    composed.tracks = scene.tracks.filter(track => track.id !== titleTrack.id).concat(titleTrack).map(track => {
      const next = structuredClone(track); next.enabled = true;
      expect(next.locked, "B2 owned tracks must be unlocked").toBe(false);
      next.elements = track.id === titleTrack.id ? [titles[0]] : next.elements.filter(e => ["sample.mp4", "sample.wav"].includes(e.name)).map(e => {
        const element = structuredClone(e);
        element.enabled = true; element.channels = [];
        expect(element.source_span.profile).toBe("classic_trimmed");
        const span = element.source_span;
        const duration = BigInt(span.source_duration.ticks), base = BigInt(span.source_duration.timebase);
        expect(duration >= 3n * base, "Synthetic media source must contain the whole three-second range").toBe(true);
        span.trim_start = { ticks: "0", timebase: 1 };
        span.trim_end = { ticks: (duration - 3n * base).toString(), timebase: span.source_duration.timebase };
        element.timeline_range = { start: { ticks: "0", timebase: 1 }, duration: { ticks: "3", timebase: 1 } };
        if (element.content.retime) expect(element.content.retime.rate).toEqual({ numerator: 1, denominator: 1 });
        return element;
      });
      return next;
    });
    const chineseTrack = structuredClone(titleTrack);
    chineseTrack.id = randomUUID(); chineseTrack.name = "B2 independent Chinese track";
    chineseTrack.enabled = true; chineseTrack.locked = false; chineseTrack.channels = [];
    chineseTrack.elements = [titles[1]];
    composed.tracks.push(chineseTrack);
    expect(composed.tracks.some(track => track.id === composed.main_track_id && track.kind === "video")).toBe(true);
    const selectedMedia = composed.tracks.flatMap(track => track.elements).filter(e => ["sample.mp4", "sample.wav"].includes(e.name));
    expect(selectedMedia.map(e => e.name).sort()).toEqual(["sample.mp4", "sample.wav"]);
    expect(new Set(composed.tracks.flatMap(track => [track.id, ...track.elements.map(e => e.id)])).size)
      .toBe(composed.tracks.reduce((count, track) => count + 1 + track.elements.length, 0));
    expect(composed.tracks.every(track => track.elements.length <= 1), "Each owned output track has at most one element; no disabled overlap remains").toBe(true);
    // Authored stack is explicit: media first, fixed text above it. The oracle
    // checks white glyphs over the actual source color, not receipt parity.
    const operations = [{ type: "settings_set", settings }, { type: "sequence_put", sequence: composed }, { type: "bookmark", operation: { kind: "put", bookmark } }];
    // Notes are project history and must not appear in the output composition.
    await tool("opencut_edit_apply", { project_id: projectId, expected_revision: before.revision, idempotency_key: randomUUID(), label: "B2 mixed media font sound composition", operations });
    fixed = await inspect();
    expect(fixed.project.sequences.find(s => s.id === sequenceId).bookmarks).toEqual(expect.arrayContaining([...scene.bookmarks, bookmark]));
    } else fixed = before;
    const savedScene = fixed.project.sequences.find(s => s.id === sequenceId);
    titles = savedScene.tracks.flatMap(track => track.elements).filter(e => e.enabled && e.name.startsWith("B2 independent") && e.content.kind === "text");
    expect(titles).toHaveLength(2);
    titles.sort((a, b) => a.visual.transform.position_y_px - b.visual.transform.position_y_px);
    fonts = titles.map(e => fixed.project.fonts.find(f => f.id === e.content.text.font.font_id));
    expect(fonts.every(Boolean)).toBe(true);
    settings = fixed.project.settings;
    metadata = { project_id: projectId, revision: fixed.revision, sequence_id: sequenceId,
      settings: settings.settings, titles: titles.map(e => ({ id: e.id, text: e.content.text, visual: e.visual })),
      fonts: fonts.map(f => ({ id: f.id, family: f.family, content: f.content })), bookmarks: savedScene.bookmarks };
    for (const savedFile of process.env.OPENCUT_B2_REUSE_JOB ? [`${recordPrefix}-job-evidence.json`, `${recordPrefix}-video-evidence.json`] : []) {
      try {
        const prior = JSON.parse(await fs.readFile(path.join(directory, savedFile), "utf8"));
        if (prior.revision === fixed.revision) { preview = prior.preview ?? preview; pictures = prior.pictures ?? pictures; metadata = { ...prior, ...metadata }; }
      } catch (error) { if (error.code !== "ENOENT") throw error; }
    }
    await fs.writeFile(path.join(directory, `${recordPrefix}-composition-evidence.json`), JSON.stringify(metadata, null, 2) + "\n");
    if (phase !== "export") {

    await page.reload();
    await expect(page.getByRole("main", { name: "OpenCut editor", exact: true })).toBeVisible({ timeout: 30000 });
    await page.getByRole("combobox", { name: "Scene", exact: true }).selectOption(sequenceId);
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill("0");
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
    const live = page.locator("canvas.editor-scene-frame"), writeCount = writes.length;
    await page.getByRole("button", { name: "Play scene preview", exact: true }).click();
    // Cold font/media initialization is a functional wait, not a latency claim.
    await expect(live).toBeVisible({ timeout: 30000 });
    const generation = await live.getAttribute("data-generation");
    await expect.poll(() => page.evaluate(() => window.__scenePreviewWitness.frames.filter(f => f.time > .3).length)).toBeGreaterThan(2);
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill("1.5");
    await page.getByLabel("Playhead (seconds)", { exact: true }).press("Enter");
    await expect.poll(() => live.getAttribute("data-generation")).not.toBe(generation);
    await expect.poll(() => page.evaluate(() => window.__scenePreviewWitness.frames.some(f => f.time >= 1.5))).toBe(true);
    await page.getByRole("button", { name: "Pause scene audio", exact: true }).click();
    await expect(page.getByText("Scene sound paused", { exact: true })).toBeVisible();
    await page.evaluate(() => window.__readScenePreviewSamples());
    preview = await page.evaluate(() => { const w = window.__scenePreviewWitness; return { frames: w.frames, progresses: w.progresses, bindings: w.bindings, controls: w.controls, blocks: w.blocks, overflows: w.overflows }; });
    pictures = await page.evaluate(() => window.__readB2MixedSamples());
    expect(pictures).toHaveLength(2);
    for (let index = 0; index < pictures.length; index++) {
      pictures[index].output = path.join(directory, `${recordPrefix}-preview-${index}.png`);
      await fs.writeFile(pictures[index].output, Buffer.from(pictures[index].png, "base64"));
      delete pictures[index].png;
    }
    expect(preview.frames.filter(f => f.time > .3).length).toBeGreaterThan(2);
    expect(preview.bindings.some(b => b.fontIds?.length === 2)).toBe(true);
    expect(preview.frames.every(f => f.width === 320 && f.height === 180)).toBe(true);
    expect(fontAuthorizations.length).toBeGreaterThan(0);
    for (const authorization of fontAuthorizations)
      expect([...authorization.font_ids].sort()).toEqual(fonts.map(font => font.id).sort());
    expect(fontDownloads.length).toBeLessThanOrEqual(4); // two fonts, two owner generations
    expect((await inspect()).revision).toBe(fixed.revision); expect(writes.length).toBe(writeCount);
    const previewEvidence = { ...metadata, preview, pictures, preview_passed: true, font_downloads: fontDownloads.length, font_authorizations: fontAuthorizations.length, pageerrors: errors, network };
    await fs.writeFile(path.join(directory, `${recordPrefix}-preview-evidence.json`), JSON.stringify(previewEvidence, null, 2) + "\n");
    await fs.writeFile(path.join(directory, `${recordPrefix}-video-evidence.json`), JSON.stringify(previewEvidence, null, 2) + "\n");
    metadata = previewEvidence;
    check("B2 mixed preview playback, seek generation and session font reuse", previewEvidence);
    if (phase === "preview") return previewEvidence;
    }
    await page.reload(); // owner shutdown closes the sound device and releases fonts
    await expect(page.getByRole("main", { name: "OpenCut editor", exact: true })).toBeVisible({ timeout: 30000 });
    const reusedJob = process.env.OPENCUT_B2_REUSE_JOB;
    if (reusedJob) {
      job = await tool("opencut_job_view", { job_id: reusedJob });
      submission = metadata.submission?.job_id === reusedJob ? metadata.submission : { source: "existing_public_job", job_id: reusedJob, target: job.target };
      await page.getByRole("button", { name: "Tasks", exact: true }).click();
    } else {
    await page.getByRole("button", { name: "Export video", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "Export video", exact: true });
    await expect(dialog.getByRole("button", { name: "Start video export", exact: true })).toBeEnabled();
    const submitting = page.waitForResponse(r => r.url().endsWith("opencut_render_submit"));
    await dialog.getByRole("button", { name: "Start video export", exact: true }).click();
    const response = await submitting, submitted = await response.json();
    expect(submitted.error).toBeNull();
    expect(response.request().postDataJSON()).toMatchObject({ kind: "video", request: { project_id: projectId, revision: fixed.revision, sequence_id: sequenceId } });
    submission = { source: "ui_submit", job_id: submitted.data.job_id, request: response.request().postDataJSON() };
    await dialog.getByRole("button", { name: "View tasks", exact: true }).click();
    await expect.poll(async () => { job = await tool("opencut_job_view", { job_id: submitted.data.job_id }); return ["succeeded", "failed", "cancelled"].includes(job.state) ? job.state : "pending"; }, { timeout: 180000, intervals: [250, 500, 1000] }).not.toBe("pending");
    }
    expect(job.target).toMatchObject({ project_id: projectId, revision: fixed.revision, sequence_id: sequenceId });
    expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
    expect(job.kind).toBe("render.video");
    artifact = job.artifacts.find(a => a.mime_type === "video/mp4"); expect(artifact).toBeTruthy();
    const jobEvidence = { ...metadata, job, submission, job_passed: true, pageerrors: errors, network };
    await fs.writeFile(path.join(directory, `${recordPrefix}-job-evidence.json`), JSON.stringify(jobEvidence, null, 2) + "\n");
    const taskPanel = page.getByRole("region", { name: "Tasks", exact: true });
    await taskPanel.getByRole("button", { name: "Refresh tasks", exact: true }).click();
    const rows = []; let cursor = null;
    do { const pageOfJobs = await tool("opencut_job_query", { cursor, limit: 100 }); rows.push(...pageOfJobs.jobs); cursor = pageOfJobs.next_cursor ?? null; } while (cursor);
    const displayed = rows.filter(row => row.target?.project_id === projectId || row.resource_target?.project_id === projectId).reverse();
    const index = displayed.findIndex(row => row.job_id === job.job_id); expect(index).toBeGreaterThanOrEqual(0);
    const task = taskPanel.locator(".task-card").nth(index);
    await expect(task).toContainText(fixed.revision); await expect(task).toContainText("succeeded");
    const downloading = page.waitForEvent("download");
    await task.getByRole("button", { name: "Download", exact: true }).click();
    const download = await downloading, output = path.join(directory, `${recordPrefix}-mixed.mp4`);
    await download.saveAs(output);
    const bytes = await fs.readFile(output), sha256 = createHash("sha256").update(bytes).digest("hex");
    expect(sha256).toBe(artifact.content_hash);
    expect(errors).toEqual([]);
    const evidence = { ...metadata, output, sha256, job, submission, preview, pictures, export_passed: true, download_scope: "browser fallback with native Save File picker disabled", reused_job: reusedJob ?? null,
      font_downloads: metadata.font_downloads ?? fontDownloads.length, font_authorizations: metadata.font_authorizations ?? fontAuthorizations.length, pageerrors: errors, network };
    await fs.writeFile(path.join(directory, `${recordPrefix}-export-evidence.json`), JSON.stringify(evidence, null, 2) + "\n");
    await fs.writeFile(path.join(directory, `${recordPrefix}-video-evidence.json`), JSON.stringify(evidence, null, 2) + "\n");
    check(evidence.preview_passed ? "B2 mixed fixed-font media/sound continuous preview and actual MP4 UI task download" : "B2 mixed fixed-font media/sound MP4 UI task and actual download; continuous preview unverified", evidence);
    return evidence;
  } catch (error) {
    await Promise.race([Promise.allSettled([...diagnosticPending]), new Promise(resolve => setTimeout(resolve, 1500))]);
    for (const [request, record] of pendingRequests) record.duration_ms = performance.now() - beganRequests.get(request);
    let observed = null;
    try { observed = await page.evaluate(() => {
      const w = window.__scenePreviewWitness;
      return { body: document.body.innerText.slice(0, 16384), devices: window.__scenePreviewDeviceStates?.() ?? [],
        witness: w ? { frames: w.frames.slice(-64), progresses: w.progresses.slice(-64), controls: w.controls.slice(-64), bindings: w.bindings.slice(-64), blocks: w.blocks, overflows: w.overflows,
          session: w.session ? { generation: w.session.generation, overlay_epoch: w.session.overlay_epoch, phase: w.session.phase, presentation_time: w.session.presentation_time } : null } : null };
    }); } catch (capture) { observed = { error: String(capture) }; }
    await fs.writeFile(path.join(directory, `${recordPrefix}-video-failure.json`), JSON.stringify({ project_id: projectId, revision: fixed?.revision, phase, error: String(error), pageerrors: errors, network, observed }, null, 2) + "\n");
    throw error;
  } finally { await page.close(); }
}
