import { measureClassicText } from "../visual-preview-renderer";
import type { EditorApi } from "../../api";
import type { CanvasTextPlanning } from "./canvas-types";
import { loadCanvasFont } from "./canvas-font";

/** Application owns sampling/geometry and native font coverage. The existing
 * Classic executor owns browser font metrics; DOM never invents a layout. */
export function canvasTextPlanner(api: EditorApi): CanvasTextPlanning {
  return async (scope, time, signal) => {
    if (!scope.elementId) throw new Error("Canvas text element is missing.");
    const result = await api.sdk.call(
      "opencut_text_v1_canvas_plan",
      {
        project_id: scope.projectId,
        revision: scope.revision,
        sequence_id: scope.sequenceId,
        element_id: scope.elementId,
        time,
      },
      { signal },
    );
    const identity = result.identity;
    if (
      result.schema_version !== 1 ||
      identity.projectId !== scope.projectId ||
      identity.revision !== scope.revision ||
      identity.sequenceId !== scope.sequenceId ||
      result.layer.elementId !== scope.elementId ||
      identity.time.ticks !== time.ticks ||
      identity.time.timebase !== time.timebase
    )
      throw new Error("The edit target changed.");
    const font = await loadCanvasFont(api, scope, result.layer.font, signal);
    try {
      signal.throwIfAborted();
      const ctx = document.createElement("canvas").getContext("2d");
      if (!ctx) throw new Error("Canvas text editing unavailable");
      const layout = measureClassicText(
        ctx,
        result.layer.text,
        font.family,
        result.layer.font,
      );
      return {
        scope,
        width: identity.width,
        height: identity.height,
        layer: result.layer,
        block: layout.block,
      };
    } finally {
      font.release();
    }
  };
}
