import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { installScenePreviewWitness, trackScenePreviewPumps } from "./verify-scene-preview.mjs";
import { densePcmWindows, measurePcmWindow, observePcmOrigin } from "./scene-preview-pcm.mjs";

// One necessary later-block coverage run; prior performance/lifecycle suites
// keep their own execution identities. This does not certify post-master/DAC.
export async function verifyScenePreviewPcm(browser, endpoint, directory, check, {terminalOnly = false} = {}) {
  const fixture = JSON.parse(await fs.readFile(path.join(directory, terminalOnly ? "scene-preview-fixture.json" : "scene-preview-dense-fixture.json"), "utf8"));
  const page = await browser.newPage({locale: "en-US", viewport: {width: 1440, height: 1100}});
  const windows = terminalOnly ? [] : densePcmWindows();
  const expectedFrames = terminalOnly ? "384000" : "14400000";
  const measurements = [], writes = [], errors = [];
  let originalStartObserved = false, completed = false;
  await page.addInitScript(installScenePreviewWitness, {rolling: true, deferReadback: true});
  const witness = trackScenePreviewPumps(page, {rolling: true, pcmWindows: windows});
  page.on("pageerror", e => errors.push(e.message));
  page.on("request", r => { if (r.url().endsWith("opencut_edit_apply")) writes.push(r.postDataJSON()); });
  const inspect = async () => {
    const reply = await page.evaluate(async project_id => (await fetch("/api/tools/opencut_project_inspect", {
      method: "POST", headers: {"Content-Type": "application/json"}, body: JSON.stringify({project_id, revision: null}),
    })).json(), fixture.project_id);
    expect(reply.error).toBeNull(); return reply.data;
  };
  try {
    await page.goto(`${endpoint}/editor`);
    await expect(page.getByRole("button", {name: "New project", exact: true})).toBeEnabled();
    const fixed = await inspect();
    if (fixture.revision) expect(fixed.revision).toBe(fixture.revision);
    const sequence = fixed.project.sequences.find(s => s.id === "main");
    if (!terminalOnly) {
    expect(sequence.tracks).toHaveLength(5);
    for (const [lane, track] of sequence.tracks.entries()) {
      expect(track.kind).toBe(lane < 2 ? "video" : "audio"); expect(track.elements).toHaveLength(20);
      for (const [k, element] of track.elements.entries()) {
        expect(element.timeline_range).toEqual({start: {ticks: String(292 * k), timebase: 19}, duration: {ticks: "8", timebase: 1}});
        expect(element.content.retime.rate).toEqual({numerator: 1, denominator: 1});
        expect(element.content.retime.maintain_pitch).toBe(false);
        expect(element.content.audio.gain).toEqual({unit: "decibels", value: -18});
        if (lane < 2) expect(element.content.source_audio_enabled).toBe(false);
      }
    }
    } else {
      for (const track of sequence.tracks) for (const e of track.elements) {
        expect(Number(e.timeline_range.start.ticks)).toBe(0);
        expect(Number(e.timeline_range.duration.ticks) / e.timeline_range.duration.timebase).toBe(8);
      }
    }
    await page.goto(`${endpoint}/editor/${fixture.project_id}`);
    await expect(page.locator(".editor-timeline-element")).toHaveCount(terminalOnly ? sequence.tracks.reduce((n,t) => n + t.elements.length,0) : 100);
    const guide = page.getByRole("dialog", {name: "Welcome to OpenCut", exact: true});
    if (await guide.isVisible()) await guide.getByRole("button", {name: "Close", exact: true}).click();
    await page.getByRole("button", {name: "Play scene preview", exact: true}).click();
    const deadline = Date.now() + (terminalOnly ? 30000 : 340000);
    let last;
    do {
      if (Date.now() > deadline) throw Error("LaterPCM coverage deadline exceeded");
      last = await witness();
      expect(last.overflows).toBe(0); expect(last.workerOverflows ?? 0).toBe(0);
      if (last.session) originalStartObserved = observePcmOrigin(originalStartObserved, last.session);
      for (const captured of last.selectedPcm ?? []) {
        expect(captured.id).toBe(windows[measurements.length]?.id);
        expect({...captured, values: undefined}).toEqual({...windows[measurements.length], values: undefined});
        measurements.push(measurePcmWindow(captured));
        if (measurements.length % 10 === 0) console.log(`LaterPCM observed${measurements.length}/39 windows`);
      }
      if (!last.endedSession) await page.waitForTimeout(1000);
    } while (!last.endedSession);
    expect(measurements).toHaveLength(windows.length);
    expect(originalStartObserved).toBe(true);
    expect({...last.pcmCapture, transferred_frames: undefined}).toEqual({expected: windows.length, completed: windows.length, partial_frames: 0, transferred_frames: undefined});
    // The Worker can close between polls after its final real receipt. Require
    // observed transfer coverage through the last chosen window; terminal copied
    // frames remain the actual device/session observation, not a fabricated sum.
    expect(BigInt(last.pcmCapture.transferred_frames) >= BigInt(windows.length ? windows.at(-1).start_sample + windows.at(-1).frames : 0)).toBe(true);
    expect(BigInt(last.pcmCapture.transferred_frames) <= BigInt(expectedFrames)).toBe(true);
    expect(last.endedSession.consumed_frames).toBe(expectedFrames);
    await expect.poll(async () => (await witness()).deviceStates).toEqual(["closed"]);
    expect((await inspect()).revision).toBe(fixed.revision); expect(writes).toEqual([]); expect(errors).toEqual([]);
    completed = true;
    check(terminalOnly ? "short original scene: stream origin survives ended-to-Stop cursor transition, actual terminal consumption and closed device" : "actual later stereoPCM:20 authored source interiors and19 exact-silent gaps, original revision and terminal consumed cursor", {
      [terminalOnly ? "scene_preview_pcm_terminal" : "scene_preview_pcm"]: {project_id: fixture.project_id, revision: fixed.revision, identity: last.session.identity,
        windows: measurements, observed_frames: windows.reduce((n,w) => n + w.frames,0), channels: 2, transferred_frames: last.pcmCapture.transferred_frames, terminal_consumed_frames: last.endedSession.consumed_frames, final_control_state: last.session.state, final_control_start_sample: last.session.actual_start_sample,
        expected: {frequency_hz: 440, rms: .125 / Math.sqrt(2) * 3 * 10 ** (-18 / 20), rms_tolerance: .0005, gap_peak: 0, stereo_tolerance: 1e-7},
        capture_bounds: {partial_frames: 4096, queued_windows: 8, declared_windows: windows.length, overflow: 0},
        readonly: true, scope: terminalOnly ? "Short real scene origin/terminal cursor/device closure only, no acoustic or sustained performance proof." : "RealWorker transferred pre-masterFloat32, interiors only; identical mono voices do not establish arbitrary stereo/edge/retime/physicalDAC or current performance gates."},
    });
  } finally {
    // Keep bounded measurements even if an unrelated terminal assertion fails;
    // never discard a valid acoustic span and force another unchanged playback.
    try { await fs.writeFile(path.join(directory, terminalOnly ? "scene-preview-pcm-terminal-evidence.json" : "scene-preview-pcm-evidence.json"), JSON.stringify({completed, originalStartObserved, windows: measurements, errors}, null, 2)); }
    finally { await page.close(); }
  }
}
