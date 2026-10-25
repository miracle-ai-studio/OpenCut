import { editorSnapshot } from "./project-inspection.mjs";
/** One current real drag/escape PCM witness; reuse the original eight-second scene. */
import { chromium, expect } from '@playwright/test';
import fs from 'node:fs/promises';
import path from 'node:path';
import { OpenCutClient } from '../../../packages/sdk-ts/dist/index.js';
import { installScenePreviewWitness, trackScenePreviewPumps } from './verify-scene-preview.mjs';

const args = Object.fromEntries(process.argv.slice(2).reduce((a, v, i, s) => i % 2 ? a : [...a, [v.slice(2), s[i + 1]]], []));
const { workspace, endpoint, report } = args;
if (await fs.readFile(report, 'utf8').then(JSON.parse).then(r => r.status === 'passed_scoped_current_gain_drag_pcm').catch(e => { if (e.code !== 'ENOENT') throw e; return false; })) { console.log('Reuse completed current gain-preview evidence'); process.exit(0); }
const fixture = JSON.parse(await fs.readFile(path.join(workspace, 'scene-preview-fixture.json'), 'utf8'));
const client = new OpenCutClient({ baseUrl: endpoint, token: (await fs.readFile(args.credential, 'utf8')).trim() });
const call = (tool, input) => client.call(tool, input), inspect = () => editorSnapshot(call('opencut_project_inspect', { project_id: fixture.project_id, revision: null }));
const before = await inspect(), audio = before.project.sequences[0].tracks.flatMap(t => t.elements).find(e => e.content.kind === 'audio');
expect(before.revision).toBe(fixture.revision); expect(audio.content.audio.gain).toEqual({ unit: 'decibels', value: 0 });
const browser = await chromium.launch({ headless: true, executablePath: process.env.OPENCUT_CHROME });
const page = await browser.newPage({ locale: 'en-US', viewport: { width: 1440, height: 1300 } });
await page.addInitScript(installScenePreviewWitness, { deferReadback: true });
await page.addInitScript(() => {
  globalThis.__f44Starts = [];
  const Original = globalThis.Worker;
  globalThis.Worker = class extends Original {
    constructor(...args) {
      super(...args); if (args[1]?.name !== 'opencut-audio-pump') return;
      const post = this.postMessage.bind(this);
      this.postMessage = (data, ...rest) => {
        if (data.channel === 'opencut-audio-pump' && data.kind === 'start') {
          if (globalThis.__f44Starts.length >= 16) throw Error('Gain witness exceeded its start bound');
          const s = data.session; globalThis.__f44Starts.push({ token: data.token, session_id: s.session_id, generation: s.generation, overlay_epoch: s.overlay_epoch });
        }
        return post(data, ...rest);
      };
    }
  };
});
const witness = trackScenePreviewPumps(page, { pcmWindows: [{ id: 'gain-interior', start_sample: 12000, frames: 4096 }] });
const writes = [], controls = [], errors = [];
page.on('pageerror', e => errors.push(e.message));
page.on('request', r => { if (/\/opencut_(edit_apply|project_audio_gain_plan)$/.test(r.url())) writes.push(r.postDataJSON()); if (r.url().endsWith('/opencut_audio_preview_control')) controls.push(r.postDataJSON()); });
const measurements = [];
let lastObservation;
const measure = async tenths => {
  let observed;
  await expect.poll(async () => {
    observed = await witness(); const s = observed.session;
    const starts = await page.evaluate(() => globalThis.__f44Starts), start = starts.findLast(v => v.token === observed.progress?.token);
    lastObservation = { session: s && { session_id: s.session_id, generation: s.generation, overlay_epoch: s.overlay_epoch, state: s.state, temporary_gain: s.temporary_gain }, progress: observed.progress, actual_start_binding: start, selected_windows: observed.selectedPcm?.map(w => ({ id: w.id, frames: w.frames, values: w.values.length })), pcmCapture: observed.pcmCapture, pumpWitness: observed.pumpWitness, blocks: observed.blocks, firstSoundAt: observed.firstSoundAt };
    return !!s && (s.temporary_gain?.decibels_tenths ?? null) === tenths && start?.session_id === s.session_id && start.generation === s.generation && start.overlay_epoch === s.overlay_epoch && BigInt(observed.progress.consumedFrames) > 0n && observed.selectedPcm?.length === 1;
  }, { timeout: 15000 }).toBe(true);
  const window = observed.selectedPcm[0]; expect(window.frames).toBe(4096); expect(window.values.every(Number.isFinite)).toBe(true);
  const db = (tenths ?? 0) / 10, channels = [0, 1].map(c => window.values.filter((_, i) => i % 2 === c));
  const acoustic = channels.map(values => {
    const crossings = []; for (let i = 1; i < values.length; i++) if (values[i - 1] <= 0 && values[i] > 0) crossings.push(i - 1 - values[i - 1] / (values[i] - values[i - 1]));
    const frequency = (crossings.length - 1) * 48000 / (crossings.at(-1) - crossings[0]);
    const rms = Math.sqrt(values.reduce((sum, v) => sum + v * v, 0) / values.length), expected = .125 / Math.sqrt(2) * 10 ** (db / 20);
    expect(Math.abs(frequency - 440)).toBeLessThan(1); expect(Math.abs(rms - expected)).toBeLessThan(.0005);
    return { frequency_hz: frequency, rms, expected_rms: expected };
  });
  expect(Math.max(...channels[0].map((v, i) => Math.abs(v - channels[1][i])))).toBeLessThan(1e-7);
  const s = observed.session; measurements.push({ session_id: s.session_id, generation: s.generation, overlay_epoch: s.overlay_epoch, actual_worker_start: lastObservation.actual_start_binding, requested_tenths: tenths, sample_start: 12000, frames: 4096, channels: acoustic });
  return s;
};
let original, paused;
try {
  await page.goto(`${endpoint}/editor/${fixture.project_id}`); await expect(page.locator('.save-status')).toHaveText('Saved');
  const welcome = page.getByRole('dialog', { name: 'Welcome to OpenCut', exact: true }); if (await welcome.isVisible()) await welcome.getByRole('button', { name: 'Close', exact: true }).click();
  const slider = page.locator(`[data-element-id="${audio.id}"] .editor-volume-line`), surface = page.locator(`[data-element-id="${audio.id}"] .editor-gain-surface`);
  await page.getByRole('button', { name: 'Play scene preview', exact: true }).click(); original = await measure(null);
  await slider.scrollIntoViewIfNeeded(); const r = await slider.boundingBox(), p = { x: r.x + r.width / 2, y: r.y + r.height / 2 };
  expect(await slider.evaluate((e, p) => e.contains(document.elementFromPoint(p.x, p.y)), p)).toBe(true);
  await page.mouse.move(p.x, p.y); await page.mouse.down(); const box = await surface.boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height * (1 - Math.sqrt((10 ** (-12 / 20) - .001) / 9.999)));
  await expect(slider).toHaveAttribute('aria-valuenow', '-12'); await measure(-120);
  expect(await inspect()).toEqual(before); expect(writes).toEqual([]);
  await page.keyboard.press('Escape'); await page.mouse.up(); await measure(null); await expect(slider).toHaveAttribute('aria-valuenow', '0');
  await page.getByRole('button', { name: 'Pause scene audio', exact: true }).click(); await expect(page.getByText('Scene sound paused', { exact: true })).toBeVisible();
  paused = await call('opencut_audio_preview_inspect', { session_id: original.session_id }); expect(paused.state).toBe('paused'); expect(paused.temporary_gain).toBeNull();
  expect(paused.owner_id).toBe(original.owner_id); expect(paused.lease_id).toBe(original.lease_id); expect(await inspect()).toEqual(before); expect(writes).toEqual([]);
  await page.goto(`${endpoint}/editor`);
  let stopped; await expect.poll(async () => { stopped = await call('opencut_audio_preview_inspect', { session_id: original.session_id }); return stopped.state; }, { timeout: 10000 }).toBe('stopped');
  expect(errors).toEqual([]); expect(controls.some(c => c.command.kind === 'set_temporary_gain')).toBe(true); expect(controls.some(c => c.command.kind === 'clear_temporary_gain')).toBe(true);
  await fs.writeFile(report, JSON.stringify({ status: 'passed_scoped_current_gain_drag_pcm', candidate: args.candidate, project_id: fixture.project_id, revision: before.revision, measurements, controls: controls.map(c => c.command.kind), paused: { state: paused.state, temporary_gain: paused.temporary_gain, owner_lease_retained: true }, stopped: { state: stopped.state, session_id: stopped.session_id }, project_writes: 0, project_and_revision_unchanged: true, new_jobs: 0, new_sources: 0, page_errors: errors, scope: 'Actual pointer draft and Escape use original8s48000Hz mono440/.125 source;4096-frame actual Worklet transfer interiors at0/-12/0dB match independent frequency/RMS and stereo duplication. Current pre-master gain behavior,original owner/lease,generation fences and teardown only; no post-master/DAC,whole S1/hot residual,performance or broad A7/D8 claim.' }, null, 2) + '\n');
  console.log(JSON.stringify({ status: 'passed_scoped_current_gain_drag_pcm', project_writes: 0, new_jobs: 0 }));
} catch (error) { await fs.writeFile(path.join(workspace, 'f44-current-preview-failure.json'), JSON.stringify({ message: String(error), stack: error.stack, measurements, lastObservation, control_kinds: controls.map(c => c.command.kind), session_ids: [...new Set(controls.map(c => c.session_id))], page_errors: errors }, null, 2) + '\n'); await page.screenshot({ path: path.join(workspace, 'f44-current-preview-failure.png'), fullPage: true }); throw error; }
finally { await page.mouse.up().catch(() => {}); if (!page.isClosed()) await page.goto(`${endpoint}/editor`).catch(() => {}); await browser.close(); }
