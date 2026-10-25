import type { ToolOutput } from "../../../../lib/editor-sdk";
import type { EditRecoveryState, RecoveryContext, RecoveryIntent, RecoveryRecord } from "../edit-recovery";
export type CaptionReceipt = ToolOutput<"opencut_subtitle_generate">;
export type CaptionContext = Omit<RecoveryContext, "intent"> & {
  intent: Extract<RecoveryIntent, { kind: "subtitle_generate" | "subtitle_import" }>;
};
export type CaptionRecord = Omit<RecoveryRecord, "context" | "receipt"> & {
  context: CaptionContext;
  receipt: CaptionReceipt | null;
};
export type CaptionRecoveryState = Omit<EditRecoveryState, "record" | "receipt"> & {
  record: CaptionRecord | null;
  receipt: CaptionReceipt | null;
};
export function captionRecoveryView(state: EditRecoveryState): CaptionRecoveryState {
  const intent = state.record?.context.intent;
  const receipt = state.receipt && (state.receipt.kind === "subtitle_generate" || state.receipt.kind === "subtitle_import")
    ? state.receipt.caption : null;
  if (!state.record || !intent || (intent.kind !== "subtitle_generate" && intent.kind !== "subtitle_import"))
    return { phase: receipt ? "saved" : "idle", record: null, receipt, error: null };
  const saved = state.record.receipt;
  return { ...state, receipt, record: { ...state.record,
    context: { ...state.record.context, intent },
    receipt: saved && (saved.kind === "subtitle_generate" || saved.kind === "subtitle_import") ? saved.caption : null,
  } };
}
