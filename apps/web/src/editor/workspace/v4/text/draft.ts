import { converter, formatHex } from "culori";
import { secondsForDisplay } from "../../../model/time";
import type {
  V4Text,
  V4TextEditorValue,
  V4TextFont,
  V4TextInitial,
} from "./types";

export type TextDraft = {
  name: string;
  content: string;
  fontId: string;
  size: string;
  color: string;
  opacity: string;
  align: V4Text["align"];
  decoration: V4Text["decoration"];
  letterSpacing: string;
  lineHeight: string;
  start: string;
  duration: string;
};

/** These are the two authored weights represented by TextV4. The importer and
 * renderer still validate bytes, face identity, licensing and glyph support. */
export const editableTextFonts = (fonts: readonly V4TextFont[]) =>
  fonts.filter((font) => font.weight === 400 || font.weight === 700);

const toRgb = converter("rgb");
function displayColor(color: V4Text["color"]) {
  return formatHex(
    toRgb({
      mode: color.encoding === "linear_srgb" ? "lrgb" : "rgb",
      r: color.red,
      g: color.green,
      b: color.blue,
    }),
  )!;
}

/** Display rounding is not an edit: the original color and time survive a save
 * unless their corresponding input actually changed. */
export function textDraft(initial: V4TextInitial): TextDraft {
  const text = initial.text;
  return {
    name: initial.name,
    content: text.content,
    fontId: text.font.kind === "locked" ? text.font.font_id : "",
    size: String(text.font_size.value),
    color: displayColor(text.color),
    opacity: String(text.color.alpha * 100),
    align: text.align,
    decoration: text.decoration,
    letterSpacing: String(text.letter_spacing_px),
    lineHeight: String(text.line_height_multiplier),
    start: secondsForDisplay(initial.timeline_range.start).toFixed(6),
    duration: secondsForDisplay(initial.timeline_range.duration).toFixed(6),
  };
}

function numeric(value: string, name: string, positive = false) {
  const result = value.trim() ? Number(value) : NaN;
  if (!Number.isFinite(result) || (positive && result <= 0))
    throw new Error(
      positive
        ? `${name} must be greater than zero.`
        : `${name} must be a finite number.`,
    );
  return result;
}

export function textDraftValue(
  initial: V4TextInitial,
  draft: TextDraft,
  fonts: readonly V4TextFont[],
): V4TextEditorValue {
  const original = textDraft(initial);
  const font = editableTextFonts(fonts).find(
    (item) => item.id === draft.fontId,
  );
  if (!font)
    throw new Error("Choose an imported font face before saving text.");
  const text = structuredClone(initial.text);
  text.content = draft.content;
  text.font = { kind: "locked", font_id: font.id };
  text.font_weight = font.weight === 700 ? "bold" : "normal";
  text.font_style = font.style;
  text.font_size.value = numeric(draft.size, "Font size", true);
  text.align = draft.align;
  text.decoration = draft.decoration;
  text.letter_spacing_px = numeric(draft.letterSpacing, "Letter spacing");
  text.line_height_multiplier = numeric(draft.lineHeight, "Line height", true);
  if (draft.color !== original.color) {
    if (!/^#[0-9a-f]{6}$/i.test(draft.color))
      throw new Error("Choose a valid text color.");
    text.color = {
      encoding: "srgb",
      red: parseInt(draft.color.slice(1, 3), 16) / 255,
      green: parseInt(draft.color.slice(3, 5), 16) / 255,
      blue: parseInt(draft.color.slice(5, 7), 16) / 255,
      alpha: initial.text.color.alpha,
    };
  }
  if (draft.opacity !== original.opacity) {
    const alpha = numeric(draft.opacity, "Opacity") / 100;
    if (alpha < 0 || alpha > 1)
      throw new Error("Opacity must be between 0 and 100.");
    text.color.alpha = alpha;
  }
  return {
    name: draft.name,
    text,
    timeline_range: {
      start:
        draft.start === original.start
          ? initial.timeline_range.start
          : draft.start,
      duration:
        draft.duration === original.duration
          ? initial.timeline_range.duration
          : draft.duration,
    },
  };
}
