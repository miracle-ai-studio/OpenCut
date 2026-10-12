// Real product recovery, synthetic old data, fresh Chromium profiles only.
import { chromium, expect as baseExpect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import http from "node:http";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
const expect = baseExpect.configure({ timeout: 120000 });
const directory = process.env.OPENCUT_WEB_WORKSPACE,
  endpoint = process.env.OPENCUT_WEB_URL;
const binary = process.env.OPENCUT_WEB_BINARY;
const fixtures = [
  { id: "mv", name: "red.mp4", type: "video", duration: 2 },
  { id: "ma", name: "tone.wav", type: "audio", duration: 30 },
  { id: "shelf", name: "shelf.png", type: "image" },
];
const ffmpeg = (...args) => execFileSync("ffmpeg", ["-v", "error", ...args]);
ffmpeg(
  "-f",
  "lavfi",
  "-i",
  "color=red:s=320x180:r=10:d=2",
  "-c:v",
  "libx264",
  "-pix_fmt",
  "yuv420p",
  "-y",
  path.join(directory, "red.mp4"),
);
ffmpeg(
  "-f",
  "lavfi",
  "-i",
  "sine=frequency=800:sample_rate=48000:duration=30",
  "-ac",
  "2",
  "-y",
  path.join(directory, "tone.wav"),
);
ffmpeg(
  "-f",
  "lavfi",
  "-i",
  "color=green:s=4x4",
  "-frames:v",
  "1",
  "-y",
  path.join(directory, "shelf.png"),
);
for (const item of fixtures) {
  const bytes = await fs.readFile(path.join(directory, item.name));
  Object.assign(item, {
    base64: bytes.toString("base64"),
    size: bytes.length,
    sha256: createHash("sha256").update(bytes).digest("hex"),
  });
}
function call(name, input) {
  const reply = JSON.parse(
    execFileSync(
      binary,
      [
        "--json",
        "--database",
        path.join(directory, "workspace.sqlite"),
        "call",
        name,
        "--input",
        "-",
      ],
      { input: JSON.stringify(input), encoding: "utf8" },
    ),
  );
  expect(reply.error).toBeNull();
  return reply.data;
}
const report = {
  status: "running",
  checks: [],
  consoleErrors: [],
  calls: [],
  chunks: [],
  controlledFaults: [],
};
const check = (name) => {
  report.checks.push(name);
  console.log(name);
};
const oldServer = http.createServer((_request, response) => {
  response.setHeader("Content-Type", "text/html");
  response.end(
    "<!doctype html><title>Isolated synthetic Classic origin</title><main>Classic fixture</main>",
  );
});
await new Promise((resolve) => oldServer.listen(0, "127.0.0.1", resolve));
const oldOrigin = `http://127.0.0.1:${oldServer.address().port}`;
const browser = await chromium.launch({
  headless: true,
  executablePath: process.env.OPENCUT_CHROME,
});
const context = await browser.newContext({
  locale: "en-US",
  viewport: { width: 1440, height: 1000 },
  acceptDownloads: true,
});
const page = await context.newPage();
page.setDefaultTimeout(120000);
page.on("pageerror", (e) => report.consoleErrors.push(e.message));
page.on("request", (request) => {
  const url = new URL(request.url());
  if (url.pathname.startsWith("/v1/tools/"))
    report.calls.push({
      tool: url.pathname.split("/").at(-1),
      input: request.postDataJSON(),
    });
  if (url.pathname.includes("/stream-uploads/") && request.method() === "PUT")
    report.chunks.push({
      path: url.pathname,
      bytes: request.postDataBuffer()?.length,
    });
});
async function seed(target, id, name) {
  return target.evaluate(
    async ({ fixtures, id, name }) => {
      const write = (dbName, store, rows) =>
        new Promise((resolve, reject) => {
          const request = indexedDB.open(dbName, 1);
          request.onupgradeneeded = () =>
            request.result.createObjectStore(store, { keyPath: "id" });
          request.onerror = () => reject(request.error);
          request.onsuccess = () => {
            const db = request.result,
              tx = db.transaction(store, "readwrite");
            rows.forEach((row) => tx.objectStore(store).put(row));
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
        id,
        name,
        version: 1,
        fps: 10,
        canvasSize: { width: 320, height: 180 },
        scenes: [
          { id: "main", name: "Main", isMain: true, tracks: [], bookmarks: [] },
        ],
      };
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
                trimEnd: 28,
              },
            ],
          },
        ],
      };
      await write("video-editor-projects", "projects", [
        raw,
        { id: `damaged-${id}`, name: "Damaged", version: 1, scenes: [null] },
      ]);
      await write(
        `video-editor-media-${id}`,
        "media-metadata",
        fixtures.map(({ id, name, type, size, duration }) => ({
          id,
          name,
          type,
          size,
          ...(duration ? { duration } : {}),
        })),
      );
      await write(`video-editor-timelines-${id}`, "timeline", [timeline]);
      const root = await navigator.storage.getDirectory(),
        dir = await root.getDirectoryHandle(`media-files-${id}`, {
          create: true,
        });
      for (const item of fixtures) {
        const file = await dir.getFileHandle(item.id, { create: true }),
          writable = await file.createWritable();
        await writable.write(
          Uint8Array.from(atob(item.base64), (c) => c.charCodeAt(0)),
        );
        await writable.close();
      }
      return raw;
    },
    { fixtures, id, name },
  );
}
async function oldState(target, id) {
  return target.evaluate(async (id) => {
    const names = [
      "video-editor-projects",
      `video-editor-media-${id}`,
      `video-editor-timelines-${id}`,
    ];
    const databases = (await indexedDB.databases())
      .filter((db) => names.includes(db.name))
      .sort((a, b) => (a.name < b.name ? -1 : 1));
    const rows = [];
    for (const { name, version } of databases) {
      rows.push(
        await new Promise((resolve, reject) => {
          const r = indexedDB.open(name);
          r.onerror = () => reject(r.error);
          r.onsuccess = () => {
            const db = r.result,
              store = db.objectStoreNames[0],
              tx = db.transaction(store, "readonly"),
              all = tx.objectStore(store).getAll();
            all.onsuccess = () => {
              resolve({ name, version, store, data: all.result });
              db.close();
            };
          };
        }),
      );
    }
    const dir = await (
        await navigator.storage.getDirectory()
      ).getDirectoryHandle(`media-files-${id}`),
      hashes = {};
    for await (const [name, handle] of dir.entries()) {
      const digest = await crypto.subtle.digest(
        "SHA-256",
        await (await handle.getFile()).arrayBuffer(),
      );
      hashes[name] = [...new Uint8Array(digest)]
        .map((b) => b.toString(16).padStart(2, "0"))
        .join("");
    }
    return { rows, hashes };
  }, id);
}
const dialog = () =>
  page.getByRole("dialog", { name: "Recover Classic projects", exact: true });
async function recovery() {
  await page
    .getByRole("button", { name: "Recover Classic projects", exact: true })
    .click();
  await expect(
    dialog().getByLabel("Classic project on this site"),
  ).toBeVisible();
}
async function prepare() {
  await dialog()
    .getByRole("button", { name: /^(Prepare|Retry) recovery$/ })
    .click();
  await expect(
    dialog().getByRole("button", { name: "Recover project", exact: true }),
  ).toBeVisible();
}
async function consent() {
  const button = dialog().getByRole("button", {
    name: "Recover project",
    exact: true,
  });
  await expect(button).toBeDisabled();
  await dialog().getByRole("checkbox").check();
  await expect(button).toBeEnabled();
  return button;
}
async function verifyPixels(label) {
  const guide = page.getByRole("dialog", {
    name: "Welcome to OpenCut",
    exact: true,
  });
  if (await guide.isVisible())
    await guide.getByRole("button", { name: "Close", exact: true }).click();
  await page.getByRole("button", { name: "Render frame", exact: true }).click();
  const img = page.getByAltText("Rendered project frame");
  await expect(img).toBeVisible({ timeout: 60000 });
  const bytes = await img.evaluate(async (i) => [
    ...new Uint8Array(await (await fetch(i.src)).arrayBuffer()),
  ]);
  const file = path.join(directory, `${label}-frame.png`);
  await fs.writeFile(file, Buffer.from(bytes));
  const rgb = [
    ...ffmpeg(
      "-i",
      file,
      "-frames:v",
      "1",
      "-vf",
      "scale=1:1",
      "-pix_fmt",
      "rgb24",
      "-f",
      "rawvideo",
      "-",
    ),
  ];
  expect(rgb[0]).toBeGreaterThan(240);
  expect(rgb[1]).toBeLessThan(10);
  expect(rgb[2]).toBeLessThan(10);
  report[label + "_rgb"] = rgb;
}
async function verifyAudio(project) {
  const accepted = call("opencut_job_v3_render", {
    project_id: project.project.id,
    revision: project.revision,
    sequence_id: "main",
    container: "mp4",
    video_codec: "h264",
    idempotency_key: "verify-recovered-audio",
  });
  const deadline = Date.now() + 120000;
  let job;
  do {
    job = call("opencut_job_view", { job_id: accepted.job_id });
    if (["succeeded", "failed", "cancelled"].includes(job.state)) break;
    if (Date.now() >= deadline)
      throw new Error(
        `Render observation timed out; original ${accepted.job_id} remains inspectable`,
      );
    await new Promise((resolve) => setTimeout(resolve, 200));
  } while (true);
  expect(job.state).toBe("succeeded");
  const artifact = job.artifacts.find((a) => a.mime_type === "video/mp4");
  const response = await page.request.get(
    `${endpoint}/v1/artifacts/${encodeURIComponent(artifact.artifact_id)}`,
  );
  expect(response.ok()).toBe(true);
  const bytes = await response.body();
  expect(createHash("sha256").update(bytes).digest("hex")).toBe(
    artifact.content_hash,
  );
  const output = path.join(directory, "recovered-export.mp4");
  await fs.writeFile(output, bytes);
  const pcm = ffmpeg(
    "-i",
    output,
    "-vn",
    "-ac",
    "2",
    "-ar",
    "48000",
    "-f",
    "s16le",
    "-",
  );
  const samples = [];
  for (let frame = 24000; frame < 72000; frame++)
    samples.push(pcm.readInt16LE(frame * 4));
  const rms = Math.sqrt(
    samples.reduce((s, n) => s + n * n, 0) / samples.length,
  );
  expect(Math.abs(rms - 4095 / 2)).toBeLessThan(100);
  const power = Object.fromEntries(
    [330, 660, 800].map((hz) => {
      let re = 0,
        im = 0;
      samples.forEach((n, i) => {
        re += n * Math.cos((2 * Math.PI * hz * i) / 48000);
        im += n * Math.sin((2 * Math.PI * hz * i) / 48000);
      });
      return [hz, re * re + im * im];
    }),
  );
  expect(power[800]).toBeGreaterThan(1000 * Math.max(power[330], power[660]));
  expect(pcm.length / 4 / 48000).toBeGreaterThan(1.95);
  expect(pcm.length / 4 / 48000).toBeLessThan(2.1);
  report.audio = {
    job_id: accepted.job_id,
    rms,
    expected_per_channel_rms: 4095 / 2,
    frequency_power: power,
    duration_seconds: pcm.length / 4 / 48000,
    artifact_sha256: artifact.content_hash,
  };
}
try {
  await page.goto(`${endpoint}/editor`);
  await expect(
    page.getByRole("button", { name: "New project", exact: true }),
  ).toBeEnabled();
  await seed(page, "old", "Recovered locally");
  const before = await oldState(page, "old");
  await recovery();
  await dialog().getByLabel("Classic project on this site").selectOption("old");
  await dialog()
    .getByLabel("Classic project on this site")
    .selectOption("damaged-old");
  await dialog()
    .getByRole("button", { name: "Prepare recovery", exact: true })
    .click();
  await expect(dialog().getByRole("alert")).toContainText(
    "Damaged Classic scene",
  );
  expect(
    call("opencut_project_library", { cursor: null, limit: 100 }).items,
  ).toHaveLength(0);
  expect(await oldState(page, "old")).toEqual(before);
  await dialog().getByLabel("Classic project on this site").selectOption("old");
  check(
    "F06: damaged source reports an error before native publication; selecting the intact source retries without modifying old stores",
  );
  // Hold the second audio chunk before admission. Pause must retain the first 4 MiB.
  let held, releaseHeld, pausedUpload;
  const heldPromise = new Promise((resolve) => {
    held = resolve;
  });
  const release = new Promise((resolve) => {
    releaseHeld = resolve;
  });
  await page.route(
    "**/v1/stream-uploads/*/chunks/4194304?*",
    async (route) => {
      pausedUpload = new URL(route.request().url()).pathname.split("/")[3];
      held();
      await release;
      await route.abort();
    },
    { times: 1 },
  );
  await dialog()
    .getByRole("button", { name: "Prepare recovery", exact: true })
    .click();
  await heldPromise;
  await dialog()
    .getByRole("button", { name: "Pause recovery", exact: true })
    .click();
  releaseHeld();
  const upload = call("opencut_upload_stream_inspect", {
    upload_id: pausedUpload,
  });
  expect(upload.next_offset).toBe("4194304");
  expect(upload.job_id).toBeNull();
  expect(
    call("opencut_project_library", { cursor: null, limit: 100 }).items.filter(
      (p) => !p.name.startsWith("Preparing Classic:"),
    ),
  ).toHaveLength(0);
  report.controlledFaults.push({
    kind: "paused before second chunk",
    upload_id: pausedUpload,
    next_offset: upload.next_offset,
  });
  await page.reload();
  await recovery();
  await dialog().getByLabel("Classic project on this site").selectOption("old");
  await prepare();
  const resumed = call("opencut_upload_stream_inspect", {
    upload_id: pausedUpload,
  });
  expect(resumed.next_offset).toBe(
    String(fixtures.find((f) => f.id === "ma").size),
  );
  expect(
    report.chunks.filter(
      (c) => c.path.includes(pausedUpload) && c.path.endsWith("/0"),
    ),
  ).toHaveLength(1);
  check(
    "F06: product discovery, original file progress, pause, reload and confirmed 4 MiB offset resume use the same durable native upload",
  );
  const publish = await consent();
  await page.screenshot({
    path: path.join(directory, "recovery-review-en-dark.png"),
    fullPage: true,
  });
  // Admit the atomic commit but drop its acknowledgement.
  await page.route(
    "**/v1/tools/opencut_classic_recovery_import_v1",
    async (route) => {
      const response = await route.fetch();
      const receipt = await response.json();
      expect(receipt.error).toBeNull();
      report.lostReceipt = receipt.data;
      report.controlledFaults.push({
        kind: "atomic publish acknowledgement dropped",
        project_id: receipt.data.target_project_id,
      });
      await route.abort();
    },
    { times: 1 },
  );
  await publish.click();
  await expect(dialog().getByRole("alert")).toBeVisible();
  await dialog()
    .getByRole("button", { name: "Retry recovery", exact: true })
    .click();
  await expect(
    dialog().getByRole("button", {
      name: "Open recovered project",
      exact: true,
    }),
  ).toBeVisible();
  const id = report.lostReceipt.target_project_id;
  const library = call("opencut_project_library", {
    cursor: null,
    limit: 100,
  }).items;
  expect(library).toHaveLength(1);
  expect(library[0].project_id).toBe(id);
  const promoted = call("opencut_project_v3_inspect", {
    project_id: id,
    revision: null,
  });
  const initial = { ...promoted, project: promoted.project.base };
  expect(library[0].format).toBe("visual");
  expect(initial.project.assets).toHaveLength(3);
  for (const item of fixtures)
    expect(
      initial.project.assets.find((a) => a.name === item.name).version
        .content_hash,
    ).toBe(item.sha256);
  report.recovered = {
    project_id: id,
    revision: initial.revision,
    assets: initial.project.assets,
  };
  check(
    "F06: explicit loss consent, lost atomic acknowledgement recovery and preparation cleanup publish one project with all three original library files",
  );
  await dialog()
    .getByRole("button", { name: "Open recovered project", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Clip red.mp4", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Clip tone.wav", exact: true }),
  ).toBeVisible();
  await verifyPixels("local");
  await verifyAudio(initial);
  check(
    "F06: recovered project opens in the actual editor and renders independently expected red pixels, two-second duration and unity-gain 800 Hz PCM",
  );
  await page
    .getByRole("button", {
      name: "Rename project Recovered locally",
      exact: true,
    })
    .click();
  const rename = page.getByRole("dialog", {
    name: "Rename project",
    exact: true,
  });
  await rename.getByLabel("Project name").fill("Recovered and edited");
  await rename.getByRole("button", { name: "Save", exact: true }).click();
  await expect(rename).not.toBeVisible();
  const edited = call("opencut_project_v3_inspect", {
    project_id: id,
    revision: null,
  });
  expect(edited.project.base.name).toBe("Recovered and edited");
  expect(edited.revision).not.toBe(initial.revision);
  check(
    "F06: recovery upgrades through the native fixed-revision promotion, and the real editor can rename the recovered project",
  );
  await page
    .getByRole("button", { name: "Back to projects", exact: true })
    .click();
  // After cleanup, resume must verify the immutable recovered revision itself.
  await recovery();
  await dialog().getByLabel("Classic project on this site").selectOption("old");
  await dialog()
    .getByRole("button", { name: "Prepare recovery", exact: true })
    .click();
  await expect(
    dialog().getByRole("button", {
      name: "Open recovered project",
      exact: true,
    }),
  ).toBeVisible();
  expect(
    call("opencut_project_v3_inspect", { project_id: id, revision: null })
      .revision,
  ).toBe(edited.revision);
  await dialog()
    .getByRole("button", { name: "Close", exact: true })
    .last()
    .click();
  expect(await oldState(page, "old")).toEqual(before);
  check(
    "F06: repeat after preparation deletion preserves the exact native project, old database versions/records and all OPFS bytes",
  );
  // Actual cross-origin export carries bytes. No native network calls on the old site.
  const old = await context.newPage();
  await old.goto(oldOrigin);
  await seed(old, "cross", "Recovered across origins");
  const oldBefore = await oldState(old, "cross");
  const oldRequests = [];
  old.on("request", (r) => oldRequests.push(r.url()));
  await recovery();
  const link = dialog().getByText("Export old Classic projects", {
    exact: true,
  });
  await expect(link).toHaveAttribute("href", /^javascript:/);
  const bookmarkUrl = await link.getAttribute("href");
  await old.evaluate((url) => {
    window.location.href = url;
  }, bookmarkUrl);
  await expect(old.getByLabel("Classic project", { exact: true })).toHaveValue(
    "cross",
  );
  const download = old.waitForEvent("download");
  await old
    .getByRole("button", {
      name: "Export recovery bundle / 导出恢复包",
      exact: true,
    })
    .click();
  const saved = path.join(directory, "cross.occlassic");
  await (await download).saveAs(saved);
  expect(oldRequests).toEqual([]);
  expect(await oldState(old, "cross")).toEqual(oldBefore);
  await dialog()
    .locator("summary")
    .filter({ hasText: "Projects saved on another site" })
    .click();
  await dialog()
    .getByLabel("Classic recovery bundle", { exact: true })
    .setInputFiles(saved);
  await expect(
    dialog().getByText(`Recovery bundle from ${oldOrigin}`, { exact: false }),
  ).toBeVisible();
  await dialog()
    .getByLabel("Classic recovery bundle", { exact: true })
    .setInputFiles({
      name: "json-only.occlassic",
      mimeType: "application/octet-stream",
      buffer: Buffer.from('{"version":31}'),
    });
  await expect(dialog().getByRole("alert")).toContainText(
    "standalone project JSON",
  );
  await expect(
    dialog().getByRole("button", { name: "Retry recovery", exact: true }),
  ).toBeDisabled();
  await dialog()
    .getByLabel("Classic recovery bundle", { exact: true })
    .setInputFiles(saved);
  await expect(
    dialog().getByText(`Recovery bundle from ${oldOrigin}`, { exact: false }),
  ).toBeVisible();
  check(
    "F06: replacing a valid recovery bundle with invalid JSON clears the prior selection and cannot publish its media accidentally",
  );
  await prepare();
  await (await consent()).click();
  await expect(
    dialog().getByRole("button", {
      name: "Open recovered project",
      exact: true,
    }),
  ).toBeVisible();
  await dialog()
    .getByRole("button", { name: "Open recovered project", exact: true })
    .click();
  await verifyPixels("cross");
  expect(
    call("opencut_project_library", { cursor: null, limit: 100 }).items,
  ).toHaveLength(2);
  expect(await oldState(old, "cross")).toEqual(oldBefore);
  check(
    "F06: self-contained exporter runs at a different origin without network requests, carries actual media, imports through the product and renders correct pixels",
  );
  await old.close();
  await page
    .getByRole("button", { name: "Back to projects", exact: true })
    .click();
  await page
    .getByRole("combobox", { name: "Language", exact: true })
    .selectOption("zh");
  await page.getByRole("button", { name: "浅色主题", exact: true }).click();
  await page
    .getByRole("button", { name: "恢复 Classic 旧工程", exact: true })
    .click();
  await page.screenshot({
    path: path.join(directory, "recovery-zh-light.png"),
    fullPage: true,
  });
  expect(report.consoleErrors).toEqual([]);
  check(
    "F06: discoverable recovery dialog works with Chinese/light theme and has no page exceptions",
  );
  report.status = "passed";
} catch (error) {
  report.status = "failed";
  report.error = String(error);
  report.stack = error.stack;
  await page
    .screenshot({
      path: path.join(directory, "classic-recovery-failure.png"),
      fullPage: true,
    })
    .catch(() => {});
  throw error;
} finally {
  await fs.writeFile(
    path.join(directory, "browser-report.json"),
    JSON.stringify(report, null, 2) + "\n",
  );
  await browser.close();
  await new Promise((resolve) => oldServer.close(resolve));
}
