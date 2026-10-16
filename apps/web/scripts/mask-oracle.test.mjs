import test from "node:test";
import assert from "node:assert/strict";
import { MASK_ORACLE_MODEL, exactSeedDistances, makeMaskExpected, makeMaskReference } from "./mask-oracle.mjs";

function fixture({ width = 16, height = 8, kind = "rectangle", feather = 0, geometry = {}, quad = {} } = {}) {
  return { width, height, quad: { centerX: width / 2, centerY: height / 2, width, height, rotationRadians: 0, opacity: 1, flipX: false, flipY: false, ...quad },
    mask: { id: "independent", enabled: true, inverted: false, stroke: { width_px: 0 }, feather: { unit: "classic_pixels", value: feather },
      shape: { kind, geometry: { center_x: 0, center_y: 0, width: 0.5, height: 0.5, rotation_degrees: 0, scale: 1, ...geometry } } } };
}
const at = (plane, width, x, y) => plane[y * width + x];
const differences = (a, b, minimum = 0) => a.reduce((count, value, i) => count + Number(Math.abs(value - b[i]) > minimum), 0);

test("exact EDT matches independent exhaustive point distances, including thin, diagonal, empty and full seed sets", () => {
  let state = 7;
  for (const [width, height] of [[1, 1], [11, 7], [3, 13], [8, 8]]) for (let variant = 0; variant < 5; variant++) {
    const seeds = Uint8Array.from({ length: width * height }, (_, i) => {
      state = (state * 1664525 + 1013904223) >>> 0;
      return variant === 0 ? 0 : variant === 1 ? 1 : variant === 2 ? Number(i % width === Math.floor(i / width)) : Number(state % 7 === 0);
    });
    for (const value of [0, 1]) {
      const actual = exactSeedDistances(seeds, width, height, value);
      const points = [...seeds.keys()].filter(i => seeds[i] === value).map(i => [i % width, Math.floor(i / width)]);
      for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
        const expected = Math.sqrt(Math.min(...points.map(([px, py]) => (px - x) ** 2 + (py - y) ** 2)));
        assert.equal(at(actual, width, x, y), expected);
      }
    }
  }
  const seeds = new Uint8Array(25); seeds[1 * 5 + 1] = 1;
  assert.equal(at(exactSeedDistances(seeds, 5, 5), 5, 3, 2), Math.sqrt(5));
});
test("integer straight edge has independently known pixel-center distances and feather ramp bytes", () => {
  const spec = fixture({ feather: 8, geometry: { center_x: -0.25, width: 0.5, height: 1 } });
  const reference = makeMaskReference(spec);
  assert.equal(reference.model, MASK_ORACLE_MODEL);
  assert.deepEqual([...reference.hardAlpha.slice(0, 16)], [...Array(8).fill(255), ...Array(8).fill(0)]);
  // First outside seed is x=8; last inside seed x=7. At x=4..11 the
  // signed distances are +4,+3,+2,+1,-1,-2,-3,-4 (no continuous edge at .5).
  assert.deepEqual([...reference.alpha.slice(4, 12)], [255, 244, 215, 174, 81, 40, 11, 0]);
  for (let y = 1; y < 8; y++) assert.deepEqual(reference.alpha.slice(y * 16, (y + 1) * 16), reference.alpha.slice(0, 16));
  assert.deepEqual(makeMaskExpected(spec), reference.alpha);
});
test("partial hard coverage seeds straight-white red even below half alpha; opacity and unused scale do not change the mask", () => {
  const spec = fixture({ width: 8, height: 8, geometry: { center_x: 0.109375, width: 0.5, height: 0.5 } });
  const reference = makeMaskReference(spec);
  assert.deepEqual([...reference.hardAlpha.slice(3 * 8, 4 * 8)], [0, 0, 32, 255, 255, 255, 223, 0]);
  assert.equal(at(reference.seedPlane, 8, 2, 3), 1);
  assert.ok(at(reference.hardAlpha, 8, 2, 3) < 128);
  const changed = structuredClone(spec); changed.quad.opacity = 0; changed.mask.shape.geometry.scale = 1e100;
  assert.deepEqual(makeMaskExpected(changed), reference.alpha);
});
test("ellipse area integrates to pi*a*b and retains central symmetry with fractional antialias coverage", () => {
  const spec = fixture({ width: 64, height: 48, kind: "ellipse", geometry: { width: 0.5, height: 0.5, rotation_degrees: 27.5 } });
  const reference = makeMaskReference(spec), area = reference.alpha.reduce((sum, value) => sum + value / 255, 0);
  assert.ok(Math.abs(area - Math.PI * 16 * 12) < 0.15, String(area));
  for (let y = 0; y < 48; y++) for (let x = 0; x < 64; x++) assert.equal(at(reference.alpha, 64, x, y), at(reference.alpha, 64, 63 - x, 47 - y));
  assert.ok(reference.alpha.some(value => value > 0 && value < 255));
  assert.equal(at(reference.alpha, 64, 32, 24), 255); assert.equal(at(reference.alpha, 64, 0, 0), 0);
});
test("f64 local rounding precedes drawImage warp and is observable across the half-pixel boundary", () => {
  const spec = fixture({ width: 32, height: 24, quad: { width: 10.4999999, height: 8 } });
  const reference = makeMaskReference(spec);
  assert.equal(reference.localWidth, 10);
  const rounded = structuredClone(spec); rounded.quad.width = Math.fround(rounded.quad.width);
  assert.equal(makeMaskReference(rounded).localWidth, 11);
  assert.ok(differences(reference.alpha, makeMaskExpected(rounded)) > 0);
});
test("frame flip and quarter rotation follow inverse affine sampling without changing mask-local nonuniform dimensions", () => {
  const spec = fixture({ width: 32, height: 32, quad: { width: 16, height: 8 }, geometry: { center_x: 0.2, center_y: -0.1, width: 0.4, height: 0.5 } });
  const original = makeMaskExpected(spec), flip = structuredClone(spec), rotate = structuredClone(spec);
  flip.quad.flipX = true; rotate.quad.rotationRadians = Math.PI / 2;
  const reflected = makeMaskExpected(flip), rotated = makeMaskExpected(rotate);
  for (let y = 0; y < 32; y++) for (let x = 0; x < 32; x++) {
    assert.equal(at(reflected, 32, x, y), at(original, 32, 31 - x, y));
    assert.equal(at(rotated, 32, x, y), at(original, 32, y, 31 - x));
  }
  const ellipse = fixture({ width: 64, height: 48, kind: "ellipse", quad: { width: 40, height: 20 }, geometry: { rotation_degrees: 90 } });
  const alpha = makeMaskExpected(ellipse);
  assert.equal(at(alpha, 64, 39, 24), 0); assert.equal(at(alpha, 64, 32, 31), 255);
});
test("local clipping occurs before frame rotation and empty/full masks do not gain imaginary outside-frame seeds", () => {
  const clipped = fixture({ width: 32, height: 32, quad: { width: 16, height: 8, rotationRadians: Math.PI / 4 }, geometry: { center_x: 0.5, width: 1, height: 2 } });
  const alpha = makeMaskExpected(clipped), c = Math.SQRT1_2;
  for (let y = 0; y < 32; y++) for (let x = 0; x < 32; x++) {
    const dx = x + 0.5 - 16, dy = y + 0.5 - 16, u = c * (dx + dy), v = c * (-dx + dy);
    if (Math.abs(u) > 9 || Math.abs(v) > 5 || u < -1) assert.equal(at(alpha, 32, x, y), 0);
  }
  for (const [geometry, expected] of [[{ width: 3, height: 3 }, 255], [{ center_x: 10 }, 0]]) {
    const result = makeMaskExpected(fixture({ feather: 24, geometry })); assert.ok(result.every(value => value === expected));
  }
  assert.ok(makeMaskExpected({ ...fixture(), mask: null }).every(value => value === 255));
  assert.ok(makeMaskExpected({ ...fixture(), quad: null }).every(value => value === 0));
});
test("320x180 four static tiles repeat exactly and missing/shifted/no-feather negative controls remain distinguishable", () => {
  const specs = [[80, 45, "rectangle", 0], [240, 45, "ellipse", 0], [80, 135, "rectangle", 24], [240, 135, "ellipse", 24]].map(([x, y, kind, feather]) =>
    fixture({ width: 320, height: 180, kind, feather, quad: { centerX: x, centerY: y, width: 96, height: 64 }, geometry: { width: 0.5, height: 0.5 } }));
  for (const spec of specs) {
    const reference = makeMaskReference(spec);
    assert.deepEqual(makeMaskExpected(spec), reference.alpha);
    assert.equal(at(reference.alpha, 320, spec.quad.centerX, spec.quad.centerY), 255);
    assert.equal(at(reference.alpha, 320, 0, 0), 0);
    assert.ok(differences(reference.alpha, makeMaskExpected({ ...spec, mask: null })) > 50000);
    const shifted = structuredClone(spec); shifted.quad.centerX += 6;
    assert.ok(differences(reference.alpha, makeMaskExpected(shifted), 8) > 200);
    if (spec.mask.feather.value > 0) {
      const hard = structuredClone(spec); hard.mask.feather.value = 0;
      assert.ok(differences(reference.alpha, makeMaskExpected(hard), 8) > 500);
      assert.ok(reference.alpha.some(value => value > 32 && value < 223));
    }
    assert.ok(reference.edgePlane.some(Boolean));
  }
});
test("invalid inputs fail explicitly and model rejects Classic's undefined subnormal half-feather", () => {
  for (const change of [s => s.width = 0, s => s.width = 4097, s => s.quad.width = 10000, s => s.quad.centerX = NaN, s => s.mask.shape.kind = "heart", s => s.mask.inverted = true,
    s => s.mask.enabled = false, s => s.mask.feather.unit = "legacy_canvas_fraction", s => s.mask.stroke.width_px = 1, s => s.mask.shape.geometry.scale = -1, s => s.mask.feather.value = 1e-45]) {
    const spec = fixture(); change(spec); assert.throws(() => makeMaskExpected(spec), RangeError);
  }
  assert.deepEqual(makeMaskExpected(fixture({ feather: 1e-100 })), makeMaskExpected(fixture()));
  assert.throws(() => exactSeedDistances([0, 2], 2, 1), /binary/);
});
