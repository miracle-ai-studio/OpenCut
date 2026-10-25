import test from "node:test";
import assert from "node:assert/strict";
import { prepareClassicAssSubtitle } from "../src/classic-subtitle-layout.mjs";
import {
  wrapCaptionText,
  wrapClassicSubtitleText,
} from "../src/caption-layout.mjs";
const ctx = () => ({
  font: "",
  letterSpacing: "0px",
  save() {},
  restore() {},
  measureText(s) {
    return {
      width: s.length * 10,
      actualBoundingBoxAscent: 8,
      actualBoundingBoxDescent: 2,
    };
  },
});
const text = (align = "center", background = false) => ({
  content: "A B C",
  align,
  decoration: "none",
  fontSizePx: 10,
  lineHeightPx: 12,
  letterSpacingPx: 0,
  color: [1, 1, 1, 1],
  background: {
    enabled: background,
    color: [0, 0, 0, 0.5],
    paddingXPx: 2,
    paddingYPx: 3,
    offsetXPx: 1,
    offsetYPx: 2,
    cornerRadiusPercent: 0,
  },
  transform: {
    centerX: 160,
    centerY: 90,
    scaleX: 1,
    scaleY: 1,
    rotationRadians: 0,
  },
});
const directive = {
  elementId: "cue",
  wrapProfile: "classic-v31-greedy-whitespace-v1",
  maxWidthPx: 30,
  bottomMarginPx: 9,
};
const placement = {
  left_margin_px: 10,
  right_margin_px: 20,
  vertical_margin_px: 30,
  vertical_align: "bottom",
};
test("Classic border3 background participates in source placement and greedy wrapping", () => {
  const result = prepareClassicAssSubtitle(
    ctx(),
    text("center", true),
    "fixed",
    { style: "normal", weight: 400 },
    directive,
    placement,
    { width: 320, height: 180 },
  );
  assert.equal(result.wrappedText, "A B\nC");
  assert.equal(result.text.transform.centerX, 154);
  assert.equal(result.text.transform.centerY, 133);
  assert.equal(result.measurements.length, 4);
});
test("all original vertical and horizontal placements use measured visual rectangles", () => {
  for (const align of ["left", "center", "right"])
    for (const vertical_align of ["top", "middle", "bottom"]) {
      const result = prepareClassicAssSubtitle(
        ctx(),
        text(align),
        "fixed",
        { style: "normal", weight: 400 },
        directive,
        { ...placement, vertical_align },
        { width: 320, height: 180 },
      );
      assert.equal(
        result.text.transform.centerX,
        align === "left" ? 10 : align === "right" ? 300 : 155,
      );
      assert.equal(
        result.text.transform.centerY,
        vertical_align === "top" ? 42 : vertical_align === "middle" ? 90 : 138,
      );
    }
});
test("zero-width source margin boxes preserve words without relaxing frozen V3", () => {
  assert.equal(
    wrapClassicSubtitleText("中文 A B", 0, (s) => s.length * 10).wrappedText,
    "中文\nA\nB",
  );
  assert.throws(
    () => wrapCaptionText("A B", 0, (s) => s.length * 10),
    /Invalid caption wrap/,
  );
  assert.throws(
    () => wrapClassicSubtitleText("A B", -1, (s) => s.length * 10),
    /Invalid Classic subtitle/,
  );
});
test("Classic normalization keeps continuous Chinese and explicit paragraphs", () => {
  const result = wrapClassicSubtitleText(
    "\ufeff中文编辑\n\n A  B\u00a0C ",
    25,
    (s) => s.length * 10,
  );
  assert.equal(result.wrappedText, "中文编辑\n\nA\nB\nC");
});
