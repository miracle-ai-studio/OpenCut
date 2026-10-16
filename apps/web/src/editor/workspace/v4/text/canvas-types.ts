import type { ToolOutput } from "../../../../lib/editor-sdk";
import type { V4TextScope } from "./types";
import type { FontSynthesisBinding } from "./font-synthesis";
export type CanvasTextLayer =
  ToolOutput<"opencut_visual_preview_v4_text_frame">["plan"]["texts"][number];
export type CanvasTextPlan = {
  scope: V4TextScope;
  width: number;
  height: number;
  layer: CanvasTextLayer;
  /** Actual wire metadata used to register FontFace. layer.font may carry the
   * explicit requested style solely for the UI's Canvas/text-input request. */
  actualFont?: CanvasTextLayer["font"];
  fontSynthesis?: FontSynthesisBinding;
  /** Metrics from the existing fixed-font Classic execution layout. */
  block: { maxWidth: number; height: number };
};
export type CanvasTextPlanning = (
  scope: V4TextScope,
  time: { ticks: string; timebase: number },
  signal: AbortSignal,
) => Promise<CanvasTextPlan>;
