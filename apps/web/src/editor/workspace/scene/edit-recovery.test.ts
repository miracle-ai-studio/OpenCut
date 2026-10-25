import { afterEach, expect, it, vi } from "vitest";
import { EditorApi } from "../api";
import { OpenCutApiError } from "../../../lib/editor-sdk";
import { EditRecovery } from "./edit-recovery";
import type { RecoveryContext, RecoveryRecord, RecoveryReceipt } from "./edit-recovery";
import type { SessionHost } from "./session";
import type { SessionState } from "./types";
import { initialSnapshot } from "./fixture";

afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
const receiptFor = (context: RecoveryContext): RecoveryReceipt => {
  const transaction = { base_revision: context.intent.request.expected_revision, revision: "saved", transaction_id: "original-tx", created_ids: [], changed_ids: [], deleted_ids: [] };
  switch (context.intent.kind) {
    case "font_import": return {kind:"font_import",transaction};
    case "public_apply": return { kind: "public_apply", transaction };
    case "restore": return { kind: "restore", transaction };
    case "undo": return { kind: "undo", commit: { direction: "undo", target_transaction_id: context.intent.request.target_transaction_id, undo_transaction_id: null, transaction } };
    case "subtitle_generate": return { kind: "subtitle_generate", caption: { transaction, cue_count: 1, warnings: [] } };
    case "subtitle_import": return { kind: "subtitle_import", caption: { transaction, cue_count: 1, warnings: [] } };
    case "redo": return { kind: "redo", commit: { direction: "redo", target_transaction_id: "original-target", undo_transaction_id: context.intent.request.undo_transaction_id, transaction } };
  }
};
const contexts: RecoveryContext[] = [
  {sequence_id:"scene",intent:{kind:"font_import",request:{project_id:"v4-project",expected_revision:"r0",idempotency_key:"font-original",catalog_id:"pinned-face",font_id:"locked-font",required_text:"OpenCut 你好"}}},
  { sequence_id: "scene", intent: { kind: "public_apply", request: { project_id: "v4-project", expected_revision: "r0", idempotency_key: "apply-original", label: "Bookmark", operations: [] } } },
  { sequence_id: "scene", intent: { kind: "undo", request: { project_id: "v4-project", expected_revision: "r0", idempotency_key: "undo-original", target_transaction_id: "target" } } },
  { sequence_id: "scene", intent: { kind: "redo", request: { project_id: "v4-project", expected_revision: "r0", idempotency_key: "redo-original", undo_transaction_id: "undo-target" } } },
  { sequence_id: "scene", intent: { kind: "restore", request: { project_id: "v4-project", expected_revision: "r0", idempotency_key: "restore-original", target_revision: "seed", asset_versions: {} } } },
];
async function fixture() {
  const data = new Map<string, string>();
  const storage = { getItem: vi.fn((key: string) => data.get(key) ?? null), setItem: vi.fn((key: string, value: string) => { data.set(key, value); }) };
  vi.stubGlobal("sessionStorage", storage);
  const api = new EditorApi("http://localhost:8788", "secret-token");
  let record: RecoveryRecord | null = null;
  const dispatch = vi.fn(async (context: RecoveryContext) => receiptFor(context));
  const inspect = vi.fn(async () => ({ record: record ? structuredClone(record) : null }));
  const ack = vi.fn(async () => {
    if (!record) throw new OpenCutApiError("NOT_FOUND", "Absent", null, 404, "ack", []);
    if (!record.receipt) throw new Error("Original result remains unknown");
    const result = { recovery_id: record.recovery_id, resolution: "saved" as const, receipt: record.receipt };
    record = null; return result;
  });
  vi.spyOn(api.sdk, "call").mockImplementation(async (name, input) => {
    if(name==="opencut_font_source") return {face:{id:(input as {catalog_id:string}).catalog_id},details:{kind:"static"}} as never;
    if (name === "opencut_access_inspect") return { principal_id: "principal", credential_id: "credential", owner: false, grants: [] } as never;
    if (name === "opencut_edit_recovery_inspect") return await inspect() as never;
    if (name === "opencut_edit_recovery_ack") return await ack() as never;
    if (name === "opencut_edit_recovery_stage") {
      const context = (input as { context: RecoveryContext }).context;
      record ??= { recovery_id: "recovery", project_id: "v4-project", request_digest: "opaque-host-digest", context: structuredClone(context), receipt: null };
      expect(record.context).toEqual(context); return structuredClone(record) as never;
    }
    if (["opencut_font_import","opencut_edit_apply", "opencut_edit_undo_apply", "opencut_edit_redo_apply", "opencut_project_restore"].includes(name)) {
      expect(input).toEqual(record!.context.intent.request);
      const result = await dispatch(record!.context); record!.receipt = result;
      return (result.kind === "undo" || result.kind === "redo" ? result.commit :
        result.kind === "subtitle_generate" || result.kind === "subtitle_import" ? result.caption : result.transaction) as never;
    }
    throw new Error(`Unexpected ${name}`);
  });
  await api.authenticateConnection();
  let state: SessionState | null = null;
  const host: SessionHost = { api: () => api, state: () => state, generation: () => 1, viewGeneration: () => 1, changeView: () => 1,
    publish: next => { state = next; }, acquire: () => () => {}, refreshTasks: async () => {} };
  const fresh = () => { state = { head: initialSnapshot(), historical: null, sequenceId: "scene", recovery: undefined } as SessionState;
    return new EditRecovery(host, async () => {}); };
  return { api, data, storage, dispatch, inspect, ack, fresh, state: () => state!, record: () => record,
    saved: (context: RecoveryContext) => { record!.receipt = receiptFor(context); } };
}
for (const context of contexts) {
  it(`${context.intent.kind}: pointer failure leaves a discoverable staged original without dispatch; fresh page explicitly retries its exact family/key`, async () => {
    const f = await fixture(), first = f.fresh();
    f.storage.setItem.mockImplementationOnce(() => { throw new Error("Storage denied"); });
    await expect(first.execute(f.api, context)).rejects.toThrow("recovery storage");
    expect(f.dispatch).not.toHaveBeenCalled(); expect(f.record()!.context).toEqual(context);
    const second = f.fresh(); await second.restore(f.api, "v4-project");
    expect(f.state().recovery!.phase).toBe("unconfirmed"); expect(f.dispatch).not.toHaveBeenCalled();
    const result = await second.retry(); expect(result).toEqual(receiptFor(context));
    expect(f.dispatch.mock.calls).toEqual([[context]]); expect(f.record()).toBeNull();
    expect(f.data.get("opencut.edit-recovery")).not.toContain("secret-token");
    expect(f.data.get("opencut.edit-recovery")).not.toContain("expected_revision");
  });
  it(`${context.intent.kind}: committed response loss restores its original authoritative receipt without replanning or redispatch`, async () => {
    const f = await fixture(), first = f.fresh();
    f.dispatch.mockImplementationOnce(async original => { f.saved(original); throw new Error("Response lost"); });
    await expect(first.execute(f.api, context)).rejects.toThrow("Response lost");
    const second = f.fresh(); await second.restore(f.api, "v4-project");
    expect(f.state().recovery!.receipt).toEqual(receiptFor(context)); expect(f.dispatch).toHaveBeenCalledTimes(1);
    await second.release(); expect(f.state().recovery!.phase).toBe("saved"); expect(f.record()).toBeNull();
  });
}
it("a genuine edit receipt stays saved through failed acknowledgment and authorized absent-record cleanup", async () => {
  const f = await fixture(), coordinator = f.fresh();
  f.ack.mockImplementationOnce(async () => { await f.ack.getMockImplementation()!(); throw new Error("Ack response lost"); });
  const result = await coordinator.execute(f.api, contexts[1]);
  expect(result).toEqual(receiptFor(contexts[1])); expect(f.state().recovery!.phase).toBe("saved");
  expect(f.state().recovery!.error).toBe("Ack response lost");
  await coordinator.release(); expect(f.state().recovery!.record).toBeNull(); expect(f.state().recovery!.receipt).toEqual(result);
});
it("authorized absence is not a receipt and cannot resolve an unknown edit", async () => {
  const f = await fixture(), coordinator = f.fresh();
  f.dispatch.mockRejectedValueOnce(new Error("Unknown")); await expect(coordinator.execute(f.api, contexts[1])).rejects.toThrow("Unknown");
  f.ack.mockRejectedValueOnce(new OpenCutApiError("NOT_FOUND", "Absent", null, 404, "ack", []));
  await expect(coordinator.release()).rejects.toThrow("Absent");
  expect(f.state().recovery!.receipt).toBeNull(); expect(f.state().recovery!.record).not.toBeNull();
});
it("a wrong-base public reply cannot become Saved or acknowledge the original staged edit", async () => {
  const f = await fixture(), coordinator = f.fresh();
  f.dispatch.mockResolvedValueOnce({ kind: "public_apply", transaction: {
    base_revision: "another-base", revision: "different-revision", transaction_id: "other-tx", created_ids: [], changed_ids: [], deleted_ids: [] } });
  await expect(coordinator.execute(f.api, contexts[1])).rejects.toThrow("differs from the original");
  expect(f.state().recovery!.phase).toBe("unconfirmed"); expect(f.state().recovery!.receipt).toBeNull();
  expect(f.ack).not.toHaveBeenCalled(); expect(f.record()).not.toBeNull();
});
