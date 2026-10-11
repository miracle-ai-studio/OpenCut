import { describe, expect, it } from "vitest";
import { previewGeometry } from "./viewport";
describe("preview view coordinates", () => {
  it("fits an exact landscape canvas and maps actual-size zoom independently", () => {
    const fit = previewGeometry(1920, 1080, 848, 498, 1, { x: 80, y: -60 });
    expect(fit.width).toBeCloseTo(800);
    expect(fit.height).toBeCloseTo(450);
    expect(fit.pan).toEqual({ x: 0, y: 0 });
    const actual = previewGeometry(1920, 1080, 848, 498, 1 / fit.fit, {
      x: 900,
      y: -700,
    });
    expect(actual.width).toBeCloseTo(1920);
    expect(actual.height).toBeCloseTo(1080);
    expect(actual.pan.x).toBeCloseTo(560);
    expect(actual.pan.y).toBeCloseTo(-315);
  });
  it("bounds relative zoom and reclamps panning on window changes", () => {
    expect(previewGeometry(100, 200, 248, 248, 99, { x: 0, y: 0 }).scale).toBe(
      16,
    );
    const fit = previewGeometry(100, 200, 248, 248, 0.01, { x: 99, y: 99 });
    expect(fit.width).toBe(25);
    expect(fit.pan).toEqual({ x: 0, y: 0 });
    expect(
      previewGeometry(1920, 1080, 848, 498, 1, { x: 560, y: -315 }).pan,
    ).toEqual({ x: 0, y: 0 });
  });
});
