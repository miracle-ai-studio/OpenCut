import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import os from "node:os";
import { installScenePreviewWitness, trackScenePreviewPumps } from "./verify-v4-scene-preview.mjs";
import { summarizePreviewConsumption } from "./scene-preview-consumption.mjs";

const time = (n, base = 1) => ({ ticks: String(n), timebase: base });
// Freeze these gates before measurement. Eight seconds of actual 24fps video
// followed by its authored gap; at least 75% of the 192 source frames must be
// presented, rather than counting repeated requested clocks as useful pictures.
const budget = Object.freeze({ cold_ms: 10000, steady_ms: 10000,
  skew_ms: 150, distinct_video_frames: 144, deficit_ratio: .01,
  heap_delta_bytes: 64 * 1024 * 1024, gpu_bytes: 256 * 1024 * 1024 });

export async function verifyV4ScenePreviewPerformance(browser, endpoint, directory, check) {
  const fixture = JSON.parse(await fs.readFile(path.join(directory, "scene-preview-fixture.json"), "utf8"));
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
  page.setDefaultTimeout(30000);
  await page.addInitScript(installScenePreviewWitness, { deferReadback: true });
  const errors = [], writes = [];
  page.on("pageerror", e => errors.push(e.message));
  page.on("request", r => { if (r.url().endsWith("opencut_edit_v4_apply")) writes.push(r.postDataJSON()); });
  const tool = async (name, input) => {
    const r = await page.evaluate(async ({ name, input }) => (await fetch(`/v1/tools/${name}`, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input),
    })).json(), { name, input });
    expect(r.error).toBeNull(); return r.data;
  };
  const witness = trackScenePreviewPumps(page);
  let evidence;
  try {
    await page.goto(`${endpoint}/editor/${fixture.project_id}`);
    await expect(page.getByRole("button", { name: "Play scene preview", exact: true })).toBeEnabled();
    const fixed = await tool("opencut_project_v4_inspect", { project_id: fixture.project_id, revision: null });
    const original = fixed.project.sequences.find(s => s.id === "main");
    const templates = original.tracks.flatMap(t => t.elements);
    const video = templates.find(e => e.content.kind === "video"), audio = templates.find(e => e.content.kind === "audio");
    expect(video && audio).toBeTruthy();
    const denseMeta = path.join(directory, "scene-preview-dense-fixture.json");
    const known = await fs.readFile(denseMeta, "utf8").then(JSON.parse).catch(e => { if (e.code === "ENOENT") return null; throw e; });
    const projectId = known?.project_id ?? `scene-preview-dense-${crypto.randomUUID()}`;
    const copy = known ? await tool("opencut_project_v4_inspect", { project_id: projectId, revision: null })
      : await tool("opencut_project_v4_copy", { source_project_id: fixed.project.id,
        source_revision: fixed.revision, project_id: projectId,
        name: "720p continuous scene · 100 clips / 5 tracks / 300 seconds", idempotency_key: crypto.randomUUID() });
    await fs.writeFile(denseMeta, JSON.stringify({ project_id: projectId }));
    const dense = structuredClone(original);
    dense.tracks = Array.from({ length: 5 }, (_, lane) => {
      const kind = lane < 2 ? "video" : "audio", source = kind === "video" ? video : audio;
      const track = original.tracks.find(t => t.kind === kind);
      return { ...structuredClone(track), id: crypto.randomUUID(), name: `Continuous dense ${lane + 1}`,
        enabled: true, muted: false, locked: false, channels: [],
        elements: Array.from({ length: 20 }, (_, index) => {
          const e = structuredClone(source); e.id = crypto.randomUUID(); e.name = `Dense ${lane + 1}:${index + 1}`;
          e.enabled = true; e.channels = [];
          e.timeline_range = { start: time(292 * index, 19), duration: time(8) };
          e.content.retime = { ...e.content.retime, rate: { numerator: 1, denominator: 1 }, maintain_pitch: false };
          e.content.audio.gain = { unit: "decibels", value: -18 }; e.content.audio.muted = false;
          if (kind === "video") e.content.source_audio_enabled = false;
          return e;
        }) };
    });
    dense.main_track_id = dense.tracks[0].id;
    const seeded = copy.project.sequences.find(s => s.id === original.id);
    const ready = seeded?.tracks.flatMap(t => t.elements).length === 100;
    if (ready) {
      expect(seeded.tracks).toHaveLength(5);
      for (const [lane, track] of seeded.tracks.entries()) {
        expect(track.kind).toBe(lane < 2 ? "video" : "audio");
        expect(track.elements).toHaveLength(20);
        for (const [index, element] of track.elements.entries()) {
          expect(element.timeline_range).toEqual({ start: time(292 * index, 19), duration: time(8) });
          expect(element.content.retime.rate).toEqual({ numerator: 1, denominator: 1 });
          expect(element.content.retime.maintain_pitch).toBe(false);
          expect(element.content.audio.gain).toEqual({ unit: "decibels", value: -18 });
        }
      }
    }
    const receipt = ready ? { revision: copy.revision } : await tool("opencut_edit_v4_apply", { project_id: projectId, expected_revision: copy.revision,
      idempotency_key: crypto.randomUUID(), label: "Continuous dense scene from verified imported media",
      operations: [{ type: "sequence_put", sequence: dense }] });
    const cdp = await page.context().newCDPSession(page); await cdp.send("Performance.enable");
    const metrics = async () => Object.fromEntries((await cdp.send("Performance.getMetrics")).metrics.map(m => [m.name, m.value]));
    await page.goto(`${endpoint}/editor`);
    await expect(page.getByRole("button", { name: "New project", exact: true })).toBeEnabled();
    await cdp.send("HeapProfiler.collectGarbage");
    const before = await metrics();
    await page.goto(`${endpoint}/editor/${projectId}`);
    await expect(page.locator(".v4-timeline-element")).toHaveCount(100);
    await expect(page.locator(".v4-track-name")).toHaveCount(5);
    const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
    if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
    const beforeWrites = writes.length;
    const began = Date.now();
    await page.getByRole("button", { name: "Play scene preview", exact: true }).click();
    const live = page.getByRole("img", { name: "Live scene preview", exact: true });
    await expect(live).toBeVisible();
    await expect.poll(async () => (await witness()).firstSoundAt, { timeout: 20000 }).not.toBeNull();
    const warm = await metrics(), heaps = [warm.JSHeapUsedSize];
    const start = (await witness()).progresses.find(p => Number(p.frames) > 0);
    const deadline = Date.now() + 25000;
    while ((await witness()).progresses.at(-1).at - start.at < budget.steady_ms) {
      if (Date.now() > deadline) throw new Error("Ten-second real Worklet window did not complete");
      const current = await metrics(); heaps.push(current.JSHeapUsedSize);
      await page.waitForTimeout(250);
    }
    const steady = await witness(), cpu = await metrics();
    const reports = steady.progresses.filter(p => p.at >= start.at && p.at <= start.at + budget.steady_ms);
    const duration = reports.at(-1).at - reports[0].at;
    const frames = Number(BigInt(reports.at(-1).frames) - BigInt(reports[0].frames));
    let stalledMs = 0;
    for (let i = 1; i < reports.length; i++) if (reports[i - 1].stalled) stalledMs += reports[i].at - reports[i - 1].at;
    const moving = steady.frames.filter(f => f.time > 0 && f.time < 8);
    const distinct = new Set(moving.flatMap(f => f.pts)).size;
    const skews = steady.frames.filter(f => f.time > 0 && f.time < 10).map(f => (f.actual - f.time) * 1000);
    const runtimes = steady.frames.map(f => JSON.parse(f.runtime));
    const samples = steady.pcm;
    const crossings = [];
    for (let i = 1; i < samples.length; i++) if (samples[i - 1] <= 0 && samples[i] > 0)
      crossings.push(i - 1 - samples[i - 1] / (samples[i] - samples[i - 1]));
    const hz = (crossings.length - 1) * 48000 / (crossings.at(-1) - crossings[0]);
    const rms = Math.sqrt(samples.reduce((sum, n) => sum + n * n, 0) / samples.length);
    evidence = { budget, machine: { platform: os.platform(), arch: os.arch(), cpu: os.cpus()[0]?.model },
      fixture: { project_id: projectId, revision: receipt.revision, width: 1280, height: 720,
        placements: 100, tracks: 5, seconds: 300, source_seconds: 8, unique_assets: 2,
        starts: "292*k/19; k=0..19", source_audio_disabled: true, audio_voices: 3, gain_db: -18, rate: "1/1", maintain_pitch: false },
      cold_ms: steady.firstSoundAt - began, distinct_presented_source_pts: distinct,
      max_skew_ms: Math.max(...skews), minimum_skew_ms: Math.min(...skews),
      consumption: { elapsed_ms: duration, frames, reports: reports.length,
        stalled_ratio: stalledMs / duration, deficit_ratio: Math.max(0, 1 - frames / 48000 / (duration / 1000)),
        stalled_reports_by_second: Object.fromEntries(Array.from({length:10},(_,second)=>[second, reports.filter(p=>p.stalled && Math.floor((p.at-start.at)/1000)===second).length])) },
      independent_pcm: { frequency_hz: hz, rms, expected_rms: .125 / Math.sqrt(2) * 3 * 10 ** (-18 / 20) },
      main_thread: { task_ms: (cpu.TaskDuration - warm.TaskDuration) * 1000, script_ms: (cpu.ScriptDuration - warm.ScriptDuration) * 1000,
        method: "CDP page main-thread time during the real steady window; excludes driver/native CPU" },
      heap: { baseline_bytes: before.JSHeapUsedSize, peak_bytes: Math.max(...heaps),
        delta_bytes: Math.max(...heaps) - before.JSHeapUsedSize, method: "CDP page V8 heap, one baseline GC; no steady-window GC" },
      runtime: runtimes.at(-1), errors };
    evidence.execution = Object.fromEntries(["planMs", "bindMs", "totalMs"].map(key => [key,
      { mean: runtimes.reduce((n,r)=>n+Number(r[key] ?? 0),0)/runtimes.length, maximum: Math.max(...runtimes.map(r=>Number(r[key] ?? 0))) }]));
    const timings = values => values.length ? { count: values.length, mean_ms: values.reduce((n,v)=>n+v,0)/values.length,
      maximum_ms: Math.max(...values), p95_ms: [...values].sort((a,b)=>a-b)[Math.ceil(values.length*.95)-1] } : { count: 0 };
    const controls = steady.controls.filter(c => c.began >= start.at && c.ended <= start.at + budget.steady_ms);
    const supplies = steady.supplies.filter(s => s.at >= start.at && s.at <= start.at + budget.steady_ms);
    evidence.transport = {
      controls: Object.fromEntries(["report_consumed", "heartbeat"].map(kind => [kind, timings(controls.filter(c=>c.kind===kind).map(c=>c.ended-c.began))])),
      pcm_arrival_gaps: timings(supplies.slice(1).map((s,i)=>s.at-supplies[i].at)),
      block_sizes: [...new Set(supplies.map(s=>s.frames))],
      method: "Page-observed authenticated control RTT and bounded PCM transfer arrival gaps; no native/device latency inference",
    };
    evidence.device_observations = summarizePreviewConsumption(reports, controls, steady.supplies, 48000);
    evidence.resources = {
      peak_gpu_bytes: Math.max(...runtimes.map(r => Number(r.peakGpuBytes))),
      peak_accounted_bytes: Math.max(...runtimes.map(r => Number(r.peakAccountedBytes))),
      retained_decoded_bytes: Math.max(...runtimes.map(r => Number(r.retainedDecodedBytes))),
      retained_encoded_bytes: Math.max(...runtimes.map(r => Number(r.retainedEncodedBytes))),
      method: "Classic execution allocation accounting; excludes driver/native RSS and process GPU overhead",
    };
    await page.getByRole("button", { name: "Pause scene audio", exact: true }).click();
    await expect(page.getByText("Scene sound paused", { exact: true })).toBeVisible();
    await page.evaluate(() => window.__readScenePreviewSamples());
    const sampled = (await witness()).frames.filter(f => f.time >= 0 && f.time < 8 && f.color !== null);
    evidence.independent_pixels = sampled.map(f => ({ time: f.time, color: f.color }));
    const gates = {};
    const gate = (name, assertion) => { try { assertion(); gates[name] = true; } catch { gates[name] = false; } };
    gate("independent_pixels", () => {
      expect(new Set(sampled.map(f => Math.floor(f.time))).size).toBe(8);
      for (const f of sampled) {
        expect([f.width, f.height]).toEqual([1280, 720]);
        const expected = Math.floor(f.time) % 2 ? [0, 255, 0, 255] : [255, 0, 0, 255];
        f.color.forEach((n,c) => expect(Math.abs(n - expected[c])).toBeLessThanOrEqual(4));
      }
    });
    gate("extent", () => expect(steady.session.total_frames).toBe("14400000"));
    gate("cold", () => expect(evidence.cold_ms).toBeLessThanOrEqual(budget.cold_ms));
    gate("stalls", () => expect(evidence.consumption.stalled_ratio).toBeLessThanOrEqual(budget.deficit_ratio));
    gate("device_quantum_supply", () => {
      expect(evidence.device_observations.status).toBe("complete_observed_span");
      expect(evidence.device_observations.render_quantum_deficit_ratio).toBeLessThanOrEqual(budget.deficit_ratio);
    });
    gate("deficit", () => expect(evidence.consumption.deficit_ratio).toBeLessThanOrEqual(budget.deficit_ratio));
    gate("useful_video_pts", () => expect(distinct).toBeGreaterThanOrEqual(budget.distinct_video_frames));
    gate("synchronization", () => {
      expect(evidence.max_skew_ms).toBeLessThanOrEqual(budget.skew_ms);
      expect(evidence.minimum_skew_ms).toBeGreaterThanOrEqual(-10);
    });
    gate("heap", () => expect(evidence.heap.delta_bytes).toBeLessThanOrEqual(budget.heap_delta_bytes));
    gate("gpu", () => {
      expect(evidence.resources.peak_gpu_bytes).toBeGreaterThan(0);
      expect(evidence.resources.peak_gpu_bytes).toBeLessThanOrEqual(budget.gpu_bytes);
    });
    gate("independent_pcm", () => {
      expect(Math.abs(hz - 440)).toBeLessThan(1);
      expect(Math.abs(rms - evidence.independent_pcm.expected_rms)).toBeLessThan(.001);
    });
    const head = await tool("opencut_project_v4_inspect", { project_id: projectId, revision: null });
    gate("readonly", () => { expect(writes).toHaveLength(beforeWrites); expect(head.revision).toBe(receipt.revision); });
    gate("page_errors", () => expect(errors).toEqual([]));
    evidence.gates = gates;
    const failed = Object.entries(gates).filter(([,ok])=>!ok).map(([name])=>name);
    if (failed.length) throw new Error(`Dense scene gates unmet: ${failed.join(", ")}`);
    check("720p dense continuous scene: actual 10s consumption, independently authored pixels/PCM, useful decoded PTS cadence and scoped CPU/heap", { scene_preview_performance: evidence });
  } catch (e) {
    const w = await witness().catch(() => null);
    const diagnosis = { error: String(e), alerts: await page.getByRole("alert").allTextContents(),
      clock: w?.progress, frame_count: w?.frames.length, last_frame: w?.frames.at(-1),
      last_consumer: w?.progresses.at(-1),
      session: w?.session && { state: w.session.state, failure_code: w.session.failure_code, total_frames: w.session.total_frames,
        consumed_frames: w.session.consumed_frames, authoritative_time: w.session.authoritative_time } };
    await fs.writeFile(path.join(directory, "scene-preview-performance-failure.json"), JSON.stringify({ evidence, diagnosis }, null, 2));
    throw e;
  } finally { await page.close(); }
}
