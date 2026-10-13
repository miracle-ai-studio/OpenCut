import { OpenCutApiError } from "../../../lib/editor-sdk";
import type { ToolInput, ToolOutput } from "../../../lib/editor-sdk";
import type { EditorApi } from "../api";
import type { V4SessionHost } from "./session";
import { fontRecoveryBlocked } from "./text/font-recovery";
import { RecoveryPointersV2 } from "./edit-recovery-pointers";

export type RecoveryContextV2 = ToolInput<"opencut_edit_recovery_v2_stage">["context"];
export type RecoveryIntentV2 = RecoveryContextV2["intent"];
type Inspection = ToolOutput<"opencut_edit_recovery_v2_inspect">;
type Discovery = NonNullable<Inspection["record"]>;
export type RecoveryRecordV2 = Extract<Discovery, { kind: "current_v2" }>["record"];
export type RecoveryReceiptV2 = NonNullable<RecoveryRecordV2["receipt"]>;
export type EditRecoveryState = {
  phase: "idle" | "reading" | "staging" | "applying" | "unconfirmed" | "saved" | "conflict" | "failed";
  record: RecoveryRecordV2 | null;
  receipt: RecoveryReceiptV2 | null;
  error: string | null;
};
export const emptyEditRecovery = (): EditRecoveryState => ({ phase: "idle", record: null, receipt: null, error: null });
const message = (error: unknown) => error instanceof Error ? error.message : String(error);
export function exactRecovery(value: unknown): string {
  return JSON.stringify(value, (_key, child) => child && typeof child === "object" && !Array.isArray(child)
    ? Object.fromEntries(Object.entries(child).sort(([a], [b]) => a.localeCompare(b))) : child);
}
function fixed<T>(value: T): T {
  const cloned = structuredClone(value);
  const freeze = (child: unknown) => { if (child && typeof child === "object" && !Object.isFrozen(child)) { Object.values(child).forEach(freeze); Object.freeze(child); } };
  freeze(cloned); return cloned;
}
function transaction(receipt: RecoveryReceiptV2) { return receipt.kind === "undo" || receipt.kind === "redo" ? receipt.commit.transaction : receipt.transaction; }
function assertReceipt(intent: RecoveryIntentV2, receipt: RecoveryReceiptV2) {
  const tx = transaction(receipt);
  if (receipt.kind !== intent.kind || tx.base_revision !== intent.request.expected_revision || !tx.transaction_id || !tx.revision)
    throw new Error("The recovered receipt differs from the original edit.");
  if (intent.kind === "undo" && (receipt.kind !== "undo" || receipt.commit.direction !== "undo" || receipt.commit.target_transaction_id !== intent.request.target_transaction_id))
    throw new Error("The undo receipt differs from its original target.");
  if (intent.kind === "redo" && (receipt.kind !== "redo" || receipt.commit.direction !== "redo" || receipt.commit.undo_transaction_id !== intent.request.undo_transaction_id))
    throw new Error("The redo receipt differs from its original target.");
}
type Fence = { api: EditorApi; identity: string; projectId: string; generation: number; view: number; serial: number };

/** One fixed-head write port. The host stores the exact original request;
 * inspection is read-only, and only an explicit user retry dispatches it. */
export class V4EditRecovery {
  private serial = 0;
  private legacy: { identity: string; projectId: string } | null = null;
  private snapshot: { identity: string; projectId: string; value: EditRecoveryState } | null = null;
  private readonly pointers = new RecoveryPointersV2();
  constructor(private readonly host: V4SessionHost, private readonly refresh: () => Promise<void>) {}
  private fence(api: EditorApi, projectId: string): Fence {
    if (!api.connectionIdentity) throw new Error("Authenticate the connection before editing.");
    return { api, identity: api.connectionIdentity, projectId, generation: this.host.generation(), view: this.host.viewGeneration(), serial: this.serial };
  }
  private current(fence: Fence) { return fence.serial === this.serial && fence.api === this.host.api() && fence.identity === fence.api.connectionIdentity &&
    fence.generation === this.host.generation() && fence.view === this.host.viewGeneration(); }
  private check(fence: Fence) { if (!this.current(fence)) throw new DOMException("The edit target changed.", "AbortError"); }
  private value(fence: Fence) { return this.snapshot?.identity === fence.identity && this.snapshot.projectId === fence.projectId ? this.snapshot.value : emptyEditRecovery(); }
  private publish(fence: Fence, value: EditRecoveryState) {
    if (!this.current(fence)) return;
    this.snapshot = { identity: fence.identity, projectId: fence.projectId, value };
    const state = this.host.state();
    if (state?.head.project.id === fence.projectId) this.host.publish({ ...state, recovery: value });
  }
  invalidate() { this.serial++; }
  resetPointer() { this.pointers.reset(); }
  async restore(api: EditorApi, projectId: string): Promise<{ state: EditRecoveryState; legacy: boolean }> {
    this.serial++;
    const fence = this.fence(api, projectId), previous = this.value(fence);
    this.publish(fence, { ...previous, phase: "reading", error: null });
    try {
      const inspected = await api.v4.recoveryV2Inspect(projectId); this.check(fence);
      if (inspected.schema_version !== 2) throw new Error("The service returned invalid edit recovery.");
      if (inspected.record?.kind === "legacy_v1") {
        this.legacy = { identity: fence.identity, projectId };
        this.publish(fence, emptyEditRecovery()); return { state: emptyEditRecovery(), legacy: true };
      }
      this.legacy = null;
      const record = inspected.record?.record ?? null;
      if (record && (record.schema_version !== 2 || record.project_id !== projectId || record.context.intent.request.project_id !== projectId || !record.recovery_id || !record.request_digest))
        throw new Error("The service returned a different original edit.");
      let storageError: string | null = null;
      try {
        const hints = this.pointers.list(fence.identity, projectId);
        if (record) this.pointers.put(fence.identity, projectId, record.recovery_id);
        else for (const hint of hints) this.pointers.remove(fence.identity, projectId, hint.recoveryId);
      } catch (error) { storageError = message(error); }
      if (record?.receipt) assertReceipt(record.context.intent, record.receipt);
      const value: EditRecoveryState = record ? { record: fixed(record), receipt: record.receipt ?? null, phase: record.receipt ? "saved" : "unconfirmed", error: storageError }
        : previous.receipt ? { ...previous, record: null, phase: "saved", error: storageError } : { ...emptyEditRecovery(), error: storageError, phase: storageError ? "failed" : "idle" };
      this.publish(fence, value); return { state: value, legacy: false };
    } catch (error) {
      if (!this.current(fence)) return { state: this.snapshot?.value ?? emptyEditRecovery(), legacy: false };
      const value: EditRecoveryState = { ...previous, phase: previous.receipt ? "saved" : previous.record ? "unconfirmed" : "failed", error: message(error) };
      this.publish(fence, value); return { state: value, legacy: false };
    }
  }
  private async dispatch(api: EditorApi, intent: RecoveryIntentV2): Promise<RecoveryReceiptV2> {
    switch (intent.kind) {
      case "public_apply": return { kind: "public_apply", transaction: await api.sdk.call("opencut_edit_v4_apply", intent.request) };
      case "undo": return { kind: "undo", commit: await api.sdk.call("opencut_edit_v4_undo_apply", intent.request) };
      case "redo": return { kind: "redo", commit: await api.sdk.call("opencut_edit_v4_redo_apply", intent.request) };
      case "restore": return { kind: "restore", transaction: await api.sdk.call("opencut_project_v4_restore", intent.request) };
    }
  }
  private async acknowledge(fence: Fence, record: RecoveryRecordV2, known: RecoveryReceiptV2 | null) {
    this.check(fence);
    let proof;
    try { proof = await fence.api.v4.recoveryV2Ack(fence.projectId, record.recovery_id); }
    catch (error) {
      // Only an already-known original receipt can survive an ack response
      // lost after deletion. Absence alone can never declare an edit saved.
      if (!known || !(error instanceof OpenCutApiError) || error.code !== "NOT_FOUND") throw error;
      const observed = await fence.api.v4.recoveryV2Inspect(fence.projectId, record.recovery_id);
      this.check(fence); if (observed.record !== null) throw error;
      this.pointers.remove(fence.identity, fence.projectId, record.recovery_id); return known;
    }
    this.check(fence);
    if (proof.schema_version !== 2 || proof.recovery_id !== record.recovery_id ||
      (proof.resolution === "saved" ? !proof.receipt : proof.receipt !== null) || (known && exactRecovery(known) !== exactRecovery(proof.receipt)))
      throw new Error("The recovery acknowledgment differs from the original receipt.");
    if (proof.receipt) assertReceipt(record.context.intent, proof.receipt);
    this.pointers.remove(fence.identity, fence.projectId, record.recovery_id); return proof.receipt ?? null;
  }
  async execute(api: EditorApi, context: RecoveryContextV2): Promise<RecoveryReceiptV2> {
    if (fontRecoveryBlocked(this.host.state()?.fontRecovery)) throw new Error("Resolve the original font import before starting another operation.");
    const captured = fixed(context), projectId = captured.intent.request.project_id, fence = this.fence(api, projectId);
    const previous = this.value(fence);
    if (this.legacy?.identity === fence.identity && this.legacy.projectId === projectId && this.host.state()?.timelineEdit.attempt?.recoveryId)
      throw new Error("Resolve the original timeline operation before starting another edit.");
    if (["reading", "staging", "applying"].includes(previous.phase) || (previous.record && exactRecovery(previous.record.context) !== exactRecovery(captured)) || (!previous.record && previous.error))
      throw new Error("Resolve the original edit before starting another operation.");
    let record = previous.record, receipt = previous.receipt && record ? previous.receipt : null;
    try {
      this.publish(fence, { record, receipt, phase: "staging", error: null });
      const staged = await api.v4.recoveryV2Stage({ context: captured }); this.check(fence);
      // Stage success is not an edit receipt. Read back the exact authorized
      // record before dispatch, even when re-staging an existing unknown write.
      const inspected = await api.v4.recoveryV2Inspect(projectId, staged.recovery_id); this.check(fence);
      if (inspected.schema_version !== 2 || inspected.record?.kind !== "current_v2") throw new Error("The original edit was not staged.");
      record = inspected.record.record;
      if (record.schema_version !== 2 || record.recovery_id !== staged.recovery_id || record.project_id !== projectId ||
        exactRecovery(record.context) !== exactRecovery(captured) || record.request_digest !== staged.request_digest) throw new Error("The staged edit differs from the original request.");
      if (record.receipt) assertReceipt(captured.intent, record.receipt);
      this.publish(fence, { record: fixed(record), receipt: record.receipt ?? null, phase: record.receipt ? "saved" : "unconfirmed", error: null });
      this.pointers.put(fence.identity, projectId, record.recovery_id); this.check(fence);
      receipt = record.receipt ?? null;
      if (!receipt) {
        this.publish(fence, { record, receipt: null, phase: "applying", error: null });
        const observed = await this.dispatch(api, captured.intent);
        assertReceipt(captured.intent, observed);
        receipt = observed;
      }
      assertReceipt(captured.intent, receipt);
      this.publish(fence, { record, receipt, phase: "saved", error: null });
      // A late old-API success leaves the durable record intact for the new
      // authenticated view. It cannot ack away another view's recovery target.
      if (this.current(fence)) {
        try { await this.acknowledge(fence, record, receipt); this.publish(fence, { record: null, receipt, phase: "saved", error: null }); }
        catch (error) { this.publish(fence, { record, receipt, phase: "saved", error: message(error) }); }
      }
      return receipt;
    } catch (error) {
      this.publish(fence, { record, receipt, phase: receipt ? "saved" : record ? error instanceof OpenCutApiError && error.code === "REVISION_CONFLICT" ? "conflict" : "unconfirmed" : "failed", error: message(error) });
      throw error;
    }
  }
  async publicApply(api: EditorApi, request: ToolInput<"opencut_edit_v4_apply">, sequenceId: string | null) {
    const receipt = await this.execute(api, { intent: { kind: "public_apply", request }, sequence_id: sequenceId });
    if (receipt.kind !== "public_apply") throw new Error("The service returned a different edit family."); return receipt.transaction;
  }
  async retry() {
    const api = this.host.api(), state = this.host.state(), record = state?.recovery?.record;
    if (!api || !state || !record || state.historical) throw new Error("Open the original current project to recover this edit.");
    const release = this.host.acquire(record.project_id);
    try { const receipt = await this.execute(api, record.context); if (api === this.host.api()) await this.refresh(); return receipt; }
    finally { release(); }
  }
  async release() {
    const api = this.host.api(), state = this.host.state(), record = state?.recovery?.record;
    if (!api || !state || !record) throw new Error("No original edit needs recovery.");
    const fence = this.fence(api, record.project_id), release = this.host.acquire(record.project_id);
    try {
      const receipt = await this.acknowledge(fence, record, state.recovery!.receipt);
      this.publish(fence, { ...emptyEditRecovery(), phase: receipt ? "saved" : "idle", receipt });
      if (receipt) await this.refresh();
    } catch (error) { this.publish(fence, { ...state.recovery!, error: message(error) }); throw error; }
    finally { release(); }
  }
  dismissSaved() {
    const api = this.host.api(), state = this.host.state();
    if (!api || !state || state.recovery?.record || !state.recovery?.receipt) throw new Error("Resolve the original edit before dismissing it.");
    this.publish(this.fence(api, state.head.project.id), emptyEditRecovery());
  }
}
