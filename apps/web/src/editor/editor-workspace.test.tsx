// @vitest-environment jsdom
// Actual component + HTTP client; controlled responses exercise user journeys
// and out-of-order network completion. These are not browser playback tests.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { EditorWorkspace } from './editor-workspace';

const snapshot = (id: string, revision = `r${id}`) => ({
  revision,
  project: { id, name: `Project ${id}`, schema_version: 2, default_sequence_id: `s${id}`,
    sequences: [{ id: `s${id}`, name: 'Main', width: 1920, height: 1080,
      fps: { numerator: 24, denominator: 1 }, tracks: [], caption_cues: [] }], assets: [] },
});
const envelope = (data: unknown) => new Response(JSON.stringify({ data, error: null }), {
  headers: { 'content-type': 'application/json' },
});
const history = (id: string, revision = `r${id}`) => ({
  project_id: id, head_revision: revision, next_cursor: null,
  revisions: [
    { revision, parent_revision: `${id}-initial`, label: `${id} head`, created_at_ms: 2 },
    { revision: `${id}-initial`, parent_revision: null, label: `${id} initial`, created_at_ms: 1 },
  ],
});
const deferred = () => {
  let resolve!: (response: Response) => void;
  const promise = new Promise<Response>((r) => { resolve = r; });
  return { promise, resolve };
};
type Call = { url: string; body: Record<string, any>; method: string };
function server(intercept?: (call: Call) => Promise<Response> | Response | undefined) {
  const calls: Call[] = [], revisions = new Map([['A', 'rA'], ['B', 'rB']]);
  vi.stubGlobal('fetch', vi.fn(async (input: unknown, init: RequestInit = {}) => {
    const call = { url: String(input), body: typeof init.body === 'string' ? JSON.parse(init.body) : {},
      method: init.method ?? 'GET' };
    calls.push(call);
    const response = intercept?.(call);
    if (response) return response;
    const { url, body } = call;
    if (url.endsWith('/v1/job-events/stream')) return new Response('');
    if (url.endsWith('/v1/capabilities')) return envelope({
      rendering: { project_frame_preview: false, composited_frame_preview: false,
        project_range_preview: false, keyframes: false }, parameters: [], job_kinds: [],
    });
    if (url.endsWith('/v1/projects')) return envelope([snapshot('A'), snapshot('B')]);
    const match = /\/v1\/projects\/([^/]+)\/snapshot$/.exec(url);
    if (match) return envelope(snapshot(match[1], revisions.get(match[1])));
    if (url.endsWith('/v1/tools/opencut_job_list')) return envelope([]);
    if (url.endsWith('/v1/tools/opencut_project_history'))
      return envelope(history(body.project_id, revisions.get(body.project_id)));
    if (url.endsWith('/v1/tools/opencut_revision_inspect'))
      return envelope(snapshot(body.project_id, body.revision));
    if (/\/opencut_(edit_apply|revision_restore)$/.test(url)) {
      const revision = `updated-${calls.length}`;
      revisions.set(body.project_id, revision);
      return envelope({ revision });
    }
    throw new Error(`Unexpected request ${url}`);
  }));
  return calls;
}
async function connect() {
  render(<EditorWorkspace />);
  fireEvent.click(screen.getByRole('button', { name: 'Disconnected' }));
  await screen.findByText('Project A');
}
const button = (name: string) => screen.getByRole('button', { name }) as HTMLButtonElement;
const writes = (calls: Call[]) => calls.filter((c) => /\/opencut_(edit_apply|revision_restore)$/.test(c.url));
const activeProject = () => document.querySelector('.project.active')?.textContent;
afterEach(() => { cleanup(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

it('disconnects the old target when reconnect fails, including subsequent renders', async () => {
  const calls = server(({ url }) => {
    if (url.startsWith('http://b/')) throw new Error('B unreachable');
  });
  await connect();
  fireEvent.change(screen.getByLabelText('API address'), { target: { value: 'http://b' } });
  fireEvent.click(screen.getByRole('banner').querySelector('button')!);
  await screen.findByRole('button', { name: 'Disconnected: B unreachable' });
  fireEvent.change(screen.getByLabelText('Bearer token'), { target: { value: 'another render' } });
  fireEvent.click(button('Track'));
  expect(writes(calls)).toHaveLength(0);
  expect(activeProject()).toBeUndefined();
});

it('connecting an empty host clears the previous project and history', async () => {
  const calls = server(({ url }) => url === 'http://b/v1/projects' ? envelope([]) : undefined);
  await connect();
  fireEvent.change(screen.getByLabelText('API address'), { target: { value: 'http://b' } });
  fireEvent.click(screen.getByRole('banner').querySelector('button')!);
  await waitFor(() => expect(screen.queryByText('Project A')).toBeNull());
  await screen.findByRole('button', { name: 'Connected' });
  fireEvent.click(button('Track'));
  expect(writes(calls)).toHaveLength(0);
  expect(screen.queryByText('A initial')).toBeNull();
});

it('a delayed historical snapshot cannot replace the selected project', async () => {
  const late = deferred();
  server(({ url }) => url.endsWith('/opencut_revision_inspect') ? late.promise : undefined);
  await connect();
  fireEvent.click(screen.getByText('A initial').closest('button')!);
  fireEvent.click(screen.getByText('Project B'));
  await waitFor(() => expect(activeProject()).toContain('Project B'));
  await act(async () => late.resolve(envelope(snapshot('A', 'A-initial'))));
  expect(activeProject()).toContain('Project B');
  expect(screen.queryByText('READ-ONLY HISTORICAL SNAPSHOT')).toBeNull();
  expect(screen.queryByText('A initial')).toBeNull();
});

it('a completed create cannot replace a newer project selection', async () => {
  const late = deferred();
  server(({ url, method }) => url.endsWith('/v1/projects') && method === 'POST' ? late.promise : undefined);
  await connect();
  fireEvent.click(button('New project'));
  fireEvent.click(screen.getByText('Project B'));
  await waitFor(() => expect(activeProject()).toContain('Project B'));
  await act(async () => late.resolve(envelope(snapshot('C'))));
  expect(activeProject()).toContain('Project B');
  expect(screen.queryByText('C initial')).toBeNull();
});

it('a delayed history refresh cannot populate a different project', async () => {
  let delay = false;
  const late = deferred();
  server(({ url, body }) => delay && url.endsWith('/opencut_project_history') && body.project_id === 'A'
    ? late.promise : undefined);
  await connect();
  delay = true;
  fireEvent.click(button('Refresh history'));
  fireEvent.click(screen.getByText('Project B'));
  await waitFor(() => expect(activeProject()).toContain('Project B'));
  await act(async () => late.resolve(envelope(history('A'))));
  expect(screen.queryByText('A initial')).toBeNull();
  expect(screen.getByText('B initial')).toBeTruthy();
});

it('editing after undo invalidates redo and prevents a second restore', async () => {
  const calls = server();
  await connect();
  fireEvent.click(button('Undo latest action'));
  await waitFor(() => expect(button('Redo restore').disabled).toBe(false));
  fireEvent.click(button('Track'));
  await waitFor(() => expect(writes(calls)).toHaveLength(2));
  await waitFor(() => expect(button('Redo restore').disabled).toBe(true));
  fireEvent.click(button('Redo restore'));
  expect(writes(calls).filter((c) => c.url.endsWith('/opencut_revision_restore'))).toHaveLength(1);
});

it('a conflicting restore preserves the historical view and never creates redo', async () => {
  server(({ url }) => url.endsWith('/opencut_revision_restore') ? new Response(JSON.stringify({
    data: null, error: { code: 'REVISION_CONFLICT', message: 'Head changed', details: {} },
  }), { status: 409 }) : undefined);
  await connect();
  fireEvent.click(screen.getByText('A initial').closest('button')!);
  await screen.findByText('READ-ONLY HISTORICAL SNAPSHOT');
  fireEvent.click(button('Restore this revision'));
  await screen.findByRole('button', { name: 'Revision changed — refreshed' });
  expect(screen.getByText('READ-ONLY HISTORICAL SNAPSHOT')).toBeTruthy();
  expect(button('Redo restore').disabled).toBe(true);
});

it('restoring from the historical view refreshes the head before returning to editing', async () => {
  const calls = server();
  await connect();
  fireEvent.click(screen.getByText('A initial').closest('button')!);
  await screen.findByText('READ-ONLY HISTORICAL SNAPSHOT');
  fireEvent.click(button('Restore this revision'));
  await waitFor(() => expect(screen.queryByText('READ-ONLY HISTORICAL SNAPSHOT')).toBeNull());
  fireEvent.click(button('Track'));
  await waitFor(() => expect(writes(calls)).toHaveLength(2));
  expect(writes(calls)[1].body.request.expected_revision).toMatch(/^updated-/);
});
