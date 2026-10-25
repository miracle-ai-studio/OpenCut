import type { GraphicInitial } from "./types";
export const graphicInitial = (): GraphicInitial => ({
  name: "Independent shape",
  timeline_range: {
    start: { ticks: "1001", timebase: 30000 },
    duration: { ticks: "1001", timebase: 300 },
  },
  graphic: {
    geometry: { kind: "rectangle", corner_radius_percent: 17.25 },
    source_size: { width: 160, height: 90 },
    fill: {
      encoding: "linear_srgb",
      red: 0.123456789,
      green: 0.25,
      blue: 0.75,
      alpha: 0.654321,
    },
    stroke: {
      color: {
        encoding: "srgb",
        red: 0.31,
        green: 0.42,
        blue: 0.53,
        alpha: 0.87654321,
      },
      width_px: 2.25,
      align: "outside",
    },
  },
  transform: {
    position_x_px: 17.125,
    position_y_px: -20,
    scale_x: 1.25,
    scale_y: 0.75,
    rotation_degrees: 12.25,
  },
  opacity: 0.91234567,
});
