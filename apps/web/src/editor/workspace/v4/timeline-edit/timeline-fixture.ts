import { vi } from "vitest";
import { setupV4 } from "../fixture";
import { V4TimelineEdit } from "./controller";
import { emptyTimelineEdit } from "./types";
import type { TimelineEditHost, TimelinePlan, TimelineRequest } from "./types";
import type { ToolInput } from "../../../../lib/editor-sdk";
import type { V4EditReceipt } from "../types";

/** Protocol/lifecycle fixture only. No source/ripple/curve algorithm is mirrored;
 * Rust tests and the actual product verifier prove those independent semantics. */
export async function setupTimelineEdit() {
  const base = await setupV4();
  let state = { ...base.controller.getSnapshot().v4!, timelineEdit: emptyTimelineEdit() };
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
  const edit = new V4TimelineEdit(host, planner, refresh);
  const receipts = new Map<string, V4EditReceipt>();
  const commit = (request: ToolInput<"opencut_edit_v4_apply">) => {
    const existing = receipts.get(request.idempotency_key);
    if (existing) return existing;
    const current = base.current();
    current.revision = "timeline-" + request.idempotency_key;
    current.project.sequences[0].tracks[0].elements[0] = structuredClone(suppliedElement);
    const receipt: V4EditReceipt = { base_revision: request.expected_revision, revision: current.revision,
      transaction_id: "tx-" + request.idempotency_key, changed_ids: ["image"], created_ids: [], deleted_ids: [] };
    receipts.set(request.idempotency_key, receipt);
    return receipt;
  };
  const previous = vi.mocked(base.api.sdk.call).getMockImplementation()!;
  vi.mocked(base.api.sdk.call).mockImplementation(async (name, input, options) => {
    if (name === "opencut_edit_v4_apply") return commit(input as ToolInput<"opencut_edit_v4_apply">) as never;
    return previous(name, input, options);
  });
  const set = (next: Partial<typeof state>) => { state = { ...state, ...next }; emit(); };
  return { ...base, edit, planner, refresh, planFor, commit, host, state: () => state, set, busy: () => busy,
    subscribe: (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; },
    changeView: () => { view++; },
    changeApi: () => { activeApi = null; generation++; },
    reopen: () => { generation++; state = { ...state, historical: null,
      timelineEdit: { selection: [], attempt: edit.resume(base.api, state.head.project.id) } }; emit(); },
    applyCalls: () => base.call.mock.calls.filter(([name]) => name === "opencut_edit_v4_apply"),
  };
}
