import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { execFileSync } from "node:child_process";

export async function verifyLibrary(page, inspect, directory, check) {
  const source = await inspect(),
    name = source.project.base.name;
  const card = (name) =>
    page
      .locator(".project-card-wrap")
      .filter({ has: page.getByText(name, { exact: true }) });
  await page
    .getByRole("button", { name: "Back to projects", exact: true })
    .click();
  await page.locator(".project-actions summary").click();
  await page
    .getByRole("button", { name: "Project information", exact: true })
    .click();
  const info = page.getByRole("dialog", {
    name: "Project information",
    exact: true,
  });
  await expect(info.getByText("1920 × 1080", { exact: true })).toBeVisible();
  await expect(info.getByText("Created", { exact: true })).toBeVisible();
  await expect(info.getByText("Modified", { exact: true })).toBeVisible();
  await info.getByRole("button", { name: "Done", exact: true }).click();
  await page.locator(".project-actions summary").click();
  await page
    .getByRole("button", { name: "Duplicate project", exact: true })
    .click();
  await expect(page.locator(".project-card")).toHaveCount(2);
  const copyName = `(1) ${name}`;
  await card(copyName).locator(".project-card").click();
  await expect(page.locator(".project-name")).toHaveText(copyName);
  const copy = await inspect();
  const expected = structuredClone(source.project);
  expected.base.id = copy.project.base.id;
  expected.base.name = copyName;
  expect(copy.project.base.id).not.toBe(source.project.base.id);
  expect(copy.project).toEqual(expected);
  await page.getByRole("button", { name: "Media", exact: true }).click();
  await page
    .getByRole("button", { name: "Preview sample.mp4", exact: true })
    .click();
  const preview = page.getByRole("dialog", { name: "sample.mp4", exact: true });
  await expect
    .poll(() => preview.locator("video").evaluate((video) => video.readyState))
    .toBeGreaterThan(0);
  await preview.getByRole("button", { name: "Close", exact: true }).click();
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
  ).toBeEnabled();
  await dialog.getByRole("button", { name: "Export", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await page
    .getByRole("button", { name: "Tasks", exact: true })
    .first()
    .click();
  const download = page
    .locator(".task-card")
    .filter({ hasText: "Export" })
    .getByRole("button", { name: "Download", exact: true });
  await expect(download).toBeVisible({ timeout: 120000 });
  await page.evaluate(() => {
    window.savedExportBlob = null;
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
  await download.click();
  await expect
    .poll(() => page.evaluate(() => window.savedExportBlob?.size ?? 0))
    .toBeGreaterThan(0);
  const bytes = await page.evaluate(async () =>
    Array.from(new Uint8Array(await window.savedExportBlob.arrayBuffer())),
  );
  const exported = path.join(directory, "copied-project-export.mp4");
  await fs.writeFile(exported, Buffer.from(bytes));
  const probe = JSON.parse(
    execFileSync(
      "ffprobe",
      ["-v", "error", "-show_streams", "-show_format", "-of", "json", exported],
      { encoding: "utf8" },
    ),
  );
  expect(probe.streams.some((s) => s.codec_type === "video")).toBe(true);
  expect(probe.streams.some((s) => s.codec_type === "audio")).toBe(true);
  expect(Math.abs(Number(probe.format.duration) - 4)).toBeLessThan(0.2);
  const pcm = execFileSync("ffmpeg", [
    "-v",
    "error",
    "-i",
    exported,
    "-t",
    "1",
    "-vn",
    "-ac",
    "1",
    "-ar",
    "48000",
    "-f",
    "f32le",
    "pipe:1",
  ]);
  let sum = 0,
    crossings = 0,
    previous = 0;
  for (let index = 0; index < pcm.length; index += 4) {
    const value = pcm.readFloatLE(index);
    sum += value * value;
    if (previous <= 0 && value > 0) crossings++;
    previous = value;
  }
  expect(Math.sqrt(sum / (pcm.length / 4))).toBeGreaterThan(0.04);
  expect(crossings).toBeGreaterThan(430);
  expect(crossings).toBeLessThan(450);
  check(
    "project duplicate preserves the full document, independently previews shared media and exports four seconds of video with the reference audio tone",
  );
  await page
    .getByRole("button", { name: "Back to projects", exact: true })
    .click();
  await page
    .getByRole("checkbox", { name: "Select all projects", exact: true })
    .check();
  await page
    .getByRole("button", { name: "Duplicate selected", exact: true })
    .click();
  await expect(page.locator(".project-card")).toHaveCount(4);
  for (const expected of [name, copyName, `(2) ${name}`, `(3) ${name}`])
    await expect(page.getByText(expected, { exact: true })).toBeVisible();
  await page
    .getByRole("combobox", { name: "Sort projects", exact: true })
    .selectOption("name");
  await page.getByRole("button", { name: "Descending", exact: true }).click();
  expect(
    await page.locator(".project-card > strong").allTextContents(),
  ).toEqual([copyName, `(2) ${name}`, `(3) ${name}`, name]);
  await page.getByRole("button", { name: "List view", exact: true }).click();
  await expect(page.locator(".project-list")).toBeVisible();
  await page.getByLabel("Search projects", { exact: true }).fill("(3)");
  await expect(page.locator(".project-card")).toHaveCount(1);
  await page.getByLabel("Search projects", { exact: true }).fill("");
  await page.screenshot({
    path: path.join(directory, "project-library-en-light.png"),
    fullPage: true,
  });
  await page.locator(".project-actions summary").first().click();
  const menu = page.locator(".project-action-menu:visible");
  const rename = menu.getByRole("button", { name: /Rename project/ });
  await expect(rename).toBeVisible();
  const menuBox = await menu.boundingBox(),
    renameBox = await rename.boundingBox();
  expect(renameBox.y).toBeGreaterThanOrEqual(menuBox.y);
  await page.screenshot({
    path: path.join(directory, "project-library-menu-en-light.png"),
    fullPage: true,
  });
  await page.locator(".project-actions summary").first().click();
  await page
    .getByRole("combobox", { name: "Language", exact: true })
    .selectOption("zh");
  await page.getByRole("button", { name: "深色主题", exact: true }).click();
  await page.locator(".project-actions summary").first().click();
  await page.getByRole("button", { name: "项目信息", exact: true }).click();
  const chinese = page.getByRole("dialog", { name: "项目信息", exact: true });
  await expect(chinese.getByText("创建时间", { exact: true })).toBeVisible();
  await expect
    .poll(() => chinese.evaluate((el) => getComputedStyle(el).opacity))
    .toBe("1");
  await page.screenshot({
    path: path.join(directory, "project-library-zh-dark.png"),
    fullPage: true,
  });
  await chinese.getByRole("button", { name: "完成", exact: true }).click();
  await page
    .getByRole("combobox", { name: "语言", exact: true })
    .selectOption("en");
  await page.getByRole("button", { name: "Light theme", exact: true }).click();
  await page.reload();
  await expect(page.locator(".project-card")).toHaveCount(4);
  await expect(page.locator(".project-list")).toBeVisible();
  await expect(
    page.getByRole("combobox", { name: "Sort projects", exact: true }),
  ).toHaveValue("name");
  await expect(
    page.getByRole("button", { name: "Ascending", exact: true }),
  ).toBeVisible();
  await card(copyName).getByRole("checkbox").check();
  await card(`(3) ${name}`)
    .getByRole("checkbox")
    .click({ modifiers: ["Shift"] });
  await expect(page.getByText("3 selected", { exact: true })).toBeVisible();
  await page
    .getByRole("button", { name: "Delete selected", exact: true })
    .click();
  let deletion = page.getByRole("dialog", {
    name: "Delete projects",
    exact: true,
  });
  await expect(
    deletion.getByRole("button", { name: "Delete projects", exact: true }),
  ).toBeDisabled();
  await deletion.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(page.locator(".project-card")).toHaveCount(4);
  await page
    .getByRole("button", { name: "Clear selection", exact: true })
    .click();
  await card(name).locator(".project-actions summary").click();
  await page
    .getByRole("button", { name: "Delete project", exact: true })
    .click();
  deletion = page.getByRole("dialog", { name: "Delete projects", exact: true });
  await deletion.getByRole("textbox").fill("DELETE");
  await deletion
    .getByRole("button", { name: "Delete projects", exact: true })
    .click();
  await expect(deletion).toHaveCount(0, { timeout: 45000 });
  await expect(page.locator(".project-card")).toHaveCount(3);
  await card(copyName).locator(".project-card").click();
  await page.getByRole("button", { name: "Media", exact: true }).click();
  await page
    .getByRole("button", { name: "Preview sample.mp4", exact: true })
    .click();
  const surviving = page.getByRole("dialog", {
    name: "sample.mp4",
    exact: true,
  });
  await expect
    .poll(() => surviving.locator("video").evaluate((v) => v.readyState))
    .toBeGreaterThan(0);
  await surviving.getByRole("button", { name: "Close", exact: true }).click();
  await page.getByRole("button", { name: "Render frame", exact: true }).click();
  await expect(page.getByAltText("Rendered project frame")).toBeVisible({
    timeout: 60000,
  });
  await page
    .getByRole("button", { name: "Back to projects", exact: true })
    .click();
  await page
    .getByRole("checkbox", { name: "Select all projects", exact: true })
    .check();
  await page
    .getByRole("button", { name: "Delete selected", exact: true })
    .click();
  deletion = page.getByRole("dialog", { name: "Delete projects", exact: true });
  await deletion.getByRole("textbox").fill("DELETE");
  await deletion
    .getByRole("button", { name: "Delete projects", exact: true })
    .click();
  await expect(page.locator(".project-card")).toHaveCount(0, {
    timeout: 45000,
  });
  await page.reload();
  await expect(
    page.getByText("Start with a blank canvas", { exact: true }),
  ).toBeVisible();
  check(
    "single and batch project deletion, guarded confirmation and cancellation, Shift selection, saved library preferences and surviving copy media/render after original deletion",
  );
  check(
    "project library batch duplicate, original numbered naming, search, sort, list layout, bilingual information and both themes persist correctly",
  );
}
