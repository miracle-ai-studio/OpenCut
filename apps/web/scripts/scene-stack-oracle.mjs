// Acceptance-only reference mathematics. No product renderer, frame plan,
// Canvas shape path, product raster or WGSL source is imported.
import { makeMaskReference } from "./mask-oracle.mjs";
import { compositeClassicRgba8 } from "./blend-oracle.mjs";
export const SCENE_STACK_ORACLE_MODEL =
  "independent-scene-polygons-cubic-winding-gaussian-font-canvas-v1";
export const SCENE_SIZE = { width: 320, height: 180 };
const W = 320,
  H = 180,
  gray = (n) => ({
    encoding: "srgb",
    red: n / 255,
    green: n / 255,
    blue: n / 255,
    alpha: 1,
  });
const geometry = () => ({
  center_x: 0,
  center_y: 0,
  width: 0.6,
  height: 0.7,
  rotation_degrees: 0,
  scale: 1,
});
const base = (kind, id) => ({
  id,
  enabled: true,
  inverted: false,
  shape: { kind, geometry: geometry() },
  feather: { unit: "classic_pixels", value: 0 },
  stroke: { color: gray(255), width_px: 0, align: "center" },
});
export function sceneStackCases(fontId) {
  const diamond = base("diamond", "scene-diamond");
  diamond.inverted = true;
  const star = base("star", "scene-star");
  star.shape.geometry.width = 0.65;
  star.shape.geometry.height = 0.85;
  const heart = base("heart", "scene-heart");
  heart.shape.geometry.height = 0.9;
  const freeform = base("freeform", "scene-freeform");
  freeform.shape = {
    kind: "freeform",
    path: {
      center_x: 0,
      center_y: 0,
      rotation_degrees: 0,
      scale: 1,
      closed: true,
      points: [
        {
          id: "point-left",
          x: -0.3,
          y: -0.3,
          in_x: -0.3,
          in_y: -0.3,
          out_x: -0.3,
          out_y: -0.3,
        },
        {
          id: "point-right",
          x: 0.3,
          y: -0.3,
          in_x: 0.3,
          in_y: -0.3,
          out_x: 0.3,
          out_y: -0.3,
        },
        {
          id: "point-bottom",
          x: 0,
          y: 0.35,
          in_x: 0,
          in_y: 0.35,
          out_x: 0,
          out_y: 0.35,
        },
      ],
    },
  };
  const rect = base("rectangle", "scene-stack-box");
  rect.shape.geometry.width = 0.7;
  rect.shape.geometry.height = 0.75;
  rect.stroke = {
    color: { encoding: "srgb", red: 0.1, green: 0.8, blue: 0.2, alpha: 1 },
    width_px: 8,
    align: "outside",
  };
  const ellipse = base("ellipse", "scene-stack-ellipse");
  ellipse.shape.geometry.width = 0.35;
  ellipse.shape.geometry.height = 0.45;
  ellipse.inverted = true;
  const split = base("split", "scene-split");
  split.shape = {
    kind: "split",
    center_x: 0,
    center_y: 0,
    rotation_degrees: 0,
  };
  split.feather.value = 24;
  split.stroke = {
    color: { encoding: "srgb", red: 0.9, green: 0.2, blue: 0.1, alpha: 1 },
    width_px: 4,
    align: "center",
  };
  const text = base("text", "scene-text");
  text.shape = {
    kind: "text",
    mask: {
      content: "HI",
      font: { kind: "locked", font_id: fontId },
      font_size: { unit: "classic_height90", value: 24 },
      font_weight: "normal",
      font_style: "normal",
      decoration: "none",
      letter_spacing_px: 0,
      line_height_multiplier: 1.2,
      center_x: 0,
      center_y: 0,
      rotation_degrees: 0,
      scale: 1,
    },
  };
  return [
    { id: "diamond-inverted", masks: [diamond] },
    { id: "star", masks: [star] },
    { id: "heart", masks: [heart] },
    { id: "freeform", masks: [freeform] },
    { id: "stack-invert-stroke", masks: [rect, ellipse] },
    { id: "split-feather-stroke", masks: [split] },
    {
      id: "blur-and-adjustment",
      masks: [],
      rect: { left: 96, top: 42, width: 128, height: 96 },
      element_blur: 45,
      scene_blur: 30,
    },
    { id: "text-mask", masks: [text], needs_font: true },
  ].map((c, index) => ({
    ...c,
    index,
    at: (index + 0.5) / 2,
    start: index / 2,
    duration: 0.5,
    back: 32,
    front: 224,
  }));
}
function rotate(x, y, g) {
  const a = (g.rotation_degrees * Math.PI) / 180,
    c = Math.cos(a),
    s = Math.sin(a);
  return [
    W / 2 + g.center_x * W + x * c - y * s,
    H / 2 + g.center_y * H + x * s + y * c,
  ];
}
function polygon(mask, stroke = false) {
  const k = mask.shape.kind,
    g = k === "freeform" ? mask.shape.path : mask.shape.geometry;
  if (k === "freeform")
    return g.points.map((p) => rotate(p.x * W * g.scale, p.y * H * g.scale, g));
  const offset = stroke
    ? mask.stroke.align === "outside"
      ? mask.stroke.width_px / 2
      : mask.stroke.align === "inside"
        ? -mask.stroke.width_px / 2
        : 0
    : 0;
  const x = (Math.max(g.width, 0.01) * W) / 2 + offset,
    y = (Math.max(g.height, 0.01) * H) / 2 + offset;
  const p = (a, b) => rotate(a, b, g);
  if (k === "diamond") return [p(0, -y), p(x, 0), p(0, y), p(-x, 0)];
  if (k === "star")
    return Array.from({ length: 10 }, (_, i) => {
      const a = (i * Math.PI) / 5 - Math.PI / 2,
        r = i % 2 ? 0.45 : 1;
      return p(Math.cos(a) * x * r, Math.sin(a) * y * r);
    });
  if (k === "heart") {
    // Independent cubic polynomial evaluation, not Canvas Path2D or product
    // shape code. The fixed Classic silhouette has two cubic arcs.
    const start = [0, -y * 0.475],
      arcs = [
        [start, [x, -y * 1.225], [x, -y * 0.125], [0, y * 0.725]],
        [[0, y * 0.725], [-x, -y * 0.125], [-x, -y * 1.225], start],
      ],
      out = [];
    for (const points of arcs)
      for (let n = 0; n < 256; n++) {
        const t = n / 256,
          u = 1 - t;
        out.push(
          p(
            u ** 3 * points[0][0] +
              3 * u * u * t * points[1][0] +
              3 * u * t * t * points[2][0] +
              t ** 3 * points[3][0],
            u ** 3 * points[0][1] +
              3 * u * u * t * points[1][1] +
              3 * u * t * t * points[2][1] +
              t ** 3 * points[3][1],
          ),
        );
      }
    return out;
  }
  return [p(-x, -y), p(x, -y), p(x, y), p(-x, y)];
}
function inside(points, x, y) {
  let yes = false;
  for (let i = 0, j = points.length - 1; i < points.length; j = i++) {
    const a = points[i],
      b = points[j];
    if (
      a[1] > y !== b[1] > y &&
      x < ((b[0] - a[0]) * (y - a[1])) / (b[1] - a[1]) + a[0]
    )
      yes = !yes;
  }
  return yes;
}
const distance = (x, y, a, b) => {
  const dx = b[0] - a[0],
    dy = b[1] - a[1],
    t = Math.max(
      0,
      Math.min(
        1,
        ((x - a[0]) * dx + (y - a[1]) * dy) / (dx * dx + dy * dy || 1),
      ),
    );
  return Math.hypot(x - a[0] - t * dx, y - a[1] - t * dy);
};
const quad = {
  centerX: 160,
  centerY: 90,
  width: W,
  height: H,
  rotationRadians: 0,
  flipX: false,
  flipY: false,
  opacity: 1,
};
function alpha(mask, textAlpha) {
  if (mask.shape.kind === "text") {
    if (textAlpha?.length !== W * H)
      throw Error("Independent fixed-font alpha plane is required");
    return Uint8Array.from(textAlpha);
  }
  if (["rectangle", "ellipse"].includes(mask.shape.kind))
    return makeMaskReference({
      width: W,
      height: H,
      quad,
      mask: {
        ...mask,
        inverted: false,
        stroke: { ...mask.stroke, width_px: 0 },
      },
    }).alpha;
  const output = new Uint8Array(W * H);
  if (mask.shape.kind === "split") {
    const s = mask.shape,
      angle = (s.rotation_degrees * Math.PI) / 180,
      c = Math.cos(angle),
      v = Math.sin(angle),
      cx = 160 + s.center_x * W,
      cy = 90 + s.center_y * H,
      f = mask.feather.value;
    for (let y = 0; y < H; y++)
      for (let x = 0; x < W; x++) {
        const d = (x + 0.5 - cx) * c + (y + 0.5 - cy) * v;
        output[y * W + x] = Math.round(
          255 * (f ? Math.max(0, Math.min(1, d / f + 0.5)) : Number(d >= 0)),
        );
      }
    return output;
  }
  const points = polygon(mask);
  for (let y = 0; y < H; y++)
    for (let x = 0; x < W; x++) {
      let n = 0;
      for (let sy = 0; sy < 4; sy++)
        for (let sx = 0; sx < 4; sx++)
          n += Number(inside(points, x + (sx + 0.5) / 4, y + (sy + 0.5) / 4));
      output[y * W + x] = Math.round((n * 255) / 16);
    }
  return output;
}
function stroke(mask) {
  const output = new Uint8Array(W * H);
  if (!mask.stroke.width_px) return output;
  const points =
    mask.shape.kind === "split"
      ? [
          [160, 0],
          [160, H],
        ]
      : polygon(mask, true);
  for (let y = 0; y < H; y++)
    for (let x = 0; x < W; x++) {
      let d = Infinity;
      for (let i = 1; i < points.length; i++)
        d = Math.min(d, distance(x + 0.5, y + 0.5, points[i - 1], points[i]));
      if (mask.shape.kind !== "split")
        d = Math.min(d, distance(x + 0.5, y + 0.5, points.at(-1), points[0]));
      output[y * W + x] = d < mask.stroke.width_px / 2 ? 255 : 0;
    }
  return output;
}
function quantize(n) {
  return Math.max(0, Math.min(255, Math.round(n)));
}
function sample(raw, x, y, c) {
  const l = Math.floor(x),
    t = Math.floor(y),
    fx = x - l,
    fy = y - t;
  let n = 0;
  for (let dy = 0; dy < 2; dy++)
    for (let dx = 0; dx < 2; dx++) {
      const px = Math.max(0, Math.min(W - 1, l + dx)),
        py = Math.max(0, Math.min(H - 1, t + dy));
      n += raw[(py * W + px) * 4 + c] * (dx ? fx : 1 - fx) * (dy ? fy : 1 - fy);
    }
  return n;
}
/** Independent normalized discrete Gaussian integration. Each pass is rounded
 * to RGBA8 as required by the published Classic intermediate texture format.
 * Large iterative sigmas and GPU ULP identity are outside this small fixture. */
export function gaussianSceneRgba(raw, intensity) {
  if (raw.length !== W * H * 4 || intensity < 0 || !Number.isFinite(intensity))
    throw Error("Invalid independent Gaussian input");
  let input = Uint8Array.from(raw);
  for (const [axis, sigma] of [
    [0, ((intensity / 5) * W) / 1920],
    [1, ((intensity / 5) * H) / 1080],
  ]) {
    if (sigma < 0.001) continue;
    if (sigma > 10)
      throw Error(
        "Independent Gaussian fixture is limited to one pass per axis",
      );
    const weights = Array.from({ length: 61 }, (_, i) =>
        Math.exp(-((i - 30) ** 2) / (2 * sigma * sigma)),
      ),
      sum = weights.reduce((a, b) => a + b, 0),
      out = new Uint8Array(input.length);
    for (let y = 0; y < H; y++)
      for (let x = 0; x < W; x++)
        for (let c = 0; c < 4; c++) {
          let value = 0;
          for (let i = 0; i < 61; i++)
            value +=
              sample(
                input,
                x + (axis === 0 ? i - 30 : 0),
                y + (axis === 1 ? i - 30 : 0),
                c,
              ) * weights[i];
          out[(y * W + x) * 4 + c] = quantize(value / sum);
        }
    input = out;
  }
  return input;
}
export function referenceSceneCase(
  c,
  textAlpha,
  {
    omitMask = -1,
    invertWrong = false,
    omitStroke = false,
    omitEffects = false,
  } = {},
) {
  let foreground = new Uint8Array(W * H * 4);
  const rect = c.rect ?? { left: 0, top: 0, width: W, height: H };
  for (let y = 0; y < H; y++)
    for (let x = 0; x < W; x++)
      if (
        x >= rect.left &&
        x < rect.left + rect.width &&
        y >= rect.top &&
        y < rect.top + rect.height
      ) {
        const p = (y * W + x) * 4;
        foreground.set([c.front, c.front, c.front, 255], p);
      }
  if (c.element_blur && !omitEffects)
    foreground = gaussianSceneRgba(foreground, c.element_blur);
  for (const [index, m] of c.masks.entries()) {
    if (index === omitMask) continue;
    const a = alpha(m, textAlpha);
    for (let p = 0; p < a.length; p++) {
      let value = a[p];
      if (m.inverted !== invertWrong) value = 255 - value;
      foreground[p * 4 + 3] = quantize((foreground[p * 4 + 3] * value) / 255);
    }
  }
  let result = new Uint8Array(foreground.length);
  for (let p = 0; p < W * H; p++)
    result.set(
      compositeClassicRgba8(
        [c.back, c.back, c.back, 255],
        foreground.subarray(p * 4, p * 4 + 4),
        "normal",
      ),
      p * 4,
    );
  if (!omitStroke)
    for (const m of c.masks) {
      const a = stroke(m),
        color = m.stroke.color;
      for (let p = 0; p < a.length; p++)
        if (a[p])
          result.set(
            compositeClassicRgba8(
              result.subarray(p * 4, p * 4 + 4),
              [
                quantize(color.red * 255),
                quantize(color.green * 255),
                quantize(color.blue * 255),
                quantize(color.alpha * a[p]),
              ],
              "normal",
            ),
            p * 4,
          );
    }
  if (c.scene_blur && !omitEffects)
    result = gaussianSceneRgba(result, c.scene_blur);
  return result;
}
function difference(a, b, p) {
  return Math.max(
    ...[0, 1, 2, 3].map((c) => Math.abs(a[p * 4 + c] - b[p * 4 + c])),
  );
}
export function sceneReferenceProbes(c, expected, textAlpha) {
  const negatives = [
    {
      name: "missing-foreground",
      raw: referenceSceneCase({ ...c, front: c.back }, textAlpha),
    },
    ...(["diamond", "star", "heart", "freeform", "text"].includes(
      c.masks[0]?.shape.kind,
    )
      ? [
          {
            name: "wrong-box-shape",
            raw: referenceSceneCase(
              {
                ...c,
                masks: c.masks.map((m, i) =>
                  i
                    ? m
                    : {
                        ...m,
                        shape: {
                          kind: "rectangle",
                          geometry: m.shape.geometry ?? {
                            center_x: 0,
                            center_y: 0,
                            width: 0.6,
                            height: 0.65,
                            rotation_degrees: 0,
                            scale: 1,
                          },
                        },
                      },
                ),
              },
              textAlpha,
            ),
          },
        ]
      : []),
    ...c.masks.map((_, i) => ({
      name: `missing-mask-${i}`,
      raw: referenceSceneCase(c, textAlpha, { omitMask: i }),
    })),
    ...(c.masks.some((m) => m.inverted)
      ? [
          {
            name: "wrong-inversion",
            raw: referenceSceneCase(c, textAlpha, { invertWrong: true }),
          },
        ]
      : []),
    ...(c.masks.some((m) => m.stroke.width_px > 0)
      ? [
          {
            name: "missing-stroke",
            raw: referenceSceneCase(c, textAlpha, { omitStroke: true }),
          },
        ]
      : []),
    ...(c.element_blur || c.scene_blur
      ? [
          {
            name: "missing-blur",
            raw: referenceSceneCase(c, textAlpha, { omitEffects: true }),
          },
        ]
      : []),
  ];
  const points = [];
  const add = (x, y) => {
    if (
      x < 2 ||
      y < 2 ||
      x >= W - 2 ||
      y >= H - 2 ||
      points.some((p) => p.x === x && p.y === y)
    )
      return;
    points.push({
      x,
      y,
      expected: Array.from(
        expected.subarray((y * W + x) * 4, (y * W + x) * 4 + 4),
      ),
    });
  };
  // A fixed off-center grid plus both sides of authored edges. Probe selection
  // is entirely from authored facts and this reference, never observed pixels.
  for (const y of [28, 48, 68, 88, 108, 128, 148])
    for (const x of [34, 62, 94, 126, 158, 190, 222, 254, 286]) {
      const p = y * W + x;
      let stable = true;
      if (!c.element_blur && !c.scene_blur && c.id !== "split-feather-stroke")
        for (const [dx, dy] of [
          [-2, 0],
          [2, 0],
          [0, -2],
          [0, 2],
        ])
          if (
            [0, 1, 2].some(
              (k) =>
                expected[p * 4 + k] !==
                expected[((y + dy) * W + x + dx) * 4 + k],
            )
          )
            stable = false;
      if (stable) add(x, y);
    }
  if (c.element_blur)
    for (const x of [90, 93, 95, 97, 99, 102, 105, 217, 221, 224, 227, 230])
      add(x, 74);
  if (c.id === "split-feather-stroke")
    for (const x of [145, 149, 151, 154, 157, 162, 165, 168, 170, 174])
      add(x, 64);
  const controls = [];
  for (const negative of negatives) {
    let best = null;
    for (let y = 4; y < H - 4; y += 2)
      for (let x = 4; x < W - 4; x += 2) {
        const p = y * W + x,
          delta = difference(expected, negative.raw, p);
        if (delta < 24) continue;
        // Complex-shape AA is explicitly excluded from binary edge controls.
        if (
          !c.element_blur &&
          !c.scene_blur &&
          c.id !== "split-feather-stroke" &&
          c.id !== "text-mask" &&
          [0, 1, 2].some(
            (k) =>
              expected[p * 4 + k] !== expected[(p + 2) * 4 + k] ||
              expected[p * 4 + k] !== expected[(p - 2) * 4 + k],
          )
        )
          continue;
        if (!best || delta > best.delta) best = { x, y, delta };
      }
    if (!best)
      throw Error(`Oracle ${c.id} does not distinguish ${negative.name}`);
    add(best.x, best.y);
    controls.push({ name: negative.name, ...best });
  }
  if (points.length < 8)
    throw Error(
      "Independent scene fixture needs at least eight separated probes",
    );
  return { points, negative_controls: controls };
}
