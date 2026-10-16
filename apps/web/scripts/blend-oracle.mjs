// Independent numeric reference. No product renderer, WGSL, plan or receipt is
// loaded here. RGB equations follow the mathematical blend definitions in
// https://www.w3.org/TR/compositing-1/#blending ; the explicitly named Classic
// differences below were established by reading its original v31 shader.
// Numbers are normalized texture values, with no sRGB/linear conversion.
export const BLEND_ORACLE_MODEL = "independent-classic-blend-rgba-v1";

export const CLASSIC_BLEND_MODES = Object.freeze([
  "normal",
  "darken",
  "multiply",
  "color-burn",
  "lighten",
  "screen",
  "plus-lighter",
  "color-dodge",
  "overlay",
  "soft-light",
  "hard-light",
  "difference",
  "exclusion",
  "hue",
  "saturation",
  "color",
  "luminosity",
]);

const clamp = (value) => Math.min(1, Math.max(0, value));
const luminance = ([r, g, b]) => (30 * r + 59 * g + 11 * b) / 100;
const saturation = (rgb) => Math.max(...rgb) - Math.min(...rgb);

function requireMode(mode) {
  if (!CLASSIC_BLEND_MODES.includes(mode))
    throw new RangeError(`Unknown independent blend mode: ${String(mode)}`);
}

function requireComponents(value, count, label) {
  if (
    value?.length !== count ||
    !Array.from(value).every(
      (component) =>
        Number.isFinite(component) && component >= 0 && component <= 1,
    )
  )
    throw new RangeError(`${label} must have ${count} components in [0, 1]`);
}

// Non-separable "HSL" modes use weighted luminosity and channel range, not
// conversion to CSS cylindrical HSL. Sort channels to assign chroma endpoints;
// keep the original middle-channel position between those endpoints.
function withSaturation(rgb, range) {
  const order = [0, 1, 2].sort((a, b) => rgb[a] - rgb[b]);
  const [low, mid, high] = order;
  const result = [0, 0, 0];
  if (rgb[high] !== rgb[low]) {
    result[high] = range;
    result[mid] = (range * (rgb[mid] - rgb[low])) / (rgb[high] - rgb[low]);
  }
  return result;
}

// Translate to the requested luminosity, then contract chroma about that
// luminosity until the color lies in the RGB cube. With inputs in [0,1], the
// initial channel span is <= 1, so both bounds cannot be violated at once.
function withLuminance(rgb, target) {
  const chroma = rgb.map((component) => component - luminance(rgb));
  let scale = 1;
  for (const component of chroma) {
    if (component < 0) scale = Math.min(scale, target / -component);
    if (component > 0) scale = Math.min(scale, (1 - target) / component);
  }
  return chroma.map((component) => target + scale * component);
}

function hardLight(backdrop, source) {
  if (source <= 0.5) return backdrop * (source * 2);
  return backdrop + (1 - backdrop) * (source * 2 - 1);
}

function softLight(backdrop, source) {
  // Interpolate from the backdrop toward its square for a dark source, or
  // toward a smooth highlight curve for a light source.
  const strength = Math.abs(source * 2 - 1);
  const target =
    source <= 0.5
      ? backdrop ** 2
      : backdrop <= 0.25
        ? 16 * backdrop ** 3 - 12 * backdrop ** 2 + 4 * backdrop
        : Math.sqrt(backdrop);
  return backdrop + strength * (target - backdrop);
}

function channelBlend(backdrop, source, mode, classic) {
  switch (mode) {
    case "normal":
      return source;
    case "darken":
      return Math.min(backdrop, source);
    case "multiply":
      return backdrop * source;
    case "color-burn": {
      // Classic prioritizes the source endpoint, unlike W3C (white, black).
      if (!classic && backdrop === 1) return 1;
      if (source === 0) return 0;
      const divisor = classic ? Math.max(source, 1e-4) : source;
      return Math.max(0, 1 - (1 - backdrop) / divisor);
    }
    case "lighten":
      return Math.max(backdrop, source);
    case "screen":
      return backdrop + source - backdrop * source;
    case "plus-lighter":
      return Math.min(1, backdrop + source);
    case "color-dodge": {
      // Classic also floors the divisor. This affects floating point samples
      // near the endpoint, but no interior UNORM8 input is that close to it.
      if (!classic && backdrop === 0) return 0;
      if (source === 1) return 1;
      const divisor = classic ? Math.max(1 - source, 1e-4) : 1 - source;
      return Math.min(1, backdrop / divisor);
    }
    case "overlay":
      return hardLight(source, backdrop);
    case "soft-light":
      return softLight(backdrop, source);
    case "hard-light":
      return hardLight(backdrop, source);
    case "difference":
      return Math.abs(backdrop - source);
    case "exclusion":
      return backdrop * (1 - source) + source * (1 - backdrop);
    default:
      throw new RangeError(`Not a separable blend mode: ${mode}`);
  }
}

function rgbBlend(backdrop, source, mode, classic) {
  switch (mode) {
    case "hue":
      return withLuminance(
        withSaturation(source, saturation(backdrop)),
        luminance(backdrop),
      );
    case "saturation":
      return withLuminance(
        withSaturation(backdrop, saturation(source)),
        luminance(backdrop),
      );
    case "color":
      return withLuminance(source, luminance(backdrop));
    case "luminosity":
      return withLuminance(backdrop, luminance(source));
    default:
      return backdrop.map((value, channel) =>
        channelBlend(value, source[channel], mode, classic),
      );
  }
}

/** RGB overlap only. The default locks Classic's dodge/burn endpoint rules.
 * `w3c` exists for explicit difference tests, never as an implicit correction. */
export function blendRgbReference(backdrop, source, mode, rules = "classic") {
  requireMode(mode);
  requireComponents(backdrop, 3, "Backdrop RGB");
  requireComponents(source, 3, "Source RGB");
  if (rules !== "classic" && rules !== "w3c")
    throw new RangeError(`Unknown blend rules: ${rules}`);
  return rgbBlend(
    Array.from(backdrop),
    Array.from(source),
    mode,
    rules === "classic",
  );
}

/** One Classic blend pass, without texture quantization.
 * Inputs and output are raw normalized texture RGBA. In particular, the scene
 * RGB is NOT unpremultiplied before blending, nor divided by output alpha.
 * Classic forms an effective source from uncovered source color and overlap
 * color, then interpolates from the stored backdrop RGB toward it. This is NOT
 * standard straight-RGBA source-over when the backdrop is partly transparent.
 * A normal-only stack starting at transparent black stores premultiplied RGB;
 * later non-normal modes nevertheless blend that stored RGB directly.
 * plus-lighter uses clamped RGB addition inside this same alpha rule, not the
 * Porter-Duff lighter operator. Keep these legacy behaviors explicit. */
export function compositeClassicRgba(backdrop, source, mode) {
  requireMode(mode);
  requireComponents(backdrop, 4, "Backdrop RGBA");
  requireComponents(source, 4, "Source RGBA");
  const b = Array.from(backdrop).slice(0, 3);
  const s = Array.from(source).slice(0, 3);
  const overlap = rgbBlend(b, s, mode, true);
  const effectiveSource = s.map(
    (value, channel) => value + backdrop[3] * (overlap[channel] - value),
  );
  return [
    ...b.map((value, channel) =>
      clamp(value + source[3] * (effectiveSource[channel] - value)),
    ),
    1 - (1 - backdrop[3]) * (1 - source[3]),
  ];
}

/** Standard counterpart for diagnostic tests: both input/output RGB are
 * straight; transparent output has RGB zero. Blend colors use W3C endpoint
 * rules. plus-lighter is the Porter-Duff plus operator with clamped channels. */
export function compositeStandardRgba(backdrop, source, mode) {
  requireMode(mode);
  requireComponents(backdrop, 4, "Backdrop RGBA");
  requireComponents(source, 4, "Source RGBA");
  const b = Array.from(backdrop).slice(0, 3);
  const s = Array.from(source).slice(0, 3);
  const ba = backdrop[3],
    sa = source[3];
  if (mode === "plus-lighter") {
    const alpha = Math.min(1, ba + sa);
    return alpha === 0
      ? [0, 0, 0, 0]
      : [
          ...b.map(
            (value, channel) => clamp(ba * value + sa * s[channel]) / alpha,
          ),
          alpha,
        ];
  }
  const coverage = [ba * (1 - sa), sa * (1 - ba), ba * sa];
  const alpha = coverage.reduce((sum, value) => sum + value, 0);
  if (alpha === 0) return [0, 0, 0, 0];
  const colors = [b, s, rgbBlend(b, s, mode, false)];
  return [
    ...b.map((_, channel) =>
      clamp(
        coverage.reduce(
          (sum, weight, region) => sum + weight * colors[region][channel],
          0,
        ) / alpha,
      ),
    ),
    alpha,
  ];
}

function requireBytes(value, label) {
  if (
    !value ||
    !Number.isInteger(value.length) ||
    value.length === 0 ||
    value.length % 4 !== 0 ||
    !Array.from(value).every(
      (component) =>
        Number.isInteger(component) && component >= 0 && component <= 255,
    )
  )
    throw new RangeError(`${label} must contain complete RGBA8 pixels`);
}

/** Compose equally sized independent layer-target rasters. Input alpha already
 * includes layer opacity. Each call models one UNORM8 scene-target write; it
 * intentionally performs neither geometry sampling nor PNG color conversion. */
export function compositeClassicRgba8(backdrop, source, mode) {
  requireMode(mode);
  requireBytes(backdrop, "Backdrop");
  requireBytes(source, "Source");
  if (backdrop.length !== source.length)
    throw new RangeError("Blend oracle raster dimensions differ");
  const output = new Uint8Array(backdrop.length);
  for (let offset = 0; offset < output.length; offset += 4) {
    const b = Array.from(
      backdrop.slice(offset, offset + 4),
      (value) => value / 255,
    );
    const s = Array.from(
      source.slice(offset, offset + 4),
      (value) => value / 255,
    );
    const pixel = compositeClassicRgba(b, s, mode);
    for (let channel = 0; channel < 4; channel++)
      output[offset + channel] = Math.round(clamp(pixel[channel]) * 255);
  }
  return output;
}

/** Bottom-to-top order is supplied by the authored test fixture, never by a
 * product plan. Intermediate scene-target quantization occurs after EACH layer. */
export function composeClassicBlendStackRgba8(background, layers) {
  requireBytes(background, "Background");
  if (!Array.isArray(layers))
    throw new TypeError("Blend oracle layers must be an array");
  return layers.reduce(
    (backdrop, layer) =>
      compositeClassicRgba8(backdrop, layer.rgba, layer.mode),
    Uint8Array.from(background),
  );
}
