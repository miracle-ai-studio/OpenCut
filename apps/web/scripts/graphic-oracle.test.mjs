import test from "node:test";
import assert from "node:assert/strict";
import {
  composeGraphicOracle,
  measureGraphicOracle,
  sampleStraightRgba,
} from "./graphic-oracle.mjs";

const source = (width, height, data) => ({ width, height, data });
const solid = (width, height, rgba) =>
  source(
    width,
    height,
    Array.from({ length: width * height }, () => rgba).flat(),
  );
const layer = (source, overrides = {}) => ({
  source,
  centerX: 1,
  centerY: 1,
  scaleX: 1,
  scaleY: 1,
  rotation: 0,
  opacity: 1,
  ...overrides,
});
const pixel = (rgba, x, y, width) =>
  Array.from(rgba.slice((y * width + x) * 4, (y * width + x + 1) * 4));

test("straight-alpha interpolation differs from Canvas premultiplied interpolation", () => {
  // Halfway from opaque red to transparent black: RGB = 127.5, A = 127.5.
  // Rounded layer bytes (128, 128) over black give red round(128*128/255)=64.
  // Canvas premultiplied interpolation would incorrectly predict red 128.
  const edge = source(2, 1, [255, 0, 0, 255, 0, 0, 0, 0]);
  assert.deepEqual(sampleStraightRgba(edge, 1, 0.5), [127.5, 0, 0, 127.5]);
  const result = composeGraphicOracle(
    [layer(edge, { centerX: 0.5, centerY: 0.5 })],
    1,
    1,
  );
  assert.deepEqual(Array.from(result), [64, 0, 0, 255]);
  assert.notDeepEqual(Array.from(result), [128, 0, 0, 255]);
});

test("linear interpolation uses all four neighboring straight RGBA texels", () => {
  const corners = source(
    2,
    2,
    [200, 0, 0, 255, 0, 100, 0, 128, 0, 0, 80, 64, 40, 40, 40, 0],
  );
  assert.deepEqual(sampleStraightRgba(corners, 1, 1), [60, 35, 30, 111.75]);
});

test("texel centers are half integer and edge sampling clamps within the source", () => {
  const pair = source(2, 1, [200, 0, 0, 255, 0, 100, 0, 255]);
  assert.deepEqual(sampleStraightRgba(pair, 0.5, 0.5), [200, 0, 0, 255]);
  assert.deepEqual(sampleStraightRgba(pair, 1.5, 0.5), [0, 100, 0, 255]);
  assert.deepEqual(sampleStraightRgba(pair, 0, 0), [200, 0, 0, 255]);
  assert.deepEqual(sampleStraightRgba(pair, 2, 1), [0, 100, 0, 255]);
  assert.equal(sampleStraightRgba(pair, -0.001, 0.5), null);
  assert.equal(sampleStraightRgba(pair, 2.001, 0.5), null);
});

test("an unpadded source clips outside its quad instead of bleeding into transparent padding", () => {
  const unpadded = layer(solid(1, 1, [255, 0, 0, 255]), {
    centerX: 1.1,
    centerY: 0.5,
  });
  const result = composeGraphicOracle([unpadded], 3, 1);
  assert.deepEqual(
    Array.from(result),
    [0, 0, 0, 255, 255, 0, 0, 255, 0, 0, 0, 255],
  );
  const padded = layer(source(3, 1, [0, 0, 0, 0, 255, 0, 0, 255, 0, 0, 0, 0]), {
    centerX: 1.1,
    centerY: 0.5,
  });
  assert.notDeepEqual(composeGraphicOracle([padded], 3, 1), result);
});

test("integer transparent margins do not move an already padded sampling grid", () => {
  const a = layer(source(3, 1, [0, 0, 0, 0, 255, 0, 0, 255, 0, 0, 0, 0]), {
    centerX: 1.1,
    centerY: 0.5,
  });
  const b = layer(source(5, 1, [0, 0, 0, 0, ...a.source.data, 0, 0, 0, 0]), {
    centerX: 1.1,
    centerY: 0.5,
  });
  assert.deepEqual(
    composeGraphicOracle([a], 3, 1),
    composeGraphicOracle([b], 3, 1),
  );
});

test("inverse rotation preserves the independently specified corner orientation", () => {
  const corners = source(
    2,
    2,
    [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255],
  );
  const result = composeGraphicOracle(
    [layer(corners, { rotation: Math.PI / 2 })],
    2,
    2,
  );
  assert.deepEqual(pixel(result, 0, 0, 2), [0, 0, 255, 255]);
  assert.deepEqual(pixel(result, 1, 0, 2), [255, 0, 0, 255]);
  assert.deepEqual(pixel(result, 0, 1, 2), [255, 255, 255, 255]);
  assert.deepEqual(pixel(result, 1, 1, 2), [0, 255, 0, 255]);
});

test("nonuniform scale is applied in local axes before rotation", () => {
  const stripe = layer(solid(1, 1, [255, 0, 0, 255]), {
    centerX: 1.5,
    centerY: 1,
    scaleX: 2,
    scaleY: 1,
    rotation: Math.PI / 2,
  });
  const result = composeGraphicOracle([stripe], 3, 2);
  for (const y of [0, 1]) {
    assert.deepEqual(pixel(result, 0, y, 3), [0, 0, 0, 255]);
    assert.deepEqual(pixel(result, 1, y, 3), [255, 0, 0, 255]);
    assert.deepEqual(pixel(result, 2, y, 3), [0, 0, 0, 255]);
  }
});

test("negative scale flips the local source without moving its center", () => {
  const pair = source(2, 1, [255, 0, 0, 255, 0, 0, 255, 255]);
  const flipped = layer(pair, { centerY: 0.5, scaleX: -1 });
  assert.deepEqual(
    Array.from(composeGraphicOracle([flipped], 2, 1)),
    [0, 0, 255, 255, 255, 0, 0, 255],
  );
});

test("opacity is applied once to alpha with the intermediate UNORM8 rounding", () => {
  const result = composeGraphicOracle(
    [
      layer(solid(1, 1, [100, 200, 250, 128]), {
        centerX: 0.5,
        centerY: 0.5,
        opacity: 0.5,
      }),
    ],
    1,
    1,
  );
  assert.deepEqual(Array.from(result), [25, 50, 63, 255]);
});

test("ordered source-over preserves the contribution of both translucent layers", () => {
  const a = layer(solid(1, 1, [255, 0, 0, 128]), {
    centerX: 0.5,
    centerY: 0.5,
  });
  const b = layer(solid(1, 1, [0, 0, 255, 128]), {
    centerX: 0.5,
    centerY: 0.5,
  });
  assert.deepEqual(
    Array.from(composeGraphicOracle([a, b], 1, 1)),
    [64, 0, 128, 255],
  );
  assert.deepEqual(
    Array.from(composeGraphicOracle([b, a], 1, 1)),
    [128, 0, 64, 255],
  );
});

test("mixed-scene composition preserves an independently decoded background", () => {
  const foreground = layer(solid(1, 1, [255, 0, 0, 128]), {
    centerX: 0.5,
    centerY: 0.5,
  });
  const background = [0, 0, 255, 255];
  assert.deepEqual(
    Array.from(composeGraphicOracle([foreground], 1, 1, background)),
    [128, 0, 127, 255],
  );
  assert.deepEqual(background, [0, 0, 255, 255]);
  assert.throws(
    () => composeGraphicOracle([foreground], 1, 1, [0]),
    /dimensions/,
  );
});

test("unchanged metrics reject missing shapes, one-pixel displacement, color and opacity loss", () => {
  const base = layer(solid(64, 64, [220, 60, 100, 255]), {
    centerX: 50,
    centerY: 50,
  });
  const expected = composeGraphicOracle([base], 100, 100);
  const compare = (actual) =>
    measureGraphicOracle(actual, expected, 100, 100, [
      { kind: "rectangle", x: 0, y: 0 },
    ]);
  const exact = compare(expected);
  assert.equal(exact.iou, 1);
  assert.equal(exact.mean_channel_error, 0);
  assert.ok(exact.inner_pixels > 1000);
  assert.ok(exact.boundary_pixels > 100);
  assert.ok(compare(composeGraphicOracle([], 100, 100)).iou < 0.99);
  assert.ok(
    compare(composeGraphicOracle([{ ...base, centerX: 51 }], 100, 100)).iou <
      0.99,
  );
  assert.ok(
    compare(composeGraphicOracle([{ ...base, opacity: 0.8 }], 100, 100))
      .inner_max_channel_error > 3,
  );
  assert.ok(
    compare(
      composeGraphicOracle(
        [{ ...base, source: solid(64, 64, [100, 60, 220, 255]) }],
        100,
        100,
      ),
    ).inner_max_channel_error > 3,
  );
  const corrupted = expected.slice();
  corrupted[(50 * 100 + 50) * 4] = 0;
  const bad = compare(corrupted);
  assert.equal(bad.inner_pixels, exact.inner_pixels);
  assert.equal(bad.boundary_pixels, exact.boundary_pixels);
  assert.equal(bad.inner_max_channel_error, 220);
});

test("malformed input fails instead of producing NaN acceptance metrics", () => {
  assert.throws(
    () =>
      composeGraphicOracle(
        [layer(solid(1, 1, [0, 0, 0, 255]), { scaleX: 0 })],
        1,
        1,
      ),
    /Invalid/,
  );
  assert.throws(
    () => composeGraphicOracle([layer(source(1, 1, []))], 1, 1),
    /Invalid/,
  );
  assert.throws(() => measureGraphicOracle([0], [], 1, 1, []), /dimensions/);
});
