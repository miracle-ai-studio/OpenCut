import type { V4TextFont, V4TextInitial } from "./types";

export const textFonts = (): V4TextFont[] => [
  {
    id: "regular",
    family: "Locked Example",
    weight: 400,
    style: "normal",
    face_index: 0,
    content: { id: "font-v1", content_hash: "f".repeat(64), size: 200 },
    origin: { kind: "legacy" },
  },
  {
    id: "bold-italic",
    family: "Locked Example",
    weight: 700,
    style: "italic",
    face_index: 0,
    content: { id: "font-v2", content_hash: "a".repeat(64), size: 220 },
    origin: { kind: "legacy" },
  },
];
export const textInitial = (): V4TextInitial => ({
  name: "Opening title",
  timeline_range: {
    start: { ticks: "1001", timebase: 30000 },
    duration: { ticks: "1001", timebase: 300 },
  },
  text: {
    content: "OpenCut\n你好",
    font: { kind: "locked", font_id: "regular" },
    font_weight: "normal",
    font_style: "normal",
    decoration: "none",
    align: "center",
    font_size: { unit: "classic_height90", value: 15 },
    color: {
      encoding: "linear_srgb",
      red: 0.123456789,
      green: 0.25,
      blue: 0.75,
      alpha: 0.654321,
    },
    letter_spacing_px: -0.75,
    line_height_multiplier: 1.2,
    background: {
      enabled: true,
      color: { encoding: "srgb", red: 0, green: 0, blue: 0, alpha: 0.6 },
      corner_radius_percent: 20,
      padding_x_at_size15: 7,
      padding_y_at_size15: 11,
      offset_x_px: -5,
      offset_y_px: 9,
    },
  },
});
