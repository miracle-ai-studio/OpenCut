import { vi } from "vitest";
import { setup } from "../fixture";
import { EditRecovery } from "../edit-recovery";
import { TimelineEdit } from "./controller";
import { emptyTimelineEdit } from "./types";
import type { TimelineEditHost, TimelinePlan, TimelineRequest } from "./types";
import type { ToolInput } from "../../../../lib/editor-sdk";
import type { EditReceipt } from "../types";

/** Protocol/lifecycle fixture only. No source/ripple/curve algorithm is mirrored;
 * Rust tests and the actual product verifier prove those independent semantics. */
export async function setupTimelineEdit() {
  const base = await setup();
  let state = { ...base.controller.getSnapshot().scene!, timelineEdit: emptyTimelineEdit() };
  let generation = 1, view = 1, busy = false;
  let activeApi: typeof base.api | null = base.api;
  const listeners = new Set<() => void>();
  const emit = () => { state = { ...state }; listeners.forEach(listener => listener()); };
  const host: TimelineEditHost = {
    api: () => activeApi, state: () => state, generation: () => generation, viewGeneration: () => view,
    publish: (timelineEdit, primaryId) => {
      state = { ...state, timelineEdit, ...(primaryId === undefined ? {} : { selectedId: primaryId }) }; emit();
    },
    acquire: () => {
      if (busy) throw new Error("Wait for the current edit to finish.");
      busy = true; emit();
      return () => { busy = false; emit(); };
    },
  };
  const suppliedElement = structuredClone(state.head.project.sequences[0].tracks[0].elements[0]);
  suppliedElement.timeline_range.start = { ticks: "4", timebase: 3 };
  const planFor = (request: TimelineRequest): TimelinePlan => ({
    original_request_digest: "opaque-shared-request-digest",
    transaction: { project_id: request.project_id, expected_revision: request.expected_revision,
      idempotency_key: request.idempotency_key, label: request.label,
      operations: [{ type: "element_put", sequence_id: "scene", track_id: "track", element: suppliedElement }] },
    decision: { created_ids: [], changed_ids: ["image"], deleted_ids: [], selection: [{ track_id: "track", element_id: "image" }],
      ripple_shifted_ids: [], removed_transition_ids: [] },
  });
  const planner = vi.fn(async (_api: typeof base.api, request: TimelineRequest, _signal?: AbortSignal) => planFor(request));
  const refresh = vi.fn(async () => { state = { ...state, head: structuredClone(base.current()) }; emit(); });
  const recovery = new EditRecovery({ ...host, changeView: () => ++view, refreshTasks: async () => {},
    publish: next => { state = { ...next, timelineEdit: next.timelineEdit ?? emptyTimelineEdit() }; emit(); },
  }, refresh);
  const edit = new TimelineEdit(host, planner, refresh, recovery);
  const receipts = new Map<string, EditReceipt>();
  const commit = (request: ToolInput<"opencut_edit_apply">) => {
    const existing = receipts.get(request.idempotency_key);
    if (existing) return existing;
    const current = base.current();
    current.revision = "timeline-" + request.idempotency_key;
    current.project.sequences[0].tracks[0].elements[0] = structuredClone(suppliedElement);
    const receipt: EditReceipt = { base_revision: request.expected_revision, revision: current.revision,
      transaction_id: "tx-" + request.idempotency_key, changed_ids: ["image"], created_ids: [], deleted_ids: [] };
    receipts.set(request.idempotency_key, receipt);
    return receipt;
  };
  const previous = vi.mocked(base.api.sdk.call).getMockImplementation()!;
  vi.mocked(base.api.sdk.call).mockImplementation(async (name, input, options) => {
    if (name === "opencut_edit_apply") return commit(input as ToolInput<"opencut_edit_apply">) as never;
    return previous(name, input, options);
  });
  const set = (next: Partial<typeof state>) => { state = { ...state, ...next }; emit(); };
  return { ...base, edit, recovery, planner, refresh, planFor, commit, host, state: () => state, set, busy: () => busy,
    subscribe: (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; },
    changeView: () => { view++; },
    changeApi: () => { activeApi = null; generation++; },
    reopen: () => { generation++; state = { ...state, historical: null,
      timelineEdit: { selection: [], attempt: edit.resume(base.api, state.head.project.id) } }; emit(); },
    applyCalls: () => base.call.mock.calls.filter(([name]) => name === "opencut_edit_apply"),
  };
}
