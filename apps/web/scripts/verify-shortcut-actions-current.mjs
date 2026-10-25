import { editorSnapshot } from "./project-inspection.mjs";
/** F54 integration: reuse one owned peer and both existing sources. No render
 * Jobs or unit-target builds. Persist requests before mutation; resume stages. */
import { chromium, expect } from '@playwright/test';
import fs from 'node:fs/promises';
import path from 'node:path';
import { OpenCutClient } from '../../../packages/sdk-ts/dist/index.js';
import { ACTIONS, defaultBindings } from '../src/editor/shortcuts/catalog.ts';

const args = Object.fromEntries(process.argv.slice(2).reduce((all, v, i, a) => i % 2 ? all : [...all, [v.slice(2), a[i + 1]]], []));
const file = path.join(args.workspace, 'agent-shortcut-actions-v4-checkpoint.json');
const state = await fs.readFile(file, 'utf8').then(JSON.parse).catch(e => {
  if (e.code !== 'ENOENT') throw e;
  return { candidate: args.candidate, stages: {}, requests: {}, plans: {} };
});
expect(state.candidate).toBe(args.candidate);
const done = await fs.readFile(args.report, 'utf8').then(JSON.parse).catch(e => { if (e.code !== 'ENOENT') throw e; return null; });
if (done?.status === 'passed_scoped_f54_actions' && done.repeat_guards && done.candidate === args.candidate) {
  console.log('Reuse completed F54 workflow.'); process.exit(0);
}
const save = async () => { await fs.writeFile(file + '.pending', JSON.stringify(state, null, 2) + '\n'); await fs.rename(file + '.pending', file); };
const stage = async (name, run) => {
  if (state.stages[name]) return state.stages[name];
  console.log('Begin ' + name); const value = await run(); state.stages[name] = value; await save(); console.log('Passed ' + name); return value;
};
const client = new OpenCutClient({ baseUrl: args.endpoint, token: (await fs.readFile(args.credential, 'utf8')).trim() });
const project = 'agent-long-varied-gain-copy';
const call = (tool, input) => client.call(tool, input);
const inspect = () => editorSnapshot(call('opencut_project_inspect', { project_id: project, revision: null }));
const original = await stage('original-peer', inspect);
const sequence = original.project.sequences[0].id;
const t = n => ({ ticks: String(n * 120000), timebase: 120000 });
const edit = (name, operations) => stage(name, async () => {
  const current = await inspect();
  const input = state.requests[name] ?? { project_id: project, expected_revision: current.revision, idempotency_key: 'f54-' + name, label: name, operations };
  state.requests[name] = input; await save();
  await call('opencut_edit_apply', input); return inspect();
});
const browser = await chromium.launch({ headless: true, executablePath: process.env.OPENCUT_CHROME });
const page = await browser.newPage({ locale: 'en-US', viewport: { width: 1440, height: 1300 } });
const errors = [], calls = [];
page.on('pageerror', e => errors.push(e.message));
page.on('request', r => { if (r.url().includes('/api/tools/')) calls.push(new URL(r.url()).pathname.split('/').at(-1)); });
const bindings = defaultBindings();
bindings['remove-media-asset'] = ['alt+r']; bindings['remove-media-assets'] = ['alt+shift+r'];
await page.addInitScript(config => {
  localStorage.setItem('opencut.shortcuts', JSON.stringify(config));
}, { bindings });
const open = async () => {
  await page.goto(`${args.endpoint}/editor/${project}`);
  await expect(page.locator('.save-status')).toHaveText('Saved');
  const welcome = page.getByRole('dialog', { name: 'Welcome to OpenCut', exact: true });
  if (await welcome.isVisible()) await welcome.getByRole('button', { name: 'Close', exact: true }).click();
};
const library = () => page.locator('.editor-media-library');
const afterPaint = () => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
const undo = async (name, direction = 'undo', viaKey = false) => stage(name, async () => {
  let receipt;
  const tool = `opencut_edit_${direction}_apply`;
  if (state.requests[name]) await call(tool, state.requests[name]);
  else {
    await page.route(`**/api/tools/${tool}`, async route => {
      state.requests[name] = route.request().postDataJSON(); await save();
      const response = await route.fetch(); receipt = (await response.json()).data; await route.fulfill({ response });
    });
    try {
      if (viaKey) {
        await expect(page.getByRole('dialog', { name: 'Remove media from project', exact: true })).toHaveCount(0);
        await expect(page.locator('.save-status')).toHaveText('Saved');
        await expect(page.getByRole('region', { name: 'History', exact: true }).getByRole('button', { name: direction === 'undo' ? 'Undo' : 'Redo', exact: true })).toBeEnabled();
        await library().focus(); await expect(library()).toBeFocused();
        await page.keyboard.press(direction === 'undo' ? 'Meta+z' : 'Meta+Shift+z');
      } else await page.getByRole('region', { name: 'History', exact: true }).getByRole('button', { name: direction === 'undo' ? 'Undo' : 'Redo', exact: true }).click();
      await page.getByRole('region', { name: 'History plan', exact: true }).getByRole('button', { name: `Apply ${direction}`, exact: true }).click();
      await expect.poll(() => !!receipt).toBe(true);
    } finally { await page.unroute(`**/api/tools/${tool}`); }
  }
  await expect(page.locator('.save-status')).toHaveText('Saved'); return inspect();
});
const reviewRemoval = async (ids, chord) => {
  for (const asset of (await inspect()).project.assets)
    await page.getByRole('checkbox', { name: `Select media ${asset.name}`, exact: true }).setChecked(ids.includes(asset.id));
  await library().focus();
  const response = page.waitForResponse(r => r.url().endsWith('/opencut_media_remove_plan'));
  await page.keyboard.press(chord);
  const result = await (await response).json(); expect(result.ok).not.toBe(false);
  const modal = page.getByRole('dialog', { name: 'Remove media from project', exact: true });
  await expect(modal).toBeVisible(); return { modal, transaction: result.data };
};
try {
  await open();
  await stage('help-inventory-focus-and-navigation', async () => {
    await library().focus(); await page.keyboard.press('Shift+/');
    const modal = page.getByRole('dialog', { name: 'Keyboard shortcuts', exact: true });
    await expect(modal).toBeVisible();
    const inventory = [];
    for (const action of ACTIONS) {
      const run = modal.getByRole('button', { name: `Run: ${action.label}`, exact: true });
      await expect(run).toHaveCount(1);
      const reason = await run.getAttribute('title');
      expect(reason).not.toBe('This action is not available in the current editor view.');
      inventory.push({ id: action.id, label: action.label, help: true, default_keys: action.keys, registered: true, enabled_in_initial_context: await run.isEnabled(), reason });
    }
    const before = await inspect();
    await page.keyboard.press('Backspace'); await page.keyboard.press('s');
    expect((await inspect()).project).toEqual(before.project);
    await modal.getByRole('button', { name: 'Close', exact: true }).click();
    const search = page.getByRole('textbox', { name: 'Search media', exact: true });
    const reads = calls.filter(n => n === 'opencut_edit_timeline_plan' || n === 'opencut_edit_timeline_plan').length;
    await search.fill(''); await search.press('s'); expect(await search.inputValue()).toBe('s'); await search.fill('');
    expect(calls.filter(n => n === 'opencut_edit_timeline_plan' || n === 'opencut_edit_timeline_plan').length).toBe(reads);
    const timeline = page.locator('.editor-timeline-panel'); await timeline.focus(); await expect(timeline).toBeFocused();
    const cursor = page.getByRole('slider', { name: 'Scene playhead', exact: true });
    await page.keyboard.press('Home'); await page.keyboard.press('ArrowRight');
    await expect(cursor).toHaveAttribute('aria-valuenow', String(1 / 30));
    await page.keyboard.press('Home'); await page.keyboard.press('ArrowLeft'); await expect(cursor).toHaveAttribute('aria-valuenow', '0');
    await page.keyboard.press('End'); await expect(cursor).toHaveAttribute('aria-valuenow', '0.25');
    await page.keyboard.press('Home');
    expect((await inspect()).project).toEqual(before.project);
    return { inventory, action_count: inventory.length, input_modal_guards: true, exact_frame_step: '1/30', navigation_writes: 0, platform: await page.evaluate(() => navigator.platform) };
  });
  await stage('held-pan-common-scene-coordinates', async () => {
    const before = await inspect(), canvas = page.locator('.editor-canvas'), scene = page.locator('.editor-viewport-scene');
    const planCalls = calls.filter(n => n === 'opencut_media_remove_plan').length;
    await canvas.focus(); await page.keyboard.press('Alt+r'); await afterPaint();
    expect(calls.filter(n => n === 'opencut_media_remove_plan').length).toBe(planCalls);
    await page.getByRole('combobox', { name: 'Preview zoom', exact: true }).selectOption('200');
    await canvas.focus(); const box = await canvas.boundingBox(), start = await scene.boundingBox();
    await page.keyboard.down('h'); await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2); await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 + 35, box.y + box.height / 2 + 20, { steps: 3 });
    await expect(canvas).toHaveAttribute('data-panning', 'true');
    const shifted = await scene.boundingBox(); expect(Math.abs(shifted.x - start.x)).toBeGreaterThan(1);
    const overlay = await page.locator('.editor-canvas-transform').boundingBox();
    for (const k of ['x', 'y', 'width', 'height']) expect(Math.abs(overlay[k] - shifted[k])).toBeLessThan(0.01);
    await page.keyboard.up('h'); await expect(canvas).not.toHaveAttribute('data-panning');
    await page.mouse.move(box.x + box.width / 2 + 50, box.y + box.height / 2 + 30);
    expect(await scene.boundingBox()).toEqual(shifted); await page.mouse.up();
    await page.getByRole('button', { name: 'Fit preview', exact: true }).click();
    expect((await inspect()).project).toEqual(before.project);
    return { pan_delta: { x: shifted.x - start.x, y: shifted.y - start.y }, common_overlay_bounds: true, released_on_keyup: true, project_writes: 0 };
  });
  const bound = await stage('bind-existing-short-source', async () => {
    const current = await inspect(), media = await call('opencut_project_media_query', { project_id: project, limit: 50, cursor: null });
    const short = media.media.find(m => m.asset.version.content_hash === 'a4610ab2402eb8a260dcfe62596cd896a911d9c7eab7b85fd95064d548fcab51'); expect(short).toBeTruthy();
    state.plans.bind ??= { project_id: project, expected_revision: current.revision, sequence_id: sequence, idempotency_key: 'f54-bind-short', label: 'F54 existing short binding', bindings: [{ media_id: short.media_id, version: short.asset.version, asset_id: 'f54-short-source', name: short.asset.name, mode: 'bind' }], placements: [], new_tracks: [], ripple: false };
    await save(); state.requests.bind ??= (await call('opencut_media_import_plan', state.plans.bind)).transaction; await save();
    await call('opencut_edit_apply', state.requests.bind); return inspect();
  });
  await open();
  await stage('single-remove-cancel-save-replay', async () => {
    const before = await inspect();
    let review = await reviewRemoval(['f54-short-source'], 'Alt+r');
    await review.modal.getByRole('button', { name: 'Cancel', exact: true }).click(); expect(await inspect()).toEqual(before);
    review = await reviewRemoval(['f54-short-source'], 'Alt+r');
    state.requests['single-removal'] = review.transaction; await save();
    await review.modal.getByRole('button', { name: 'Confirm media removal', exact: true }).click();
    await expect(review.modal).toHaveCount(0);
    await expect.poll(async () => (await inspect()).project.assets.some(a => a.id === 'f54-short-source')).toBe(false);
    const after = await inspect();
    expect(after.project.sequences).toEqual(before.project.sequences);
    const receipt = await call('opencut_edit_apply', review.transaction); expect(receipt.revision).toBe(after.revision);
    expect(await inspect()).toEqual(after);
    return { revision: after.revision, original_request_replay: true, cancelled_plan_writes: 0, scenes_unchanged: true };
  });
  await undo('undo-single-removal', 'undo', true);
  await stage('two-scene-lock-fixture', async () => {
    const s = structuredClone(original.project.sequences[0]), sourceTrack = structuredClone(s.tracks[0]), sourceVideo = structuredClone(sourceTrack.elements.find(e => e.content.kind === 'video'));
    const video = (id, start) => ({ ...structuredClone(sourceVideo), id, name: id, channels: [], timeline_range: { start: t(start), duration: t(.25) }, source_span: { profile: 'classic_trimmed', trim_start: t(1), trim_end: t(298.75), source_duration: t(300) }, content: { ...structuredClone(sourceVideo.content), retime: { ...sourceVideo.content.retime, rate: { numerator: 1, denominator: 1 } } } });
    const track = { ...sourceTrack, id: 'f54-peer-video', name: 'F54 locked peer video', locked: true, channels: [], elements: [video('f54-peer-left', 0), video('f54-peer-right', .25)] };
    const captionTrack = { ...sourceTrack, id: 'f54-peer-captions', name: 'F54 retained captions', kind: 'caption', elements: [], channels: [], locked: false };
    const peer = { id: 'f54-peer-scene', name: 'F54 dependent peer', main_track_id: track.id, tracks: [track, captionTrack], bookmarks: [],
      transitions: [{ id: 'f54-peer-transition', outgoing_element_id: 'f54-peer-left', incoming_element_id: 'f54-peer-right', duration: t(.125), transition: 'legacy_crossfade_v3' }],
      captions: [{ id: 'f54-peer-caption', track_id: captionTrack.id, timeline_range: { start: t(0), duration: t(.5) }, text: 'Retain caption text', source_anchor: { asset_id: 'long-source', source_range: { start: t(298.75), duration: t(.25) } },
        style: { font: { kind: 'unresolved_family', family: 'Arial', policy: { mode: 'strict' } }, font_size: { unit: 'classic_height90', value: 15 }, color: { encoding: 'srgb', red: 1, green: 1, blue: 1, alpha: 1 }, background_color: null, max_width_px: null, line_height_px: null, text_align: 'center', layout: 'legacy_caption_v2' } }] };
    return edit('apply-peer-fixture-unit-rate', [{ type: 'sequence_put', sequence: peer }]);
  });
  await stage('locked-missing-duplicate-and-stale-plan-guards', async () => {
    const before = await inspect();
    const request = { project_id: project, expected_revision: before.revision, idempotency_key: 'f54-guard', label: 'F54 protected removal', asset_ids: ['long-source', 'f54-short-source'] };
    const fail = async (input, code) => { let e; try { await call('opencut_media_remove_plan', input); } catch (error) { e = error; } expect(e?.code).toBe(code); expect(await inspect()).toEqual(before); };
    await fail(request, 'CONSTRAINT_VIOLATION');
    await fail({ ...request, asset_ids: ['long-source', 'long-source'] }, 'INVALID_ARGUMENT');
    await fail({ ...request, asset_ids: ['not-bound'] }, 'NOT_FOUND');
    await fail({ ...request, expected_revision: original.revision }, 'REVISION_CONFLICT');
    return { locked_atomic: true, missing_duplicate_stale: true, writes: 0 };
  });
  await edit('unlock-peer-fixture', [{ type: 'track_lock_set', sequence_id: 'f54-peer-scene', track_id: 'f54-peer-video', locked: false }]);
  await open();
  const fixture = await stage('captured-unlocked-fixture', inspect);
  await stage('plural-remove-all-scenes-caption-transition', async () => {
    const before = await inspect(), review = await reviewRemoval(['long-source', 'f54-short-source'], 'Alt+Shift+r');
    state.requests['plural-removal'] = review.transaction; await save();
    await review.modal.getByRole('button', { name: 'Confirm media removal', exact: true }).click();
    await expect.poll(async () => (await inspect()).project.assets.length).toBe(0);
    await expect(review.modal).toHaveCount(0); await expect(page.locator('.save-status')).toHaveText('Saved');
    const after = await inspect();
    for (const s of after.project.sequences) {
      expect(s.tracks.flatMap(t => t.elements)).toHaveLength(0); expect(s.transitions).toHaveLength(0);
    }
    const caption = structuredClone(before.project.sequences.find(s => s.id === 'f54-peer-scene').captions[0]); caption.source_anchor = null;
    expect(after.project.sequences.find(s => s.id === 'f54-peer-scene').captions).toEqual([caption]);
    expect(after.project.settings).toEqual(before.project.settings);
    expect(after.project.fonts).toEqual(before.project.fonts);
    const receipt = await call('opencut_edit_apply', review.transaction); expect(receipt.revision).toBe(after.revision);
    return { revision: after.revision, asset_count: 0, scene_count: after.project.sequences.length, caption_text_retained: true, transitions_removed: true, original_request_replay: true };
  });
  const restored = await undo('undo-plural-removal', 'undo', true); expect(restored.project).toEqual(fixture.project);
  await undo('redo-plural-removal', 'redo', true);
  const restoredAgain = await undo('undo-redone-plural-removal', 'undo', true); expect(restoredAgain.project).toEqual(fixture.project);
  await undo('undo-unlock-fixture');
  // This deliberately returns the peer to locked. Undoing its creation is
  // correctly refused by the existing lock fence; explicitly unlock and remove
  // only our fixture, preserving all original content and retained history.
  await edit('restore-owned-f54-fixture', [
    { type: 'track_lock_set', sequence_id: 'f54-peer-scene', track_id: 'f54-peer-video', locked: false },
    { type: 'sequence_delete', sequence_id: 'f54-peer-scene' },
    { type: 'asset_delete', asset_id: 'f54-short-source' },
  ]);
  await open();
  await stage('repeat-and-disabled-shortcuts', async () => {
    const before = await inspect(), timeline = page.locator('.editor-timeline-panel');
    await timeline.focus(); await expect(timeline).toBeFocused();
    const snap = page.getByRole('checkbox', { name: 'Snap to element edges', exact: true }), initial = await snap.isChecked();
    await page.keyboard.down('n'); await expect(snap).toBeChecked({ checked: !initial });
    await page.evaluate(() => new Promise(r => setTimeout(r, 45)));
    await page.keyboard.down('n'); await expect(snap).toBeChecked({ checked: !initial });
    await page.keyboard.up('n'); await page.keyboard.press('n'); await expect(snap).toBeChecked({ checked: initial });
    const cursor = page.getByRole('slider', { name: 'Scene playhead', exact: true });
    await page.keyboard.press('Home'); await page.keyboard.down('ArrowRight');
    await expect(cursor).toHaveAttribute('aria-valuenow', String(1 / 30));
    await page.evaluate(() => new Promise(r => setTimeout(r, 45)));
    await page.keyboard.down('ArrowRight'); await expect(cursor).toHaveAttribute('aria-valuenow', String(2 / 30));
    await page.keyboard.up('ArrowRight'); await page.keyboard.press('Home');
    await page.keyboard.press('Escape');
    const count = calls.filter(n => /opencut_edit_timeline_plan|opencut_edit_apply/.test(n)).length;
    await page.keyboard.press('Backspace'); await afterPaint();
    expect(calls.filter(n => /opencut_edit_timeline_plan|opencut_edit_apply/.test(n)).length).toBe(count);
    expect(await inspect()).toEqual(before);
    return { physical_browser_repeat_events: true, nonrepeat_toggle_once: true, repeat_navigation_twice: true, disabled_delete_no_fallthrough: true, writes: 0 };
  });
  await stage('complete-original-reopen', async () => {
    await open(); expect((await inspect()).project).toEqual(original.project);
    return { complete_project_equal: true, reloaded: true };
  });
  expect(errors).toEqual([]);
  const report = { status: 'passed_scoped_f54_actions', candidate: args.candidate, stages: Object.keys(state.stages), original_project_restored: true,
    repeat_guards: state.stages['repeat-and-disabled-shortcuts'], help_inventory: state.stages['help-inventory-focus-and-navigation'], pan: state.stages['held-pan-common-scene-coordinates'], removal_guards: state.stages['locked-missing-duplicate-and-stale-plan-guards'],
    single: state.stages['single-remove-cancel-save-replay'], plural: state.stages['plural-remove-all-scenes-caption-transition'], page_errors: errors,
    new_jobs: 0, new_sources: 0, new_project_copies: 0, unit_tests: 0, scope: 'Actual missing V4 action registrations,help inventory,Mac modifiers,input/modal/scope guards,navigation,panning and media removal/history. Existing declared action behaviors and held/repeat controls retain source-bound earlier evidence;not physical OS/IME/all-browser/A7/D8.' };
  await fs.writeFile(args.report, JSON.stringify(report, null, 2) + '\n'); console.log(JSON.stringify({ status: report.status, stages: report.stages.length }));
} catch (error) {
  state.last_error = error.stack; await save(); await page.screenshot({ path: path.join(args.workspace, 'f54-actions-failure.png'), fullPage: true }).catch(() => {}); throw error;
} finally { await browser.close(); }
