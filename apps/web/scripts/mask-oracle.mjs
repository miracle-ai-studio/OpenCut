// Acceptance-only mathematics. No product renderer, Canvas, shader, plan or
// receipt is imported. Shapes use area integration; distances use the exact
// lower envelope of squared-distance parabolas, never jump flooding.
export const MASK_ORACLE_MODEL = "independent-area-bilinear-exact-edt-mask-v1";

const clamp = value => Math.max(0, Math.min(1, value));
const byte = value => Math.round(clamp(value) * 255);
const MAX_PIXELS = 4_194_304;
function finite(value, name) {
  if (typeof value !== "number" || !Number.isFinite(value)) throw new RangeError(`Invalid oracle ${name}`);
  return value;
}
function dimensions(width, height) {
  if (![width, height].every(value => Number.isSafeInteger(value) && value >= 1 && value <= 4096) || width * height > MAX_PIXELS)
    throw new RangeError("Oracle dimensions exceed its bounded reference scope");
}
function validate({ width, height, quad, mask }) {
  dimensions(width, height);
  if (mask === null || quad === null) return;
  if (!mask || mask.enabled !== true || mask.inverted !== false || mask.stroke?.width_px !== 0 ||
      !["rectangle", "ellipse"].includes(mask.shape?.kind) || mask.feather?.unit !== "classic_pixels")
    throw new RangeError("Oracle requires an enabled rectangle/ellipse, Classic pixels and no inversion/stroke");
  const g = mask.shape.geometry;
  if (!g) throw new RangeError("Missing oracle mask geometry");
  for (const key of ["center_x", "center_y", "width", "height", "rotation_degrees", "scale"]) finite(g[key], key);
  if (g.width <= 0 || g.height <= 0 || g.scale < 0) throw new RangeError("Invalid oracle mask extent");
  const feather = finite(mask.feather.value, "feather");
  if (feather < 0 || !Number.isFinite(Math.fround(feather))) throw new RangeError("Invalid oracle f32 feather");
  for (const key of ["centerX", "centerY", "width", "height", "rotationRadians", "opacity"]) finite(quad[key], key);
  if (quad.width <= 0 || quad.height <= 0 || quad.opacity < 0 || quad.opacity > 1 || typeof quad.flipX !== "boolean" || typeof quad.flipY !== "boolean")
    throw new RangeError("Invalid oracle logical quad");
  const w = Math.round(quad.width), h = Math.round(quad.height);
  if (w > 4096 || h > 4096 || w * h > MAX_PIXELS) throw new RangeError("Oracle local canvas exceeds bounded reference scope");
}

// Exact convex polygon / unit-pixel intersection area. This is independent of
// Canvas's scan conversion and retains fractional rectangle-edge coverage.
function clippedArea(polygon, left, top, right, bottom) {
  let points = polygon;
  for (const [axis, boundary, sign] of [[0, left, 1], [0, right, -1], [1, top, 1], [1, bottom, -1]]) {
    const output = [];
    for (let index = 0; index < points.length; index++) {
      const a = points[index], b = points[(index + 1) % points.length];
      const da = (a[axis] - boundary) * sign, db = (b[axis] - boundary) * sign;
      if (da >= 0) output.push(a);
      if ((da < 0) !== (db < 0)) {
        const t = da / (da - db);
        output.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
      }
    }
    points = output;
    if (!points.length) return 0;
  }
  let twice = 0;
  // Translate before the shoelace sum to avoid cancellation far off origin.
  for (let i = 0; i < points.length; i++) {
    const a = points[i], b = points[(i + 1) % points.length];
    twice += (a[0] - left) * (b[1] - top) - (a[1] - top) * (b[0] - left);
  }
  return clamp(Math.abs(twice) / 2);
}
function rectangle(cx, cy, a, b, rotation) {
  const c = Math.cos(rotation), s = Math.sin(rotation);
  return [[-a, -b], [a, -b], [a, b], [-a, b]].map(([x, y]) => [cx + c * x - s * y, cy + s * x + c * y]);
}
function bounds(points) {
  return { left: Math.min(...points.map(p => p[0])), right: Math.max(...points.map(p => p[0])), top: Math.min(...points.map(p => p[1])), bottom: Math.max(...points.map(p => p[1])) };
}
function adaptiveArea(f, left, right, tolerance = 1e-8) {
  const mid = (left + right) / 2, a = f(left), b = f(mid), c = f(right);
  function integrate(l, r, fl, fm, fr, area, epsilon, depth) {
    const m = (l + r) / 2, lm = f((l + m) / 2), rm = f((m + r) / 2);
    const first = (m - l) * (fl + 4 * lm + fm) / 6, second = (r - m) * (fm + 4 * rm + fr) / 6;
    const delta = first + second - area;
    if (!depth || Math.abs(delta) <= 15 * epsilon) return first + second + delta / 15;
    return integrate(l, m, fl, lm, fm, first, epsilon / 2, depth - 1) + integrate(m, r, fm, rm, fr, second, epsilon / 2, depth - 1);
  }
  return integrate(left, right, a, b, c, (right - left) * (a + 4 * b + c) / 6, tolerance, 14);
}
function ellipseCoverage(cx, cy, a, b, rotation) {
  const c = Math.cos(rotation), s = Math.sin(rotation);
  // Conic A*dx^2 + 2B*dx*dy + C*dy^2 <= 1. Each vertical slice
  // has an analytic interval; only its clipped length needs integration.
  const A = c * c / (a * a) + s * s / (b * b), B = c * s * (1 / (a * a) - 1 / (b * b)), C = s * s / (a * a) + c * c / (b * b);
  const determinant = 1 / (a * a * b * b), rx = Math.hypot(c * a, s * b), ry = Math.hypot(s * a, c * b);
  if (![A, B, C, determinant, rx, ry].every(Number.isFinite) || A <= 0 || C <= 0 || determinant <= 0)
    throw new RangeError("Ellipse is outside the oracle's numeric integration scope");
  const contains = (x, y) => { const dx = x - cx, dy = y - cy; return A * dx * dx + 2 * B * dx * dy + C * dy * dy <= 1; };
  return {
    bounds: { left: cx - rx, right: cx + rx, top: cy - ry, bottom: cy + ry },
    pixel(x, y) {
      if ([[x, y], [x + 1, y], [x + 1, y + 1], [x, y + 1]].every(([px, py]) => contains(px, py))) return 1;
      const left = Math.max(x, cx - rx), right = Math.min(x + 1, cx + rx);
      if (right <= left) return 0;
      const section = px => {
        const dx = px - cx, disc = C - determinant * dx * dx;
        if (disc <= 0) return 0;
        const center = cy - B * dx / C, half = Math.sqrt(disc) / C;
        return Math.max(0, Math.min(y + 1, center + half) - Math.max(y, center - half));
      };
      // Split at crossings with the pixel's horizontal edges so a very thin
      // sliver cannot disappear between quadrature sample locations.
      const cuts = [left, right];
      for (const py of [y, y + 1]) {
        const dy = py - cy, disc = A - determinant * dy * dy;
        if (disc > 0) for (const sign of [-1, 1]) {
          const px = cx + (-B * dy + sign * Math.sqrt(disc)) / A;
          if (px > left && px < right) cuts.push(px);
        }
      }
      cuts.sort((x, y) => x - y);
      let area = 0;
      for (let n = 1; n < cuts.length; n++) if (cuts[n] > cuts[n - 1]) area += adaptiveArea(section, cuts[n - 1], cuts[n]);
      return clamp(area);
    },
  };
}
function localCoverage(quad, mask, width, height) {
  const result = new Uint8Array(width * height);
  if (!width || !height) return result;
  const g = mask.shape.geometry, cx = quad.width * (0.5 + g.center_x), cy = quad.height * (0.5 + g.center_y);
  const a = quad.width * Math.max(g.width, 0.01) / 2, b = quad.height * Math.max(g.height, 0.01) / 2, rotation = g.rotation_degrees * Math.PI / 180;
  [cx, cy, a, b, rotation].forEach(value => finite(value, "resolved shape geometry"));
  let shape;
  if (mask.shape.kind === "rectangle") {
    const polygon = rectangle(cx, cy, a, b, rotation);
    shape = { bounds: bounds(polygon), pixel: (x, y) => clippedArea(polygon, x, y, x + 1, y + 1) };
  } else shape = ellipseCoverage(cx, cy, a, b, rotation);
  const box = shape.bounds;
  for (let y = Math.max(0, Math.floor(box.top)); y < Math.min(height, Math.ceil(box.bottom)); y++)
    for (let x = Math.max(0, Math.floor(box.left)); x < Math.min(width, Math.ceil(box.right)); x++) result[y * width + x] = byte(shape.pixel(x, y));
  return result;
}
function bilinear(plane, width, height, x, y) {
  const sx = Math.max(0, Math.min(width - 1, x - 0.5)), sy = Math.max(0, Math.min(height - 1, y - 0.5));
  const x0 = Math.floor(sx), y0 = Math.floor(sy), x1 = Math.min(x0 + 1, width - 1), y1 = Math.min(y0 + 1, height - 1), tx = sx - x0, ty = sy - y0;
  return (plane[y0 * width + x0] * (1 - tx) + plane[y0 * width + x1] * tx) * (1 - ty) + (plane[y1 * width + x0] * (1 - tx) + plane[y1 * width + x1] * tx) * ty;
}
function warp(local, localWidth, localHeight, width, height, quad) {
  const result = new Uint8Array(width * height);
  if (!localWidth || !localHeight) return result;
  const polygon = rectangle(quad.centerX, quad.centerY, quad.width / 2, quad.height / 2, quad.rotationRadians), box = bounds(polygon);
  const c = Math.cos(quad.rotationRadians), s = Math.sin(quad.rotationRadians);
  for (let y = Math.max(0, Math.floor(box.top)); y < Math.min(height, Math.ceil(box.bottom)); y++)
    for (let x = Math.max(0, Math.floor(box.left)); x < Math.min(width, Math.ceil(box.right)); x++) {
      const coverage = clippedArea(polygon, x, y, x + 1, y + 1);
      if (!coverage) continue;
      const dx = x + 0.5 - quad.centerX, dy = y + 0.5 - quad.centerY;
      const u = (c * dx + s * dy) * (quad.flipX ? -1 : 1), v = (-s * dx + c * dy) * (quad.flipY ? -1 : 1);
      const alpha = bilinear(local, localWidth, localHeight, (u / quad.width + 0.5) * localWidth, (v / quad.height + 0.5) * localHeight);
      result[y * width + x] = Math.round(alpha * coverage);
    }
  return result;
}

// Exact separable Euclidean distance transform. A 1D row/column is the lower
// envelope of parabolas (x-seed)^2 + prior_distance_squared. Infinite rows are
// kept infinite, so empty/full masks do not invent a border outside the frame.
function squaredLine(input) {
  const sites = [], starts = [], output = new Float64Array(input.length);
  for (let q = 0; q < input.length; q++) if (Number.isFinite(input[q])) {
    let start = -Infinity;
    while (sites.length) {
      const p = sites.at(-1);
      start = ((input[q] + q * q) - (input[p] + p * p)) / (2 * (q - p));
      if (start > starts.at(-1)) break;
      sites.pop(); starts.pop();
    }
    sites.push(q); starts.push(sites.length === 1 ? -Infinity : start);
  }
  if (!sites.length) { output.fill(Infinity); return output; }
  let k = 0;
  for (let x = 0; x < output.length; x++) {
    while (k + 1 < sites.length && starts[k + 1] <= x) k++;
    output[x] = (x - sites[k]) ** 2 + input[sites[k]];
  }
  return output;
}
export function exactSeedDistances(seedPlane, width, height, seedValue = 1) {
  dimensions(width, height);
  if (seedPlane?.length !== width * height || ![0, 1].includes(seedValue) || !seedPlane.every(value => value === 0 || value === 1))
    throw new RangeError("Exact EDT requires a complete binary seed plane");
  const rows = new Float64Array(seedPlane.length), result = new Float64Array(seedPlane.length);
  for (let y = 0; y < height; y++) {
    const input = new Float64Array(width);
    for (let x = 0; x < width; x++) input[x] = seedPlane[y * width + x] === seedValue ? 0 : Infinity;
    rows.set(squaredLine(input), y * width);
  }
  for (let x = 0; x < width; x++) {
    const input = new Float64Array(height);
    for (let y = 0; y < height; y++) input[y] = rows[y * width + x];
    const column = squaredLine(input);
    for (let y = 0; y < height; y++) result[y * width + x] = Math.sqrt(column[y]);
  }
  return result;
}
function edgePlaneFor(hardAlpha, width, height) {
  const edges = new Uint8Array(hardAlpha.length);
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const value = hardAlpha[y * width + x];
    let boundary = value > 0 && value < 255;
    for (let dy = -1; dy <= 1 && !boundary; dy++) for (let dx = -1; dx <= 1; dx++) {
      const px = x + dx, py = y + dy;
      if (px >= 0 && py >= 0 && px < width && py < height && hardAlpha[py * width + px] !== value) { boundary = true; break; }
    }
    edges[y * width + x] = Number(boundary);
  }
  return edges;
}

/** Reference for the static rectangle/ellipse profile. Quad is the fixture's
 * independent f64 logical extent after element scale, without graphic padding.
 * Opacity is deliberately not multiplied: the mask pass only scales an already
 * rendered layer's alpha. A null mask returns the multiplicative identity.
 *
 * Area coverage is mathematically defined, not a claim of Skia's exact AA
 * kernel. Rotated/curved hard edges and texture-quad borders need separately
 * reported local statistics; do not loosen whole-frame thresholds to hide
 * those differences. For exact ramp assertions prefer integer axis-aligned
 * rectangle edges and ellipse cardinal-axis samples with unambiguous seeds.
 */
export function makeMaskReference(spec) {
  validate(spec);
  const { width, height, quad, mask } = spec;
  const localWidth = quad && mask ? Math.round(quad.width) : 0, localHeight = quad && mask ? Math.round(quad.height) : 0;
  const hardAlpha = mask === null ? new Uint8Array(width * height).fill(255) : quad === null ? new Uint8Array(width * height)
    : warp(localCoverage(quad, mask, localWidth, localHeight), localWidth, localHeight, width, height, quad);
  // White Canvas pixels are uploaded UNPREMULTIPLIED. Classic's seed predicate
  // reads red, not alpha: any surviving white coverage becomes a red=1 seed.
  const seedPlane = Uint8Array.from(hardAlpha, alpha => Number(alpha > 0));
  const feather = mask && quad ? Math.fround(mask.feather.value) : 0;
  let alpha = hardAlpha.slice();
  if (feather > 0) {
    const half = Math.fround(feather / 2);
    if (!half) throw new RangeError("Subnormal feather has an undefined zero-width smoothstep in Classic");
    const inside = exactSeedDistances(seedPlane, width, height, 1), outside = exactSeedDistances(seedPlane, width, height, 0);
    alpha = Uint8Array.from(seedPlane, (_, index) => {
      const t = clamp((outside[index] - inside[index] + half) / (2 * half));
      return byte(t * t * (3 - 2 * t));
    });
  }
  return { model: MASK_ORACLE_MODEL, width, height, localWidth, localHeight, alpha, hardAlpha, seedPlane, edgePlane: edgePlaneFor(hardAlpha, width, height) };
}
export function makeMaskExpected(spec) { return makeMaskReference(spec).alpha; }
