import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { TextEditorValue, TextTimeInput } from "./types";

export type TextPlanRequest = ToolInput<"opencut_text_plan">;
export type TextPlanResult = ToolOutput<"opencut_text_plan">;
export type FontImportRequest = ToolInput<"opencut_font_import">;
export type FontImportResult = ToolOutput<"opencut_font_import">;
export type FontCatalog = ToolOutput<"opencut_font_catalog">;
type Values = Extract<TextPlanRequest["command"], { kind: "create" }>["values"];

export const textTimeInput = (value: TextTimeInput): Values["start"] =>
  typeof value === "string"
    ? { kind: "decimal_seconds", value }
    : { kind: "exact", time: value };
export const textPlanValues = (value: TextEditorValue): Values => ({
  name: value.name,
  text: value.text,
  start: textTimeInput(value.timeline_range.start),
  duration: textTimeInput(value.timeline_range.duration),
});

/** A read-only plan must never swap the original fixed head or write identity. */
export function checkTextPlan(
  request: TextPlanRequest,
  result: TextPlanResult,
) {
  if (
    result.revision !== request.expected_revision ||
    !result.initial
  )
    throw new Error("The text plan differs from the requested revision.");
  const tx = result.transaction;
  if (request.command.kind === "defaults") {
    if (tx || result.selection)
      throw new Error("Text defaults must not contain a transaction.");
    return;
  }
  if (
    tx &&
    (tx.project_id !== request.project_id ||
      tx.expected_revision !== request.expected_revision ||
      tx.idempotency_key !== request.idempotency_key ||
      tx.operations.length === 0)
  )
    throw new Error("The text plan differs from the original edit.");
  const target =
    request.command.kind === "edit"
      ? request.command.target
      : {
          element_id: request.command.element_id,
          track_id: request.command.track.track_id,
        };
  if (
    result.selection?.element_id !== target.element_id ||
    result.selection?.track_id !== target.track_id
  )
    throw new Error("The text plan differs from the selected title.");
}
