import { describe, expect, it } from "vitest";
import { editableTextFonts, textDraft, textDraftValue } from "./draft";
import { textFonts, textInitial } from "./fixture";

describe("fixed-font text authoring input", () => {
  it("preserves exact source time, linear color, alpha and background while changing content", () => {
    const initial = textInitial(),
      draft = textDraft(initial);
    draft.content = "Edited\n中英";
    const value = textDraftValue(initial, draft, textFonts());
    expect(value.text).toEqual({ ...initial.text, content: "Edited\n中英" });
    expect(value.timeline_range.start).toBe(initial.timeline_range.start);
    expect(value.timeline_range.duration).toBe(initial.timeline_range.duration);
    expect(initial.text.content).toBe("OpenCut\n你好");
    expect(value.text.background).not.toBe(initial.text.background);
  });

  it("passes edited decimal time verbatim for the authoritative planner to parse", () => {
    const initial = textInitial(),
      draft = textDraft(initial);
    draft.start = " 1.001000 ";
    draft.duration = "0.333333";
    const value = textDraftValue(initial, draft, textFonts());
    expect(value.timeline_range).toEqual({
      start: " 1.001000 ",
      duration: "0.333333",
    });
    draft.duration = "invalid";
    expect(
      textDraftValue(initial, draft, textFonts()).timeline_range.duration,
    ).toBe("invalid");
  });

  it("changes actual locked face, style and weight atomically without synthesizing bold or italic", () => {
    const initial = textInitial(),
      draft = textDraft(initial),
      fonts = textFonts();
    draft.fontId = "bold-italic";
    const { text } = textDraftValue(initial, draft, fonts);
    expect(text.font).toEqual({ kind: "locked", font_id: "bold-italic" });
    expect(text.font_style).toBe("italic");
    expect(text.font_weight).toBe("bold");
    const unrepresented = { ...fonts[0], id: "medium", weight: 500 };
    expect(editableTextFonts([...fonts, unrepresented])).toEqual(fonts);
    draft.fontId = "medium";
    expect(() =>
      textDraftValue(initial, draft, [...fonts, unrepresented]),
    ).toThrow("imported font face");
    draft.fontId = "bold-italic";
    expect(() => textDraftValue(initial, draft, [])).toThrow(
      "imported font face",
    );
  });

  it("applies intentional display color and alpha edits without silently quantizing other channels", () => {
    const initial = textInitial(),
      draft = textDraft(initial);
    draft.opacity = "50";
    expect(textDraftValue(initial, draft, textFonts()).text.color).toEqual({
      ...initial.text.color,
      alpha: 0.5,
    });
    draft.color = "#ff0080";
    expect(textDraftValue(initial, draft, textFonts()).text.color).toEqual({
      encoding: "srgb",
      red: 1,
      green: 0,
      blue: 128 / 255,
      alpha: 0.5,
    });
    expect(initial.text.color.encoding).toBe("linear_srgb");
  });

  it("rejects invalid numeric authoring rather than coercing blank fields or clamping overshoot", () => {
    const initial = textInitial(),
      original = textDraft(initial);
    for (const size of ["", "0", "-1", "NaN", "Infinity"])
      expect(() =>
        textDraftValue(initial, { ...original, size }, textFonts()),
      ).toThrow("Font size");
    expect(() =>
      textDraftValue(initial, { ...original, lineHeight: "0" }, textFonts()),
    ).toThrow("Line height");
    expect(() =>
      textDraftValue(initial, { ...original, opacity: "101" }, textFonts()),
    ).toThrow("Opacity");
    expect(
      textDraftValue(
        initial,
        { ...original, letterSpacing: "-40.5" },
        textFonts(),
      ).text.letter_spacing_px,
    ).toBe(-40.5);
  });
});

describe("text background authoring", () => {
  it("edits every background channel while keeping all unrelated authored fields and exact time", () => {
    const initial = textInitial(),
      draft = textDraft(initial);
    Object.assign(draft, {
      backgroundEnabled: false,
      backgroundColor: "#804020",
      backgroundOpacity: "35",
      backgroundRadius: "62.5",
      backgroundPaddingX: "18.75",
      backgroundPaddingY: "0",
      backgroundOffsetX: "-12.5",
      backgroundOffsetY: "3.25",
    });
    const value = textDraftValue(initial, draft, textFonts());
    expect(value.text).toEqual({
      ...initial.text,
      background: {
        enabled: false,
        color: {
          encoding: "srgb",
          red: 128 / 255,
          green: 64 / 255,
          blue: 32 / 255,
          alpha: 0.35,
        },
        corner_radius_percent: 62.5,
        padding_x_at_size15: 18.75,
        padding_y_at_size15: 0,
        offset_x_px: -12.5,
        offset_y_px: 3.25,
      },
    });
    expect(value.timeline_range.start).toBe(initial.timeline_range.start);
    expect(value.timeline_range.duration).toBe(initial.timeline_range.duration);
    expect(initial.text.background.padding_x_at_size15).toBe(7);
  });
  it("toggle and geometry preserve unedited linear color and exact alpha instead of display quantization", () => {
    const initial = textInitial();
    initial.text.background.color = { ...initial.text.color };
    const draft = textDraft(initial);
    draft.backgroundEnabled = false;
    draft.backgroundOffsetX = "10";
    const value = textDraftValue(initial, draft, textFonts());
    expect(value.text.background.color).toEqual(initial.text.background.color);
    draft.backgroundOpacity = "25";
    expect(
      textDraftValue(initial, draft, textFonts()).text.background.color,
    ).toEqual({ ...initial.text.background.color, alpha: 0.25 });
  });
  it.each([
    ["backgroundRadius", "101"],
    ["backgroundRadius", "-1"],
    ["backgroundPaddingX", "-0.1"],
    ["backgroundPaddingY", "-1"],
    ["backgroundOffsetX", "NaN"],
    ["backgroundOffsetY", "Infinity"],
    ["backgroundOpacity", "101"],
    ["backgroundOpacity", "-1"],
    ["backgroundColor", "invalid"],
    ["backgroundRadius", ""],
  ])("rejects %s=%s without clamping or a partial value", (field, value) => {
    const initial = textInitial();
    expect(() =>
      textDraftValue(
        initial,
        { ...textDraft(initial), [field]: value },
        textFonts(),
      ),
    ).toThrow();
  });
});
