import { describe, it, expect } from "vitest";
import { copyNames, sortProjects } from "./library";
import type { ProjectRef } from "../model/types";
const project = (
  id: string,
  name: string,
  ticks: string,
  timebase: number,
  date: string,
): ProjectRef => ({
  id,
  name,
  revision: id,
  format: "visual",
  library: {
    project_id: id,
    name,
    revision: id,
    format: "visual",
    created_at: date,
    modified_at: date,
    duration: { ticks, timebase },
    width: 640,
    height: 360,
    fps: { numerator: 25, denominator: 1 },
    scene_count: 1,
    asset_count: 0,
  },
});
describe("Classic project library behavior", () => {
  it("sorts exact mixed timebases and preserves deterministic ties", () => {
    const a = project("a", "Z", "1", 2, "2026-09-29T12:00:00Z"),
      b = project("b", "A", "3", 5, "2026-09-30T12:00:00Z");
    expect(sortProjects([b, a], "duration", true)).toEqual([a, b]);
    expect(sortProjects([a, b], "name", true)).toEqual([b, a]);
    expect(sortProjects([a, b], "modified", false)).toEqual([b, a]);
    const c = project("c", "Z", "5", 10, "2026-09-29T12:00:00Z");
    expect(sortProjects([c, a], "duration", false)).toEqual([a, c]);
  });
  it("continues original numbered names across batch copies and copies of copies", () => {
    const list = ["Trip", "(1) Trip", "(3) Trip", "Notes"].map((name, i) =>
      project(String(i), name, "0", 1, ""),
    );
    expect(copyNames(list, [list[0], list[1], list[3]])).toEqual([
      "(4) Trip",
      "(5) Trip",
      "(1) Notes",
    ]);
  });
});
