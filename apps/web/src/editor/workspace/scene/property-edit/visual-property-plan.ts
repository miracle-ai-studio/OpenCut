import type { EditorApi } from "../../api";
import type { ToolInput } from "../../../../lib/editor-sdk";
import { same } from "../audio-gain-plan";
import { expectedVisualElement, type VisualField, type VisualTarget } from "./visual-properties";

export type VisualPropertyPlan = {
  field: VisualField; value: number;
  transaction: ToolInput<"opencut_edit_apply">;
  expected: VisualTarget[];
};
export async function planVisualProperty(input: {
  api: EditorApi; project: string; revision: string; sequence: string;
  targets: readonly VisualTarget[]; field: VisualField; value: number;
  signal: AbortSignal; check(): void;
}): Promise<VisualPropertyPlan> {
  const {api, project, revision, sequence, targets, field, value, signal, check} = input;
  const id = crypto.randomUUID(), label = "Change clip picture property";
  const transaction: ToolInput<"opencut_edit_apply"> = {
    project_id: project, expected_revision: revision, idempotency_key: id, label, operations: [],
  };
  const expected = targets.map(target => ({...target, element:expectedVisualElement(target.element, field, value)}));
  for (let i=0; i<targets.length; i++) {
    const target = targets[i], next = expected[i];
    if (same(target.element, next.element)) continue;
    signal.throwIfAborted(); check();
    const plan = await api.sdk.call("opencut_element_properties_plan", {
      project_id:project, expected_revision:revision, sequence_id:sequence,
      element_id:target.element.id, idempotency_key:id, label,
      transform:field === "opacity" ? null : next.element.visual!.transform,
      opacity:field === "opacity" ? value : null, audio_gain:null, muted:null, retime:null,
    }, {signal});
    signal.throwIfAborted(); check();
    const op = plan.operations[0];
    if (plan.project_id !== project || plan.expected_revision !== revision || plan.idempotency_key !== id ||
        plan.label !== label || plan.branch_name != null || plan.operations.length !== 1 ||
        op?.type !== "element_put" || op.sequence_id !== sequence || op.track_id !== target.trackId || !same(op.element,next.element))
      throw Error("The picture property plan changed fields outside the requested edit.");
    transaction.operations.push(op);
  }
  return {field, value, transaction, expected};
}
