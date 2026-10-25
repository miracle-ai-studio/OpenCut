import { chromium, expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { execFileSync } from "node:child_process";
const directory = process.env.OPENCUT_WEB_WORKSPACE,
  endpoint = process.env.OPENCUT_WEB_URL,
  revision = process.env.OPENCUT_BASE_REVISION;
const browser = await chromium.launch({
    headless: true,
    executablePath: process.env.OPENCUT_CHROME,
  }),
  page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1000 },
  });
const calls = [],
  errors = [];
page.on("pageerror", (e) => errors.push(e.message));
page.on("request", (r) => {
  if (r.url().endsWith("/api/tools/opencut_job_submit"))
    calls.push(r.postDataJSON());
});
try {
  await page.goto(`${endpoint}/editor`);
  const image = page.locator(
    `.project-cover img[data-cover-revision="${revision}"]`,
  );
  await expect(image).toBeVisible({ timeout: 180000 });
  await expect
    .poll(() => image.evaluate((i) => i.complete && i.naturalWidth > 0))
    .toBe(true);
  const data = await image.evaluate(async (i) =>
      Array.from(new Uint8Array(await (await fetch(i.src)).arrayBuffer())),
    ),
    file = path.join(directory, "base-cover.png");
  await fs.writeFile(file, Buffer.from(data));
  const rgb = [
    ...execFileSync("ffmpeg", [
      "-v",
      "error",
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
    ]),
  ];
  expect(rgb[1]).toBeGreaterThan(180);
  expect(rgb[0]).toBeLessThan(40);
  expect(rgb[2]).toBeLessThan(40);
  expect(calls).toHaveLength(1);
  expect(calls[0]).toMatchObject({
    kind: "preview.composited_frame",
    input: {
      project_id: "legacy-base-cover",
      revision,
      sequence_id: "main",
      requested_time: { ticks: "0", timebase: 1 },
    },
  });
  await page
    .getByRole("combobox", { name: "Language", exact: true })
    .selectOption("zh");
  await expect(image).toHaveAttribute("alt", "Legacy base cover 的封面");
  await page.locator(".project-actions summary").click();
  await expect(
    page.getByRole("button", { name: "刷新封面", exact: true }),
  ).toBeVisible();
  await page.locator(".project-actions summary").click();
  await page.getByRole("button", { name: "浅色主题", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await expect(image).toBeVisible();
  await page.screenshot({
    path: path.join(directory, "base-cover-zh-light.png"),
    fullPage: true,
  });
  expect(errors).toEqual([]);
  await fs.writeFile(
    path.join(directory, "base-cover-browser-report.json"),
    JSON.stringify(
      {
        status: "passed",
        checks: [
          "F05: legacy base project uses the native composited-frame job at its exact revision; independent green pixels survive removed source files",
          "F05: Chinese cover label and discoverable refresh action, light theme retain the thumbnail without new content or frame jobs",
        ],
        calls,
        rgb,
        consoleErrors: errors,
      },
      null,
      2,
    ),
  );
} catch (error) {
  await page.screenshot({
    path: path.join(directory, "base-cover-failure.png"),
    fullPage: true,
  });
  throw error;
} finally {
  await browser.close();
}
