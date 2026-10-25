import { editorSnapshot } from "./project-inspection.mjs";
/** F50: retained projects, one settings workflow and three exported frames. */
import { chromium, expect } from '@playwright/test';
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { OpenCutClient } from '../../../packages/sdk-ts/dist/index.js';

const args = Object.fromEntries(process.argv.slice(2).reduce((a, v, i, s) => i % 2 ? a : [...a, [v.slice(2), s[i + 1]]], []));
const { workspace, endpoint, report } = args;
const checkpoint = path.join(workspace, 'agent-canvas-settings-v4-checkpoint.json');
const previous = JSON.parse(await fs.readFile(path.join(workspace, 'agent-timeline-lifecycle-v4-checkpoint.json'), 'utf8'));
const project = previous.stages['prepare-compat-caption'].project.id;
const peer = JSON.parse(await fs.readFile(args['property-source'], 'utf8')).audio_project_id;
const state = await fs.readFile(checkpoint, 'utf8').then(JSON.parse).catch(e => { if (e.code !== 'ENOENT') throw e; return { stages: {}, requests: {} }; });
const client = new OpenCutClient({ baseUrl: endpoint, token: (await fs.readFile(args.credential, 'utf8')).trim() });
const call = (name, input) => client.call(name, input);
const inspect = (id = project) => editorSnapshot(call('opencut_project_inspect', { project_id: id, revision: null }));
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const save = async () => { await fs.writeFile(checkpoint + '.pending', JSON.stringify(state, null, 2) + '\n'); await fs.rename(checkpoint + '.pending', checkpoint); };
const stage = async (name, run) => { if (state.stages[name]) return state.stages[name]; console.log('Begin ' + name); const value = await run(); state.stages[name] = value; await save(); console.log('Passed ' + name); return value; };
const baseline = await stage('retain-original-projects', async () => ({ primary: await inspect(), peer: await inspect(peer) }));
const browser = await chromium.launch({ headless: true, executablePath: process.env.OPENCUT_CHROME });
const page = await browser.newPage({ locale: 'en-US', viewport: { width: 1440, height: 1100 } });
const errors = [], writes = [];
let active = project;
page.on('pageerror', e => errors.push(e.message));
page.on('request', r => { if (/\/api\/tools\/opencut_edit_(apply|undo_apply|redo_apply)$/.test(r.url())) writes.push(r.postDataJSON()); });
const ready = () => expect(page.locator('.save-status')).toHaveText('Saved', { timeout: 30000 });
const open = async (id = project) => {
  active = id;
  await page.goto(`${endpoint}/editor/${id}`);
  await expect(page.getByRole('main', { name: 'OpenCut editor', exact: true })).toBeVisible();
  const welcome = page.getByRole('dialog', { name: 'Welcome to OpenCut', exact: true });
  if (await welcome.isVisible()) await welcome.getByRole('button', { name: 'Close', exact: true }).click();
  await ready();
};
const form = async () => { await ready(); await page.getByRole('button', { name: 'Canvas and background', exact: true }).click(); return page.getByRole('dialog', { name: 'Canvas and background', exact: true }); };
const close = async d => { await d.getByRole('button', { name: 'Close', exact: true }).click(); await expect(d).not.toBeVisible(); };
const ui = (name, tool, run) => stage(name, async () => {
  if (state.requests[name]) { const receipt = await call(tool, state.requests[name]); const id = state.requests[name].project_id; expect((await inspect(id)).revision).toBe(receipt.transaction?.revision ?? receipt.revision); await open(id); return inspect(id); }
  let receipt, count = 0;
  await page.route('**/api/tools/' + tool, async route => {
    count++; state.requests[name] = route.request().postDataJSON(); await save();
    const response = await route.fetch(), body = await response.json(); expect(body.error).toBeNull(); receipt = body.data; await route.fulfill({ response });
  });
  try { await run(); await expect.poll(() => !!receipt, { timeout: 30000 }).toBe(true); await ready(); expect(count).toBe(1); const result = await inspect(active); expect(result.revision).toBe(receipt.transaction?.revision ?? receipt.revision); return result; }
  finally { await page.unroute('**/api/tools/' + tool); }
});
const edit = (name, fill) => ui(name, 'opencut_edit_apply', async () => { const d = await form(); await fill(d); await d.getByRole('button', { name: 'Save canvas and frame rate', exact: true }).click(); await ready(); await close(d); });
const history = async (name, redo = false) => {
  await open(active);
  return ui(name, redo ? 'opencut_edit_redo_apply' : 'opencut_edit_undo_apply', async () => {
    await page.getByRole('region', { name: 'History', exact: true }).getByRole('button', { name: redo ? 'Redo' : 'Undo', exact: true }).click();
    await page.getByRole('region', { name: 'History plan', exact: true }).getByRole('button', { name: redo ? 'Apply redo' : 'Apply undo', exact: true }).click();
  });
};
const historySnapshot = revision => call('opencut_project_history', { project_id: project, from_revision: revision, limit: 100 });
const onlySettings = (result, settings) => { const expected = structuredClone(baseline.primary.project); expected.settings = settings; expect(result.project).toEqual(expected); };

try {
  await open();
  await stage('actual-invalid-dimensions-no-write', async () => {
    const before = await inspect(), h = await historySnapshot(before.revision), n = writes.length, d = await form();
    await d.getByRole('spinbutton', { name: 'Canvas width', exact: true }).fill('0');
    await d.getByRole('button', { name: 'Save canvas and frame rate', exact: true }).click();
    await expect(d.getByRole('alert')).toHaveText('Enter positive 32-bit dimensions and frame-rate values.');
    await close(d); expect(writes.length).toBe(n); expect(await inspect()).toEqual(before); expect(await historySnapshot(before.revision)).toEqual(h);
    return { zero_width_rejected_before_dispatch: true, project_history_unchanged: true };
  });
  const custom = await edit('actual-custom-size-fractional-fps', async d => {
    for (const [label, value] of [['Canvas width', '480'], ['Canvas height', '270'], ['Frame-rate numerator', '24000'], ['Frame-rate denominator', '1001']]) await d.getByRole('spinbutton', { name: label, exact: true }).fill(value);
  });
  const customSettings = structuredClone(baseline.primary.project.settings);
  Object.assign(customSettings.settings, { canvas_size: { width: 480, height: 270 }, canvas_size_mode: 'custom', last_custom_canvas_size: { width: 480, height: 270 }, fps: { numerator: 24000, denominator: 1001 } });
  onlySettings(custom, customSettings);
  await stage('actual-custom-reload', async () => {
    await open(); const d = await form();
    for (const [label, value] of [['Canvas width', '480'], ['Canvas height', '270'], ['Frame-rate numerator', '24000'], ['Frame-rate denominator', '1001']]) await expect(d.getByRole('spinbutton', { name: label, exact: true })).toHaveValue(value);
    await close(d); expect((await inspect()).project).toEqual(custom.project); return { saved_values_reloaded: true, all_authored_times_and_content_retained: true };
  });
  const preset = await edit('actual-preset-keeps-custom-memory', d => d.getByRole('combobox', { name: 'Canvas preset', exact: true }).selectOption('1920x1080'));
  const presetSettings = structuredClone(customSettings); presetSettings.settings.canvas_size = { width: 1920, height: 1080 }; presetSettings.settings.canvas_size_mode = 'preset'; onlySettings(preset, presetSettings);
  await open();
  const restored = await edit('actual-restore-custom-after-reload', d => d.getByRole('button', { name: 'Restore last custom size', exact: true }).click());
  onlySettings(restored, customSettings);
  await stage('actual-original-fallback-and-view-no-write', async () => {
    expect(baseline.primary.project.settings.settings.original_canvas_size).toBeNull();
    const before = await inspect(), h = await historySnapshot(before.revision), n = writes.length, d = await form();
    await d.getByRole('spinbutton', { name: 'Canvas width', exact: true }).fill('111');
    await d.getByRole('button', { name: 'Use original size', exact: true }).click();
    await expect(d.getByRole('spinbutton', { name: 'Canvas width', exact: true })).toHaveValue('480');
    await expect(d.getByRole('spinbutton', { name: 'Canvas height', exact: true })).toHaveValue('270');
    await expect(d.getByText('No original size is recorded. Original size keeps the current canvas dimensions.', { exact: true })).toBeVisible();
    await close(d); await page.getByRole('button', { name: 'Zoom in', exact: true }).click(); await page.getByRole('button', { name: 'Zoom in', exact: true }).click();
    await open(); expect(writes.length).toBe(n); expect(await inspect()).toEqual(before); expect(await historySnapshot(before.revision)).toEqual(h);
    return { null_original_falls_back_to_saved_canvas_with_reason: true, unsaved_settings_and_zoom_do_not_write: true };
  });
  const isolation = await stage('actual-peer-original-source-and-isolation', async () => {
    const n = writes.length; await open(peer); const settings = baseline.peer.project.settings.settings, d = await form();
    await expect(d.getByRole('spinbutton', { name: 'Canvas width', exact: true })).toHaveValue(String(settings.canvas_size.width));
    await d.getByRole('button', { name: 'Restore last custom size', exact: true }).click();
    await expect(d.getByRole('spinbutton', { name: 'Canvas width', exact: true })).toHaveValue(String(settings.last_custom_canvas_size.width));
    const original = settings.original_canvas_size; expect(original).toBeTruthy();
    const source = baseline.peer.project.assets.find(a => a.kind === 'video' && a.metadata.intrinsic_size?.width === original.width && a.metadata.intrinsic_size?.height === original.height); expect(source).toBeTruthy();
    await d.getByRole('button', { name: 'Use original size', exact: true }).click();
    await expect(d.getByRole('spinbutton', { name: 'Canvas width', exact: true })).toHaveValue(String(original.width));
    await expect(d.getByRole('spinbutton', { name: 'Canvas height', exact: true })).toHaveValue(String(original.height));
    await close(d); expect(await inspect(peer)).toEqual(baseline.peer); await open();
    expect((await inspect()).project).toEqual(restored.project); expect(writes.length).toBe(n);
    return { peer_project_id: peer, peer_settings_retained: true, original_size: original, original_source_asset_id: source.id, original_source_hash: source.version.content_hash, primary_and_peer_custom_memory_isolated: true, isolation_stage_peer_writes: 0 };
  });
  await open(peer);
  const peerCustom = await edit('actual-peer-custom-export-settings', async d => {
    for (const [label, value] of [['Canvas width', '480'], ['Canvas height', '270'], ['Frame-rate numerator', '24000'], ['Frame-rate denominator', '1001']]) await d.getByRole('spinbutton', { name: label, exact: true }).fill(value);
  });
  const expectedPeer = structuredClone(baseline.peer.project);
  Object.assign(expectedPeer.settings.settings, { canvas_size: { width: 480, height: 270 }, canvas_size_mode: 'custom', last_custom_canvas_size: { width: 480, height: 270 }, fps: { numerator: 24000, denominator: 1001 } });
  expect(peerCustom.project).toEqual(expectedPeer); expect((await inspect()).project).toEqual(restored.project);
  const output = await stage('one-three-frame-fractional-export', async () => {
    const input = { kind: 'video_range', request: { project_id: peer, revision: peerCustom.revision, sequence_id: peerCustom.project.sequences[0].id, idempotency_key: 'f50-current-three-long-source-frames', options: { format: 'mp4', quality: 'high', include_audio: false }, range: { start_frame: '0', frame_count: '3' } } };
    if (state.requests.export) expect(state.requests.export).toEqual(input); else { state.requests.export = input; await save(); }
    const accepted = await call('opencut_render_submit', input); let job = await call('opencut_job_view', { job_id: accepted.job_id });
    const deadline = Date.now() + 180000;
    while (!['succeeded', 'failed', 'cancelled'].includes(job.state)) { if (Date.now() > deadline) throw Error('Observe original job ' + job.job_id + '; timeout is not termination'); await new Promise(r => setTimeout(r, 400)); job = await call('opencut_job_view', { job_id: accepted.job_id }); }
    expect(job.state, JSON.stringify(job.failure)).toBe('succeeded'); const file = path.join(workspace, 'f50-' + job.job_id + '.mp4'); let bytes = await fs.readFile(file).catch(e => { if(e.code !== 'ENOENT') throw e; return null; }); if(!bytes){const response=await client.downloadArtifact(job.artifacts[0]);bytes=Buffer.from(await response.arrayBuffer());await fs.writeFile(file,bytes);} expect(sha(bytes)).toBe(job.artifacts[0].content_hash);
    const probe = JSON.parse(execFileSync('ffprobe', ['-v', 'error', '-count_frames', '-show_streams', '-show_packets', '-show_format', '-of', 'json', file], { encoding: 'utf8' }));
    expect(probe.streams).toHaveLength(1); const video = probe.streams[0]; expect(video.codec_type).toBe('video'); expect([video.width, video.height]).toEqual([480, 270]); expect(video.avg_frame_rate).toBe('24000/1001'); expect(video.nb_read_frames).toBe('3'); expect(video.time_base).toBe('1/24000');const packets=probe.packets.filter(p=>p.codec_type==='video').sort((a,b)=>Number(a.pts)-Number(b.pts));expect(packets.map(p=>Number(p.pts))).toEqual([0,1001,2002]);expect(packets.map(p=>Number(p.duration))).toEqual([1001,1001,1001]);const presentationEnd=(Number(packets.at(-1).pts)+Number(packets.at(-1).duration))/24000;expect(presentationEnd).toBe(3*1001/24000);let movieTimescale;for(let at=0;at+8<=bytes.length;){const size=bytes.readUInt32BE(at);if(bytes.toString('ascii',at+4,at+8)==='moov'){for(let child=at+8;child+8<=at+size;){const n=bytes.readUInt32BE(child);if(bytes.toString('ascii',child+4,child+8)==='mvhd'){expect(bytes[child+8]).toBe(0);movieTimescale=bytes.readUInt32BE(child+20);}if(!n)break;child+=n;}}if(!size)break;at+=size;}expect(movieTimescale).toBe(1000);expect(Math.abs(Number(video.duration)-presentationEnd)).toBeLessThanOrEqual(1/movieTimescale);
    expect((await inspect()).project).toEqual(restored.project); expect((await inspect(peer)).project).toEqual(peerCustom.project);
    return { project_id: peer, revision: peerCustom.revision, original_long_source_retained: true, job_id: job.job_id, path: file, width: video.width, height: video.height, fps: video.avg_frame_rate, observed_frames: Number(video.nb_read_frames), independent_expected_seconds: 3 * 1001 / 24000, observed_packet_end_seconds: presentationEnd, observed_stream_duration_seconds: Number(video.duration), movie_timescale: movieTimescale, stream_summary_rounding_seconds: Math.abs(Number(video.duration)-presentationEnd), packet_pts: packets.map(p=>Number(p.pts)), packet_durations: packets.map(p=>Number(p.duration)), exact_packet_clock_verified: true, result: job.result.data };
  });
  expect((await history('actual-peer-custom-export-undo')).project).toEqual(baseline.peer.project);
  await open();
  expect((await history('actual-restore-custom-undo')).project).toEqual(preset.project);
  expect((await history('actual-preset-undo')).project).toEqual(custom.project);
  expect((await history('actual-custom-undo')).project).toEqual(baseline.primary.project);
  expect((await history('actual-custom-redo', true)).project).toEqual(custom.project);
  expect((await history('actual-custom-redo-undo')).project).toEqual(baseline.primary.project);
  await stage('actual-final-reopen-both-originals', async () => { await open(); expect((await inspect()).project).toEqual(baseline.primary.project); expect((await inspect(peer)).project).toEqual(baseline.peer.project); return { both_original_project_graphs_restored: true, peer_changed_only_for_identical_export_settings_and_undone: true }; });
  expect(errors).toEqual([]); delete state.failure; state.status = 'passed_scoped_f50_canvas_fps'; await save();
  await fs.writeFile(report, JSON.stringify({ status: state.status, project_id: project, peer_project_id: peer, settings_revision: restored.revision, export_settings_revision: peerCustom.revision, unsupported_primary_export: state.export_refusal, actual: { custom_reload_and_preset_restore: true, original_fallback: state.stages['actual-original-fallback-and-view-no-write'], original_source_and_isolation: isolation, full_graph_undo_redo_reopen: true, authored_times_channels_and_content_unchanged: true }, output, page_errors: errors, completed_stages: Object.keys(state.stages), new_jobs: 1, new_sources: 0, new_project_copies: 0, product_changes: 0, builds: 0, unit_tests: 0, scope: 'F50 declared custom size/FPS persistence, preset/custom memory, recorded source and explicit null fallback, per-project isolation, no view-only writes, exact history and actual saved480x270/24000:1001 three-frame output of original300s tail source in the retained peer. Original primary transition preflight refusal is explicit and remains unsupported; no transition deletion or fixture copy. Reuses prior format/quality/audio matrix; no broad density, VFR, all-browser, A7/S1/D8 or whole-migration claim.' }, null, 2) + '\n');
  console.log(JSON.stringify({ status: state.status, job_id: output.job_id }));
} catch (error) {
  state.failure = { message: String(error), stack: error.stack, page_errors: errors }; await save(); await page.screenshot({ path: path.join(workspace, 'f50-settings-failure.png'), fullPage: true }); throw error;
} finally { await browser.close(); }
