import { editorSnapshot } from "./project-inspection.mjs";
/** F40/F41: one retained scene; actual input, atomic plans and exact history. */
import { chromium, expect } from '@playwright/test';
import fs from 'node:fs/promises';
import path from 'node:path';
import { OpenCutClient } from '../../../packages/sdk-ts/dist/index.js';

const args = Object.fromEntries(process.argv.slice(2).reduce((a, v, i, s) => i % 2 ? a : [...a, [v.slice(2), s[i + 1]]], []));
const { workspace, endpoint, report } = args;
const file = path.join(workspace, 'agent-selection-ripple-v4-checkpoint.json');
const state = JSON.parse(await fs.readFile(file, 'utf8'));
state.executions ??= {}; state.plans ??= {};
const fixture = state.stages.fixture, project = fixture.project.id, sequence = fixture.project.sequences[0].id;
const client = new OpenCutClient({ baseUrl: endpoint, token: (await fs.readFile(args.credential, 'utf8')).trim() });
const call = (name, input) => client.call(name, input);
const inspect = () => editorSnapshot(call('opencut_project_inspect', { project_id: project, revision: null }));
const seconds = t => Number(t.ticks) / t.timebase;
const time = n => ({ ticks: String(Math.round(n * 120000)), timebase: 120000 });
const seq = s => s.project.sequences[0];
const target = (track_id, element_id) => ({ track_id, element_id });
const A = target('f40-t0', 'f40-a'), B = target('f40-t1', 'f40-b');
const save = async () => { await fs.writeFile(file + '.pending', JSON.stringify(state, null, 2) + '\n'); await fs.rename(file + '.pending', file); };
const stage = async (name, run) => {
  if (state.stages[name]) return state.stages[name];
  console.log('Begin ' + name); const value = await run(); state.stages[name] = value; await save(); console.log('Passed ' + name); return value;
};
const gcd = (a, b) => b === 0n ? a : gcd(b, a % b);
const canonical = value => {
  if (Array.isArray(value)) return value.map(canonical);
  if (!value || typeof value !== 'object') return value;
  if (Object.keys(value).length === 2 && 'ticks' in value && 'timebase' in value) {
    const n = BigInt(value.ticks), d = BigInt(value.timebase), g = gcd(n < 0n ? -n : n, d); return `${n / g}/${d / g}`;
  }
  return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, canonical(v)]));
};
const graph = (actual, expected) => expect(canonical(actual)).toEqual(canonical(expected));
const node = (snapshot, id) => seq(snapshot).tracks.flatMap(t => t.elements).find(e => e.id === id) ?? seq(snapshot).captions.find(c => c.id === id);
const historyAt = revision => call('opencut_project_history', { project_id: project, from_revision: revision, limit: 100 });
const execute = async (name, tool, input) => {
  if (state.executions[name]) expect(state.executions[name]).toEqual({ tool, input });
  else { state.executions[name] = { tool, input }; await save(); }
  return call(tool, input);
};
const browser = await chromium.launch({ headless: true, executablePath: process.env.OPENCUT_CHROME });
const page = await browser.newPage({ locale: 'en-US', viewport: { width: 1440, height: 1300 } });
page.setDefaultTimeout(20000);
const errors = [], writes = [], planCalls = [];
page.on('pageerror', e => errors.push(e.message));
page.on('request', r => {
  if (/\/api\/tools\/opencut_edit_(apply|undo_apply|redo_apply)$/.test(r.url())) writes.push(r.postDataJSON());
  if (r.url().endsWith('/opencut_edit_timeline_plan')) planCalls.push(r.postDataJSON());
});
const panel = page.locator('.editor-timeline-panel');
const ready = () => expect(page.locator('.save-status')).toHaveText('Saved', { timeout: 30000 });
const open = async () => {
  await page.goto(`${endpoint}/editor/${project}`);
  await expect(page.getByRole('main', { name: 'OpenCut editor', exact: true })).toBeVisible();
  const welcome = page.getByRole('dialog', { name: 'Welcome to OpenCut', exact: true });
  if (await welcome.isVisible()) await welcome.getByRole('button', { name: 'Close', exact: true }).click();
  await ready();
};
const selected = () => page.locator('[data-element-id][data-selected="true"]').evaluateAll(ns => ns.map(n => n.dataset.elementId));
const select = async (ids) => {
  await ready(); await panel.focus(); await page.keyboard.press('Escape');
  for (let i = 0; i < ids.length; i++) await page.locator(`[data-element-id="${ids[i]}"] .editor-element-select`).click({ position: { x: 8, y: 10 }, modifiers: i ? ['ControlOrMeta'] : [] });
  await expect.poll(async () => (await selected()).sort()).toEqual([...ids].sort());
};
const mode = async value => { const box = panel.getByRole('checkbox', { name: 'Ripple affected tracks', exact: true }); await box.setChecked(value); };
const controlled = async () => {
  const zoom = panel.getByRole('slider', { name: 'Timeline zoom', exact: true });
  await zoom.focus(); await page.keyboard.press('Home'); for (let i = 0; i < 9; i++) await page.keyboard.press('ArrowRight');
  await expect(zoom).toHaveValue('1'); await panel.getByRole('checkbox', { name: 'Snap to element edges', exact: true }).uncheck();
};
const hit = async id => {
  const locator = page.locator(`[data-element-id="${id}"] .editor-element-select`); await locator.scrollIntoViewIfNeeded();
  return locator.evaluate(e => {
    const r = e.getBoundingClientRect(), x = r.left + Math.min(35, r.width / 2), y = r.top + 12, actual = document.elementFromPoint(x, y);
    if (!e.contains(actual)) throw Error('Physical input does not hit the visible timeline element'); return { x, y };
  });
};
const drag = async (id, dx, dy, cancel = false) => {
  const p = await hit(id); await page.mouse.move(p.x, p.y); await page.mouse.down(); await page.mouse.move(p.x + dx, p.y + dy, { steps: 5 });
  if (cancel) await page.keyboard.press('Escape'); await page.mouse.up();
};
const review = () => page.getByRole('dialog', { name: 'Review timeline changes', exact: true });
const discard = async () => { await review().getByRole('button', { name: 'Discard timeline changes', exact: true }).click(); await expect(review()).not.toBeVisible(); };
const apply = async () => { await review().getByRole('button', { name: 'Apply timeline changes', exact: true }).click(); await review().getByRole('button', { name: 'Done', exact: true }).click(); await ready(); };
const ui = (name, tool, run) => stage(name, async () => {
  if (state.executions[name]) { const { tool: original, input } = state.executions[name]; expect(original).toBe(tool); const receipt = await call(tool, input); expect((await inspect()).revision).toBe(receipt.transaction?.revision ?? receipt.revision); await open(); return inspect(); }
  if (state.plans[name]?.input) {
    const original = state.plans[name];
    if (!original.reply) { original.reply = await call('opencut_edit_timeline_plan', original.input); await save(); }
    expect(original.reply.transaction).toBeTruthy();
    await execute(name, tool, original.reply.transaction);
    state.resumed_original_ui_plans ??= {}; state.resumed_original_ui_plans[name] = { original_request_reused: true, public_apply_recovery: true, no_new_ui_intent_or_key: true }; await save();
    await open(); return inspect();
  }
  let receipt, count = 0;
  await page.route('**/api/tools/opencut_edit_timeline_plan', async route => {
    const input = route.request().postDataJSON(); state.plans[name] = { input }; await save();
    const response = await route.fetch(), body = await response.json(); expect(body.error).toBeNull(); state.plans[name].reply = body.data; await save(); await route.fulfill({ response });
  });
  await page.route('**/api/tools/' + tool, async route => {
    count++; const input = route.request().postDataJSON(); state.executions[name] = { tool, input }; await save();
    const response = await route.fetch(), body = await response.json(); expect(body.error).toBeNull(); receipt = body.data; await route.fulfill({ response });
  });
  try { await run(); await expect.poll(() => !!receipt, { timeout: 30000 }).toBe(true); await ready(); expect(count).toBe(1); const after = await inspect(); expect(after.revision).toBe(receipt.transaction?.revision ?? receipt.revision); return after; }
  finally { await page.unroute('**/api/tools/' + tool); await page.unroute('**/api/tools/opencut_edit_timeline_plan'); }
});
const history = (name, redo = false) => ui(name, redo ? 'opencut_edit_redo_apply' : 'opencut_edit_undo_apply', async () => {
  await page.getByRole('region', { name: 'History', exact: true }).getByRole('button', { name: redo ? 'Redo' : 'Undo', exact: true }).click();
  await page.getByRole('region', { name: 'History plan', exact: true }).getByRole('button', { name: redo ? 'Apply redo' : 'Apply undo', exact: true }).click();
});
const setupLock = (name, track_id) => stage(name, async () => {
  const before = await inspect(); const original = state.executions[name]?.input;
  await execute(name, 'opencut_edit_apply', original ?? { project_id: project, expected_revision: before.revision, idempotency_key: name, label: 'One atomic lock guard', operations: [{ type: 'track_lock_set', sequence_id: sequence, track_id, locked: true }] });
  await open(); return inspect();
});
const splitForm = async (retain, ripple, at = 1.8) => {
  await page.getByRole('button', { name: 'Split selection', exact: true }).click();
  const d = page.getByRole('dialog', { name: 'Split selection', exact: true });
  await expect(d.getByRole('checkbox', { name: 'Ripple affected tracks', exact: true })).toBeChecked({ checked: ripple });
  await d.getByRole('textbox', { name: 'Boundary (scene seconds)', exact: true }).fill(String(at));
  await d.getByRole('combobox', { name: 'Keep after split', exact: true }).selectOption(retain);
  await d.getByRole('button', { name: 'Review timeline changes', exact: true }).click();
};
const seek = async at => { const ruler = panel.getByRole('slider', { name: 'Scene playhead', exact: true }); await ruler.click({ position: { x: at * 80, y: 8 } }); await expect(ruler).toHaveAttribute('aria-valuenow', String(at)); };
const sample = (channel, at) => {
  const keys = channel.keys; if (at <= seconds(keys[0].time)) return keys[0].value;
  for (let i = 1; i < keys.length; i++) { const a = keys[i - 1], b = keys[i]; if (at <= seconds(b.time)) return a.value + (b.value - a.value) * (at - seconds(a.time)) / (seconds(b.time) - seconds(a.time)); }
  return keys.at(-1).value;
};
const splitExpected = (actual, name, retain, ripple, targets) => {
  const expected = structuredClone(fixture.project), request = state.plans[name].input.command;
  expect(request.kind).toBe('split'); expect(request.retain).toBe(retain); expect(request.ripple).toBe(ripple); expect(seconds(request.at)).toBe(1.8);
  for (const id of targets) {
    const original = node(fixture, id), source = request.targets.find(t => t.target.element_id === id), right = retain === 'right';
    const element = structuredClone(original), start = seconds(original.timeline_range.start), duration = seconds(original.timeline_range.duration), leftDuration = 1.8 - start;
    element.id = right ? source.right_element_id : id;
    element.timeline_range = { start: time(right && !ripple ? 1.8 : start), duration: time(right ? start + duration - 1.8 : leftDuration) };
    const found = node(actual, element.id); expect(found).toBeTruthy();
    const owner = expected.sequences[0].tracks.find(t => t.elements.some(e => e.id === id));
    if (owner) {
      element.name = original.name + (right ? ' (right)' : ' (left)');
      const rate = original.content.retime.rate.numerator / original.content.retime.rate.denominator;
      if (right) element.source_span.trim_start = time(seconds(original.source_span.trim_start) + leftDuration * rate);
      else element.source_span.trim_end = time(seconds(original.source_span.trim_end) + (duration - leftDuration) * rate);
      expect(found.channels.length).toBe(original.channels.length);
      for (let i = 0; i < found.channels.length; i++) {
        expect(found.channels[i].binding).toEqual(original.channels[i].binding);
        expect(found.channels[i].target).toEqual({ kind: 'element', element_id: element.id });
        for (const at of [0, seconds(element.timeline_range.duration) / 2, seconds(element.timeline_range.duration)]) expect(sample(found.channels[i], at)).toBeCloseTo(-12 + 6 * (at + (right ? leftDuration : 0)), 9);
      }
      element.channels = found.channels; owner.elements[owner.elements.findIndex(e => e.id === id)] = element;
      if (ripple) for (const e of owner.elements) if (e.id !== element.id && Math.round(seconds(e.timeline_range.start) * 120000) >= Math.round((start + duration) * 120000)) e.timeline_range.start = time(seconds(e.timeline_range.start) - (right ? leftDuration : duration - leftDuration));
    } else {
      const cues = expected.sequences[0].captions, index = cues.findIndex(c => c.id === id); cues[index] = element;
      if (ripple) for (const c of cues) if (c.id !== element.id && c.track_id === element.track_id && Math.round(seconds(c.timeline_range.start) * 120000) >= Math.round((start + duration) * 120000)) c.timeline_range.start = time(seconds(c.timeline_range.start) - (right ? leftDuration : duration - leftDuration));
    }
  }
  graph(actual.project, expected);
};

try {
  await open();
  await stage('corrected-original-read-only-plans', async () => {
    const head = await inspect(); expect(head.revision).toBe(fixture.revision); const records = {};
    for (const name of ['off_grid_group', 'main_pin_group']) {
      const plan = await call('opencut_edit_timeline_plan', state.requests[name]);
      const proposed = plan.transaction ? await call('opencut_edit_plan', plan.transaction) : head.project;
      if (name === 'off_grid_group') { const elements = proposed.sequences[0].tracks.flatMap(t => t.elements); expect(seconds(elements.find(e => e.id === 'f40-a').timeline_range.start)).toBe(1.5); expect(seconds(elements.find(e => e.id === 'f40-b').timeline_range.start)).toBe(1.75); }
      else { expect(plan.transaction).toBeNull(); graph(proposed, fixture.project); }
      records[name] = { transaction_needed: !!plan.transaction, decision: plan.decision };
    }
    expect(await inspect()).toEqual(head); return { records, original_requests_reused: true, no_write: true };
  });
  await stage('selection-and-ripple-view-only', async () => {
    const before = await inspect(), h = await historyAt(before.revision), n = writes.length, p = planCalls.length;
    await select(['f40-a']); await page.locator('[data-element-id="f40-b"] .editor-element-select').click({ position: { x: 8, y: 10 }, modifiers: ['Shift'] });
    await expect.poll(selected).toEqual(['f40-a', 'f40-a-next', 'f40-b']);
    await page.locator('[data-element-id="f40-a-next"] .editor-element-select').click({ position: { x: 8, y: 10 }, modifiers: ['ControlOrMeta'] });
    await expect.poll(selected).toEqual(['f40-a', 'f40-b']);
    await panel.focus(); await page.keyboard.press('ControlOrMeta+a');
    await expect.poll(async () => (await selected()).sort()).toEqual([...seq(fixture).tracks.flatMap(t => t.elements.map(e => e.id)), ...seq(fixture).captions.map(c => c.id)].sort());
    await page.keyboard.press('Escape'); await expect.poll(selected).toEqual([]);
    await mode(true); await select(['f40-a']); await page.getByRole('button', { name: 'Split selection', exact: true }).click();
    const d = page.getByRole('dialog', { name: 'Split selection', exact: true }), box = d.getByRole('checkbox', { name: 'Ripple affected tracks', exact: true });
    await expect(box).toBeChecked(); await box.uncheck(); await d.getByRole('button', { name: 'Close', exact: true }).click();
    await expect(panel.getByRole('checkbox', { name: 'Ripple affected tracks', exact: true })).not.toBeChecked();
    await mode(true); await open(); await expect(panel.getByRole('checkbox', { name: 'Ripple affected tracks', exact: true })).toBeChecked();
    expect(writes.length).toBe(n); expect(planCalls.length).toBe(p); expect(await inspect()).toEqual(before); expect(await historyAt(before.revision)).toEqual(h);
    return { range_includes_middle: true, modifier_toggle: true, select_all_escape: true, shared_form_toolbar_mode: true, reload_persistent: true, project_history_writes: 0 };
  });
  await controlled(); await mode(false);
  await stage('physical-box-and-cancel-no-write', async () => {
    await select(['f40-reference']); const before = await inspect(), h = await historyAt(before.revision), n = writes.length, p = planCalls.length;
    const lane0 = page.locator('[data-file-track="f40-t0"]'), lane1 = page.locator('[data-file-track="f40-t1"]'); await lane0.scrollIntoViewIfNeeded();
    const a = await lane0.boundingBox(), b = await lane1.boundingBox(); const start = { x: a.x + 35, y: a.y + 6 }, end = { x: a.x + 245, y: b.y + b.height - 6 };
    expect(await page.evaluate(p => document.elementFromPoint(p.x, p.y)?.getAttribute('data-file-track'), start)).toBe('f40-t0');
    for (const cancel of [true, false]) {
      await page.mouse.move(start.x, start.y); await page.mouse.down(); await page.mouse.move(end.x, end.y, { steps: 5 }); await expect(page.locator('.editor-selection-box')).toBeVisible();
      if (cancel) await page.keyboard.press('Escape'); await page.mouse.up(); await expect.poll(async () => (await selected()).sort()).toEqual(cancel ? ['f40-reference'] : ['f40-a', 'f40-b']);
    }
    await drag('f40-a', 40, 80, true); await expect.poll(selected).toEqual(['f40-a', 'f40-b']);
    expect(writes.length).toBe(n); expect(planCalls.length).toBe(p); expect(await inspect()).toEqual(before); expect(await historyAt(before.revision)).toEqual(h);
    return { actual_box_selects_two_tracks: true, escape_restores_selection: true, cancelled_group_drag_no_plan: true, head_history_unchanged: true };
  });
  const moved = await ui('physical-rigid-group-apply', 'opencut_edit_apply', async () => { await select(['f40-a', 'f40-b']); await drag('f40-a', 40, 80); await apply(); });
  const expectedMove = structuredClone(fixture.project), tracks = expectedMove.sequences[0].tracks;
  const a = tracks[1].elements.shift(), b = tracks[2].elements.shift(); a.timeline_range.start = time(1.5); b.timeline_range.start = time(1.75); tracks[2].elements.push(a); tracks[3].elements.push(b); graph(moved.project, expectedMove);
  expect(state.plans['physical-rigid-group-apply'].reply.transaction).toEqual(state.executions['physical-rigid-group-apply'].input);
  graph((await history('group-undo')).project, fixture.project); graph((await history('group-redo', true)).project, expectedMove); graph((await history('group-redo-undo')).project, fixture.project);
  await open(); await controlled();
  await setupLock('locked-group-destination', 'f40-t2');
  await stage('whole-group-lock-rejection', async () => {
    const before = await inspect(), n = writes.length, p = planCalls.length; await select(['f40-a', 'f40-b']); await drag('f40-a', 40, 80);
    await expect(panel.getByRole('alert')).toHaveText('The destination track is locked.'); expect(writes.length).toBe(n); expect(planCalls.length).toBe(p); expect(await inspect()).toEqual(before);
    const input = { ...state.requests.off_grid_group, expected_revision: before.revision, idempotency_key: 'f40-lock-reject-native' };
    await expect(call('opencut_edit_timeline_plan', input)).rejects.toThrow(); expect(await inspect()).toEqual(before);
    return { physical_client_guard: true, native_whole_batch_guard: true, no_partial_movement: true };
  });
  graph((await history('locked-destination-undo')).project, fixture.project); await open(); await controlled();
  await stage('stale-group-plan-atomic-conflict', async () => {
    if (!state.stale_plan) {
      await select(['f40-a', 'f40-b']);
      const [response] = await Promise.all([page.waitForResponse(r => r.url().endsWith('/opencut_edit_timeline_plan')), drag('f40-a', 40, 80)]);
      const envelope = await response.json(); expect(envelope.error).toBeNull(); state.stale_plan = envelope.data; await save();
    }
    const planned = state.stale_plan, before = await inspect(), track = structuredClone(seq(before).tracks.find(t => t.id === 'f40-t3')); track.name = 'F40 external head change';
    await execute('external-head-change', 'opencut_edit_apply', state.executions['external-head-change']?.input ?? { project_id: project, expected_revision: before.revision, idempotency_key: 'f40-one-external-head-change', label: 'F40 competing external revision', operations: [{ type: 'track_put', sequence_id: sequence, track }] });
    const external = await inspect(); let code;
    try { await execute('stale-group-apply', 'opencut_edit_apply', planned.transaction); } catch (e) { code = e.code; }
    expect(code).toBe('REVISION_CONFLICT'); expect(await inspect()).toEqual(external);
    const expected = structuredClone(fixture.project); expected.sequences[0].tracks.find(t => t.id === 'f40-t3').name = track.name; graph(external.project, expected);
    if (await review().isVisible()) await discard(); await open(); return { fixed_plan_revision: planned.transaction.expected_revision, external_revision: external.revision, error: code, complete_graph_unmoved: true, external_only_change: true };
  });
  graph((await history('external-head-change-undo')).project, fixture.project); await open(); await controlled();
  const cases = [
    { name: 'left-off-form', retain: 'left', ripple: false },
    { name: 'left-on-shortcut', retain: 'left', ripple: true, key: 'w' },
    { name: 'right-off-shortcut', retain: 'right', ripple: false, key: 'q' },
    { name: 'right-on-form', retain: 'right', ripple: true },
    { name: 'multi-track-left-on', retain: 'left', ripple: true, ids: ['f40-a', 'f40-b'] },
    { name: 'caption-media-right-on', retain: 'right', ripple: true, ids: ['f40-a', 'f39-legacy-caption'] },
  ];
  for (const c of cases) {
    const name = 'split-' + c.name, ids = c.ids ?? ['f40-a'];
    const after = await ui(name, 'opencut_edit_apply', async () => {
      await mode(c.ripple); await select(ids);
      if (c.key) { await seek(1.8); await panel.focus(); await page.keyboard.press(c.key); }
      else await splitForm(c.retain, c.ripple);
      await apply();
    });
    splitExpected(after, name, c.retain, c.ripple, ids); graph((await history(name + '-undo')).project, fixture.project);
  }
  await open(); await controlled();
  await stage('boundary-splits-are-noops', async () => {
    const before = await inspect(), h = await historyAt(before.revision), n = writes.length;
    for (const [at, key] of [[1, 'q'], [3, 'w']]) {
      await mode(true); await select(['f40-a']); await seek(at);
      const [response] = await Promise.all([page.waitForResponse(r => r.url().endsWith('/opencut_edit_timeline_plan')), panel.focus().then(() => page.keyboard.press(key))]);
      const body = await response.json(); expect(body.error).toBeNull(); expect(body.data.transaction).toBeNull();
      await expect(review().getByRole('status')).toHaveText('The plan makes no changes.'); await expect(review().getByRole('button', { name: 'Apply timeline changes', exact: true })).toHaveCount(0); await discard();
    }
    expect(writes.length).toBe(n); expect(await inspect()).toEqual(before); expect(await historyAt(before.revision)).toEqual(h);
    return { q_at_start_and_w_at_end: true, no_empty_fragments_or_history: true };
  });
  await setupLock('locked-multitrack-split-source', 'f40-t1');
  await stage('locked-multitrack-split-rejected', async () => {
    await select(['f40-a', 'f40-b']); const before = await inspect();
    await expect(page.getByRole('button', { name: 'Split selection', exact: true })).toBeDisabled();
    const input = { project_id: project, expected_revision: before.revision, sequence_id: sequence, idempotency_key: 'f41-one-locked-multi-split', label: 'F41 mixed locked atomic guard', command: { kind: 'split', targets: [A, B].map((t, i) => ({ target: t, right_element_id: 'f41-rejected-right-' + i })), at: time(1.8), retain: 'left', ripple: true } };
    await expect(call('opencut_edit_timeline_plan', input)).rejects.toThrow(); expect(await inspect()).toEqual(before);
    return { disabled_ui: true, native_rejects_entire_batch: true, no_partial_split: true };
  });
  graph((await history('locked-source-undo')).project, fixture.project);
  await stage('reopen-final-fixture', async () => { await open(); graph((await inspect()).project, fixture.project); return { graph_restored: true }; });
  graph((await history('restore-original-full-project')).project, state.baseline.project);
  await stage('reopen-original-project', async () => { await open(); graph((await inspect()).project, state.baseline.project); return { original_full_project_restored: true }; });
  expect(errors).toEqual([]); delete state.failure; state.status = 'passed_scoped_f40_f41_selection_group_ripple'; await save();
  await fs.writeFile(report, JSON.stringify({ status: state.status, project_id: project, fixture_revision: fixture.revision, evidence_ownership: { prior_accepted_stages: args['prior-stage-evidence'], remaining_stages_candidate: args.candidate, resumed_original_ui_plans: state.resumed_original_ui_plans ?? {}, note: 'Prior passed snapshots keep original e3c5338c/ab290f53 candidate. Only failed caption emission and remaining boundary/lock/history stages use the later Native candidate; the captured original mixed UI request is recovered through the public planner/apply without a new intent or key.' }, source_defects: state.native_diagnostics, repaired_original_plans: state.stages['corrected-original-read-only-plans'], view_only: state.stages['selection-and-ripple-view-only'], box_cancel: state.stages['physical-box-and-cancel-no-write'], rigid_group: { plan: state.plans['physical-rigid-group-apply'], exact_offsets_and_track_distance: true, one_original_apply: true, full_graph_undo_redo: true }, locks: [state.stages['whole-group-lock-rejection'], state.stages['locked-multitrack-split-rejected']], conflict: state.stages['stale-group-plan-atomic-conflict'], split_cases: cases, boundary: state.stages['boundary-splits-are-noops'], original_restored: true, page_errors: errors, new_jobs: 0, new_sources: 0, new_project_copies: 0, scope: 'Actual selection/modifier/range/box/cancel, rigid exact-time/cross-track moves, main pin, atomic lock and revision fences, Q/W and form retain-left/right/ripple/multitrack/caption linkage, source mapping and existing curve samples, mode persistence and exact history/reopen. Reuses prior F39/F43 renderer/source-clock and F29 retime/PCM proofs; no broad S1/A7/D8 or whole-migration claim.' }, null, 2) + '\n');
  console.log(JSON.stringify({ status: state.status, new_jobs: 0, new_sources: 0 }));
} catch (error) {
  state.failure = { message: String(error), stack: error.stack, page_errors: errors, dialogs: await page.getByRole('dialog').allTextContents().catch(() => []) }; await save();
  await page.screenshot({ path: path.join(workspace, 'f40-f41-failure.png'), fullPage: true }); throw error;
} finally { await browser.close(); }
