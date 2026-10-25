import { editorSnapshot } from "./project-inspection.mjs";
/** Reuse the retained long-source peer: actual line/input, separation and PCM. */
import { chromium, expect } from '@playwright/test';
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { OpenCutClient } from '../../../packages/sdk-ts/dist/index.js';

const args = Object.fromEntries(process.argv.slice(2).reduce((a, v, i, s) => i % 2 ? a : [...a, [v.slice(2), s[i + 1]]], []));
const { workspace, endpoint, report } = args, project = 'agent-long-varied-gain-copy';
const file = path.join(workspace, 'agent-source-gain-v4-checkpoint.json');
const state = await fs.readFile(file, 'utf8').then(JSON.parse).catch(e => { if (e.code !== 'ENOENT') throw e; return { stages: {}, requests: {}, plans: {} }; });
const client = new OpenCutClient({ baseUrl: endpoint, token: (await fs.readFile(args.credential, 'utf8')).trim() });
const call = (tool, input) => client.call(tool, input), inspect = (id = project) => editorSnapshot(call('opencut_project_inspect', { project_id: id, revision: null }));
const sha = b => createHash('sha256').update(b).digest('hex');
const save = async () => { await fs.writeFile(file + '.pending', JSON.stringify(state, null, 2) + '\n'); await fs.rename(file + '.pending', file); };
const stage = async (name, run) => { if (state.stages[name]) return state.stages[name]; console.log('Begin ' + name); const value = await run(); state.stages[name] = value; await save(); console.log('Passed ' + name); return value; };
const original = await stage('original-peer', () => inspect()), sequence = original.project.sequences[0].id;
const node = (s, id) => s.project.sequences[0].tracks.flatMap(t => t.elements).find(e => e.id === id);
const video = node(original, 'f27-audio-video');
expect(video.content.audio.gain).toEqual({ unit: 'decibels', value: -6 });
const browser = await chromium.launch({ headless: true, executablePath: process.env.OPENCUT_CHROME });
const page = await browser.newPage({ locale: 'en-US', viewport: { width: 1440, height: 1300 } });
const errors = [], writes = [], plans = []; let active = project;
page.on('pageerror', e => errors.push(e.message));
page.on('request', r => { if (/\/opencut_edit_(apply|undo_apply|redo_apply)$/.test(r.url())) writes.push(r.postDataJSON()); if (/\/opencut_project_(audio_gain|source_audio)_plan$/.test(r.url())) plans.push(r.postDataJSON()); });
const ready = () => expect(page.locator('.save-status')).toHaveText('Saved', { timeout: 30000 });
const open = async (id = project) => { active = id; await page.goto(`${endpoint}/editor/${id}`); await expect(page.getByRole('main', { name: 'OpenCut editor', exact: true })).toBeVisible(); const d = page.getByRole('dialog', { name: 'Welcome to OpenCut', exact: true }); if (await d.isVisible()) await d.getByRole('button', { name: 'Close', exact: true }).click(); await ready(); };
const clip = id => page.locator(`[data-element-id="${id}"]`), slider = id => clip(id).locator('.editor-volume-line');
const select = async id => { await ready(); await clip(id).locator('.editor-element-select').click({ position: { x: 9, y: 10 } }); };
const modeZoom = async value => { const input = page.getByRole('slider', { name: 'Timeline zoom', exact: true }); await input.focus(); if (value === 8) await page.keyboard.press('End'); else { await page.keyboard.press('Home'); for (let i = 1; i < Math.round(value * 10); i++) await page.keyboard.press('ArrowRight'); } await expect(input).toHaveValue(String(value)); };
const ui = (name, planTool, run, applyTool = 'opencut_edit_apply') => stage(name, async () => {
  if (state.requests[name]) { const receipt = await call(applyTool, state.requests[name]); expect((await inspect()).revision).toBe(receipt.transaction?.revision ?? receipt.revision); await open(); return inspect(); }
  if (state.plans[name]?.input) { const original = state.plans[name]; original.reply ??= await call(planTool, original.input); expect(original.reply.transaction).toBeTruthy(); state.requests[name] = original.reply.transaction; state.recovered_ui_plan ??= []; state.recovered_ui_plan.push(name); await save(); await call(applyTool, original.reply.transaction); await open(); return inspect(); }
  let receipt, count = 0;
  if (planTool) await page.route('**/api/tools/' + planTool, async route => { const input = route.request().postDataJSON(); state.plans[name] = { input }; await save(); const response = await route.fetch(), body = await response.json(); expect(body.error).toBeNull(); state.plans[name].reply = body.data; await save(); await route.fulfill({ response }); });
  await page.route('**/api/tools/' + applyTool, async route => { count++; state.requests[name] = route.request().postDataJSON(); await save(); const response = await route.fetch(), body = await response.json(); expect(body.error).toBeNull(); receipt = body.data; await route.fulfill({ response }); });
  try { await run(); await expect.poll(() => !!receipt, { timeout: 30000 }).toBe(true); await ready(); expect(count).toBe(1); if (planTool) expect(state.requests[name]).toEqual(state.plans[name].reply.transaction); const after = await inspect(); expect(after.revision).toBe(receipt.transaction?.revision ?? receipt.revision); return after; }
  finally { if (planTool) await page.unroute('**/api/tools/' + planTool); await page.unroute('**/api/tools/' + applyTool); }
});
const history = (name, redo = false) => ui(name, null, async () => { await page.getByRole('region', { name: 'History', exact: true }).getByRole('button', { name: redo ? 'Redo' : 'Undo', exact: true }).click(); await page.getByRole('region', { name: 'History plan', exact: true }).getByRole('button', { name: redo ? 'Apply redo' : 'Apply undo', exact: true }).click(); }, redo ? 'opencut_edit_redo_apply' : 'opencut_edit_undo_apply');
const sourceAction = async name => { await select(video.id); const region = page.getByRole('region', { name: 'Source audio controls', exact: true }); await region.getByRole('button', { name, exact: true }).click(); await expect(region).toContainText('Source audio changes saved.'); await region.getByRole('button', { name: 'Close', exact: true }).click(); };
const pointerBegin = async id => {
  await slider(id).scrollIntoViewIfNeeded(); await expect(slider(id)).toHaveAttribute('aria-disabled', 'false');
  const point = await slider(id).evaluate(e => { const rect = n => { const r = n.getBoundingClientRect(); return { left: r.left, top: r.top, right: r.right, bottom: r.bottom, width: r.width, height: r.height }; }, r = rect(e), v = rect(e.closest('.editor-timeline-viewport')), x = r.left + r.width / 2, y = Math.max(r.top + 2, Math.min(r.top + r.height / 2, v.bottom - 2)), actual = document.elementFromPoint(x, y); return { x, y, height: r.height, inside: e.contains(actual), line: r, view: v, surface: rect(e.closest('.editor-gain-surface')), covering: document.elementsFromPoint(x, y).slice(0, 5).map(n => ({ tag: n.tagName, class: n.className, rect: rect(n) })) }; });
  expect(point.inside, JSON.stringify(point)).toBe(true); expect(point.height).toBe(14); await page.mouse.move(point.x, point.y); await page.mouse.down(); return point;
};
const pointerMove = async (id, db, extra = 0) => {
  const r = await clip(id).locator('.editor-gain-surface').boundingBox();
  const percent = (1 - Math.sqrt((10 ** (db / 20) - .001) / 9.999));
  await page.mouse.move(r.x + r.width / 2, r.y + r.height * percent + extra);
  const witness = await page.evaluate(() => globalThis.__f44Pointer);
  expect(witness.id).toBe(id); const progress = 1 - Math.max(0, Math.min(1, (witness.y - witness.top) / witness.height));
  const tenths = Math.max(-600, Math.min(200, Math.round(200 * Math.log10(.001 + progress * progress * 9.999))));
  await expect(slider(id)).toHaveAttribute('aria-valuenow', String(tenths / 10)); return { tenths, witness };
};
const changedGain = (before, id, value) => { const expected = structuredClone(before.project); node({ project: expected }, id).content.audio.gain = { unit: 'decibels', value }; return expected; };
const dragSave = async (name, id, desired) => {
  const after = await ui(name, 'opencut_project_audio_gain_plan', async () => { await pointerBegin(id); state.pointer ??= {}; state.pointer[name] = await pointerMove(id, desired); await save(); await page.mouse.up(); await expect(page.locator('.editor-gain-status')).toContainText('Element audio changes saved.'); });
  const expected = state.pointer[name].tenths; expect(state.plans[name].input.command).toEqual({ kind: 'set_decibels', decibels_tenths: expected }); return { after, db: expected / 10 };
};
const publicEdit = (name, operations) => stage(name, async () => { const before = await inspect(), input = state.requests[name] ?? { project_id: project, expected_revision: before.revision, idempotency_key: 'f42-f44-' + name, label: name, operations }; state.requests[name] = input; await save(); await call('opencut_edit_apply', input); await open(); return inspect(); });
const wav = (name, fixed, gain) => stage(name, async () => {
  const input = { project_id: project, revision: fixed.revision, sequence_id: sequence, sample_rate: 48000, idempotency_key: 'f42-f44-' + name };
  if (state.requests[name]) expect(state.requests[name]).toEqual(input); else { state.requests[name] = input; await save(); }
  const accepted = await call("opencut_render_submit", { kind: "audio", request: input }); let job = await call('opencut_job_view', { job_id: accepted.job_id }); const until = Date.now() + 120000;
  while (!['succeeded', 'failed', 'cancelled'].includes(job.state)) { if (Date.now() > until) throw Error('Observe original job ' + job.job_id + '; timeout is not terminal'); await new Promise(r => setTimeout(r, 400)); job = await call('opencut_job_view', { job_id: accepted.job_id }); }
  expect(job.state, JSON.stringify(job.failure)).toBe('succeeded'); const file = path.join(workspace, 'f42-f44-' + job.job_id + '.wav'); let bytes = await fs.readFile(file).catch(e => { if (e.code !== 'ENOENT') throw e; return null; }); if (!bytes) { bytes = Buffer.from(await (await client.downloadArtifact(job.artifacts[0])).arrayBuffer()); await fs.writeFile(file, bytes); } expect(sha(bytes)).toBe(job.artifacts[0].content_hash);
  const reference = JSON.parse(await fs.readFile(args.reference, 'utf8')), raw = execFileSync('ffmpeg', ['-v', 'error', '-i', file, '-f', 'f32le', '-acodec', 'pcm_f32le', 'pipe:1']), expectedSource = await fs.readFile(reference.pcm_path);
  expect(sha(expectedSource)).toBe(reference.pcm_sha256); expect(raw.length).toBe(12000 * 8); let max = 0, negative = 0;
  for (let at = 0; at < raw.length; at += 4) { const sample = expectedSource.readFloatLE(at); max = Math.max(max, Math.abs(raw.readFloatLE(at) - sample * gain)); negative = Math.max(negative, Math.abs(sample * 10 ** (-6 / 20))); }
  expect(max).toBeLessThanOrEqual(1e-5); expect(negative).toBeGreaterThan(.01);
  return { job_id: job.job_id, path: file, frames: 12000, channels: 2, sample_rate: 48000, max_error: max, threshold: 1e-5, expected_gain: gain, omitted_or_duplicate_original_video_negative: negative, independent_reference: args.reference, original_source: reference.source_sha256, original_source_start_seconds: reference.source_start_seconds, original_source_rate: reference.source_rate, execution: job.result.data.execution };
});

await page.addInitScript(() => { addEventListener('pointermove', e => { const owner = e.target instanceof Element && e.target.closest('[data-element-id]'), surface = owner?.querySelector('.editor-gain-surface'); if (surface) { const r = surface.getBoundingClientRect(); globalThis.__f44Pointer = { id: owner.dataset.elementId, y: e.clientY, top: r.top, height: r.height }; } }, true); });
try {
  await open();
  await stage('video-line-zoom-boundary-cancel', async () => {
    const before = await inspect(), n = writes.length, p = plans.length;
    for (const zoom of [.1, 1, 8]) { await modeZoom(zoom); await pointerBegin(video.id); await pointerMove(video.id, -60, 15); await pointerMove(video.id, 20, -15); await pointerMove(video.id, -12); await page.keyboard.press('Escape'); await page.mouse.up(); await expect(slider(video.id)).toHaveAttribute('aria-valuenow', '-6'); }
    expect(writes.length).toBe(n); expect(plans.length).toBe(p); expect(await inspect()).toEqual(before); return { zoom: [.1, 1, 8], hitbox_px: 14, minimum: -60, maximum: 20, cancelled_no_plan_or_write: true };
  });
  await modeZoom(8); const videoDrag = await dragSave('actual-video-line-drag', video.id, -9.7); expect(videoDrag.after.project).toEqual(changedGain(original, video.id, videoDrag.db));
  expect((await history('video-line-undo')).project).toEqual(original.project); expect((await history('video-line-redo', true)).project).toEqual(videoDrag.after.project); expect((await history('video-line-redo-undo')).project).toEqual(original.project); await open();
  const separated = await ui('actual-separate-original-long-source', 'opencut_project_source_audio_plan', () => sourceAction('Separate source audio'));
  const detached = state.plans['actual-separate-original-long-source'].reply.decision.detached, audio = node(separated, detached.element_id), expectedSeparated = structuredClone(original.project);
  expect(audio).toEqual({ ...video, id: detached.element_id, visual: null, content: { kind: 'audio', asset_id: video.content.asset_id, audio: video.content.audio, retime: video.content.retime } });
  node({ project: expectedSeparated }, video.id).content.source_audio_enabled = false; const created = separated.project.sequences[0].tracks.find(t => t.id === detached.track_id); expect(created.kind).toBe('audio'); expect(created.elements).toEqual([audio]); expect(created.muted).toBe(false); expect(created.gain).toEqual({ unit: 'decibels', value: 0 }); expect(created.channels).toEqual([]); expectedSeparated.sequences[0].tracks.push(created); expect(separated.project).toEqual(expectedSeparated);
  await modeZoom(8); const audioDrag = await dragSave('actual-detached-audio-line-drag', audio.id, -12); expect(audioDrag.after.project).toEqual(changedGain(separated, audio.id, audioDrag.db));
  const muted = await ui('actual-element-mute-retains-gain', 'opencut_project_audio_gain_plan', () => clip(audio.id).getByRole('button', { name: 'Mute ' + audio.name, exact: true }).click()); const expectedMuted = structuredClone(audioDrag.after.project); node({ project: expectedMuted }, audio.id).content.audio.muted = true; expect(muted.project).toEqual(expectedMuted);
  const unmuted = await ui('actual-element-unmute-retains-gain', 'opencut_project_audio_gain_plan', () => clip(audio.id).getByRole('button', { name: 'Unmute ' + audio.name, exact: true }).click()); expect(unmuted.project).toEqual(audioDrag.after.project);
  await stage('audio-line-zoom-cancel-linear-protection', async () => {
    const before = await inspect(), n = writes.length, p = plans.length;
    for (const zoom of [.1, 1, 8]) { await modeZoom(zoom); await pointerBegin(audio.id); await pointerMove(audio.id, -60, 15); await pointerMove(audio.id, 20, -15); await page.keyboard.press('Escape'); await page.mouse.up(); await expect(slider(audio.id)).toHaveAttribute('aria-valuenow', String(audioDrag.db)); }
    await expect(slider('f27-audio-element')).toHaveCount(0); await expect(clip('f27-audio-element')).toContainText('Legacy linear gain cannot be edited with the volume line.'); expect(writes.length).toBe(n); expect(plans.length).toBe(p); expect(await inspect()).toEqual(before); return { zoom: [.1, 1, 8], hitbox_px: 14, linear_legacy_preserved: true, cancelled_no_write: true };
  });
  const templateState = JSON.parse(await fs.readFile(path.join(workspace, 'agent-selection-ripple-v4-checkpoint.json'), 'utf8')), channel = templateState.authored_sequence.tracks.find(t => t.id === 'f40-t0').elements[0].channels[0];
  const animated = await publicEdit('one-animated-video-and-audio-guard', [video.id, audio.id].map(id => { const e = structuredClone(node(unmuted, id)), ch = structuredClone(channel); ch.id = 'f44-' + id + '-automation'; ch.target.element_id = id; ch.keys.forEach((k, i) => { k.id = ch.id + '-' + i; k.time = { ticks: String(i * 30000), timebase: 120000 }; }); e.channels = [ch]; return { type: 'element_put', sequence_id: sequence, track_id: unmuted.project.sequences[0].tracks.find(t => t.elements.some(e => e.id === id)).id, element: e }; }));
  await stage('automation-static-lines-hidden-no-overwrite', async () => { const n = writes.length, p = plans.length; for (const id of [video.id, audio.id]) { await expect(slider(id)).toHaveCount(0); await expect(clip(id).getByRole('region', { name: 'Volume automation', exact: true })).toBeVisible(); await expect(clip(id).getByRole('button', { name: 'Move volume automation', exact: true })).toBeVisible(); } expect(writes.length).toBe(n); expect(plans.length).toBe(p); expect(await inspect()).toEqual(animated); return { both_types_show_existing_keyframe_editor: true, static_line_hidden: true, all_curves_retained: true, no_write: true }; });
  expect((await history('animated-guard-undo')).project).toEqual(unmuted.project);
  const locked = await publicEdit('one-locked-gain-guard', [{ type: 'track_lock_set', sequence_id: sequence, track_id: detached.track_id, locked: true }]);
  await stage('locked-line-and-mute-disabled', async () => { const n = writes.length, p = plans.length; await expect(slider(audio.id)).toHaveAttribute('aria-disabled', 'true'); await expect(clip(audio.id).getByRole('button', { name: 'Mute ' + audio.name, exact: true })).toBeDisabled(); await slider(audio.id).scrollIntoViewIfNeeded(); const r = await slider(audio.id).boundingBox(); await page.mouse.click(r.x + r.width / 2, r.y + r.height / 2); expect(writes.length).toBe(n); expect(plans.length).toBe(p); expect(await inspect()).toEqual(locked); return { no_plan_or_write: true, curve_gain_intact: true }; });
  expect((await history('locked-gain-undo')).project).toEqual(unmuted.project); await open();
  const separatedPcm = await wav('one-separated-gain-pcm', unmuted, 10 ** (audioDrag.db / 20) + .25);
  const restored = await ui('actual-restore-retains-edited-detached', 'opencut_project_source_audio_plan', () => sourceAction('Restore source audio')); const expectedRestored = structuredClone(unmuted.project); node({ project: expectedRestored }, video.id).content.source_audio_enabled = true; expect(restored.project).toEqual(expectedRestored); expect(node(restored, audio.id).content.audio.gain).toEqual({ unit: 'decibels', value: audioDrag.db });
  const restoredPcm = await wav('one-restored-source-and-detached-pcm', restored, 10 ** (-6 / 20) + 10 ** (audioDrag.db / 20) + .25);
  expect((await history('restore-source-undo')).project).toEqual(unmuted.project); expect((await history('unmute-undo')).project).toEqual(muted.project); expect((await history('mute-undo')).project).toEqual(audioDrag.after.project); expect((await history('detached-gain-undo')).project).toEqual(separated.project); expect((await history('separation-undo')).project).toEqual(original.project);
  await stage('no-audio-video-refusal', async () => { const id = 'scene-preview-fc284e7a-6b31-436f-a444-2befa7059942', before = await inspect(id), absent = before.project.sequences[0].tracks.flatMap(t => t.elements).find(e => e.content.kind === 'video'), n = writes.length, p = plans.length; await open(id); await select(absent.id); const r = page.getByRole('region', { name: 'Source audio controls', exact: true }); await expect(r.getByRole('button', { name: 'Separate source audio', exact: true })).toBeDisabled(); await expect(r.getByRole('button', { name: 'Restore source audio', exact: true })).toBeDisabled(); await expect(slider(absent.id)).toHaveAttribute('aria-disabled', 'true'); expect(writes.length).toBe(n); expect(plans.length).toBe(p); expect(await inspect(id)).toEqual(before); return { original_absent_asset_reused: true, project_unchanged: true }; });
  await stage('reopen-original-peer', async () => { await open(); expect((await inspect()).project).toEqual(original.project); return { full_original_graph_restored: true }; });
  expect(errors).toEqual([]); delete state.failure; state.status = 'passed_scoped_f42_long_separation_f44_line_matrix'; await save();
  await fs.writeFile(report, JSON.stringify({ status: state.status, project_id: project, original_revision: original.revision, candidate: args.candidate, actual: { video_gain: videoDrag.db, audio_gain: audioDrag.db, separated_id: audio.id, zoom_and_bounds: [state.stages['video-line-zoom-boundary-cancel'], state.stages['audio-line-zoom-cancel-linear-protection']], mute_preserves_gain: true, automation: state.stages['automation-static-lines-hidden-no-overwrite'], locked: state.stages['locked-line-and-mute-disabled'], no_audio: state.stages['no-audio-video-refusal'], full_graph_undo_redo_reopen: true }, pcm: { separated: separatedPcm, restored: restoredPcm }, new_jobs: 2, new_sources: 0, new_project_copies: 0, builds: 0, new_unit_tests: 0, page_errors: errors, remaining: ['F42 multiple-audio primary selection/codec priming/gap refusal and supported-clock matrix require explicit evidence','F44 actual current playback draft PCM is separate from this saved-gain output; prior evidence retains its own candidate','S1 original post-master residual,broader A7/human/physical/D8 remain open'] }, null, 2) + '\n'); console.log(JSON.stringify({ status: state.status, new_jobs: 2 }));
} catch (error) { state.failure = { message: String(error), stack: error.stack, page_errors: errors }; await save(); await page.screenshot({ path: path.join(workspace, 'f42-f44-current-failure.png'), fullPage: true }); throw error; }
finally { await browser.close(); }
