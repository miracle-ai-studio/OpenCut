import { expect, it } from "vitest";
import { canvasTextGeometry, canvasTextViewport } from "./canvas-geometry";
import type { CanvasTextPlan } from "./canvas-types";
const plan = (align = "center") =>
  ({
    width: 320,
    height: 180,
    block: { maxWidth: 80, height: 72 },
    layer: {
      text: {
        align,
        fontSizePx: 30,
        transform: {
          centerX: 177,
          centerY: 70,
          scaleX: 2,
          scaleY: 0.5,
          rotationRadians: Math.PI / 2,
        },
      },
    },
  }) as CanvasTextPlan;
it("places fixed-plan text through zoom and T*S*R without commuting rotation", () => {
  const geometry = canvasTextGeometry(plan(), 640);
  expect(geometry).toMatchObject({
    zoom: 2,
    width: 80,
    left: -40,
    top: -36,
    height: 72,
  });
  expect(geometry.transform).toBe(
    `translate(354px, 140px) scale(2) scale(2, 0.5) rotate(${Math.PI / 2}rad)`,
  );
});
it("retains the application align anchor and gives empty text an input target", () => {
  expect(canvasTextGeometry(plan("left"), 320).left).toBe(0);
  expect(canvasTextGeometry(plan("right"), 320).left).toBe(-80);
  const empty = plan();
  empty.block.maxWidth = 0;
  expect(canvasTextGeometry(empty, 320).width).toBe(30);
});

it("text targets follow the contained native image inside a wide maximum-height viewport", () => {
  const fitted = canvasTextViewport(plan(), { width: 800, height: 280 });
  expect(fitted.width).toBeCloseTo((320 * 280) / 180, 9);
  expect(fitted.height).toBe(280);
  expect(fitted.left).toBeCloseTo((800 - fitted.width) / 2, 9);
  expect(fitted.top).toBe(0);
});
