import type { Element, GainPlan, GainRequest } from "./types";

// Object key order is not part of the generated JSON contract.
export function same(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  if (Array.isArray(a) || Array.isArray(b))
    return (
      Array.isArray(a) &&
      Array.isArray(b) &&
      a.length === b.length &&
      a.every((v, i) => same(v, b[i]))
    );
  if (
    a !== null &&
    b !== null &&
    typeof a === "object" &&
    typeof b === "object"
  ) {
    const left = Object.entries(a),
      right = Object.keys(b);
    return (
      left.length === right.length &&
      left.every(
        ([k, v]) =>
          Object.hasOwn(b, k) && same(v, (b as Record<string, unknown>)[k]),
      )
    );
  }
  return false;
}
/** Validate the response's command boundary; do not rewrite its transaction or
 * calculate the opaque server digest. The service still owns all domain rules. */
export function assertGainPlan(
  request: GainRequest,
  plan: GainPlan,
  element: Element,
  trackMuted: boolean,
  animated: boolean,
) {
  if (element.content.kind !== "video" && element.content.kind !== "audio")
    throw new Error("Select an audio element.");
  const old = element.content.audio;
  const gain =
    request.command.kind === "set_decibels"
      ? { unit: "decibels", value: request.command.decibels_tenths / 10 }
      : old.gain;
  const muted =
    request.command.kind === "set_muted" ? request.command.muted : old.muted;
  const decision = plan.decision,
    tx = plan.transaction;
  const expected = {
    ...element,
    content: { ...element.content, audio: { ...old, gain, muted } },
  };
  const changed = !same(expected, element);
  const operation = tx?.operations[0];
  if (
    !plan.original_request_digest ||
    decision.element_id !== request.element_id ||
    !same(decision.old_gain, old.gain) ||
    !same(decision.new_gain, gain) ||
    decision.old_element_muted !== old.muted ||
    decision.element_muted !== muted ||
    decision.track_muted !== trackMuted ||
    decision.animated !== animated ||
    changed !== Boolean(tx) ||
    (tx &&
      (tx.project_id !== request.project_id ||
        tx.expected_revision !== request.expected_revision ||
        tx.idempotency_key !== request.idempotency_key ||
        tx.label !== request.label ||
        tx.operations.length !== 1 ||
        operation?.type !== "element_put" ||
        operation.sequence_id !== request.sequence_id ||
        operation.track_id !== request.track_id ||
        !same(operation.element, expected)))
  )
    throw new Error("Volume plan does not match the captured request.");
}
