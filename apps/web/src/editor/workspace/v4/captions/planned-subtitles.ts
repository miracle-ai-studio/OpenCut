import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { EditorApi } from "../../api";
import { loadCanvasFont } from "../text/canvas-font";
import {
  measureClassicText,
  prepareCaptionTextV3,
} from "../visual-preview-renderer";

export type SubtitleTextRequest = ToolInput<"opencut_subtitle_text_v1_plan">;
export type SubtitleTextPlan = ToolOutput<"opencut_subtitle_text_v1_plan">;
type Observation = NonNullable<SubtitleTextRequest["observations"]>[number];
type Metrics = Omit<
  Observation["raster"]["layout"],
  | "fontId"
  | "fontContentHash"
  | "validatedTextHash"
  | "fontWeight"
  | "fontStyle"
  | "faceIndex"
  | "transform"
>;

/** The application selects cues, IDs, timing and placement. This consumer only
 * observes the existing renderer with an authorized, hash-verified fixed font. */
export async function planEditableSubtitles(
  api: EditorApi,
  input: SubtitleTextRequest,
  signal: AbortSignal,
): Promise<SubtitleTextPlan> {
  const request = structuredClone({ ...input, observations: null });
  const prepared = await api.sdk.call(
    "opencut_subtitle_text_v1_plan",
    request,
    { signal },
  );
  signal.throwIfAborted();
  if (
    prepared.schema_version !== 1 ||
    prepared.revision !== request.expected_revision ||
    prepared.transaction ||
    !prepared.cues.length ||
    new Set(prepared.cues.map((cue) => cue.layer.elementId)).size !==
      prepared.cues.length
  )
    throw new Error("The subtitle plan differs from the fixed revision.");
  const ctx = document.createElement("canvas").getContext("2d");
  if (!ctx) throw new Error("Subtitle text measurement is unavailable.");
  const fonts = new Map<string, Awaited<ReturnType<typeof loadCanvasFont>>>();
  try {
    const observations: Observation[] = [];
    for (const cue of prepared.cues) {
      signal.throwIfAborted();
      const { layer, directive } = cue;
      if (directive.elementId !== layer.elementId)
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
      const dimensions = { width: prepared.width, height: prepared.height };
      const wrapped = prepareCaptionTextV3(
        ctx,
        layer.text,
        font.family,
        layer.font,
        directive,
        dimensions,
      );
      // The private renderer declaration exposes only its compact legacy metric
      // view. Runtime returns the complete layout used by frame V2/V3; no metric
      // or wrapping algorithm is reproduced here. Rust validates every field.
      const text = wrapped.text as typeof layer.text;
      if (text.content !== wrapped.wrappedText)
        throw new Error("The subtitle layout target changed.");
      const wrappedHash = bytesToHex(
        sha256(new TextEncoder().encode(text.content)),
      );
      const layout = measureClassicText(
        ctx,
        text,
        font.family,
        layer.font,
      ) as Metrics;
      observations.push({
        caption: {
          elementId: layer.elementId,
          originalTextHash: layer.font.validatedTextHash,
          wrappedText: wrapped.wrappedText,
          measurements: wrapped.measurements,
        },
        raster: {
          elementId: layer.elementId,
          ...dimensions,
          layout: {
            fontId: layer.font.fontId,
            fontContentHash: layer.font.contentHash,
            validatedTextHash: wrappedHash,
            fontWeight: layer.font.weight,
            fontStyle: layer.font.style,
            faceIndex: layer.font.faceIndex,
            ...layout,
            transform: structuredClone(text.transform),
          },
        },
      });
    }
    signal.throwIfAborted();
    const planned = await api.sdk.call(
      "opencut_subtitle_text_v1_plan",
      { ...request, observations },
      { signal },
    );
    if (
      planned.schema_version !== 1 ||
      planned.revision !== request.expected_revision ||
      !planned.transaction ||
      !planned.selection ||
      planned.transaction.project_id !== request.project_id ||
      planned.transaction.expected_revision !== request.expected_revision ||
      planned.transaction.idempotency_key !== request.idempotency_key ||
      planned.width !== prepared.width ||
      planned.height !== prepared.height ||
      JSON.stringify(planned.cues.map((cue) => cue.layer.elementId)) !==
        JSON.stringify(prepared.cues.map((cue) => cue.layer.elementId)) ||
      planned.selection.element_id !== prepared.cues[0].layer.elementId ||
      planned.selection.track_id !== request.track.track_id
    )
      throw new Error("The subtitle plan differs from the fixed revision.");
    return planned;
  } finally {
    for (const font of fonts.values()) font.release();
  }
}
