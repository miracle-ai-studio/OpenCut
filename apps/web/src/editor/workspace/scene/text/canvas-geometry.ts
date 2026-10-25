import type { CanvasTextPlan } from "./canvas-types";
/** Consume the sampled application transform; CSS multiplication is T·S·R,
 * including nonuniform scale. Viewport zoom is outside that transform. */
export function canvasTextGeometry(
  plan: CanvasTextPlan,
  viewportWidth: number,
) {
  const { text } = plan.layer,
    tr = text.transform;
  const zoom = viewportWidth / plan.width;
  const width = Math.max(plan.block.maxWidth, text.fontSizePx);
  const left =
    text.align === "left" ? 0 : text.align === "right" ? -width : -width / 2;
  return {
    zoom,
    width,
    height: plan.block.height,
    left,
    top: -plan.block.height / 2,
    transform: `translate(${tr.centerX * zoom}px, ${tr.centerY * zoom}px) scale(${zoom}) scale(${tr.scaleX}, ${tr.scaleY}) rotate(${tr.rotationRadians}rad)`,
  };
}

/** Match the native image/canvas `object-fit:contain` rectangle, including
 * letterboxing caused by the editor's maximum preview height. */
export function canvasTextViewport(
  plan: Pick<CanvasTextPlan, "width" | "height">,
  viewport: { width: number; height: number },
) {
  const fit = Math.min(
    viewport.width / plan.width,
    viewport.height / plan.height,
  );
  return {
    width: plan.width * fit,
    height: plan.height * fit,
    left: (viewport.width - plan.width * fit) / 2,
    top: (viewport.height - plan.height * fit) / 2,
  };
}
