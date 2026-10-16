import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { spawn } from "node:child_process";

export const GRAPHIC_OUTPUT_ORACLE_MODEL =
  "independent-graphic-output-yuv420p-v1";
export const GRAPHIC_OUTPUT_RAW_MODEL =
  "independent-graphic-output-raw-rgba-v1";
export const graphicOutputHash = (bytes) =>
  createHash("sha256").update(bytes).digest("hex");
const FRAME_BYTES = 320 * 180 * 4;

// Chroma sampling belongs to the observable output format. This reference only
// converts independently authored RGBA to uncompressed YUV420P and back. It does
// not run the production renderer or H264 encoder, so H264 quantization errors
// remain visible to the unchanged acceptance limits.
export async function observeYuv420Rgba(
  rgba,
  {
    width = 320,
    height = 180,
    ffmpeg = process.env.OPENCUT_FFMPEG ?? "ffmpeg",
  } = {},
) {
  const frameBytes = width * height * 4;
  if (
    !Number.isSafeInteger(width) ||
    !Number.isSafeInteger(height) ||
    width < 2 ||
    height < 2 ||
    width % 2 ||
    height % 2 ||
    !rgba.length ||
    rgba.length % frameBytes
  )
    throw Error("Invalid opaque RGBA sequence dimensions");
  for (let i = 3; i < rgba.length; i += 4)
    if (rgba[i] !== 255) throw Error("Output observation requires opaque RGBA");
  const convert = (bytes, inputFormat, outputFormat, outputBytes) =>
    new Promise((resolve, reject) => {
      const child = spawn(
        ffmpeg,
        [
          "-hide_banner",
          "-loglevel",
          "error",
          "-f",
          "rawvideo",
          "-pixel_format",
          inputFormat,
          "-video_size",
          `${width}x${height}`,
          "-framerate",
          "30",
          "-i",
          "pipe:0",
          "-f",
          "rawvideo",
          "-pix_fmt",
          outputFormat,
          "pipe:1",
        ],
        { stdio: ["pipe", "pipe", "pipe"] },
      );
      const chunks = [],
        errors = [];
      let length = 0;
      child.on("error", reject);
      child.stdin.on("error", reject);
      child.stderr.on("data", (chunk) => errors.push(chunk));
      child.stdout.on("data", (chunk) => {
        length += chunk.length;
        if (length > outputBytes) {
          child.kill();
          reject(Error("Unexpected color observation output length"));
        } else chunks.push(chunk);
      });
      child.on("close", (code) => {
        if (code !== 0)
          reject(
            Error(
              `Color observation failed (${code}): ${Buffer.concat(errors).toString()}`,
            ),
          );
        else if (length !== outputBytes)
          reject(Error("Truncated color observation sequence"));
        else resolve(Buffer.concat(chunks));
      });
      child.stdin.end(bytes);
    });
  const yuv = await convert(rgba, "rgba", "yuv420p", (rgba.length * 3) / 8);
  return convert(yuv, "yuv420p", "rgba", rgba.length);
}

export function archiveRawMp4Observations(facts) {
  const raw = (facts.pixels ?? []).filter((p) => p.kind === "mp4" && !p.model);
  if (!raw.length) return false;
  facts.pixels_raw_rgba_model = GRAPHIC_OUTPUT_RAW_MODEL;
  facts.pixels_raw_rgba ??= [];
  for (const observation of raw)
    if (
      !facts.pixels_raw_rgba.some(
        (p) => JSON.stringify(p) === JSON.stringify(observation),
      )
    )
      facts.pixels_raw_rgba.push(structuredClone(observation));
  facts.pixels = facts.pixels.filter((p) => !raw.includes(p));
  return true;
}

export function hasCurrentPixelObservation(facts, sample, referenceKey) {
  return (facts.pixels ?? []).some(
    (p) =>
      p.kind === sample.kind &&
      p.time === sample.time &&
      p.passed &&
      (sample.kind !== "mp4" ||
        (p.model === GRAPHIC_OUTPUT_ORACLE_MODEL &&
          p.reference_key === referenceKey)),
  );
}

export function validateGraphicReferenceSeed(
  metadata,
  identity,
  expected,
  base,
  specs,
) {
  for (const key of [
    "source_sha256",
    "fonts",
    "titles_sha256",
    "captions_sha256",
    "specs_sha256",
    "graphic_oracle_source_sha256",
    "independent_base_source_sha256",
    "ffmpeg_version_sha256",
  ])
    if (JSON.stringify(metadata[key]) !== JSON.stringify(identity[key]))
      throw Error(`Independent reference seed identity differs: ${key}`);
  if (
    metadata.model !== identity.graphic_model ||
    metadata.width !== 320 ||
    metadata.height !== 180 ||
    metadata.frame_count !== 90 ||
    metadata.fps !== 30 ||
    metadata.independent !== true
  )
    throw Error("Independent reference seed model differs");
  if (
    graphicOutputHash(specs) !== identity.specs_sha256 ||
    expected.length !== FRAME_BYTES * 90 ||
    base.length !== expected.length ||
    graphicOutputHash(expected) !== metadata.expected_sha256 ||
    graphicOutputHash(base) !== metadata.base_sha256
  )
    throw Error("Independent reference seed content differs");
}

export async function loadGraphicOutputReference({
  directory,
  identity,
  generate,
  seedDirectory,
  ffmpeg,
}) {
  const key = graphicOutputHash(JSON.stringify(identity));
  const cache = path.join(directory, "b4-graphic-output-reference", key);
  const files = [
    "expected.rgba",
    "base.rgba",
    "expected-observed.rgba",
    "base-observed.rgba",
  ];
  const metadataPath = path.join(cache, "metadata.json");
  let metadata;
  try {
    metadata = JSON.parse(await fs.readFile(metadataPath, "utf8"));
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  if (metadata) {
    if (JSON.stringify(metadata.identity) !== JSON.stringify(identity))
      throw Error("Graphic output reference cache identity differs");
    const bytes = await Promise.all(
      files.map((file) => fs.readFile(path.join(cache, file))),
    );
    for (const [i, file] of files.entries())
      if (
        bytes[i].length !== FRAME_BYTES * 90 ||
        graphicOutputHash(bytes[i]) !== metadata.files[file]
      )
        throw Error(`Graphic output reference cache content differs: ${file}`);
    return {
      metadata: { ...metadata, directory: cache },
      expected: bytes[2],
      base: bytes[3],
    };
  }
  let reference;
  if (seedDirectory) {
    const [seed, expected, base, specsText] = await Promise.all([
      fs
        .readFile(path.join(seedDirectory, "metadata.json"), "utf8")
        .then(JSON.parse),
      fs.readFile(path.join(seedDirectory, "expected.rgba")),
      fs.readFile(path.join(seedDirectory, "base.rgba")),
      fs.readFile(path.join(seedDirectory, "specs.json"), "utf8"),
    ]);
    validateGraphicReferenceSeed(
      seed,
      identity,
      expected,
      base,
      JSON.stringify(JSON.parse(specsText)),
    );
    reference = {
      expected,
      base,
      frames: seed.frames,
      unique_bases: seed.unique_bases,
    };
  } else reference = await generate();
  if (
    reference.expected.length !== FRAME_BYTES * 90 ||
    reference.base.length !== reference.expected.length
  )
    throw Error("Independent reference has wrong frame count");
  const expectedObserved = await observeYuv420Rgba(reference.expected, {
    ffmpeg,
  });
  const baseObserved = await observeYuv420Rgba(reference.base, { ffmpeg });
  const bytes = [
    reference.expected,
    reference.base,
    expectedObserved,
    baseObserved,
  ];
  metadata = {
    identity,
    key,
    files: Object.fromEntries(
      files.map((file, i) => [file, graphicOutputHash(bytes[i])]),
    ),
    frames: reference.frames,
    unique_bases: reference.unique_bases,
  };
  await fs.mkdir(cache, { recursive: true });
  for (const [i, file] of files.entries())
    await fs.writeFile(path.join(cache, file), bytes[i]);
  await fs.writeFile(metadataPath, JSON.stringify(metadata, null, 2) + "\n");
  return {
    metadata: { ...metadata, directory: cache },
    expected: expectedObserved,
    base: baseObserved,
  };
}

export function measureMixedPixels(actual, expected, base, specs, kind) {
  if (
    actual.length !== FRAME_BYTES ||
    expected.length !== FRAME_BYTES ||
    base.length !== FRAME_BYTES
  )
    throw Error("Mixed pixel image dimensions differ");
  // Both expected and no-graphic base must be in the same observation domain.
  // Original limits and the BT.601 delta threshold are unchanged.
  const limits =
    kind === "preview"
      ? { mean: 3, regionMean: 3, iou: 0.98 }
      : { mean: 5, regionMean: 6, iou: 0.9 };
  let sum = 0,
    alphaErrors = 0;
  for (let i = 0; i < actual.length; i += 4) {
    for (let c = 0; c < 3; c++)
      sum += Math.abs(actual[i + c] - expected[i + c]);
    alphaErrors += +(actual[i + 3] !== 255);
  }
  const regions = specs.map((spec) => {
    let error = 0,
      intersection = 0,
      union = 0,
      expectedPixels = 0,
      pixels = 0;
    for (let y = 90 + spec.y - 38; y < 90 + spec.y + 38; y++)
      for (let x = 160 + spec.x - 38; x < 160 + spec.x + 38; x++) {
        const i = (y * 320 + x) * 4;
        const on = (data) =>
          Math.abs(
            0.299 * (data[i] - base[i]) +
              0.587 * (data[i + 1] - base[i + 1]) +
              0.114 * (data[i + 2] - base[i + 2]),
          ) > 16;
        const a = on(actual),
          e = on(expected);
        intersection += +(a && e);
        union += +(a || e);
        expectedPixels += +e;
        pixels++;
        for (let c = 0; c < 3; c++)
          error += Math.abs(actual[i + c] - expected[i + c]);
      }
    return {
      kind: spec.kind,
      mean: error / (pixels * 3),
      iou: union ? intersection / union : 0,
      expectedPixels,
    };
  });
  return { limits, mean: sum / (320 * 180 * 3), alphaErrors, regions };
}
