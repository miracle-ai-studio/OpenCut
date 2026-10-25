import type { ScalarChannel } from "../keyframes/types";
export const gainFixture = (): ScalarChannel => ({
  id: "gain",
  kind: "scalar",
  target: { kind: "element", element_id: "audio" },
  binding: { kind: "property", parameter: "gain_decibels" },
  extrapolation: { before: "hold", after: "linear" },
  keys: [
    {
      id: "first",
      time: { ticks: "0", timebase: 120000 },
      value: -20,
      left_handle: null,
      right_handle: { dt: { ticks: "1001", timebase: 30000 }, dv: 45 },
      segment_to_next: { kind: "bezier" },
      tangent_mode: "broken",
    },
    {
      id: "second",
      time: { ticks: "2", timebase: 1 },
      value: -5,
      left_handle: { dt: { ticks: "-1", timebase: 3 }, dv: 3 },
      right_handle: null,
      segment_to_next: { kind: "linear" },
      tangent_mode: "aligned",
    },
  ],
});
