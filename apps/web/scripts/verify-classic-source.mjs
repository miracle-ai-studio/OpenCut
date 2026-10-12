// Synthetic, isolated-origin acceptance of the read-only historical input adapter.
// No user browser profile or real project database is opened.
import { chromium, expect } from "@playwright/test";
import { createServer } from "vite";
import fs from "node:fs/promises";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
const directory = path.resolve(process.env.OPENCUT_CLASSIC_WORKSPACE);
await fs.mkdir(directory, { recursive: true });
const fixtures = [
  { id: "mv", name: "video.mp4", type: "video" },
  { id: "ma", name: "audio.wav", type: "audio" },
  { id: "shelf", name: "shelf.ppm", type: "image" },
];
execFileSync("ffmpeg", [
  "-v",
  "error",
  "-f",
  "lavfi",
  "-i",
  "color=red:s=320x180:r=10:d=2",
  "-c:v",
  "libx264",
  "-pix_fmt",
  "yuv420p",
  "-y",
  path.join(directory, "video.mp4"),
]);
execFileSync("ffmpeg", [
  "-v",
  "error",
  "-f",
  "lavfi",
  "-i",
  "sine=frequency=800:sample_rate=48000:duration=2",
  "-y",
  path.join(directory, "audio.wav"),
]);
await fs.writeFile(
  path.join(directory, "shelf.ppm"),
  Buffer.concat([
    Buffer.from("P6\n4 4\n255\n"),
    Buffer.from(Array.from({ length: 48 }, (_, i) => (i % 3 === 1 ? 255 : 0))),
  ]),
);
for (const fixture of fixtures) {
  const bytes = await fs.readFile(path.join(directory, fixture.name));
  fixture.bytes = [...bytes];
  fixture.size = bytes.length;
  fixture.sha256 = createHash("sha256").update(bytes).digest("hex");
}
const server = await createServer({
  root: path.resolve(import.meta.dirname, ".."),
  configFile: false,
  server: { host: "127.0.0.1", port: 0 },
  plugins: [
    {
      name: "synthetic-classic-page",
      configureServer(server) {
        server.middlewares.use((request, response, next) => {
          if (request.url !== "/classic-input-acceptance") return next();
          response.setHeader("Content-Type", "text/html");
          response.end(
            "<!doctype html><title>Classic read-only input acceptance</title>",
          );
        });
      },
    },
  ],
});
let browser;
try {
  await server.listen();
  const endpoint = server.resolvedUrls.local[0];
  browser = await chromium.launch({
    headless: true,
    executablePath: process.env.OPENCUT_CHROME,
  });
  const page = await browser.newPage();
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));
  await page.goto(`${endpoint}classic-input-acceptance`);
  const report = await page.evaluate(async (fixtures) => {
    const source = await import("/src/editor/workspace/classic/source.ts");
    const { decodeClassic } =
      await import("/src/editor/workspace/classic/decode.ts");
    const { fileDigest } =
      await import("/src/editor/workspace/classic/identity.ts");
    const checks = [];
    const assert = (condition, detail) => {
      if (!condition) throw new Error(detail);
    };
    const expectFailure = async (action, pattern) => {
      try {
        await action();
      } catch (error) {
        assert(pattern.test(String(error)), String(error));
        return;
      }
      throw new Error("Expected recovery rejection");
    };
    assert(
      (await source.discoverClassic()).length === 0,
      "absent project DB is empty",
    );
    assert(
      !(await indexedDB.databases()).some(
        (db) => db.name === "video-editor-projects",
      ),
      "read of absent DB created persistence",
    );
    checks.push(
      "Absent database discovery aborts creation and leaves no database",
    );
    const seed = (name, store, rows) =>
      new Promise((resolve, reject) => {
        const request = indexedDB.open(name, 1);
        request.onupgradeneeded = () =>
          request.result.createObjectStore(store, { keyPath: "id" });
        request.onerror = () => reject(request.error);
        request.onsuccess = () => {
          const db = request.result,
            tx = db.transaction(store, "readwrite");
          for (const row of rows) tx.objectStore(store).put(row);
          tx.oncomplete = () => {
            db.close();
            resolve();
          };
          tx.onabort = () => {
            db.close();
            reject(tx.error);
          };
        };
      });
    const raw = {
      id: "old",
      name: "Classic recovery",
      version: 1,
      fps: 10,
      canvasSize: { width: 320, height: 180 },
      scenes: [
        { id: "main", name: "Main", isMain: true, tracks: [], bookmarks: [] },
      ],
    };
    const media = fixtures.map(({ id, name, type, size }) => ({
      id,
      name,
      type,
      size,
    }));
    const timeline = {
      id: "timeline",
      tracks: [
        {
          id: "video-track",
          name: "Video",
          type: "media",
          elements: [
            {
              id: "video-clip",
              name: "Red",
              type: "media",
              mediaId: "mv",
              startTime: 0,
              duration: 2,
              trimStart: 0,
              trimEnd: 0,
            },
          ],
        },
        {
          id: "audio-track",
          name: "Audio",
          type: "audio",
          elements: [
            {
              id: "audio-clip",
              name: "Tone",
              type: "audio",
              mediaId: "ma",
              startTime: 0,
              duration: 2,
              trimStart: 0,
              trimEnd: 0,
            },
          ],
        },
      ],
    };
    await seed("video-editor-projects", "projects", [
      raw,
      { id: "damaged", name: "Damaged", version: 1, scenes: [null] },
    ]);
    await seed("video-editor-media-old", "media-metadata", media);
    // Exercise the actual oldest main-scene database fallback.
    await seed("video-editor-timelines-old", "timeline", [timeline]);
    const root = await navigator.storage.getDirectory(),
      dir = await root.getDirectoryHandle("media-files-old", { create: true });
    for (const fixture of fixtures) {
      const writable = await (
        await dir.getFileHandle(fixture.id, { create: true })
      ).createWritable();
      await writable.write(new Uint8Array(fixture.bytes));
      await writable.close();
    }
    const before = JSON.stringify({
      projects: await source.readLegacyStore(
        "video-editor-projects",
        "projects",
      ),
      media: await source.readLegacyStore(
        "video-editor-media-old",
        "media-metadata",
      ),
      timeline: await source.readLegacyStore(
        "video-editor-timelines-old",
        "timeline",
      ),
      dbs: await indexedDB.databases(),
    });
    const discovered = await source.discoverClassic();
    assert(
      discovered.length === 2 &&
        discovered.find((p) => p.id === "old").version === 1,
      "discovery lost old project",
    );
    const captured = await source.captureClassic(
      "old",
      new AbortController().signal,
    );
    assert(
      JSON.stringify(captured.raw) === JSON.stringify(raw),
      "capture changed legacy project",
    );
    const decoded = decodeClassic(captured);
    assert(
      decoded.project.version === 31 && decoded.steps.length === 30,
      "full decoder chain did not execute",
    );
    assert(
      decoded.content === decodeClassic(captured).content,
      "decoder replay changed",
    );
    assert(
      decoded.project.scenes[0].tracks.main.elements[0].duration === 240000,
      "time conversion is not independent 2-second expected value",
    );
    checks.push(
      "Real v1 project/timeline/media stores discovered; fallback timeline captured and 30 pure steps replay deterministically",
    );
    const hashes = {};
    for (const fixture of fixtures) {
      const file = await source.readClassicFile("old", fixture);
      hashes[fixture.id] = await fileDigest(file, new AbortController().signal);
      assert(
        hashes[fixture.id] === fixture.sha256 && file.name === fixture.name,
        "old media bytes changed",
      );
    }
    checks.push(
      "Video, audio and unused library image recovered from real OPFS with independent SHA256",
    );
    await expectFailure(
      () => source.captureClassic("damaged", new AbortController().signal),
      /Damaged Classic scene/,
    );
    await expectFailure(
      () => source.readClassicFile("old", { id: "missing" }),
      /NotFound/,
    );
    await expectFailure(
      () => source.readClassicFile("old", { ...fixtures[0], size: 1 }),
      /size changed/,
    );
    const aborted = new AbortController();
    aborted.abort();
    await expectFailure(
      () => source.captureClassic("old", aborted.signal),
      /Abort/,
    );
    const pending = new AbortController(),
      read = source.readLegacyStore(
        "video-editor-projects",
        "projects",
        pending.signal,
      );
    pending.abort();
    await expectFailure(() => read, /Abort/);
    const retry = await source.captureClassic(
      "old",
      new AbortController().signal,
    );
    assert(
      decodeClassic(retry).sourceDigest === decoded.sourceDigest,
      "retry changed source identity",
    );
    checks.push(
      "Damaged scene, missing/changed file and pre-start/in-flight cancellation reject; same intact source retries",
    );
    const after = JSON.stringify({
      projects: await source.readLegacyStore(
        "video-editor-projects",
        "projects",
      ),
      media: await source.readLegacyStore(
        "video-editor-media-old",
        "media-metadata",
      ),
      timeline: await source.readLegacyStore(
        "video-editor-timelines-old",
        "timeline",
      ),
      dbs: await indexedDB.databases(),
    });
    assert(
      before === after,
      "legacy database changed or adapter created a missing timeline database",
    );
    for (const fixture of fixtures)
      assert(
        (await fileDigest(
          await source.readClassicFile("old", fixture),
          new AbortController().signal,
        )) === hashes[fixture.id],
        "original OPFS changed after failed/retried reads",
      );
    checks.push(
      "Original DB versions/all rows and all OPFS hashes unchanged after successful/failed/cancelled/retried recovery reads",
    );
    return {
      status: "passed",
      checks,
      captured,
      decoded,
      hashes,
      origin: location.origin,
    };
  }, fixtures);
  expect(pageErrors).toEqual([]);
  report.pageErrors = pageErrors;
  report.media = fixtures.map(({ bytes, ...rest }) => rest);
  await fs.writeFile(
    path.join(directory, "source-browser-report.json"),
    JSON.stringify(report, null, 2) + "\n",
  );
  console.log(
    JSON.stringify({
      status: "passed",
      checks: report.checks.length,
      report: path.join(directory, "source-browser-report.json"),
    }),
  );
} finally {
  await browser?.close();
  await server.close();
}
