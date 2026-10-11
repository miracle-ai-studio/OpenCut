import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";

export async function verifyMediaLibrary(page, inspect, directory, check) {
  await page.getByRole("button", { name: "Media", exact: true }).click();
  const media = page.getByRole("region", { name: "Media", exact: true });
  const before = await inspect();
  await page
    .getByRole("button", { name: "Media list view", exact: true })
    .click();
  await expect(page.locator(".asset-list")).toBeVisible();
  for (const key of ["type", "duration", "size", "name"]) {
    await page
      .getByRole("combobox", { name: "Sort media", exact: true })
      .selectOption(key);
    await expect(media.locator(".asset-card")).toHaveCount(
      before.project.base.assets.length,
    );
  }
  await page
    .getByRole("checkbox", { name: "Select media sample.mp4", exact: true })
    .check();
  await page
    .getByRole("checkbox", { name: "Select media sample.wav", exact: true })
    .click({ modifiers: ["Shift"] });
  await expect(page.locator(".media-selection-toolbar")).toContainText(
    "2 selected",
  );
  await page
    .getByRole("button", { name: "Remove selected media", exact: true })
    .click();
  let dialog = page.getByRole("dialog", { name: "Remove media", exact: true });
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  expect((await inspect()).project).toEqual(before.project);
  await page
    .getByRole("button", { name: "Remove selected media", exact: true })
    .click();
  dialog = page.getByRole("dialog", { name: "Remove media", exact: true });
  await dialog
    .getByRole("button", { name: "Remove media", exact: true })
    .click();
  await expect(dialog).toHaveCount(0);
  const removed = await inspect();
  expect(removed.project.base.assets).toHaveLength(0);
  expect(
    removed.project.base.sequences.flatMap((s) =>
      s.tracks.flatMap((t) => t.clips),
    ),
  ).toHaveLength(0);
  expect(
    removed.project.visual.flatMap((s) => [
      ...s.masks,
      ...s.effects,
      ...s.transitions,
    ]),
  ).toHaveLength(0);
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect
    .poll(async () => (await inspect()).project)
    .toEqual(before.project);
  await expect(
    page.getByRole("button", { name: "Preview sample.mp4", exact: true }),
  ).toBeVisible();
  await expect(page.locator(".preview-canvas > img")).toBeVisible({
    timeout: 90000,
  });
  await page.screenshot({
    path: path.join(directory, "media-library-list-en-light.png"),
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Media grid view", exact: true })
    .click();
  const bytes = Array.from(
    await fs.readFile(path.join(directory, "sample.mp4")),
  );
  await page.evaluate((bytes) => {
    const data = new DataTransfer();
    data.items.add(
      new File([new Uint8Array(bytes)], "pasted.mp4", { type: "video/mp4" }),
    );
    document
      .querySelector(".library-tab:not([hidden]) .asset-grid")
      .dispatchEvent(
        new ClipboardEvent("paste", {
          clipboardData: data,
          bubbles: true,
          cancelable: true,
        }),
      );
  }, bytes);
  await expect(
    page.getByRole("button", { name: "Preview pasted.mp4", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Remove pasted.mp4", exact: true })
    .click();
  dialog = page.getByRole("dialog", { name: "Remove media", exact: true });
  await dialog
    .getByRole("button", { name: "Remove media", exact: true })
    .click();
  await expect(dialog).toHaveCount(0);
  expect((await inspect()).project).toEqual(before.project);
  let releaseChunk;
  const chunkGate = new Promise((resolve) => (releaseChunk = resolve));
  let observedChunk;
  const chunkStarted = new Promise((resolve) => (observedChunk = resolve));
  await page.route("**/v1/stream-uploads/**/chunks/**", async (route) => {
    observedChunk();
    await chunkGate;
    await route.continue();
  });
  try {
    const chooserStarted = page.waitForEvent("filechooser");
    await page
      .getByRole("button", { name: "Replace sample.mp4", exact: true })
      .click();
    const chooser = await chooserStarted;
    expect(chooser.isMultiple()).toBe(false);
    await chooser.setFiles({
      name: "replacement.mp4",
      mimeType: "video/mp4",
      buffer: Buffer.from(bytes),
    });
    await Promise.race([
      chunkStarted,
      new Promise((_, reject) =>
        setTimeout(
          () =>
            reject(
              new Error("Replacement upload did not reach byte transport"),
            ),
          15000,
        ),
      ),
    ]);
    await page.evaluate((bytes) => {
      const data = new DataTransfer();
      data.items.add(
        new File([new Uint8Array(bytes)], "ignored-paste.mp4", {
          type: "video/mp4",
        }),
      );
      document
        .querySelector(".library-tab:not([hidden]) .asset-grid")
        .dispatchEvent(
          new ClipboardEvent("paste", {
            clipboardData: data,
            bubbles: true,
            cancelable: true,
          }),
        );
    }, bytes);
    releaseChunk();
    await expect(
      page.getByRole("button", {
        name: "Preview replacement.mp4",
        exact: true,
      }),
    ).toBeVisible({ timeout: 90000 });
    const replaced = await inspect();
    expect(replaced.project.base.assets).toHaveLength(
      before.project.base.assets.length,
    );
    const original = before.project.base.assets.find(
      (a) => a.name === "sample.mp4",
    );
    expect(
      replaced.project.base.assets.find((a) => a.name === "replacement.mp4").id,
    ).toBe(original.id);
    expect(replaced.project.base.sequences).toEqual(
      before.project.base.sequences,
    );
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(before.project);
    check(
      "replacement keeps its original media binding while a concurrent file paste is ignored, permits one replacement file and restores exact content with undo",
    );
  } finally {
    releaseChunk();
    await page.unroute("**/v1/stream-uploads/**/chunks/**");
  }
  check(
    "media grid/list and all sort choices, Shift batch selection, cancellable atomic media removal across scenes with effect/mask cleanup, exact undo, file clipboard import and single removal",
  );
}
