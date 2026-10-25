import { editorSnapshot } from "./project-inspection.mjs";
/** Retire the original paused or unknown-play generation on real document leave. */
import { chromium, expect } from '@playwright/test';
import fs from 'node:fs/promises';
import path from 'node:path';
import { OpenCutClient } from '../../../packages/sdk-ts/dist/index.js';

const args = Object.fromEntries(process.argv.slice(2).reduce((a, v, i, s) => i % 2 ? a : [...a, [v.slice(2), s[i + 1]]], []));
const fixture = JSON.parse(await fs.readFile(path.join(args.workspace, 'scene-preview-fixture.json'), 'utf8'));
const client = new OpenCutClient({ baseUrl: args.endpoint, token: (await fs.readFile(args.credential, 'utf8')).trim() });
const call = (tool, input) => client.call(tool, input), inspect = () => editorSnapshot(call('opencut_project_inspect', { project_id: fixture.project_id, revision: null }));
const prior = await fs.readFile(args.report, 'utf8').then(JSON.parse).catch(e => { if (e.code !== 'ENOENT') throw e; return null; });
if (prior?.status === 'passed_scoped_document_departure' && prior.cases.some(c => c.kind === 'initial-pause')) { console.log('Reuse completed document-departure and startup evidence'); process.exit(0); }
const before = await inspect(), errors = [], writes = [];
const cases = await fs.readFile(args.report + '.checkpoint.json', 'utf8').then(JSON.parse).catch(e => { if (e.code !== 'ENOENT') throw e; return []; });
const browser = await chromium.launch({ headless: true, executablePath: process.env.OPENCUT_CHROME });
const page = await browser.newPage({ locale: 'en-US', viewport: { width: 1440, height: 1000 } });
const trace = [];
await page.exposeBinding('__opencutDepartureTrace', (_source, value) => { if (trace.length < 100) trace.push(value); });
await page.addInitScript(() => {
  const record = value => { void globalThis.__opencutDepartureTrace({ ...value, at: Date.now() }).catch(() => {}); };
  for (const name of ['beforeunload', 'pagehide', 'unload']) addEventListener(name, () => record({ kind: 'lifecycle', name }));
  const fetchOriginal = globalThis.fetch;
  globalThis.fetch = (input, init) => {
    const url = String(input), tool = url.split('/').at(-1);
    if (!tool.startsWith('opencut_audio_preview_recovery_')) return fetchOriginal(input, init);
    const body = typeof init?.body === 'string' ? JSON.parse(init.body) : null;
    record({ kind: 'fetch', tool, keepalive: init?.keepalive ?? false, command: body?.request?.request?.command?.kind ?? null, recovery_id: body?.recovery_id ?? null });
    const result = fetchOriginal(input, init);
    void result.then(r => record({ kind: 'response', tool, status: r.status }), e => record({ kind: 'fetch-error', tool, error: String(e) }));
    return result;
  };
  globalThis.__departurePump = { starts: [], progressing: false };
  const Original = globalThis.Worker;
  globalThis.Worker = class extends Original {
    constructor(...args) {
      super(...args); if (args[1]?.name !== 'opencut-audio-pump') return;
      const post = this.postMessage.bind(this);
      this.postMessage = (data, ...rest) => {
        if (data.channel === 'opencut-audio-pump' && data.kind === 'start') globalThis.__departurePump.starts.push(data.session.session_id);
        return post(data, ...rest);
      };
      this.addEventListener('message', ({ data }) => { if (data.channel === 'opencut-audio-pump' && data.kind === 'progress' && BigInt(data.receipt.consumedFrames) > 0n) globalThis.__departurePump.progressing = true; });
    }
  };
});
page.on('pageerror', e => errors.push(e.message));
page.on('request', r => { if (/\/opencut_edit_(apply|undo_apply|redo_apply)$/.test(r.url())) writes.push(r.postDataJSON()); });
const open = async () => { await page.goto(`${args.endpoint}/editor/${fixture.project_id}`); await expect(page.locator('.save-status')).toHaveText('Saved'); const d = page.getByRole('dialog', { name: 'Welcome to OpenCut', exact: true }); if (await d.isVisible()) await d.getByRole('button', { name: 'Close', exact: true }).click(); };
try {
  for (const kind of ['paused', 'unknown-play-response', 'initial-pause']) {
    if (cases.some(c => c.kind === kind)) continue;
    console.log('Begin departure ' + kind);
    await open();
    const retirements = []; let playing, original, held, release, pictureHeld = false, releasePicture;
    const heldGate = new Promise(resolve => { release = resolve; });
    const pictureGate = new Promise(resolve => { releasePicture = resolve; });
    if (kind === 'initial-pause') await page.route('**/api/tools/opencut_visual_preview_frame', async route => {
      const response = await route.fetch();
      if (!pictureHeld && route.request().url().endsWith('_frame')) { pictureHeld = true; await pictureGate; }
      await route.fulfill({ response });
    });
    const request = r => { if (r.url().endsWith('/opencut_audio_preview_recovery_retire')) retirements.push(r.postDataJSON()); };
    page.on('request', request);
    await page.route('**/api/tools/opencut_audio_preview_recovery_dispatch', async route => {
      const response = await route.fetch(), body = await response.json();
      if (body.data?.state === 'playing') {
        playing = body.data;
        if (kind === 'unknown-play-response') { held = { input: route.request().postDataJSON(), data: body.data }; await heldGate; }
      }
      await route.fulfill({ response }).catch(error => { if (!held) throw error; });
    });
    try {
      await page.getByRole('button', { name: 'Play scene preview', exact: true }).click();
      await expect.poll(() => !!playing, { timeout: 20000 }).toBe(true);
      if (kind !== 'unknown-play-response') {
        // This case owns an acknowledged, running output. Pausing during the
        // earlier picture preparation is a separate existing startup boundary.
        if (kind === 'paused') await expect.poll(() => page.evaluate(id => globalThis.__departurePump.progressing && globalThis.__departurePump.starts.includes(id), playing.session_id), { timeout: 15000 }).toBe(true);
        else { await expect.poll(() => pictureHeld).toBe(true); expect(await page.evaluate(() => globalThis.__departurePump.starts)).toEqual([]); }
        await page.getByRole('button', { name: 'Pause scene audio', exact: true }).click();
        releasePicture();
        await expect(page.getByText('Scene sound paused', { exact: true })).toBeVisible();
        if (kind === 'initial-pause') expect(await page.evaluate(() => globalThis.__departurePump.starts)).toEqual([]);
        original = await call('opencut_audio_preview_inspect', { session_id: playing.session_id });
        expect(original.state).toBe('paused');
      } else {
        expect(held).toBeTruthy(); original = held.data;
        const pending = await call('opencut_audio_preview_recovery_inspect', { project_id: fixture.project_id });
        expect(pending.record.recovery_id).toBe(held.input.recovery_id); expect(pending.record.status).toBe('completed');
      }
      const start = Date.now(); await page.goto(`${args.endpoint}/editor`);
      let stopped; await expect.poll(async () => { stopped = await call('opencut_audio_preview_inspect', { session_id: original.session_id }); return stopped.state; }, { timeout: 5000 }).toBe('stopped');
      await fs.writeFile(path.join(args.workspace, 'preview-departure-last-observation.json'), JSON.stringify({ kind, original, stopped, retirements, held: held?.input, elapsed_ms: Date.now() - start }, null, 2) + '\n');
      release();
      // Chrome can finish a keepalive after detaching its Page request observer.
      // Actual native retirement and the cleared durable slot are authoritative;
      // the single keepalive/original-intent property is covered by recovery.test.
      expect(retirements.length).toBeLessThanOrEqual(1);
      const retirement = retirements[0]?.request;
      if (retirement) { expect(retirement.kind).toBe('control'); expect(retirement.request.session_id).toBe(original.session_id); expect(retirement.request.command.kind).toBe(kind === 'unknown-play-response' ? 'play' : 'stop'); }
      expect(stopped.owner_id).toBe(original.owner_id); expect(stopped.lease_id).toBe(original.lease_id);
      expect((await call('opencut_audio_preview_recovery_inspect', { project_id: fixture.project_id })).record).toBeNull();
      expect(await inspect()).toEqual(before);
      cases.push({ kind, candidate: args.candidate, session_id: original.session_id, original_generation: original.generation, observed_retirement_command: retirement?.request.command.kind ?? null, terminal_state: stopped.state, elapsed_ms: Date.now() - start, page_observed_requests: retirements.length, initial_pump_not_started: kind === 'initial-pause' ? true : undefined, single_keepalive_original_intent: 'covered by recovery regression; do not infer request count from detached Page events', original_owner_lease_retained: true, pending_slot_cleared: true });
      await fs.writeFile(args.report + '.checkpoint.json', JSON.stringify(cases, null, 2) + '\n');
    } finally { release(); releasePicture(); await page.unroute('**/api/tools/opencut_audio_preview_recovery_dispatch'); if (kind === 'initial-pause') await page.unroute('**/api/tools/opencut_visual_preview_frame'); page.off('request', request); }
  }
  expect(errors).toEqual([]); expect(writes).toEqual([]);
  const result = { status: 'passed_scoped_document_departure', candidate: args.candidate, project_id: fixture.project_id, revision: before.revision, cases, page_errors: errors, project_writes: 0, project_and_revision_unchanged: true, new_jobs: 0, new_sources: 0, scope: 'Actual full-document navigation retires an acknowledged Pause and an executed Play whose response is deliberately held; original durable intent only, no new admission/replayed Play, no lease-expiry wait. Prior gain/PCM windows retain their own candidate and are not rerun.' };
  await fs.writeFile(args.report, JSON.stringify(result, null, 2) + '\n'); console.log(JSON.stringify({ status: result.status, cases }));
} finally { await fs.writeFile(args.report + '.trace.json', JSON.stringify(trace, null, 2) + '\n'); await browser.close(); }
