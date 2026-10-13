import { OpenCutApiError } from "../../../../lib/editor-sdk";
import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { EditorApi } from "../../api";
import type { V4SessionHost } from "../session";
import { exactRecovery } from "../edit-recovery";
import { RecoveryPointersV2 } from "../edit-recovery-pointers";
import {
  readCaptionRecoveryLocator,
  writeCaptionRecoveryLocator,
  clearCaptionRecoveryLocator,
} from "./client-state";
export type CaptionContext =
  ToolInput<"opencut_edit_recovery_v3_stage">["context"];
export type CaptionReceipt = ToolOutput<"opencut_subtitle_generate_v4">;
export type CaptionRecord = ToolOutput<"opencut_edit_recovery_v3_stage">;
export type CaptionRecoveryState = {
  phase:
    | "idle"
    | "reading"
    | "staging"
    | "applying"
    | "unconfirmed"
    | "saved"
    | "failed";
  record: CaptionRecord | null;
  receipt: CaptionReceipt | null;
  error: string | null;
};
const empty = (): CaptionRecoveryState => ({
  phase: "idle",
  record: null,
  receipt: null,
  error: null,
});
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));
function checkReceipt(context: CaptionContext, receipt: CaptionReceipt) {
  const tx = receipt.transaction;
  if (
    tx.base_revision !== context.intent.request.expected_revision ||
    !tx.transaction_id ||
    !tx.revision ||
    receipt.cue_count < 1 ||
    receipt.cue_count > 5000
  )
    throw new Error(
      "The caption receipt differs from the original fixed edit.",
    );
}
/** The exact original caption request lives in the service, sharing its existing
 * authenticated recovery slot. A retry always requires an explicit user action. */
export class V4CaptionRecovery {
  private value = empty();
  private identity: string | null = null;
  private project: string | null = null;
  private serial = 0;
  private listeners = new Set<() => void>();
  private pointers = new RecoveryPointersV2();
  constructor(
    private host: V4SessionHost,
    private refresh: () => Promise<void>,
  ) {}
  getSnapshot = () => this.value;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(value: CaptionRecoveryState) {
    this.value = value;
    this.listeners.forEach((listener) => listener());
  }
  blocked() {
    return (
      !!this.value.record ||
      !!this.value.error ||
      ["reading", "staging", "applying"].includes(this.value.phase)
    );
  }
  invalidate() {
    this.serial++;
  }
  private fence(api: EditorApi, project: string) {
    const serial = this.serial,
      generation = this.host.generation(),
      view = this.host.viewGeneration(),
      identity = api.connectionIdentity;
    if (!identity)
      throw new Error("Authenticate the connection before editing captions.");
    const current = () =>
      api === this.host.api() &&
      identity === api.connectionIdentity &&
      serial === this.serial &&
      generation === this.host.generation() &&
      view === this.host.viewGeneration();
    return {
      current,
      check: () => {
        if (!current())
          throw new DOMException("The caption target changed.", "AbortError");
      },
      api,
      project,
      identity,
    };
  }
  async restore(api: EditorApi, project: string) {
    this.serial++;
    if (this.identity !== api.connectionIdentity || this.project !== project)
      this.publish(empty());
    this.identity = api.connectionIdentity;
    this.project = project;
    const fence = this.fence(api, project),
      previous = this.value;
    let known = !!previous.record;
    this.publish({ ...previous, phase: "reading", error: null });
    try {
      known = known || !!readCaptionRecoveryLocator(fence.identity, project);
      const result = await api.sdk.call("opencut_edit_recovery_v3_inspect", {
        project_id: project,
        recovery_id: null,
      });
      fence.check();
      if (result.schema_version !== 3)
        throw new Error("Invalid caption recovery protocol.");
      const record = result.record;
      if (!record && known)
        throw new Error(
          "The known original caption record is absent. Its prior result remains unresolved; absence alone does not prove it was saved.",
        );
      if (
        record &&
        (record.project_id !== project ||
          record.context.intent.request.project_id !== project ||
          record.schema_version !== 3)
      )
        throw new Error(
          "The service returned a different caption recovery scope.",
        );
      if (record?.receipt) checkReceipt(record.context, record.receipt);
      if (record) {
        this.pointers.put(fence.identity, project, record.recovery_id);
        writeCaptionRecoveryLocator(
          fence.identity,
          project,
          record.recovery_id,
        );
      }
      // V2/font pointers may use the same slot; only this protocol's known
      // record is removed after positive acknowledgement, never empty discovery.
      this.publish(
        record
          ? {
              record,
              receipt: record.receipt ?? null,
              phase: record.receipt ? "saved" : "unconfirmed",
              error: null,
            }
          : empty(),
      );
      return !!record;
    } catch (error) {
      if (fence.current()) {
        if (
          !known &&
          error instanceof OpenCutApiError &&
          ["UNKNOWN_TOOL", "TOOL_NOT_FOUND"].includes(error.code)
        )
          this.publish(empty());
        else
          this.publish({
            ...previous,
            phase: previous.record ? "unconfirmed" : "failed",
            error: message(error),
          });
      }
      return !!previous.record;
    }
  }
  private async dispatch(
    api: EditorApi,
    context: CaptionContext,
  ): Promise<CaptionReceipt> {
    switch (context.intent.kind) {
      case "subtitle_generate_v4":
        return api.sdk.call(
          "opencut_subtitle_generate_v4",
          context.intent.request,
        );
      case "subtitle_import_v4":
        return api.sdk.call(
          "opencut_subtitle_import_v4",
          context.intent.request,
        );
    }
  }
  private async ack(
    api: EditorApi,
    record: CaptionRecord,
    known: CaptionReceipt | null,
  ) {
    const fence = this.fence(api, record.project_id);
    let proof;
    try {
      proof = await api.sdk.call("opencut_edit_recovery_v3_ack", {
        project_id: record.project_id,
        recovery_id: record.recovery_id,
      });
    } catch (error) {
      if (
        !known ||
        !(error instanceof OpenCutApiError) ||
        error.code !== "NOT_FOUND"
      )
        throw error;
      const result = await api.sdk.call("opencut_edit_recovery_v3_inspect", {
        project_id: record.project_id,
        recovery_id: record.recovery_id,
      });
      fence.check();
      if (result.record !== null) throw error;
      this.pointers.remove(
        fence.identity,
        record.project_id,
        record.recovery_id,
      );
      clearCaptionRecoveryLocator(
        fence.identity,
        record.project_id,
        record.recovery_id,
      );
      return known;
    }
    fence.check();
    if (
      proof.schema_version !== 3 ||
      proof.recovery_id !== record.recovery_id ||
      (proof.resolution === "saved") !== !!proof.receipt ||
      (known && exactRecovery(known) !== exactRecovery(proof.receipt))
    )
      throw new Error(
        "Caption acknowledgement differs from its original receipt.",
      );
    if (proof.receipt) checkReceipt(record.context, proof.receipt);
    this.pointers.remove(fence.identity, record.project_id, record.recovery_id);
    clearCaptionRecoveryLocator(
      fence.identity,
      record.project_id,
      record.recovery_id,
    );
    return proof.receipt;
  }
  async execute(
    api: EditorApi,
    context: CaptionContext,
  ): Promise<CaptionReceipt> {
    const captured = structuredClone(context),
      project = captured.intent.request.project_id,
      fence = this.fence(api, project);
    if (this.identity !== api.connectionIdentity || this.project !== project)
      throw new Error("Inspect original caption recovery before editing.");
    if (
      ["reading", "staging", "applying"].includes(this.value.phase) ||
      (this.value.record &&
        exactRecovery(this.value.record.context) !== exactRecovery(captured)) ||
      (!this.value.record && this.value.error)
    )
      throw new Error(
        "Resolve the original caption edit before starting another operation.",
      );
    let record = this.value.record,
      receipt = record?.receipt ?? null;
    try {
      this.publish({ record, receipt, phase: "staging", error: null });
      const staged = await api.sdk.call("opencut_edit_recovery_v3_stage", {
        context: captured,
      });
      fence.check();
      const read = await api.sdk.call("opencut_edit_recovery_v3_inspect", {
        project_id: project,
        recovery_id: staged.recovery_id,
      });
      fence.check();
      record = read.record ?? null;
      if (
        !record ||
        record.schema_version !== 3 ||
        staged.request_digest !== record.request_digest ||
        record.recovery_id !== staged.recovery_id ||
        exactRecovery(record.context) !== exactRecovery(captured)
      )
        throw new Error("Staged captions differ from the original request.");
      this.pointers.put(fence.identity, project, record.recovery_id);
      writeCaptionRecoveryLocator(fence.identity, project, record.recovery_id);
      receipt = record.receipt ?? null;
      this.publish({
        record,
        receipt,
        phase: receipt ? "saved" : "applying",
        error: null,
      });
      if (!receipt) receipt = await this.dispatch(api, captured);
      checkReceipt(captured, receipt);
      // A late response does not release another authenticated view's slot.
      if (fence.current()) {
        this.publish({ record, receipt, phase: "saved", error: null });
        try {
          await this.ack(api, record, receipt);
          this.publish({ record: null, receipt, phase: "saved", error: null });
        } catch (error) {
          this.publish({
            record,
            receipt,
            phase: "saved",
            error: message(error),
          });
        }
      }
      return receipt;
    } catch (error) {
      if (fence.current())
        this.publish({
          record,
          receipt,
          phase: receipt ? "saved" : record ? "unconfirmed" : "failed",
          error: message(error),
        });
      throw error;
    }
  }
  async retry() {
    const api = this.host.api(),
      record = this.value.record,
      state = this.host.state();
    if (
      !api ||
      !record ||
      !state ||
      state.historical ||
      state.head.project.id !== record.project_id
    )
      throw new Error(
        "Open the original current project before retrying captions.",
      );
    const release = this.host.acquire(record.project_id);
    try {
      const receipt = await this.execute(api, record.context);
      if (api === this.host.api()) await this.refresh();
      return receipt;
    } finally {
      release();
    }
  }
  async release() {
    const api = this.host.api(),
      record = this.value.record;
    if (!api || !record)
      throw new Error("No original caption edit needs acknowledgment.");
    const release = this.host.acquire(record.project_id);
    try {
      const receipt = (await this.ack(api, record, this.value.receipt)) ?? null;
      this.publish({
        record: null,
        receipt,
        phase: receipt ? "saved" : "idle",
        error: null,
      });
      if (receipt && api === this.host.api()) await this.refresh();
      return receipt;
    } finally {
      release();
    }
  }
}
