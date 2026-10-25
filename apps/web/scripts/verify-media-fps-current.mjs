import { editorSnapshot } from "./project-inspection.mjs";
/** Remaining F08: actual unknown/containers, existing VFR binding and one six-frame output. */
import { chromium, expect } from '@playwright/test';
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { OpenCutClient } from '../../../packages/sdk-ts/dist/index.js';
import { importProductMedia, applyMediaReview } from './verify-editor.mjs';

const args = Object.fromEntries(process.argv.slice(2).reduce((a, v, i, s) => i % 2 ? a : [...a, [v.slice(2), s[i + 1]]], []));
const project = 'agent-long-varied-gain-copy', file = path.join(args.workspace, 'agent-media-fps-current-v4-checkpoint.json');
const state = await fs.readFile(file, 'utf8').then(JSON.parse).catch(e => { if (e.code !== 'ENOENT') throw e; return { stages: {}, requests: {}, plans: {} }; });
const client = new OpenCutClient({ baseUrl: args.endpoint, token: (await fs.readFile(args.credential, 'utf8')).trim() });
const call = (tool, input) => client.call(tool, input), inspect = () => editorSnapshot(call('opencut_project_inspect', { project_id: project, revision: null }));
const sha = b => createHash('sha256').update(b).digest('hex');
const save = async () => { await fs.writeFile(file + '.pending', JSON.stringify(state, null, 2) + '\n'); await fs.rename(file + '.pending', file); };
const stage = async (name, run) => { if (state.stages[name]) return state.stages[name]; console.log('Begin ' + name); const value = await run(); state.stages[name] = value; state.stage_candidates ??= {}; state.stage_candidates[name] = args.candidate; await save(); console.log('Passed ' + name); return value; };
const original = await stage('original-peer', inspect), sequence = original.project.sequences[0].id;
const node = (s, id) => s.project.sequences[0].tracks.flatMap(t => t.elements).find(e => e.id === id);
const browser = await chromium.launch({ headless: true, executablePath: process.env.OPENCUT_CHROME });
const page = await browser.newPage({ locale: 'en-US', viewport: { width: 1440, height: 1300 } });
page.setDefaultTimeout(30000);
const errors = [], writes = [];
page.on('pageerror', e => errors.push(e.message));
page.on('request', r => { if (r.url().endsWith('/opencut_edit_apply')) writes.push(r.postDataJSON()); });
const ready = () => expect(page.locator('.save-status')).toHaveText('Saved', { timeout: 30000 });
const open = async () => { await page.goto(`${args.endpoint}/editor/${project}`); await ready(); const d = page.getByRole('dialog', { name: 'Welcome to OpenCut', exact: true }); if (await d.isVisible()) await d.getByRole('button', { name: 'Close', exact: true }).click(); };
const ui = (name, planTool, run, applyTool = 'opencut_edit_apply') => stage(name, async () => {
  if (state.requests[name]) { const receipt = await call(applyTool, state.requests[name]); expect((await inspect()).revision).toBe(receipt.transaction?.revision ?? receipt.revision); await open(); return inspect(); }
  if (state.plans[name]) { const p = state.plans[name]; p.reply ??= await call(planTool, p.input); state.requests[name] = p.reply.transaction; await save(); await call(applyTool, p.reply.transaction); await open(); return inspect(); }
  let receipt, count = 0;
  if (planTool) await page.route('**/api/tools/' + planTool, async route => { const p = state.plans[name] = { input: route.request().postDataJSON() }; await save(); const response = await route.fetch(), body = await response.json(); expect(body.error).toBeNull(); p.reply = body.data; await save(); await route.fulfill({ response }); });
  await page.route('**/api/tools/' + applyTool, async route => { count++; state.requests[name] = route.request().postDataJSON(); await save(); const response = await route.fetch(), body = await response.json(); expect(body.error).toBeNull(); receipt = body.data; await route.fulfill({ response }); });
  try { await run(); await expect.poll(() => !!receipt, { timeout: 30000 }).toBe(true); await ready(); expect(count).toBe(1); if (planTool) expect(state.requests[name]).toEqual(state.plans[name].reply.transaction); return inspect(); }
  finally { if (planTool) await page.unroute('**/api/tools/' + planTool); await page.unroute('**/api/tools/' + applyTool); }
});
const history = name => ui(name, null, async () => { await page.getByRole('region', { name: 'History', exact: true }).getByRole('button', { name: 'Undo', exact: true }).click(); await page.getByRole('region', { name: 'History plan', exact: true }).getByRole('button', { name: 'Apply undo', exact: true }).click(); }, 'opencut_edit_undo_apply');
const edit = (name, operations) => stage(name, async () => { const before = await inspect(); const input = state.requests[name] ?? { project_id: project, expected_revision: before.revision, idempotency_key: 'f08-current-' + name, label: name, operations }; state.requests[name] = input; await save(); await call('opencut_edit_apply', input); await open(); return inspect(); });

const unknown = JSON.parse(await fs.readFile(args.unknown, 'utf8'));
const variants = JSON.parse(await fs.readFile(args.variants, 'utf8'));
const oracle = JSON.parse(await fs.readFile(args.reference, 'utf8'));
const inventory = JSON.parse(await fs.readFile(args.inventory, 'utf8'));
const longAsset = original.project.assets.find(a => a.version.content_hash === oracle.source_sha256);
const media = inventory.media.find(m => m.asset.version.content_hash === oracle.source_sha256);
expect(longAsset).toBeTruthy(); expect(media).toBeTruthy();
const allClockData = snap => snap.project.sequences;

try {
  await open();
  const unknownImported = await ui('actual-unknown-fps-import', 'opencut_media_import_plan', () => importProductMedia(page, unknown.path, writes));
  const unknownAsset = unknownImported.project.assets.find(a => a.version.content_hash === unknown.sha256);
  expect(unknownAsset.metadata.frame_rate).toBeNull();
  expect(state.plans['actual-unknown-fps-import'].reply.decisions.video_asset_ids_without_fps).toContain(unknownAsset.id);
  expect(unknownImported.project.settings).toEqual(original.project.settings);
  expect(allClockData(unknownImported)).toEqual(allClockData(original));
  expect(unknown.independent_packet_stats).toMatchObject({ packet_count: 1, average_packet_rate: null });
  const recoveredMov = await ui('recover-original-successful-mov-no-reupload', 'opencut_media_import_plan', async () => {
    await page.locator('.editor-available-media > summary').click();
    const planning = page.waitForResponse(r => r.url().endsWith('/opencut_media_import_plan')); void planning.catch(() => {});
    await page.getByRole('button', { name: 'Recover f08-short-container.mov', exact: true }).click();
    await applyMediaReview(page, writes, planning);
  });
  const formatsImported = await stage('actual-remaining-container-batch-import', async () => {
    // Both original uploads succeeded before the verifier's old five-second
    // dialog guard closed its document. Recover their published identities;
    // no second upload or replacement native Job is needed.
    const available = await call('opencut_project_media_query', { project_id: project, limit: 50, cursor: null });
    const bindings = variants.filter(v => !v.path.endsWith('.mov')).map(v => {
      const m = available.media.find(m => m.asset.version.content_hash === v.sha256); expect(m).toBeTruthy();
      state.recovered_imports ??= []; if (!state.recovered_imports.some(r => r.job_id === m.job_id)) state.recovered_imports.push({ job_id: m.job_id, media_id: m.media_id, hash: v.sha256, mime: m.asset.metadata.mime_type });
      return { media_id: m.media_id, version: m.asset.version, asset_id: 'f08-recovered-' + path.extname(v.path).slice(1), name: m.asset.name, mode: 'bind' };
    });
    const name = 'actual-remaining-container-batch-import', before = await inspect(), p = state.plans[name] ?? { input: { project_id: project, expected_revision: before.revision, sequence_id: sequence, idempotency_key: 'f08-current-recover-original-published-containers', label: 'Recover original completed TS and WebM', bindings, placements: [], new_tracks: [], ripple: false } };
    state.plans[name] = p; await save(); if (!p.reply) { p.reply = await call('opencut_media_import_plan', p.input); expect(await inspect()).toEqual(before); await save(); }
    state.requests[name] ??= p.reply.transaction; expect(state.requests[name]).toEqual(p.reply.transaction); await save(); await call('opencut_edit_apply', state.requests[name]); await open(); return inspect();
  });
  const formatAssets = variants.map(v => {
    const a = formatsImported.project.assets.find(a => a.version.content_hash === v.sha256);
    expect(a.metadata.frame_rate).toEqual(v.expected_project_fps);
    return { path: v.path, source_sha256: v.sha256, codec: v.codec, asset_id: a.id, metadata_fps: a.metadata.frame_rate, independent_classic_packet_rate: v.independent_classic_packet_rate, ffprobe_packet_rate: v.independent_packet_rate };
  });
  expect(allClockData(formatsImported)).toEqual(allClockData(original)); expect(formatsImported.project.settings).toEqual(original.project.settings);
  const e = structuredClone(node(original, 'f27-audio-video'));
  const channel = structuredClone(JSON.parse(await fs.readFile(path.join(args.workspace, 'agent-selection-ripple-v4-checkpoint.json'), 'utf8')).authored_sequence.tracks.find(t => t.id === 'f40-t0').elements[0].channels[0]);
  channel.id = 'f08-opacity'; channel.binding.parameter = 'opacity'; channel.target.element_id = e.id;
  channel.keys.forEach((k, i) => { k.id = 'f08-opacity-' + i; k.time = { ticks: String(i * 30000), timebase: 120000 }; k.value = .5 + i * .5; }); e.channels = [channel];
  const settings = structuredClone(original.project.settings); settings.settings.fps = { numerator: 12, denominator: 1 };
  const prepared = await edit('one-low-fps-preserved-curve', [{ type: 'settings_set', settings }, { type: 'element_put', sequence_id: sequence, track_id: original.project.sequences[0].tracks.find(t => t.elements.some(x => x.id === e.id)).id, element: e }]);
  const raised = await stage('rebind-original-vfr-no-upload', async () => {
    const name = 'rebind-original-vfr-no-upload', p = state.plans[name] ?? { input: { project_id: project, expected_revision: prepared.revision, sequence_id: sequence, idempotency_key: 'f08-current-original-vfr-bind', label: 'F08 original VFR bind without reupload', bindings: [{ media_id: media.media_id, version: media.asset.version, asset_id: longAsset.id, name: longAsset.name, mode: 'replace' }], placements: [], new_tracks: [], ripple: false } };
    state.plans[name] = p; await save();
    if (!p.reply) { const before = await inspect(); p.reply = await call('opencut_media_import_plan', p.input); expect(await inspect()).toEqual(before); await save(); }
    state.requests[name] ??= p.reply.transaction; expect(state.requests[name]).toEqual(p.reply.transaction); await save();
    await call('opencut_edit_apply', state.requests[name]); await open(); return inspect();
  });
  const decision = state.plans['rebind-original-vfr-no-upload'].reply.decisions;
  expect(decision.previous_fps).toEqual({ numerator: 12, denominator: 1 }); expect(decision.raised_fps).toEqual(oracle.expected_fps);
  expect(raised.project.settings.settings.fps).toEqual(oracle.expected_fps); expect(allClockData(raised)).toEqual(allClockData(prepared));
  await stage('current-fps-refresh-readback', async () => {
    await open(); expect((await inspect()).project).toEqual(raised.project);
    await page.getByRole('button', { name: 'Canvas and background', exact: true }).click(); const d = page.getByRole('dialog', { name: 'Canvas and background', exact: true });
    await expect(d.getByRole('spinbutton', { name: 'Frame-rate numerator', exact: true })).toHaveValue('24'); await expect(d.getByRole('spinbutton', { name: 'Frame-rate denominator', exact: true })).toHaveValue('1'); await d.getByRole('button', { name: 'Close', exact: true }).click(); return { complete_project_unchanged: true };
  });
  const output = await stage('one-six-frame-vfr-opacity-output', async () => {
    const input = state.requests.export ?? { kind: 'video_range', request: { project_id: project, revision: raised.revision, sequence_id: sequence, idempotency_key: 'f08-current-six-original-vfr-frames', options: { format: 'mp4', quality: 'high', include_audio: false }, range: { start_frame: '0', frame_count: '6' } } }; state.requests.export = input; await save();
    const accepted = await call('opencut_render_submit', input); let job; const until = Date.now() + 180000;
    do { job = await call('opencut_job_view', { job_id: accepted.job_id }); if (['succeeded', 'failed', 'cancelled'].includes(job.state)) break; if (Date.now() > until) throw Error('Observe original job ' + accepted.job_id + '; timeout is not terminal'); await new Promise(r => setTimeout(r, 400)); } while (true);
    expect(job.state, JSON.stringify(job.failure)).toBe('succeeded'); const file = path.join(args.workspace, 'f08-' + job.job_id + '.mp4'); let bytes = await fs.readFile(file).catch(e => { if (e.code !== 'ENOENT') throw e; return null; });
    if (!bytes) { bytes = Buffer.from(await (await client.downloadArtifact(job.artifacts[0])).arrayBuffer()); await fs.writeFile(file, bytes); } expect(sha(bytes)).toBe(job.artifacts[0].content_hash);
    const probe = JSON.parse(execFileSync('ffprobe', ['-v', 'error', '-select_streams', 'v:0', '-count_frames', '-show_streams', '-show_packets', '-of', 'json', file], { encoding: 'utf8' })), v = probe.streams[0], packets = probe.packets.sort((a, b) => Number(a.pts) - Number(b.pts));
    expect(v.avg_frame_rate).toBe('24/1'); expect(v.nb_read_frames).toBe('6'); expect([v.width, v.height]).toEqual([320, 180]);
    const [num, den] = v.time_base.split('/').map(Number); const times = packets.map(p => Number(p.pts) * num / den); times.forEach((t, i) => expect(Math.abs(t - i / 24)).toBeLessThan(1e-9)); const end = (Number(packets.at(-1).pts) + Number(packets.at(-1).duration)) * num / den; expect(end).toBe(.25);
    const raw = execFileSync('ffmpeg', ['-v', 'error', '-i', file, '-an', '-pix_fmt', 'rgb24', '-f', 'rawvideo', 'pipe:1']); expect(raw.length).toBe(6 * 320 * 180 * 3);
    let max = 0, frozenOpacityNegative = 0, wrongSourceClockNegative = 0; const samples = [];
    for (const f of oracle.frames) { const observed = []; for (const [x, y] of [[16, 16], [160, 90], [303, 163]]) { const at = (f.index * 320 * 180 + y * 320 + x) * 3; const pixel = [...raw.subarray(at, at + 3)]; pixel.forEach((v, c) => max = Math.max(max, Math.abs(v - f.rgb[c]))); observed.push(pixel); } const p = observed[1]; frozenOpacityNegative = Math.max(frozenOpacityNegative, ...p.map((v, c) => Math.abs(v - Math.round(oracle.source_rgb_rec709[f.index === 5 ? 1 : 0][c] * .5)))); if (f.index === 5) wrongSourceClockNegative = Math.max(...p.map((v, c) => Math.abs(v - Math.round(oracle.source_rgb_rec709[0][c] * 11 / 12)))); samples.push({ frame: f.index, expected: f.rgb, observed }); }
    expect(max).toBeLessThanOrEqual(8); expect(frozenOpacityNegative).toBeGreaterThan(20); expect(wrongSourceClockNegative).toBeGreaterThan(100);
    return { job_id: job.job_id, path: file, frames: 6, fps: v.avg_frame_rate, pts_seconds: times, exact_packet_end_seconds: end, max_channel_error: max, threshold: 8, samples, frozen_opacity_negative: frozenOpacityNegative, wrong_source_clock_negative: wrongSourceClockNegative, independent_reference: args.reference, execution: job.result };
  });
  expect((await history('undo-original-vfr-bind')).project).toEqual(prepared.project);
  expect((await history('undo-low-fps-curve')).project).toEqual(formatsImported.project);
  expect((await history('undo-container-batch')).project).toEqual(recoveredMov.project);
  expect((await history('undo-original-mov-recovery')).project).toEqual(unknownImported.project);
  expect((await history('undo-unknown-import')).project).toEqual(original.project);
  await open(); expect((await inspect()).project).toEqual(original.project); expect(errors).toEqual([]);
  delete state.failure; state.status = 'passed_scoped_f08_remaining'; await save();
  const result = { status: state.status, candidate: args.candidate, project_id: project, original_revision: original.revision, actual_unknown: { source: unknown, imported_asset_id: unknownAsset.id, metadata_fps: unknownAsset.metadata.frame_rate, project_fps_unchanged: true }, containers: formatAssets, current_vfr: { original_media_id: media.media_id, original_hash: oracle.source_sha256, no_reupload: true, previous_fps: decision.previous_fps, raised_fps: decision.raised_fps, exact_sequence_graph_and_curves_unchanged: true }, output, complete_original_project_restored_and_reopened: true, page_errors: errors, new_video_outputs: 1, new_sources: 4, new_source_bytes: unknown.size + variants.reduce((n, v) => n + v.size, 0), new_project_copies: 0, builds: 0, unit_tests: 0, scope: 'Remaining real unknown FPS/imported MOV/TS/WebM and current original300s VFR binding/export. Prior V4 two-scene/raw60-before-normalization/playhead/Undo and legacy CFR/fraction/first-video scopes retain their own candidates. Independent short output proves24fps clock,source298.75+5/4 and local opacity;no full-film repeat,all-codec/browser/DAC/A7/D8 claim.' };
  await fs.writeFile(args.report, JSON.stringify(result, null, 2) + '\n'); console.log(JSON.stringify({ status: result.status, job_id: output.job_id }));
} catch (error) { state.failure = { message: String(error), stack: error.stack, page_errors: errors }; await save(); await page.screenshot({ path: path.join(args.workspace, 'f08-current-failure.png'), fullPage: true }); throw error; }
finally { await browser.close(); }
