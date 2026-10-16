import { FONT_SYNTHESIS_PROVIDER_V1 } from "./text/font-synthesis";
import type { V4Sequence, V4SessionState } from "./types";

export type VisualPreviewProfile =
  "text" | "caption" | "graphic" | "blend" | "mask" | "scene";

export function viewedSequence(state: V4SessionState) {
  return (state.historical ?? state.head).project.sequences.find(
    (sequence) => sequence.id === state.sequenceId,
  );
}

/** Frame execution samples enabled layers; video admission checks the whole
 * authored scene, including disabled elements and content between frame times. */
export function sceneVisualProfile(
  sequence: V4Sequence | undefined,
  scope: "visible" | "authored" = "visible",
  settings?: V4SessionState["head"]["project"]["settings"],
  fonts?: V4SessionState["head"]["project"]["fonts"],
): VisualPreviewProfile {
  if (
    fonts?.some(
      (font) =>
        font.origin.kind === "generated" &&
        font.origin.provider === FONT_SYNTHESIS_PROVIDER_V1,
    )
  )
    return "scene";
  const elements =
    sequence?.tracks
      .filter((track) => scope === "authored" || track.enabled)
      .flatMap((track) => track.elements)
      .filter((element) => scope === "authored" || element.enabled) ?? [];
  const background =
    settings?.profile === "classic_global"
      ? settings.settings.background
      : settings?.formats.find((f) => f.sequence_id === sequence?.id)
          ?.background;
  if (
    background?.kind === "main_track_blur" ||
    elements.some(
      (e) =>
        e.content.kind === "effect_layer" ||
        e.content.kind === "sticker" ||
        (e.visual &&
          (e.visual.effects.length > 0 ||
            e.visual.masks.filter((m) => m.enabled).length > 1 ||
            e.visual.masks.some(
              (m) =>
                m.enabled &&
                (m.inverted ||
                  m.stroke.width_px > 0 ||
                  !["rectangle", "ellipse"].includes(m.shape.kind)),
            ))),
    )
  )
    return "scene";
  if (
    elements.some((element) =>
      element.visual?.masks.some((mask) => mask.enabled),
    )
  )
    return "mask";
  if (
    elements.some(
      (element) => element.visual && element.visual.blend_mode !== "normal",
    )
  )
    return "blend";
  if (elements.some((element) => element.content.kind === "graphic"))
    return "graphic";
  return sequence?.captions.length ? "caption" : "text";
}

export function viewedVisualProfile(
  state: V4SessionState,
  scope: "visible" | "authored" = "visible",
) {
  return sceneVisualProfile(
    viewedSequence(state),
    scope,
    (state.historical ?? state.head).project.settings,
    (state.historical ?? state.head).project.fonts,
  );
}
