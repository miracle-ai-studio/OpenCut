// Acceptance-only reference math. No renderer, shader, prepared plan or render
// receipt is imported. Sources come from independently constructed Canvas paths.
export const GRAPHIC_ORACLE_MODEL = "independent-straight-alpha-bilinear-v1";

const byte = (value) => Math.max(0, Math.min(255, Math.round(value)));

/** Sample at source edge coordinates: pixel (0, 0) has center (0.5, 0.5).
 * The admitted Classic GPU profile uploads straight RGBA and uses linear,
 * clamp-to-edge filtering. Canvas drawImage instead filters premultiplied RGBA
 * and is therefore not an equivalent reference at transparent boundaries. */
export function sampleStraightRgba(source, x, y) {
  if (x < 0 || x > source.width || y < 0 || y > source.height) return null;
  const sx = Math.max(0, Math.min(source.width - 1, x - 0.5)),
    sy = Math.max(0, Math.min(source.height - 1, y - 0.5)),
    x0 = Math.floor(sx),
    y0 = Math.floor(sy),
    x1 = Math.min(x0 + 1, source.width - 1),
    y1 = Math.min(y0 + 1, source.height - 1),
    tx = sx - x0,
    ty = sy - y0;
  return Array.from({ length: 4 }, (_, channel) => {
    const at = (px, py) => source.data[(py * source.width + px) * 4 + channel];
    return (
      (at(x0, y0) * (1 - tx) + at(x1, y0) * tx) * (1 - ty) +
      (at(x0, y1) * (1 - tx) + at(x1, y1) * tx) * ty
    );
  });
}

/** Independent inverse affine transform and source-over on an opaque canvas.
 * UNORM8 rounding occurs once at the layer target and once at the scene target.
 * Source dimensions/centers describe the oracle's own raster, never a receipt. */
export function composeGraphicOracle(layers, width, height, background = null) {
  if (background && background.length !== width * height * 4)
    throw Error("Graphic oracle background dimensions differ");
  const result = background
    ? Uint8Array.from(background)
    : new Uint8Array(width * height * 4);
  for (let i = 3; i < result.length; i += 4) result[i] = 255;
  for (const layer of layers) {
    const { source, centerX, centerY, scaleX, scaleY, rotation, opacity } =
      layer;
    if (
      !Number.isInteger(source.width) ||
      source.width < 1 ||
      !Number.isInteger(source.height) ||
      source.height < 1 ||
      source.data.length !== source.width * source.height * 4 ||
      ![centerX, centerY, scaleX, scaleY, rotation, opacity].every(
        Number.isFinite,
      ) ||
      scaleX === 0 ||
      scaleY === 0 ||
      opacity < 0 ||
      opacity > 1
    )
      throw Error("Invalid independent graphic oracle layer");
    const c = Math.cos(rotation),
      s = Math.sin(rotation);
    for (let y = 0; y < height; y++)
      for (let x = 0; x < width; x++) {
        const dx = x + 0.5 - centerX,
          dy = y + 0.5 - centerY,
          sx = (c * dx + s * dy) / scaleX + source.width / 2,
          sy = (-s * dx + c * dy) / scaleY + source.height / 2,
          sample = sampleStraightRgba(source, sx, sy);
        if (!sample) continue;
        const alpha = byte(sample[3] * opacity) / 255,
          i = (y * width + x) * 4;
        for (let channel = 0; channel < 3; channel++)
          result[i + channel] = byte(
            byte(sample[channel]) * alpha + result[i + channel] * (1 - alpha),
          );
      }
  }
  return result;
}

export function measureGraphicOracle(actual, expected, width, height, specs) {
  if (actual.length !== width * height * 4 || expected.length !== actual.length)
    throw Error("Graphic oracle image dimensions differ");
  let intersection = 0,
    union = 0,
    error = 0,
    blankError = 0;
  for (let i = 0; i < actual.length; i += 4) {
    const onA = Math.max(actual[i], actual[i + 1], actual[i + 2]) > 32,
      onE = Math.max(expected[i], expected[i + 1], expected[i + 2]) > 32;
    intersection += +(onA && onE);
    union += +(onA || onE);
    for (let channel = 0; channel < 4; channel++)
      error += Math.abs(actual[i + channel] - expected[i + channel]);
    if (!expected[i] && !expected[i + 1] && !expected[i + 2])
      blankError = Math.max(
        blankError,
        actual[i],
        actual[i + 1],
        actual[i + 2],
      );
  }
  // Classification uses expected pixels alone; actual errors cannot relabel
  // a failed interior pixel as a permitted resampling boundary.
  let innerMaxError = 0,
    innerPixels = 0,
    boundaryError = 0,
    boundaryPixels = 0;
  for (let y = 2; y < height - 2; y++)
    for (let x = 2; x < width - 2; x++) {
      const i = (y * width + x) * 4;
      let stable = true;
      for (let dy = -2; dy <= 2; dy++)
        for (let dx = -2; dx <= 2; dx++) {
          const j = ((y + dy) * width + x + dx) * 4;
          for (let channel = 0; channel < 3; channel++)
            if (Math.abs(expected[j + channel] - expected[i + channel]) > 2)
              stable = false;
        }
      if (stable) {
        if (Math.max(expected[i], expected[i + 1], expected[i + 2]) > 32)
          innerPixels++;
        for (let channel = 0; channel < 3; channel++)
          innerMaxError = Math.max(
            innerMaxError,
            Math.abs(actual[i + channel] - expected[i + channel]),
          );
      } else {
        boundaryPixels++;
        for (let channel = 0; channel < 3; channel++)
          boundaryError += Math.abs(
            actual[i + channel] - expected[i + channel],
          );
      }
    }
  const regions = specs.map((spec) => {
    let count = 0,
      error = 0,
      pixels = 0;
    for (
      let y = Math.max(0, height / 2 + spec.y - 38);
      y < Math.min(height, height / 2 + spec.y + 38);
      y++
    )
      for (
        let x = Math.max(0, width / 2 + spec.x - 38);
        x < Math.min(width, width / 2 + spec.x + 38);
        x++
      ) {
        const i = (y * width + x) * 4;
        if (Math.max(expected[i], expected[i + 1], expected[i + 2]) > 32)
          count++;
        for (let channel = 0; channel < 3; channel++)
          error += Math.abs(actual[i + channel] - expected[i + channel]);
        pixels++;
      }
    return {
      kind: spec.kind,
      expected_pixels: count,
      mean_channel_error: error / (pixels * 3),
    };
  });
  return {
    model: GRAPHIC_ORACLE_MODEL,
    width,
    height,
    iou: union ? intersection / union : 1,
    mean_channel_error: error / actual.length,
    blank_error: blankError,
    inner_max_channel_error: innerMaxError,
    inner_pixels: innerPixels,
    boundary_mean_channel_error: boundaryPixels
      ? boundaryError / (boundaryPixels * 3)
      : 0,
    boundary_pixels: boundaryPixels,
    regions,
  };
}

// Browser-only independent path construction, shared by PNG and output checks.
export async function readGraphicOracleInputs({ png, specs }) {
  const image = new Image();
  image.src = `data:image/png;base64,${png}`;
  await image.decode();
  const layers = [];
  const color = (hex, alpha) =>
    `rgba(${parseInt(hex.slice(1, 3), 16)},${parseInt(hex.slice(3, 5), 16)},${parseInt(hex.slice(5, 7), 16)},${alpha})`;
  // Derive geometry independently, without production paths or receipts.
  // An inside source ends at its logical rectangle; other strokes need
  // transparent margin. The oversized integer margin cannot affect the
  // sample grid and preserves all miter tips in these fixed fixtures.
  for (const s of specs) {
    const w = 60,
      h = 50,
      pad = s.align === "inside" ? 0 : 128,
      source = new OffscreenCanvas(w + 2 * pad, h + 2 * pad),
      c = source.getContext("2d"),
      p = new Path2D();
    if (s.kind === "rectangle")
      p.roundRect(pad, pad, w, h, (Math.min(w, h) * s.rounding) / 100);
    else if (s.kind === "ellipse") {
      const inset = s.align === "center" ? s.strokeWidth / 2 : 0;
      p.ellipse(
        pad + w / 2,
        pad + h / 2,
        w / 2 - inset,
        h / 2 - inset,
        0,
        0,
        Math.PI * 2,
      );
    } else {
      const count = s.kind === "star" ? s.points * 2 : s.sides;
      for (let i = 0; i < count; i++) {
        const radius =
            (Math.min(w, h) / 2) *
            (s.kind === "star" && i % 2 ? s.depth / 100 : 1),
          angle = -Math.PI / 2 + (i * 2 * Math.PI) / count,
          x = pad + w / 2 + radius * Math.cos(angle),
          y = pad + h / 2 + radius * Math.sin(angle);
        if (!i) p.moveTo(x, y);
        else p.lineTo(x, y);
      }
      p.closePath();
    }
    c.fillStyle = color(s.fill, s.fillAlpha);
    c.fill(p);
    c.strokeStyle = color(s.stroke, s.strokeAlpha);
    c.lineWidth = s.strokeWidth;
    if (s.align === "inside") {
      c.save();
      c.clip(p);
      c.lineWidth = 2 * s.strokeWidth;
      c.stroke(p);
      c.restore();
    } else if (s.align === "outside") {
      const border = new OffscreenCanvas(source.width, source.height),
        bc = border.getContext("2d");
      bc.strokeStyle = color(s.stroke, s.strokeAlpha);
      bc.lineWidth = 2 * s.strokeWidth;
      bc.stroke(p);
      bc.globalCompositeOperation = "destination-out";
      bc.fill(p);
      c.drawImage(border, 0, 0);
    } else c.stroke(p);
    const contain = Math.min(320 / w, 180 / h);
    layers.push({
      source: {
        width: source.width,
        height: source.height,
        data: Array.from(
          c.getImageData(0, 0, source.width, source.height).data,
        ),
      },
      centerX: 160 + s.x,
      centerY: 90 + s.y,
      scaleX: contain * s.scale_x,
      scaleY: contain * s.scale_y,
      rotation: (s.rotation_degrees * Math.PI) / 180,
      opacity: 0.9,
    });
  }
  const actual = new OffscreenCanvas(320, 180),
    aCtx = actual.getContext("2d");
  aCtx.drawImage(image, 0, 0);
  return {
    width: image.naturalWidth,
    height: image.naturalHeight,
    actual: Array.from(aCtx.getImageData(0, 0, 320, 180).data),
    layers,
  };
}
