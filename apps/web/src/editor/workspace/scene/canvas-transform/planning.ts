import type { EditorApi } from "../../api";
import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { Element } from "../types";
import type { Transform, Handle } from "./geometry";

export type GestureIntent = Extract<ToolInput<"opencut_visual_preview_edit_draft_frame">["intent"], {kind:"gesture"}>["gesture"];
export function gestureIntent(handle: Handle, grouped: boolean): GestureIntent {
  if (handle === "move") return "translate";
  if (handle === "rotate") return "rotate";
  if (grouped) return "scale_xy";
  if (handle === "left" || handle === "right") return "scale_x";
  if (handle === "top" || handle === "bottom") return "scale_y";
  return "scale_xy";
}
function animated(element: Element, parameters: readonly string[]) {
  return element.channels.some(
    (c) =>
      c.kind === "scalar" &&
      c.target.kind === "element" &&
      c.target.element_id === element.id &&
      c.binding.kind === "property" &&
      parameters.includes(c.binding.parameter),
  );
}
export function gestureProblem(
  targets: readonly TransformTarget[],
  intent: GestureIntent,
): string | null {
  if (
    targets.some((t) =>
      animated(
        t.element,
        intent === "translate"
          ? ["position_x_px", "position_y_px"]
          : intent === "rotate"
            ? ["rotation_degrees"]
            : [],
      ),
    )
  )
    return "This gesture would overwrite an animated position or rotation. Use the keyframe editor.";
  if (
    targets.length > 1 &&
    intent !== "translate" &&
    targets.some((t) => animated(t.element, ["position_x_px", "position_y_px"]))
  )
    return "Group transforms move member positions. Edit position-animated elements separately.";
  return null;
}
export function clearsScaleAnimation(
  targets: readonly TransformTarget[],
  intent: GestureIntent,
) {
  return targets.some((t) =>
    animated(
      t.element,
      intent === "scale_x"
        ? ["scale_x"]
        : intent === "scale_y"
          ? ["scale_y"]
          : intent === "scale_xy"
            ? ["scale_x", "scale_y"]
            : [],
    ),
  );
}

/** Only authored fields affected by this gesture change. Unrelated sampled
 * fields remain driven by their original channels. */
export function transformElementForGesture(
  target: TransformTarget,
  next: Transform,
  intent: GestureIntent,
  grouped: boolean,
): Element {
  const element = structuredClone(target.element);
  if (!element.visual)
    throw Error("Choose visual elements before transforming them.");
  const transform = element.visual.transform;
  if (intent === "translate" || grouped) {
    transform.position_x_px = next.position_x_px;
    transform.position_y_px = next.position_y_px;
  }
  if (intent === "rotate") transform.rotation_degrees = next.rotation_degrees;
  if (intent === "scale_x" || intent === "scale_xy")
    transform.scale_x = next.scale_x;
  if (intent === "scale_y" || intent === "scale_xy")
    transform.scale_y = next.scale_y;
  const removed =
    intent === "scale_x"
      ? ["scale_x"]
      : intent === "scale_y"
        ? ["scale_y"]
        : intent === "scale_xy"
          ? ["scale_x", "scale_y"]
          : [];
  element.channels = element.channels.filter(
    (c) =>
      !(
        c.kind === "scalar" &&
        c.target.kind === "element" &&
        c.target.element_id === element.id &&
        c.binding.kind === "property" &&
        removed.includes(c.binding.parameter)
      ),
  );
  return element;
}

export type TransformTarget = { element: Element; trackId: string };
/** Match the pure Rust selector. Mask/effect/font/color/gain animations do not
 * change the element's own position, scale or rotation. */
export function hasElementTransformAnimation(element: Element): boolean {
  return element.channels.some(
    (channel) =>
      channel.kind === "scalar" &&
      channel.target.kind === "element" &&
      channel.target.element_id === element.id &&
      channel.binding.kind === "property" &&
      [
        "position_x_px",
        "position_y_px",
        "scale_x",
        "scale_y",
        "rotation_degrees",
      ].includes(channel.binding.parameter),
  );
}
export type TransformTransaction = ToolInput<"opencut_edit_apply">;
export type TransformPlanScope = {
  projectId: string;
  revision: string;
  sequenceId: string;
};
export type SampledTransform =
  ToolOutput<"opencut_element_transform_sample">["elements"][number];
export async function sampleTransforms(
  api: EditorApi,
  scope: TransformPlanScope,
  time: { ticks: string; timebase: number },
  targets: readonly TransformTarget[],
  signal: AbortSignal,
  current: () => boolean,
): Promise<SampledTransform[]> {
  const result = await api.sdk.call(
    "opencut_element_transform_sample",
    {
      project_id: scope.projectId,
      expected_revision: scope.revision,
      sequence_id: scope.sequenceId,
      time,
      element_ids: targets.map((t) => t.element.id),
    },
    { signal },
  );
  signal.throwIfAborted();
  if (!current())
    throw new DOMException(
      "Canvas gesture is no longer current.",
      "AbortError",
    );
  if (
    result.project_id !== scope.projectId ||
    result.revision !== scope.revision ||
    result.sequence_id !== scope.sequenceId ||
    !Number.isInteger(result.time.timebase) ||
    result.time.timebase < 1 ||
    result.time.timebase > 4294967295 ||
    !/^\d+$/.test(result.time.ticks) ||
    BigInt(result.time.ticks) * BigInt(time.timebase) !==
      BigInt(time.ticks) * BigInt(result.time.timebase) ||
    result.elements.length !== targets.length
  )
    throw Error("The sampled transforms differ from the fixed target.");
  return result.elements.map((entry, index) => {
    if (
      entry.element_id !== targets[index].element.id ||
      entry.track_id !== targets[index].trackId ||
      ![
        entry.transform.position_x_px,
        entry.transform.position_y_px,
        entry.transform.scale_x,
        entry.transform.scale_y,
        entry.transform.rotation_degrees,
      ].every(Number.isFinite) ||
      !entry.transform.scale_x ||
      !entry.transform.scale_y
    )
      throw Error("The sampled transforms differ from the fixed target.");
    const geometry = entry.geometry;
    if (targets[index].element.content.kind === "text") {
      if (geometry !== null)
        throw Error("Canvas geometry differs from the fixed target.");
    } else if (
      !geometry ||
      ![geometry.intrinsic_size.width, geometry.intrinsic_size.height].every(
        (v) => Number.isInteger(v) && v > 0 && v <= 4294967295,
      ) ||
      ![
        geometry.quad.width,
        geometry.quad.height,
        geometry.quad.centerX,
        geometry.quad.centerY,
        geometry.quad.rotationRadians,
        geometry.quad.opacity,
      ].every(Number.isFinite) ||
      geometry.quad.width <= 0 ||
      geometry.quad.height <= 0 ||
      geometry.quad.opacity < 0 ||
      geometry.quad.opacity > 1 ||
      typeof geometry.quad.flipX !== "boolean" ||
      typeof geometry.quad.flipY !== "boolean"
    ) {
      throw Error("Canvas geometry differs from the fixed target.");
    }
    return structuredClone(entry);
  });
}

/** Build the explicit gesture candidate. Rust checks this exact fixed-head
 * transaction before preview and the shared atomic commit. */
export async function planTransforms(
  scope: TransformPlanScope,
  targets: readonly TransformTarget[],
  transforms: readonly Transform[],
  key: string,
  signal: AbortSignal,
  current: () => boolean,
  intent: GestureIntent,
): Promise<TransformTransaction> {
  if (
    !targets.length ||
    targets.length > 60 ||
    targets.length !== transforms.length ||
    new Set(targets.map((t) => t.element.id)).size !== targets.length
  )
    throw Error("The canvas selection is not a unique supported group.");
  const transaction: TransformTransaction = {
    project_id: scope.projectId,
    expected_revision: scope.revision,
    idempotency_key: key,
    label: "Transform on canvas",
    operations: [],
  };
  const check = () => {
    signal.throwIfAborted();
    if (!current())
      throw new DOMException(
        "Canvas gesture is no longer current.",
        "AbortError",
      );
  };
    check();
    const problem = gestureProblem(targets, intent);
    if (problem) throw Error(problem);
    transaction.operations = targets.map((target, index) => ({
      type: "element_put",
      sequence_id: scope.sequenceId,
      track_id: target.trackId,
      element: transformElementForGesture(
        target,
        transforms[index],
        intent,
        targets.length > 1,
      ),
    }));
    return transaction;
}
