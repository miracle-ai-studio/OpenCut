import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { EditorApi } from "../../api";
import { loadCanvasFont } from "../text/canvas-font";
import { measureClassicText } from "../visual-preview-renderer";
export type AssRequest = ToolInput<"opencut_subtitle_ass_v1_plan">;
export type AssPlan = ToolOutput<"opencut_subtitle_ass_v1_plan">;
type Observation = NonNullable<AssRequest["observations"]>[number];
type Metrics = Omit<
  Observation["layout"],
  | "fontId"
  | "fontContentHash"
  | "validatedTextHash"
  | "fontWeight"
  | "fontStyle"
  | "faceIndex"
  | "transform"
>;
export const assFontKey = (font: {
  family: string;
  weight: string;
  style: string;
}) => JSON.stringify([font.family, font.weight, font.style]);
/** Observe exact ASS text with the existing fixed-font renderer. No wrapping,
 * style conversion or placement is performed by this client. */
export async function planAssSubtitles(
  api: EditorApi,
  input: AssRequest,
  signal: AbortSignal,
): Promise<AssPlan> {
  const request = structuredClone({ ...input, observations: null });
  const prepared = await api.sdk.call("opencut_subtitle_ass_v1_plan", request, {
    signal,
  });
  signal.throwIfAborted();
  if (
    prepared.schema_version !== 1 ||
    prepared.revision !== request.expected_revision ||
    prepared.transaction ||
    new Set(prepared.cues.map((c) => c.layer.elementId)).size !==
      prepared.cues.length
  )
    throw new Error("The subtitle plan differs from the fixed revision.");
  if (prepared.font_requirements.some((f) => !f.resolved_font_id)) {
    if (prepared.cues.length || prepared.selection)
      throw new Error("The subtitle plan differs from the fixed revision.");
    return prepared;
  }
  if (!prepared.cues.length)
    throw new Error("The subtitle plan differs from the fixed revision.");
  const ctx = document.createElement("canvas").getContext("2d");
  if (!ctx) throw new Error("Subtitle text measurement is unavailable.");
  const fonts = new Map<string, Awaited<ReturnType<typeof loadCanvasFont>>>();
  try {
    const observations: Observation[] = [];
    for (const { layer } of prepared.cues) {
      signal.throwIfAborted();
      const textHash = bytesToHex(
        sha256(new TextEncoder().encode(layer.text.content)),
      );
      if (textHash !== layer.font.validatedTextHash)
        throw new Error("The subtitle layout target changed.");
      const key = JSON.stringify([
        layer.font.fontId,
        layer.font.contentHash,
        layer.font.weight,
        layer.font.style,
      ]);
      let font = fonts.get(key);
      if (!font) {
        font = await loadCanvasFont(
          api,
          {
            projectId: request.project_id,
            revision: request.expected_revision,
            sequenceId: request.sequence_id,
          },
          layer.font,
          signal,
        );
        fonts.set(key, font);
      }
      signal.throwIfAborted();
      const layout = measureClassicText(
        ctx,
        layer.text,
        font.family,
        layer.font,
      ) as Metrics;
      observations.push({
        elementId: layer.elementId,
        width: prepared.width,
        height: prepared.height,
        layout: {
          fontId: layer.font.fontId,
          fontContentHash: layer.font.contentHash,
          validatedTextHash: textHash,
          fontWeight: layer.font.weight,
          fontStyle: layer.font.style,
          faceIndex: layer.font.faceIndex,
          ...layout,
          transform: structuredClone(layer.text.transform),
        },
      });
    }
    const planned = await api.sdk.call(
      "opencut_subtitle_ass_v1_plan",
      { ...request, observations },
      { signal },
    );
    signal.throwIfAborted();
    if (
      planned.schema_version !== 1 ||
      planned.revision !== request.expected_revision ||
      planned.width !== prepared.width ||
      planned.height !== prepared.height ||
      !planned.transaction ||
      !planned.selection ||
      planned.transaction.project_id !== request.project_id ||
      planned.transaction.expected_revision !== request.expected_revision ||
      planned.transaction.idempotency_key !== request.idempotency_key ||
      planned.selection.track_id !== request.track.track_id ||
      planned.selection.element_id !== prepared.cues[0].layer.elementId ||
      JSON.stringify(planned.cues.map((c) => c.layer.elementId)) !==
        JSON.stringify(prepared.cues.map((c) => c.layer.elementId))
    )
      throw new Error("The subtitle plan differs from the fixed revision.");
    return planned;
  } finally {
    for (const font of fonts.values()) font.release();
  }
}
