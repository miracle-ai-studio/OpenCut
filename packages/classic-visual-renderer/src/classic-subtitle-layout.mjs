// Classic subtitles/build-subtitle-text-element.ts. Fixed fonts and all metrics
// use the original renderer; this adapter only observes source-compatible layout.
import { measureClassicText } from "./text-layout.mjs";
import {
  wrapClassicSubtitleText,
  CAPTION_WRAP_PROFILE,
} from "./caption-layout.mjs";
import { finite, reject } from "./validation.mjs";
export function prepareClassicAssSubtitle(
  ctx,
  original,
  family,
  font,
  directive,
  placement,
  dimensions,
) {
  if (
    directive.elementId === undefined ||
    directive.wrapProfile !== CAPTION_WRAP_PROFILE ||
    !placement ||
    !["top", "middle", "bottom"].includes(placement.vertical_align)
  )
    reject("INVALID_VISUAL_PLAN", "Invalid Classic subtitle layout profile");
  const { width, height } = dimensions;
  for (const key of ["left_margin_px", "right_margin_px", "vertical_margin_px"])
    finite(placement[key], key);
  const wrapped = wrapClassicSubtitleText(
    original.content,
    directive.maxWidthPx,
    (content) =>
      measureClassicText(ctx, { ...original, content }, family, font).block
        .maxWidth,
  );
  const text = {
    ...original,
    content: wrapped.wrappedText,
    transform: { ...original.transform },
  };
  const layout = measureClassicText(ctx, text, family, font);
  const left =
    text.align === "left"
      ? 0
      : text.align === "right"
        ? -layout.block.maxWidth
        : -layout.block.maxWidth / 2;
  const visual = layout.background ?? {
    left,
    top: -layout.block.height / 2,
    width: layout.block.maxWidth,
    height: layout.block.height,
  };
  const l = placement.left_margin_px,
    r = placement.right_margin_px,
    m = placement.vertical_margin_px;
  text.transform.centerX =
    text.align === "left"
      ? l - visual.left
      : text.align === "right"
        ? width - r - (visual.left + visual.width)
        : l + (width - l - r) / 2 - (visual.left + visual.width / 2);
  text.transform.centerY =
    placement.vertical_align === "top"
      ? m - visual.top
      : placement.vertical_align === "middle"
        ? height / 2 - (visual.top + visual.height / 2)
        : height - m - (visual.top + visual.height);
  return { text, ...wrapped };
}
