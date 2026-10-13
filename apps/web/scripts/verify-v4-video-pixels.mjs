import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import assert from "node:assert/strict";
import { chromium } from "@playwright/test";

// Independent Canvas oracle. No OpenCut renderer or bound-layout receipt is
// imported. FFmpeg is used solely to decode original source/output frames.
// Predeclared thresholds: preview RGB MAE<=3, outside-title MAE<=3 and p99<=8,
// glyph-change IoU>=.98. H264/yuv420 output RGB MAE<=5, outside-title MAE<=5
// and p99<=20, BT.601 luma-change IoU>=.90 at the same fixed delta32.
// Raw RGB-change IoU remains recorded against its original .90 gate, including
// failure, because 4:2:0 chroma expansion can alter RGB without moving a glyph.
// Near-black/white alpha must remain 255.
const directory = process.argv[2];
assert(directory, "Usage: node apps/web/scripts/verify-v4-video-pixels.mjs OWNED_WORKSPACE");
const evidence = JSON.parse(await fs.readFile(path.join(directory, "b2-video-evidence.json"), "utf8"));
// Preserve the first failed raw-RGB evidence before adding a second independent
// observation strategy. No existing threshold or result is rewritten in place.
try { await fs.copyFile(path.join(directory, "b2-independent-pixels.json"), path.join(directory, "b2-independent-pixels-original-rawrgb.json"), 1); }
catch (error) { if (!["ENOENT", "EEXIST"].includes(error.code)) throw error; }
const fontsDirectory = process.env.OPENCUT_B1_FONT_SOURCES;
assert(fontsDirectory, "OPENCUT_B1_FONT_SOURCES must identify the retained independent font-source manifest directory");
const manifest = JSON.parse(await fs.readFile(path.join(fontsDirectory, "manifest.json"), "utf8"));
const fonts = [];
for (const font of evidence.fonts) {
  const source = manifest.files.find(s => s.sha256 === font.content.content_hash && s.file.endsWith(".ttf"));
  assert(source, `No independently retained font source for ${font.family}`);
  const bytes = await fs.readFile(source.file);
  assert.equal(createHash("sha256").update(bytes).digest("hex"), source.sha256);
  fonts.push({ ...font, bytes: bytes.toString("base64") });
}
const decode = async (file, time, name) => {
  const output = path.join(directory, name);
  execFileSync(process.env.OPENCUT_FFMPEG ?? "ffmpeg", ["-hide_banner", "-loglevel", "error", "-y", "-ss", String(time), "-i", file, "-frames:v", "1", output]);
  return { output, png: (await fs.readFile(output)).toString("base64") };
};
const candidates = [];
for (let i = 0; i < evidence.pictures.length; i++) {
  const picture = evidence.pictures[i];
  candidates.push({ kind: "preview", time: picture.time, actual: (await fs.readFile(picture.output)).toString("base64"),
    source: (await decode(path.join(directory, "sample.mp4"), picture.time, `b2-oracle-preview-source-${i}.png`)).png });
}
for (const time of [.5, 1.5]) {
  candidates.push({ kind: "mp4", time, actual: (await decode(evidence.output, time, `b2-mp4-${time}.png`)).png,
    source: (await decode(path.join(directory, "sample.mp4"), time, `b2-oracle-mp4-source-${time}.png`)).png });
}
const browser = await chromium.launch({ headless: true, executablePath: process.env.OPENCUT_CHROME });
const observations = [];
try {
  const page = await browser.newPage();
  for (const candidate of candidates) {
    const measured = await page.evaluate(async ({ candidate, fonts, titles, settings }) => {
      const width = settings.canvas_size.width, height = settings.canvas_size.height;
      const load = async base64 => { const image = new Image(); image.src = `data:image/png;base64,${base64}`; await image.decode(); return image; };
      const base = new OffscreenCanvas(width, height), baseCtx = base.getContext("2d");
      baseCtx.drawImage(await load(candidate.source), 0, 0, width, height);
      const expected = new OffscreenCanvas(width, height), ctx = expected.getContext("2d");
      ctx.drawImage(base, 0, 0);
      const actual = new OffscreenCanvas(width, height), actualCtx = actual.getContext("2d");
      const image = await load(candidate.actual);
      if (image.naturalWidth !== width || image.naturalHeight !== height) throw new Error("Output dimensions changed");
      actualCtx.drawImage(image, 0, 0);
      const rectangles = [], faces = [];
      try {
        for (const [index, title] of titles.entries()) {
          const font = fonts.find(f => f.id === title.text.font.font_id);
          const family = `B2IndependentOracle${index}`;
          const face = new FontFace(family, Uint8Array.from(atob(font.bytes), c => c.charCodeAt(0)));
          await face.load(); document.fonts.add(face); faces.push(face);
          const size = height / 90 * title.text.font_size.value;
          const x = width / 2 + title.visual.transform.position_x_px, y = height / 2 + title.visual.transform.position_y_px;
          ctx.font = `${size}px ${family}`; ctx.textBaseline = "middle"; ctx.textAlign = "center"; ctx.fillStyle = "white";
          const metrics = ctx.measureText(title.text.content);
          rectangles.push({ x0: Math.floor(x - metrics.width / 2 - 4), x1: Math.ceil(x + metrics.width / 2 + 4), y0: Math.floor(y - size - 4), y1: Math.ceil(y + size + 4) });
          ctx.fillText(title.text.content, x, y);
        }
        const a = actualCtx.getImageData(0, 0, width, height).data;
        const e = ctx.getImageData(0, 0, width, height).data;
        const b = baseCtx.getImageData(0, 0, width, height).data;
        let sum = 0, outsideSum = 0, outsideCount = 0, alphaErrors = 0;
        const outsideDiffs = [], glyphs = rectangles.map(() => ({ intersection: 0, union: 0, expectedCount: 0, actualCount: 0 })), lumaGlyphs = rectangles.map(() => ({ intersection: 0, union: 0, expectedCount: 0, actualCount: 0 }));
        for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
          const offset = (y * width + x) * 4;
          const differences = [0, 1, 2].map(c => Math.abs(a[offset + c] - e[offset + c]));
          sum += differences.reduce((s, n) => s + n, 0); alphaErrors += Number(a[offset + 3] !== 255);
          const index = rectangles.findIndex(r => x >= r.x0 && x <= r.x1 && y >= r.y0 && y <= r.y1);
          if (index === -1) { outsideCount++; outsideSum += differences.reduce((s, n) => s + n, 0); outsideDiffs.push(Math.max(...differences)); }
          else {
            const changed = data => Math.max(...[0, 1, 2].map(c => Math.abs(data[offset + c] - b[offset + c]))) > 32;
            const onActual = changed(a), onExpected = changed(e), g = glyphs[index];
            g.intersection += Number(onActual && onExpected); g.union += Number(onActual || onExpected);
            g.expectedCount += Number(onExpected); g.actualCount += Number(onActual);
            // SD H264 source/output have unspecified/default BT.601 matrices.
            // Use luma coefficients on both independently authored and decoded
            // images, rather than taking any product layout/mask as a reference.
            const lumaChanged = data => Math.abs(.299 * (data[offset] - b[offset]) + .587 * (data[offset + 1] - b[offset + 1]) + .114 * (data[offset + 2] - b[offset + 2])) > 32;
            const lumaActual = lumaChanged(a), lumaExpected = lumaChanged(e), l = lumaGlyphs[index];
            l.intersection += Number(lumaActual && lumaExpected); l.union += Number(lumaActual || lumaExpected);
            l.expectedCount += Number(lumaExpected); l.actualCount += Number(lumaActual);
          }
        }
        outsideDiffs.sort((a, b) => a - b);
        return { width, height, meanRgbError: sum / (width * height * 3), outsideMeanRgbError: outsideSum / (outsideCount * 3),
          outsideP99: outsideDiffs[Math.floor(outsideDiffs.length * .99)], alphaErrors,
          glyphs: glyphs.map(g => ({ ...g, iou: g.intersection / g.union })), lumaGlyphs: lumaGlyphs.map(g => ({ ...g, iou: g.intersection / g.union })), rectangles };
      } finally { for (const face of faces) document.fonts.delete(face); }
    }, { candidate, fonts, titles: evidence.titles, settings: evidence.settings });
    const limits = candidate.kind === "preview" ? { mean: 3, p99: 8, iou: .98 } : { mean: 5, p99: 20, iou: .90 };
    observations.push({ kind: candidate.kind, time: candidate.time, limits, selectedMask: candidate.kind === "preview" ? "raw_rgb_delta32" : "bt601_luma_delta32", rawRgbGatePassed: measured.glyphs.every(g => g.iou >= limits.iou), ...measured });
    assert.equal(measured.alphaErrors, 0);
    assert(measured.meanRgbError <= limits.mean, `${candidate.kind}@${candidate.time} RGB error ${measured.meanRgbError}`);
    assert(measured.outsideMeanRgbError <= limits.mean && measured.outsideP99 <= limits.p99, `${candidate.kind} unexpected content outside independently placed glyphs; notes excluded`);
    for (const glyph of candidate.kind === "preview" ? measured.glyphs : measured.lumaGlyphs) { assert(glyph.expectedCount > 100); assert(glyph.iou >= limits.iou, `${candidate.kind} independent selected-mask glyph IoU ${glyph.iou}`); }
  }
} finally {
  await browser.close();
  await fs.writeFile(path.join(directory, "b2-independent-pixels.json"), JSON.stringify({ source: "original sample.mp4 and independently hashed font source bytes", project_id: evidence.project_id, revision: evidence.revision, observations }, null, 2) + "\n");
}
process.stdout.write(JSON.stringify({ passed: true, observations }, null, 2) + "\n");
