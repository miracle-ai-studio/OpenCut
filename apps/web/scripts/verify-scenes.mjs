import { verifySnapshots } from "./verify-snapshots.mjs";
import { expect } from "@playwright/test";
import path from "node:path";

export async function verifyScenes(page, inspect, directory, check) {
  await page
    .getByRole("button", { name: "Back to projects", exact: true })
    .click();
  await page.locator(".project-actions summary").click();
  await page.locator(".project-rename").click();
  const rename = page.getByRole("dialog", {
    name: "Rename project",
    exact: true,
  });
  await rename
    .getByLabel("Project name", { exact: true })
    .fill("Scene migration acceptance");
  await rename.getByRole("button", { name: "Save", exact: true }).click();
  await expect(rename).toHaveCount(0);
  await expect(page.locator(".project-card strong")).toHaveText(
    "Scene migration acceptance",
  );
  await page.locator(".project-card").click();
  await expect(page.locator(".project-name")).toHaveText(
    "Scene migration acceptance",
  );
  check(
    "project library rename persists through the shared revision and reopens",
  );
  const main = await page
    .getByLabel("Active sequence", { exact: true })
    .inputValue();
  const manager = page.getByRole("dialog", { name: "Scenes", exact: true });
  await page
    .getByRole("button", { name: "Manage scenes", exact: true })
    .click();
  await expect(
    manager.locator('input[type="checkbox"]').first(),
  ).toBeDisabled();
  const create = async (name) => {
    await manager
      .getByRole("button", { name: "New scene", exact: true })
      .click();
    await manager.getByLabel("Scene name", { exact: true }).fill(name);
    await manager
      .getByRole("button", { name: "Create scene", exact: true })
      .click();
    await expect(manager.getByLabel("Scene name", { exact: true })).toHaveCount(
      0,
    );
  };
  await create("B roll");
  const second = await page
    .getByLabel("Active sequence", { exact: true })
    .inputValue();
  expect(second).not.toBe(main);
  await manager
    .getByRole("button", { name: "Rename scene B roll", exact: true })
    .click();
  await manager.getByLabel("Scene name", { exact: true }).fill("Cutaways");
  await manager.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    manager.getByRole("button", { name: "Rename scene Cutaways", exact: true }),
  ).toBeVisible();
  await manager.getByRole("button", { name: "Close", exact: true }).click();
  await page.getByRole("button", { name: "Stickers", exact: true }).click();
  await page.getByRole("button", { name: "Rectangle", exact: true }).click();
  await expect
    .poll(
      async () =>
        (await inspect()).project.visual.find((s) => s.sequence_id === second)
          ?.shapes.length,
    )
    .toBe(1);
  await page.reload();
  await expect(page.getByLabel("Active sequence", { exact: true })).toHaveValue(
    second,
  );
  await expect(page.locator(".project-name")).toHaveText(
    "Scene migration acceptance",
  );
  await page.getByRole("button", { name: "Render frame", exact: true }).click();
  await expect
    .poll(
      async () =>
        page.locator(".preview-canvas > img").evaluateAll((images) => {
          const image = images[0];
          if (!image?.complete || !image.naturalWidth) return false;
          const canvas = document.createElement("canvas");
          canvas.width = image.naturalWidth;
          canvas.height = image.naturalHeight;
          const ctx = canvas.getContext("2d");
          ctx.drawImage(image, 0, 0);
          const center = [
            ...ctx.getImageData(
              Math.floor(canvas.width / 2),
              Math.floor(canvas.height / 2),
              1,
              1,
            ).data,
          ];
          const corner = [...ctx.getImageData(4, 4, 1, 1).data];
          return (
            center.slice(0, 3).every((c) => c >= 248) &&
            corner.slice(0, 3).every((c) => c <= 6)
          );
        }),
      { timeout: 120000 },
    )
    .toBe(true);
  check(
    "non-main scene preview independently renders a white center and black corner after reload",
  );
  await verifySnapshots(page, inspect, directory, check);
  await page
    .getByRole("button", { name: "Manage scenes", exact: true })
    .click();
  await create("Closing");
  await expect
    .poll(() => manager.evaluate((el) => getComputedStyle(el).opacity))
    .toBe("1");
  await page.screenshot({
    path: path.join(directory, "scenes-dark.png"),
    fullPage: true,
  });
  const before = (await inspect()).project;
  await manager
    .getByRole("checkbox", { name: "Select scene Cutaways", exact: true })
    .check();
  await manager
    .getByRole("checkbox", { name: "Select scene Closing", exact: true })
    .check();
  await manager
    .getByRole("button", { name: "Delete selected (2)", exact: true })
    .click();
  await manager
    .getByRole("button", { name: "Delete scenes", exact: true })
    .click();
  await expect(page.getByLabel("Active sequence", { exact: true })).toHaveValue(
    main,
  );
  await expect(manager.locator(".scene-row")).toHaveCount(1);
  await manager.getByRole("button", { name: "Close", exact: true }).click();
  await page.getByRole("button", { name: "History", exact: true }).click();
  await page
    .getByRole("button", { name: "Undo latest action", exact: true })
    .click();
  await expect.poll(async () => (await inspect()).project).toEqual(before);
  await page.getByRole("button", { name: "Redo action", exact: true }).click();
  await expect
    .poll(async () => (await inspect()).project.base.sequences.length)
    .toBe(1);
  check(
    "scenes create, rename, refresh selection, batch delete and exact content undo/redo preserve the main scene",
  );
  await page
    .getByRole("button", { name: "Manage scenes", exact: true })
    .click();
  await expect(
    manager.locator('input[type="checkbox"]').first(),
  ).toBeDisabled();
  await manager.getByRole("button", { name: "Close", exact: true }).click();
  await page.getByRole("button", { name: "Light theme", exact: true }).click();
  await page
    .getByRole("combobox", { name: "Language", exact: true })
    .selectOption("zh");
  await page.getByRole("button", { name: "管理场景", exact: true }).click();
  const chinese = page.getByRole("dialog", { name: "场景", exact: true });
  await expect(
    chinese.getByRole("button", { name: "新建场景", exact: true }),
  ).toBeVisible();
  await expect(chinese.getByText("主场景", { exact: true })).toBeVisible();
  await expect
    .poll(() => chinese.evaluate((el) => getComputedStyle(el).opacity))
    .toBe("1");
  await page.screenshot({
    path: path.join(directory, "scenes-zh-light.png"),
    fullPage: true,
  });
  await chinese.getByRole("button", { name: "关闭", exact: true }).click();
  await page
    .getByRole("combobox", { name: "语言", exact: true })
    .selectOption("en");
  await page.getByRole("button", { name: "Dark theme", exact: true }).click();
  check("scene management uses Chinese and English labels and both themes");
}

export async function verifyHistoryNavigation(page, inspect, check) {
  const initial = (await inspect()).project;
  const rename = async (name) => {
    await page.locator(".project-name").click();
    const dialog = page.getByRole("dialog", {
      name: "Rename project",
      exact: true,
    });
    await dialog.getByLabel("Project name", { exact: true }).fill(name);
    await dialog.getByRole("button", { name: "Save", exact: true }).click();
    await expect(dialog).toHaveCount(0);
    await expect(page.locator(".project-name")).toHaveText(name);
  };
  const undo = async () => {
    await page.getByRole("button", { name: "Undo", exact: true }).click();
  };
  const redo = async () => {
    await page.getByRole("button", { name: "Redo", exact: true }).click();
  };
  await rename("History first edit");
  const first = (await inspect()).project;
  await rename("History second edit");
  const second = (await inspect()).project;
  await undo();
  await expect.poll(async () => (await inspect()).project).toEqual(first);
  await undo();
  await expect.poll(async () => (await inspect()).project).toEqual(initial);
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Redo", exact: true }),
  ).toBeEnabled();
  await redo();
  await expect.poll(async () => (await inspect()).project).toEqual(first);
  await redo();
  await expect.poll(async () => (await inspect()).project).toEqual(second);
  await expect(
    page.getByRole("button", { name: "Redo", exact: true }),
  ).toBeDisabled();
  await undo();
  await expect.poll(async () => (await inspect()).project).toEqual(first);
  await rename("New edit after undo");
  await expect(
    page.getByRole("button", { name: "Redo", exact: true }),
  ).toBeDisabled();
  await undo();
  await expect.poll(async () => (await inspect()).project).toEqual(first);
  await undo();
  await expect.poll(async () => (await inspect()).project).toEqual(initial);
  await page
    .getByRole("button", { name: "Back to projects", exact: true })
    .click();
  await expect(page.locator(".project-card strong")).toHaveText(
    initial.base.name,
  );
  await page.locator(".project-card").click();
  check(
    "consecutive undo/redo restores exact documents after reload, undo works again after redo, and new edits invalidate redo",
  );
}
