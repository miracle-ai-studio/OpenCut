import type { GraphicPlanRequest, GraphicPlanResult } from "./types";
/** Plans retain fixed scope, write key and target; only Rust owns the resulting edit. */
export function checkGraphicPlan(
  request: GraphicPlanRequest,
  result: GraphicPlanResult,
) {
  if (
    result.revision !== request.expected_revision ||
    !result.initial
  )
    throw new Error("The graphic plan differs from the fixed revision.");
  if (request.command.kind === "defaults") {
    if (
      result.transaction ||
      result.selection ||
      result.initial.graphic.geometry.kind !== request.command.geometry
    )
      throw new Error("Graphic defaults differ from the selected shape.");
    return;
  }
  const target =
      request.command.kind === "edit"
        ? request.command.target
        : {
            element_id: request.command.element_id,
            track_id: request.command.track.track_id,
          },
    tx = result.transaction;
  if (
    (request.command.kind === "create" && !tx) ||
    (tx &&
      (tx.project_id !== request.project_id ||
        tx.expected_revision !== request.expected_revision ||
        tx.idempotency_key !== request.idempotency_key ||
        !tx.operations.length)) ||
    result.selection?.element_id !== target.element_id ||
    result.selection?.track_id !== target.track_id
  )
    throw new Error("The graphic plan differs from the original edit.");
}
