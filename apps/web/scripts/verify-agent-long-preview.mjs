/** Read-only browser acceptance of the retained five-minute source selection. */
import { chromium, expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { installScenePreviewWitness, trackScenePreviewPumps } from "./verify-scene-preview.mjs";
const args = Object.fromEntries(process.argv.slice(2).reduce((all, value, i, values) => i % 2 ? all : [...all, [value.replace(/^--/, ""), values[i + 1]]], []));
const hash = value => createHash("sha256").update(value).digest("hex");
const candidate = JSON.parse(await fs.readFile(args.candidate, "utf8"));
expect(hash(await fs.readFile(args.binary))).toBe(candidate.binary_sha256);
const reference = JSON.parse(await fs.readFile(args.reference, "utf8"));
expect(reference.status).toBe("passed_scoped");
const nativeRuntime = JSON.parse(await fs.readFile(args["native-runtime"], "utf8"));
const legacy = JSON.parse(await fs.readFile(reference.legacy_pcm.reference_report, "utf8"));
expect(legacy.source_sha256).toBe(reference.source.sha256);
const expectedPcmBytes = await fs.readFile(legacy.pcm_path);
expect(hash(expectedPcmBytes)).toBe(legacy.pcm_sha256);
const expectedPcm = new Float32Array(expectedPcmBytes.buffer.slice(expectedPcmBytes.byteOffset, expectedPcmBytes.byteOffset + expectedPcmBytes.length));
const snapshot = () => JSON.parse(execFileSync(args.binary, ["--json", "--database", path.join(args.workspace, "workspace.sqlite"), "call", "opencut_project_inspect", "--input", "-"], {
  input: JSON.stringify({ project_id: reference.project_id, revision: null }), encoding: "utf8", timeout: 30000,
})).data;
const before = snapshot(); expect(before.revision).toBe(reference.revision);
const checkpoint = path.join(args.workspace, "agent-long-preview-checkpoint.json");
let state; try { state = JSON.parse(await fs.readFile(checkpoint, "utf8")); } catch (error) { if (error.code !== "ENOENT") throw error; state = { stages: {} }; }
const episodeIdentity = [candidate.binary_sha256, candidate.web_js_css_tree_sha256, nativeRuntime, reference.project_id, reference.revision, legacy.pcm_sha256];
if (reference.expected_preview_mode) episodeIdentity.push(reference.expected_preview_mode);
if (reference.pcm_windows) episodeIdentity.push(reference.pcm_windows, reference.minimum_observed_time ?? 4.44);
if (reference.preserve_pts_hz) episodeIdentity.push(reference.preserve_pts_hz, reference.preserve_pts_other_hz);
if (reference.pcm_threshold !== undefined) episodeIdentity.push(reference.pcm_threshold);
const episode = hash(JSON.stringify(episodeIdentity));
if (state.stages[episode]) {
  await fs.writeFile(args.report, JSON.stringify(state.stages[episode], null, 2) + "\n"); console.log("Reuse original long-source preview stage");
} else {
  const browser = await chromium.launch({ headless: true, executablePath: process.env.OPENCUT_CHROME });
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
  await page.addInitScript(installScenePreviewWitness, {rolling: true, deferReadback: true});
  const windows = reference.pcm_windows ?? [{id: "preserve-pts", start_sample: 24000, frames: 4096}, {id: reference.expected_preview_mode === "prepared-linear" ? "prepared-linear" : legacy.pitch_mode === "prepared-soundtouch" ? "prepared-concat" : "preserve-pts-retimed", start_sample: 201600, frames: 4096}];
  const witness = trackScenePreviewPumps(page, {rolling: true, pcmWindows: windows});
  const pcmMeasurements = [], capturedPcm = new Map(); let originObservation = null;
  const errors = []; page.on("pageerror", error => errors.push(error.message));
  const network = [], pending = new WeakMap();
  const streamSessions = new Set();
  page.on("request", request => {
    const pathname = new URL(request.url()).pathname;
    if (!pathname.startsWith("/api/") || network.length >= 256) return;
    const row = { path: pathname, method: request.method() }; pending.set(request, row); network.push(row);
    if (pathname === "/api/preview/audio/stream") {
      const input = request.postDataJSON(); if (typeof input?.session_id === "string") streamSessions.add(input.session_id);
    }
  });
  page.on("response", response => { const row = pending.get(response.request()); if (row) row.status = response.status(); });
  page.on("requestfailed", request => { const row = pending.get(request); if (row) row.failure = request.failure()?.errorText; });
  const observed = [], samples = [], known = new Set();
  const measurePcm = captured => {
    expect(captured.values.length).toBe(captured.frames * 2); expect(captured.values.every(Number.isFinite)).toBe(true);
    if (captured.id !== "preserve-pts") {
      const offset = (windows.find(window => window.id === captured.id)?.reference_offset ?? 0) * 2;
      expect(Number.isSafeInteger(offset) && offset >= 0 && offset + captured.values.length <= expectedPcm.length).toBe(true);
      const maximum = Math.max(...captured.values.map((value, index) => Math.abs(value - expectedPcm[offset + index])));
      const threshold = reference.pcm_threshold ?? 1e-5;
      expect(Number.isFinite(threshold) && threshold >= 0 && threshold <= 1e-5).toBe(true);
      expect(maximum).toBeLessThanOrEqual(threshold);
      pcmMeasurements.push({id: captured.id, frames: captured.frames, channels: 2, max_absolute_error: maximum, threshold});
    } else {
      const power = (channel, frequency) => {
        let real = 0, imaginary = 0;
        for (let i = 0; i < captured.frames; i++) { const phase = 2 * Math.PI * frequency * i / 48000;
          real += captured.values[2 * i + channel] * Math.cos(phase); imaginary += captured.values[2 * i + channel] * Math.sin(phase); }
        return Math.hypot(real, imaginary);
      };
      const frequencies = reference.preserve_pts_hz ?? [440, 660];
      const other = reference.preserve_pts_other_hz ?? [880, 1320];
      const ratios = frequencies.map((frequency, channel) => power(channel, frequency) / Math.max(1e-20, power(channel, other[channel])));
      expect(Math.min(...ratios)).toBeGreaterThan(8);
      pcmMeasurements.push({id: captured.id, frames: captured.frames, channels: 2, expected_hz: frequencies, frequency_dominance: ratios});
    }
  };

  try {
    await page.goto(`${args.endpoint}/editor/${encodeURIComponent(reference.project_id)}`);
    await expect(page.getByRole("main", { name: "OpenCut editor", exact: true })).toBeVisible();
    const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
    if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
    await page.getByRole("combobox", { name: "Scene", exact: true }).selectOption("long-scene");
    await page.getByLabel("Audio preview sample rate", {exact: true}).selectOption("48000");
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill("0");
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
    await page.getByRole("button", { name: "Play scene preview", exact: true }).click();
    const live = page.locator("canvas.editor-scene-frame"); await expect(live).toBeVisible({ timeout: 60000 });
    // Freeze actual 1x1 bitmap samples at publication and materialize them
    // through the shared deferred witness. Full-canvas synchronous readback
    // while polling would change the playback being measured.
    await expect.poll(async () => {
      const sound = await witness();
      if (sound.session && !originObservation) originObservation = {sample_rate: sound.session.identity.sample_rate, start_sample: sound.session.actual_start_sample};
      if (sound.overflows || sound.workerOverflows) errors.push("Preview witness overflowed");
      for (const captured of sound.selectedPcm ?? []) if (!capturedPcm.has(captured.id)) capturedPcm.set(captured.id, captured);
      for (const published of sound.frames) if (published.time >= 0 && !known.has(published.time)) {
        known.add(published.time); observed.push(published.time);
        if (!published.color) continue;
        const frame = { time: published.time, rgb: published.color.slice(0,3) };
        // Avoid the event boundary when comparing pixels with its exact clock.
        if ([1,2,2.8,3.6,4.4].every(boundary => Math.abs(frame.time - boundary) > .04)) {
          const sourceTime = frame.time < 2 ? 294 + frame.time : 296 + (frame.time - 2) * 1.25;
          const expected = reference.independent.expected_rgb[Math.floor(sourceTime) % 2];
          const error = Math.max(...frame.rgb.map((value, index) => Math.abs(value - expected[index])));
          samples.push({ ...frame, error });
        }
      }
      return samples.some(sample => sample.time < .9) && samples.some(sample => sample.time > (reference.minimum_observed_time ?? 4.44)) && observed.length >= 3 && capturedPcm.size === windows.length;
    }, { timeout: 30000, intervals: [16] }).toBe(true);
    const rawPcm = [];
    for (const captured of capturedPcm.values()) {
      const values = Float32Array.from(captured.values), filename = path.join(args.workspace, `agent-long-preview-${episode.slice(0,12)}-${captured.start_sample}.f32`);
      await fs.writeFile(filename, new Uint8Array(values.buffer)); rawPcm.push({start_sample: captured.start_sample, frames: captured.frames, path: filename, sha256: hash(new Uint8Array(values.buffer))});
    }
    await fs.writeFile(args.report.replace(/\.json$/u, "-pcm-capture.json"), JSON.stringify({candidate, project_id: reference.project_id, revision: reference.revision, native_runtime: nativeRuntime, windows: rawPcm}, null, 2) + "\n");
    expect([...capturedPcm.keys()]).toEqual(windows.map(window => window.id));
    for (const captured of capturedPcm.values()) measurePcm(captured);
    expect(originObservation).toEqual({sample_rate: 48000, start_sample: "0"});
    expect(Math.max(...samples.map(sample => sample.error)), JSON.stringify(samples)).toBeLessThanOrEqual(8);
    await page.getByRole("button", { name: "Pause scene audio", exact: true }).click();
    await expect(page.getByText("Scene sound paused", { exact: true })).toBeVisible();
    const paused = Number(await live.getAttribute("data-time"));
    await page.waitForTimeout(200); expect(Number(await live.getAttribute("data-time"))).toBe(paused);
    expect(snapshot()).toEqual(before); expect(errors).toEqual([]);
    const picture = path.join(args.workspace, `agent-long-preview-${episode.slice(0,12)}.png`); await live.screenshot({ path: picture });
    const report = { status: "passed_scoped", candidate, native_runtime: nativeRuntime, export_reference_producer_binary_sha256: reference.binary_sha256, project_id: reference.project_id,
      revision: before.revision, source_sha256: reference.source.sha256, video_source_sha256: reference.video_source?.sha256 ?? reference.source.sha256, observed_times: observed, samples, paused_time: paused,
      generation: await live.getAttribute("data-generation"), runtime: await live.getAttribute("data-runtime"), page_errors: errors, network,
      pcm_windows: pcmMeasurements, legacy_pcm_reference_sha256: legacy.pcm_sha256, expected_preview_mode: reference.expected_preview_mode ?? legacy.pitch_mode, origin_observed: originObservation,
      screenshot: picture, scope: `${reference.scope ?? "Retained late300s VFR source with source-mode-specific PCM"}. Actual continuous preview from0 to recorded paused_time in the5.2s scene, source-derived pixels and pause; pre-master PCM, no post-master/DAC, sustained FPS or full browser matrix proof` };
    state.stages[episode] = report;
    await fs.writeFile(checkpoint + ".pending", JSON.stringify(state, null, 2) + "\n"); await fs.rename(checkpoint + ".pending", checkpoint);
    await fs.writeFile(args.report, JSON.stringify(report, null, 2) + "\n"); console.log(JSON.stringify({ status: report.status, observed_frames: observed.length, paused_time: paused }));
  } catch (error) {
    const audio_sessions = [];
    for (const session_id of streamSessions) {
      const response = await page.request.post(`${args.endpoint}/api/tools/opencut_audio_preview_inspect`, {data: {session_id}, headers: {Origin: args.endpoint}});
      const value = await response.json();
      audio_sessions.push({session_id, http_status: response.status(), state: value.data?.state,
        failure_code: value.data?.failure_code, error_code: value.error?.code});
    }
    const picture = path.join(args.workspace, "agent-long-preview-diagnostic.png");
    await page.screenshot({ path: picture });
    await fs.writeFile(args.report.replace(/\.json$/u, "-diagnostic.json"), JSON.stringify({status: "failed_scoped", candidate,
      observed_times: observed, samples, pcm_measurements: pcmMeasurements, origin_observed: originObservation,
      page_errors: errors, network, audio_sessions, error: String(error), screenshot: picture,
      page_text: (await page.locator("body").innerText()).slice(-12000)}, null, 2) + "\n");
    throw error;
  } finally { await browser.close(); }
}
