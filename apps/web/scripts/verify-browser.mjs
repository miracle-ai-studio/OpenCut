import { verifyViewport } from "./verify-viewport.mjs";
import { verifyGuide, verifyPanelLayouts } from "./verify-presentation.mjs";
import { verifySite } from "./verify-site.mjs";
import { verifyLibrary } from "./verify-library.mjs";
import { verifyMediaLibrary } from "./verify-media-library.mjs";
import { verifyTimelineView } from "./verify-timeline-view.mjs";
import { verifyScenes, verifyHistoryNavigation } from "./verify-scenes.mjs";
import { chromium, expect } from "@playwright/test";
import fs from "node:fs/promises";
import { execFileSync } from "node:child_process";
import path from "node:path";
const root = path.resolve(import.meta.dirname, "../../..");
const directory =
  process.env.OPENCUT_WEB_WORKSPACE ?? path.join(root, "tmp/web-acceptance");
const endpoint = process.env.OPENCUT_WEB_URL ?? "http://127.0.0.1:8792";
const executablePath = process.env.OPENCUT_CHROME;
const browser = await chromium.launch({ headless: true, executablePath });
const page = await browser.newPage({
  locale: "en-US",
  viewport: { width: 1440, height: 1000 },
});
page.setDefaultTimeout(120000);
const projectName = `Browser acceptance ${Date.now()}`;
const errors = [],
  requests = [];
page.on("pageerror", (error) => errors.push(error.message));
page.on("response", (response) => {
  if (response.status() >= 400)
    requests.push({ url: response.url(), status: response.status() });
});
const report = {
  browser: await browser.version(),
  endpoint,
  checks: [],
  consoleErrors: errors,
  failedRequests: requests,
};
const check = (name) => {
  report.checks.push(name);
  console.log(name);
};
try {
  await page.goto(`${endpoint}/editor`);
  await expect(
    page.getByRole("button", { name: "New project", exact: true }),
  ).toBeEnabled();
  check("built Web page connects to detached authenticated service");
  await page.getByRole("button", { name: "New project", exact: true }).click();
  await page.getByLabel("Project name").fill(projectName);
  await page
    .getByRole("button", { name: "Create project", exact: true })
    .click();
  await expect(
    page.getByRole("region", { name: "Timeline", exact: true }),
  ).toBeVisible();
  check("create project opens full editor");
  await verifyGuide(page, check);
  await page
    .getByLabel("Media files", { exact: true })
    .first()
    .setInputFiles(path.join(directory, "sample.mp4"));
  await expect(
    page.getByRole("button", {
      name: "Add sample.mp4 to timeline",
      exact: true,
    }),
  ).toBeVisible({ timeout: 60000 });
  await page
    .getByRole("button", { name: "Add sample.mp4 to timeline", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Clip sample.mp4", exact: true }),
  ).toBeVisible();
  check(
    "browser chunks and hashes media, import job finishes, fixed asset binds and inserts",
  );
  await page.getByRole("button", { name: "Render frame", exact: true }).click();
  await expect(page.getByAltText("Rendered project frame")).toBeVisible({
    timeout: 60000,
  });
  check("composited frame artifact displays in browser");
  await page.getByRole("button", { name: "Play preview", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Pause preview", exact: true }),
  ).toBeVisible({ timeout: 60000 });
  await expect
    .poll(
      () =>
        page
          .locator(".preview-stage video")
          .evaluate((video) => video.currentTime),
      { timeout: 30000 },
    )
    .toBeGreaterThan(0.5);
  await page
    .getByRole("button", { name: "Pause preview", exact: true })
    .click();
  check("continuous MSE video decodes and media clock advances");
  await page
    .getByRole("button", { name: "Clip sample.mp4", exact: true })
    .click();
  await page.getByRole("button", { name: "Split clip", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Clip sample.mp4", exact: true }),
  ).toHaveCount(2);
  check("split at playhead commits two clips");
  await page.getByRole("button", { name: "History", exact: true }).click();
  await page
    .getByRole("button", { name: "Undo latest action", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Clip sample.mp4", exact: true }),
  ).toHaveCount(1);
  await page.getByRole("button", { name: "Redo action", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Clip sample.mp4", exact: true }),
  ).toHaveCount(2);
  check("server selective undo and redo preserve edit history");
  await page.getByRole("button", { name: "Captions", exact: true }).click();
  await page.getByLabel("Subtitle file", { exact: true }).setInputFiles({
    name: "captions.srt",
    mimeType: "text/plain",
    buffer: Buffer.from(
      "1\n00:00:00,000 --> 00:00:01,000\nSynthetic caption\n",
    ),
  });
  await expect(
    page.getByRole("button", { name: "Synthetic caption", exact: true }),
  ).toBeVisible({ timeout: 30000 });
  check("subtitle import uses shared service and appears on the timeline");
  await page.getByRole("button", { name: "Export", exact: true }).click();
  const dialog = page.getByRole("dialog", {
    name: "Export project",
    exact: true,
  });
  await dialog
    .getByRole("button", { name: "Check project", exact: true })
    .click();
  await expect(
    dialog.getByRole("button", { name: "Export", exact: true }),
  ).toBeEnabled({ timeout: 30000 });
  await dialog.getByRole("button", { name: "Export", exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await page.reload();
  await expect(
    page.getByRole("region", { name: "Timeline", exact: true }),
  ).toBeVisible();
  check("browser reload retains project and background export");
  await page
    .getByRole("button", { name: "Tasks", exact: true })
    .first()
    .click();
  await expect(
    page
      .locator(".task-card")
      .filter({ hasText: "Export" })
      .getByRole("button", { name: "Download", exact: true }),
  ).toBeVisible({ timeout: 120000 });
  check(
    "render preflight and durable export complete with downloadable artifact",
  );
  await page.evaluate(() => {
    window.showSaveFilePicker = async () => ({
      createWritable: async () => {
        const chunks = [];
        return new WritableStream({
          write(chunk) {
            chunks.push(chunk);
          },
          close() {
            window.savedExportBlob = new Blob(chunks);
          },
        });
      },
    });
  });
  await page
    .locator(".task-card")
    .filter({ hasText: "Export" })
    .getByRole("button", { name: "Download", exact: true })
    .click();
  await expect
    .poll(() => page.evaluate(() => window.savedExportBlob?.size ?? 0))
    .toBeGreaterThan(0);
  const bytes = await page.evaluate(async () =>
    Array.from(new Uint8Array(await window.savedExportBlob.arrayBuffer())),
  );
  const exported = path.join(directory, "browser-export.mp4");
  await fs.writeFile(exported, Buffer.from(bytes));
  const probe = JSON.parse(
    execFileSync(
      "ffprobe",
      ["-v", "error", "-show_streams", "-show_format", "-of", "json", exported],
      { encoding: "utf8" },
    ),
  );
  if (
    !probe.streams.some((stream) => stream.codec_type === "audio") ||
    !probe.streams.some((stream) => stream.codec_type === "video") ||
    Math.abs(Number(probe.format.duration) - 4) > 0.2
  )
    throw new Error("Downloaded export failed independent media inspection");
  check(
    "streamed browser download independently contains four seconds of video and audio",
  );
  const pcm = execFileSync("ffmpeg", [
    "-v",
    "error",
    "-ss",
    "1",
    "-i",
    exported,
    "-t",
    "1",
    "-ac",
    "1",
    "-ar",
    "48000",
    "-f",
    "f32le",
    "-",
  ]);
  let energy = 0,
    crossings = 0,
    previous = 0;
  for (let offset = 0; offset < pcm.length; offset += 4) {
    const value = pcm.readFloatLE(offset);
    energy += value * value;
    if (previous < 0 && value >= 0) crossings++;
    previous = value;
  }
  const samples = pcm.length / 4,
    rms = Math.sqrt(energy / samples),
    frequency = crossings / (samples / 48000);
  if (rms < 0.01 || Math.abs(frequency - 440) > 15)
    throw new Error(
      "Decoded export audio differs from the synthetic 440 Hz reference",
    );
  report.decodedAudio = { samples, rms, frequency };
  check("independently decoded PCM retains the synthetic reference tone");

  await page.screenshot({
    path: path.join(directory, "editor.png"),
    fullPage: true,
  });
  await page.getByRole("button", { name: "Media", exact: true }).click();
  await page
    .getByRole("button", { name: "Preview sample.mp4", exact: true })
    .click();
  await expect(
    page.getByRole("dialog", { name: "sample.mp4" }).locator("video"),
  ).toBeVisible();
  check("authorized original source preview opens");
  await page
    .getByRole("dialog", { name: "sample.mp4" })
    .getByRole("button", { name: "Close", exact: true })
    .click();
  await page.getByRole("button", { name: "Media", exact: true }).click();
  await expect(page.locator(".asset-poster .media-art").first()).toBeVisible();
  check("authorized source thumbnail appears in the media library");
  const inspect = () =>
    page.evaluate(async () => {
      const project_id = location.pathname.split("/")[2];
      const response = await fetch("/v1/tools/opencut_project_v3_inspect", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ project_id, revision: null }),
      });
      const envelope = await response.json();
      if (envelope.error) throw new Error(envelope.error.message);
      return envelope.data;
    });
  await page
    .getByRole("button", { name: "Back to beginning", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Clip sample.mp4", exact: true })
    .first()
    .click();
  await page.keyboard.press("ControlOrMeta+d");
  await expect(
    page.getByRole("button", { name: "Clip sample.mp4", exact: true }),
  ).toHaveCount(3);
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Clip sample.mp4", exact: true }),
  ).toHaveCount(2);
  check(
    "duplicate shortcut and timeline undo commit through the shared application",
  );
  await page
    .getByRole("button", { name: "Clip sample.mp4", exact: true })
    .first()
    .click();
  const properties = page.getByRole("region", {
    name: "Properties",
    exact: true,
  });
  await properties
    .getByRole("button", { name: "Animation", exact: true })
    .click();
  await properties
    .getByRole("button", { name: "Add keyframe", exact: true })
    .click();
  await properties.getByLabel("Clip time", { exact: true }).fill("0.1");
  await properties.getByLabel("Value", { exact: true }).fill("0.7");
  await properties
    .getByRole("button", { name: "Add keyframe", exact: true })
    .click();
  await properties
    .getByRole("button", { name: "Custom curve", exact: true })
    .click();
  await properties.getByLabel("X1", { exact: true }).fill("0.25");
  await properties
    .getByRole("button", { name: "Apply curve", exact: true })
    .click();
  await expect
    .poll(async () => (await inspect()).project.keyframe_curves[0]?.curve.x1, {
      timeout: 30000,
    })
    .toBe(0.25);
  check(
    "Classic curve editor writes the chosen easing to the fixed project snapshot",
  );
  const curveAnchor = (await inspect()).project.keyframe_curves[0].keyframe_id;
  await page.getByRole("button", { name: "Properties", exact: true }).click();
  await properties.getByLabel("Opacity", { exact: true }).fill("0.8");
  await properties.getByLabel("Opacity", { exact: true }).press("Tab");
  await expect
    .poll(
      async () =>
        (await inspect()).project.base.sequences[0].tracks[0].clips[0].params
          .opacity,
    )
    .toBe(0.8);
  await page
    .getByRole("button", { name: "Add opacity keyframe", exact: true })
    .click();
  await expect(page.getByText("Saving…", { exact: true })).toHaveCount(0);
  const afterKeyframe = await inspect();
  if (afterKeyframe.project.keyframe_curves[0]?.keyframe_id !== curveAnchor)
    throw new Error("Property keyframe replaced a curve anchor");
  await expect(page.getByRole("alert")).toHaveCount(0);
  check("property keyframe update preserves the existing curve anchor");
  await page.getByRole("button", { name: "Stickers", exact: true }).click();
  await page.getByRole("button", { name: "Rectangle", exact: true }).click();
  await expect(page.locator(".transform-box")).toBeVisible({ timeout: 120000 });
  const box = await page.locator(".transform-box").boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(
    box.x + box.width / 2 + 24,
    box.y + box.height / 2 + 12,
    { steps: 4 },
  );
  await page.mouse.up();
  await expect
    .poll(
      async () => (await inspect()).project.visual[0]?.shapes[0]?.position_x,
    )
    .toBeGreaterThan(0);
  check("canvas drag commits shape position in canvas coordinates");
  await page.getByRole("button", { name: "Sounds", exact: true }).click();
  await page
    .getByRole("region", { name: "Sounds", exact: true })
    .getByLabel("Media files", { exact: true })
    .setInputFiles(path.join(directory, "sample.wav"));
  await expect(
    page
      .getByRole("region", { name: "Sounds", exact: true })
      .locator(".media-waveform"),
  ).toBeVisible({ timeout: 120000 });
  check("imported audio displays waveform measured by the native analysis job");
  await verifyScenes(page, inspect, directory, check);
  await verifyHistoryNavigation(page, inspect, check);
  await page.getByRole("button", { name: "Media", exact: true }).click();
  await expect(page.locator(".preview-canvas > img")).toBeVisible({
    timeout: 180000,
  });
  await expect(page.locator(".preview-loading")).toHaveCount(0);
  await page
    .getByRole("combobox", { name: "Language", exact: true })
    .selectOption("zh");
  await expect(
    page.getByRole("button", { name: "导出", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("region", { name: "时间线", exact: true }),
  ).toBeVisible();
  await page.screenshot({
    path: path.join(directory, "editor-zh-dark.png"),
    fullPage: true,
  });
  await page.getByRole("button", { name: "浅色主题", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page.screenshot({
    path: path.join(directory, "editor-zh-light.png"),
    fullPage: true,
  });
  await page
    .getByRole("combobox", { name: "语言", exact: true })
    .selectOption("en");
  await expect(
    page.getByRole("button", { name: "Export", exact: true }),
  ).toBeVisible();
  await page.screenshot({
    path: path.join(directory, "editor-en-light.png"),
    fullPage: true,
  });
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await expect(
    page.getByRole("combobox", { name: "Language", exact: true }),
  ).toHaveValue("en");
  await expect(
    page.getByRole("region", { name: "Timeline", exact: true }),
  ).toBeVisible();
  await expect(page.getByText("Access token", { exact: true })).toHaveCount(0);
  await expect(page.getByRole("alert")).toHaveCount(0);
  check(
    "Chinese and English, light and dark themes switch live and survive reload without credential prompts",
  );
  await verifyViewport(page, inspect, directory, check);
  await verifyMediaLibrary(page, inspect, directory, check);
  await verifyTimelineView(page, inspect, check);
  await verifyPanelLayouts(page, inspect, directory, check);
  await verifyLibrary(page, inspect, directory, check);
  await verifySite(browser, endpoint, directory, check);
  if (errors.length || requests.length)
    throw new Error(JSON.stringify({ errors, requests }));
  report.status = "passed";
} catch (error) {
  report.status = "failed";
  report.error = String(error);
  await page.screenshot({
    path: path.join(directory, "failure.png"),
    fullPage: true,
  });
  console.error(error);
  process.exitCode = 1;
} finally {
  await fs.writeFile(
    path.join(directory, "browser-report.json"),
    JSON.stringify(report, null, 2),
  );
  await browser.close();
}
