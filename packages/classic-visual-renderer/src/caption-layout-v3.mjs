import { measureClassicText } from "./text-layout.mjs";
import { finite, reject } from "./validation.mjs";
export const CAPTION_WRAP_PROFILE_V3 = "classic-v31-greedy-whitespace-v1";
/** Classic uses whitespace-separated words; oversized words and continuous
 * Chinese stay intact. Explicit paragraph breaks and empty paragraphs survive. */
export function wrapCaptionTextV3(content, maxWidth, measure) {
  if (typeof content !== "string" || finite(maxWidth, "caption width") <= 0) reject("INVALID_VISUAL_PLAN", "Invalid caption wrap input");
  return wrapClassicSubtitleTextV1(content,maxWidth,measure);
}
/** Exact Classic word wrapping also accepts an exhausted 0px margin box. */
export function wrapClassicSubtitleTextV1(content,maxWidth,measure) {
  if(typeof content!=="string"||finite(maxWidth,"Classic subtitle width")<0)reject("INVALID_VISUAL_PLAN","Invalid Classic subtitle wrap input");
  const measurements = [], width = text => {
    const value = finite(measure(text), "caption candidate width");
    if (value < 0) reject("INVALID_VISUAL_PLAN", "Negative caption candidate width");
    measurements.push({ content: text, width: value }); return value;
  };
  const wrappedText = content.trim().replace(/\r\n/g, "\n").split("\n").map(paragraph => {
    const trimmed = paragraph.trim(); if (!trimmed) return "";
    const words = trimmed.split(/\s+/); let current = words[0]; const lines = [];
    for (const word of words.slice(1)) {
      const candidate = `${current} ${word}`;
      if (width(candidate) <= maxWidth) current = candidate;
      else { lines.push(current); current = word; }
    }
    lines.push(current); return lines.join("\n");
  }).join("\n");
  for (const line of wrappedText.split("\n")) width(line);
  return { wrappedText, measurements };
}
export function prepareCaptionTextV3(ctx, original, family, font, directive, dimensions) {
  if (directive.wrapProfile !== CAPTION_WRAP_PROFILE_V3 || finite(directive.bottomMarginPx, "caption bottom margin") !== dimensions.height * .05 || directive.maxWidthPx > dimensions.width)
    reject("INVALID_VISUAL_PLAN", "Caption layout profile differs from the admitted fixed canvas");
  // The shared measure routine sets the exact font, baseline and spacing. It
  // supplies all greedy candidate widths rather than a second metric routine.
  const wrapped = wrapCaptionTextV3(original.content, directive.maxWidthPx, content => measureClassicText(ctx, { ...original, content }, family, font).block.maxWidth);
  const text = { ...original, content: wrapped.wrappedText, transform: { ...original.transform } };
  const layout = measureClassicText(ctx, text, family, font);
  const left = text.align === "left" ? 0 : text.align === "right" ? -layout.block.maxWidth : -layout.block.maxWidth / 2;
  const visual = layout.background ?? { left, top: -layout.block.height / 2, width: layout.block.maxWidth, height: layout.block.height };
  text.transform.centerX = text.align === "left" ? -visual.left : text.align === "right" ? dimensions.width - (visual.left + visual.width) : dimensions.width / 2 - (visual.left + visual.width / 2);
  text.transform.centerY = dimensions.height - directive.bottomMarginPx - (visual.top + visual.height);
  return { text, ...wrapped };
}
