import { editorSnapshot } from "./project-inspection.mjs";
/** One tiny two-AAC-track source, one output; reuse the original owned peer. */
import { chromium, expect } from '@playwright/test';
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { OpenCutClient } from '../../../packages/sdk-ts/dist/index.js';
import { importProductMedia } from './verify-editor.mjs';

const args = Object.fromEntries(process.argv.slice(2).reduce((a, v, i, s) => i % 2 ? a : [...a, [v.slice(2), s[i + 1]]], []));
const project = 'agent-long-varied-gain-copy', file = path.join(args.workspace, 'agent-source-codec-v4-checkpoint.json');
const state = await fs.readFile(file, 'utf8').then(JSON.parse).catch(e => { if (e.code !== 'ENOENT') throw e; return { stages: {}, requests: {}, plans: {} }; });
const client = new OpenCutClient({ baseUrl: args.endpoint, token: (await fs.readFile(args.credential, 'utf8')).trim() });
const call = (tool, input) => client.call(tool, input), inspect = () => editorSnapshot(call('opencut_project_inspect', { project_id: project, revision: null }));
const sha = b => createHash('sha256').update(b).digest('hex');
const save = async () => { await fs.writeFile(file + '.pending', JSON.stringify(state, null, 2) + '\n'); await fs.rename(file + '.pending', file); };
const stage = async (name, run) => { if (state.stages[name]) return state.stages[name]; console.log('Begin ' + name); const value = await run(); state.stages[name] = value; await save(); console.log('Passed ' + name); return value; };
const original = await stage('original-peer', inspect), sequence = original.project.sequences[0].id;
const node = (s, id) => s.project.sequences[0].tracks.flatMap(t => t.elements).find(e => e.id === id);
const browser = await chromium.launch({ headless: true, executablePath: process.env.OPENCUT_CHROME });
const page = await browser.newPage({ locale: 'en-US', viewport: { width: 1440, height: 1300 } });
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
const edit = (name, operations) => stage(name, async () => { const before = await inspect(); const input = state.requests[name] ?? { project_id: project, expected_revision: before.revision, idempotency_key: 'f42-codec-' + name, label: name, operations }; state.requests[name] = input; await save(); await call('opencut_edit_apply', input); await open(); return inspect(); });
const action = async name => { await page.locator('[data-element-id="f42-primary-video"] .editor-element-select').click({ position: { x: 9, y: 10 } }); const region = page.getByRole('region', { name: 'Source audio controls', exact: true }); await region.getByRole('button', { name, exact: true }).click(); await expect(region).toContainText('Source audio changes saved.'); await region.getByRole('button', { name: 'Close', exact: true }).click(); };
const waitJob = async id => { let j; const until = Date.now() + 90000; do { j = await call('opencut_job_view', { job_id: id }); if (['succeeded', 'failed', 'cancelled'].includes(j.state)) return j; if (Date.now() > until) throw Error('Observe original job ' + id + '; timeout is not terminal'); await new Promise(r => setTimeout(r, 400)); } while (true); };

try {
  await open();
  const imported = await ui('one-small-source-import', 'opencut_media_import_plan', () => importProductMedia(page, args.source, writes));
  const asset = imported.project.assets.find(a => !original.project.assets.some(b => b.id === a.id)); expect(asset).toBeTruthy();
  const template = structuredClone(node(original, 'f27-audio-video')), s = structuredClone(original.project.sequences[0]);
  const curves = JSON.parse(await fs.readFile(path.join(args.workspace, 'agent-selection-ripple-v4-checkpoint.json'), 'utf8')).authored_sequence;
  const channel = structuredClone(curves.tracks.find(t => t.id === 'f40-t0').elements[0].channels[0]);
  channel.id = 'f42-primary-curve'; channel.target.element_id = 'f42-primary-video';
  channel.keys.forEach((k, i) => { k.id = 'f42-primary-key-' + i; k.time = { ticks: String(i * 264000), timebase: 120000 }; k.value = -6; });
  template.id = 'f42-primary-video'; template.name = 'F42 two-track primary'; template.content.asset_id = asset.id;
  template.content.audio.gain.value = -12; template.timeline_range = { start: { ticks: '12000', timebase: 120000 }, duration: { ticks: '264000', timebase: 120000 } };
  template.source_span = { profile: 'classic_trimmed', source_duration: { ticks: '360000', timebase: 120000 }, trim_start: { ticks: '15000', timebase: 120000 }, trim_end: { ticks: '15000', timebase: 120000 } }; template.channels = [channel];
  s.tracks = [{ ...s.tracks[0], elements: [template] }];
  const fixture = await edit('one-primary-curve-fixture', [{ type: 'sequence_put', sequence: s }]);
  const separated = await ui('actual-primary-separation', 'opencut_project_source_audio_plan', () => action('Separate source audio'));
  const detached = state.plans['actual-primary-separation'].reply.decision.detached, audio = node(separated, detached.element_id);
  expect(node(separated, template.id).content.source_audio_enabled).toBe(false);
  expect(audio.content).toEqual({ kind: 'audio', asset_id: asset.id, audio: template.content.audio, retime: template.content.retime });
  expect(audio.timeline_range).toEqual(template.timeline_range); expect(audio.source_span).toEqual(template.source_span);
  expect(audio.channels).toHaveLength(1); const copied = structuredClone(audio.channels[0]);
  expect(copied.id).not.toBe(channel.id); expect(copied.keys.every((k, i) => k.id !== channel.keys[i].id)).toBe(true);
  copied.id = channel.id; copied.target.element_id = template.id; copied.keys.forEach((k, i) => k.id = channel.keys[i].id); expect(copied).toEqual(channel);
  const pcm = await stage('one-primary-priming-gap-pcm', async () => {
    const input = state.requests.wav ?? { project_id: project, revision: separated.revision, sequence_id: sequence, sample_rate: 48000, idempotency_key: 'f42-primary-priming-gap-wav' }; state.requests.wav = input; await save();
    const accepted = await call("opencut_render_submit", { kind: "audio", request: input }), job = await waitJob(accepted.job_id); expect(job.state, JSON.stringify(job.failure)).toBe('succeeded');
    const wav = path.join(args.workspace, 'f42-' + job.job_id + '.wav'); let bytes = await fs.readFile(wav).catch(e => { if (e.code !== 'ENOENT') throw e; return null; });
    if (!bytes) { bytes = Buffer.from(await (await client.downloadArtifact(job.artifacts[0])).arrayBuffer()); await fs.writeFile(wav, bytes); } expect(sha(bytes)).toBe(job.artifacts[0].content_hash);
    const reference = JSON.parse(await fs.readFile(args.reference, 'utf8')), expected = await fs.readFile(reference.pcm_path), raw = execFileSync('ffmpeg', ['-v', 'error', '-i', wav, '-f', 'f32le', '-acodec', 'pcm_f32le', 'pipe:1']);
    expect(sha(expected)).toBe(reference.pcm_sha256); expect(raw.length).toBe(110400 * 8); expect(expected.length).toBe(raw.length);
    let max = 0, silence = 0, wrongGain = 0, duplicate = 0;
    for (let p = 0; p < raw.length; p += 4) { const actual = raw.readFloatLE(p), wanted = expected.readFloatLE(p); if (!Number.isFinite(actual)) throw Error('Non-finite PCM'); max = Math.max(max, Math.abs(actual - wanted)); wrongGain = Math.max(wrongGain, Math.abs(actual - wanted * 10 ** (-6 / 20))); duplicate = Math.max(duplicate, Math.abs(actual - wanted * 2)); if (p < 4800 * 8) silence = Math.max(silence, Math.abs(actual)); }
    expect(max).toBeLessThanOrEqual(1e-5); expect(silence).toBeLessThanOrEqual(1e-7); expect(wrongGain).toBeGreaterThan(.01); expect(duplicate).toBeGreaterThan(.01);
    const tones = [0, 1].map(c => { const values = Array.from({ length: 9600 }, (_, i) => raw.readFloatLE(((i + 12000) * 2 + c) * 4)); const power = hz => { let re = 0, im = 0; for (let i = 0; i < values.length; i++) { re += values[i] * Math.cos(i * 2 * Math.PI * hz / 48000); im -= values[i] * Math.sin(i * 2 * Math.PI * hz / 48000); } return re * re + im * im; }; const wanted = [550, 825][c], wrong = wanted * 2, ratio = power(wanted) / power(wrong); expect(ratio).toBeGreaterThan(10); return { expected_hz: wanted, wrong_secondary_hz: wrong, power_ratio: ratio }; });
    return { job_id: job.job_id, path: wav, frames: 110400, channels: 2, sample_rate: 48000, max_error: max, threshold: 1e-5, leading_silence: silence, wrong_static_gain_negative: wrongGain, duplicate_original_negative: duplicate, primary_tones: tones, reference: args.reference, source_hash: reference.source_sha256, execution_result: job.result };
  });
  const restored = await ui('actual-restore-retains-copied-curve', 'opencut_project_source_audio_plan', () => action('Restore source audio'));
  const expectedRestore = structuredClone(separated.project); node({ project: expectedRestore }, template.id).content.source_audio_enabled = true; expect(restored.project).toEqual(expectedRestore);
  expect((await history('undo-source-restore')).project).toEqual(separated.project);
  const selected = structuredClone(audio); selected.content.audio.stream = 3;
  const explicit = await edit('explicit-stream-refusal-fixture', [{ type: 'element_put', sequence_id: sequence, track_id: detached.track_id, element: selected }]);
  const refusal = await stage('explicit-stream-refusal', async () => {
    const input = state.requests.refusal ?? { project_id: project, revision: explicit.revision, sequence_id: sequence, sample_rate: 48000, idempotency_key: 'f42-explicit-stream-refusal' }; state.requests.refusal = input; await save();
    let error, accepted;
    if (state.retained_refusal) { expect(state.retained_refusal.input).toEqual(input); error = state.retained_refusal.error; }
    else { try { accepted = await call("opencut_render_submit", { kind: "audio", request: input }); } catch (e) { error = { code: e.code, message: e.message, status: e.status, details: e.details, request_id: e.requestId }; } }
    if (accepted) { const job = await waitJob(accepted.job_id); expect(job.state).toBe('failed'); expect(job.artifacts).toEqual([]); error = job.failure; state.refusal_job = job.job_id; await save(); }
    expect(['UNSUPPORTED_AUDIO_CAPABILITY', 'JOB_FAILED']).toContain(error.code);
    if (error.code === 'JOB_FAILED') expect(error.status).toBe(422);
    return { error, job_id: state.refusal_job ?? null, public_error_redacted: error.code === 'JOB_FAILED', source_guard: 'classic_audio/session.rs rejects any explicit stream before adapter admission; do not claim the public generic error exposes its internal code.', original_revision_unchanged: (await inspect()).revision === explicit.revision };
  });
  expect((await history('undo-explicit-stream')).project).toEqual(separated.project);
  expect((await history('undo-primary-separation')).project).toEqual(fixture.project);
  expect((await history('undo-primary-fixture')).project).toEqual(imported.project);
  expect((await history('undo-small-source-import')).project).toEqual(original.project);
  await open(); expect((await inspect()).project).toEqual(original.project); expect(errors).toEqual([]);
  delete state.failure; state.status = 'passed_scoped_f42_primary_codec'; await save();
  await fs.writeFile(args.report, JSON.stringify({ status: state.status, candidate: args.candidate, project_id: project, original_revision: original.revision, pcm, explicit_stream_refusal: refusal, copied_gain_curves_and_span_rate_preserved: true, original_full_graph_restored_and_reopened: true, new_sources: 1, source_bytes: (await fs.stat(args.source)).size, new_project_copies: 0, successful_audio_jobs: 1, page_errors: errors, scope: 'Classic primary-track-only selection,1024 AAC priming and real container PTS gap normalized by pinned decoder,trim .125/rate5:4/scene offset .1/2.2s duration,constant-6dB automation overrides static-12. Other rate/pitch modes and Restore mixing reuse earlier scoped evidence; no stream chooser,all-codec,realtime PTS,DAC or full S1/A7/D8 claim.' }, null, 2) + '\n'); console.log(JSON.stringify({ status: state.status, job_id: pcm.job_id }));
} catch (error) { state.failure = { message: String(error), stack: error.stack, page_errors: errors }; await save(); await page.screenshot({ path: path.join(args.workspace, 'f42-primary-codec-failure.png'), fullPage: true }); throw error; }
finally { await browser.close(); }
