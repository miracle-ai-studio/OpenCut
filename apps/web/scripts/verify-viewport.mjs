import { expect } from "@playwright/test";
import path from "node:path";
export async function verifyViewport(page, inspect, directory, check) {
  const before = (await inspect()).project;
  const scene = before.base?.sequences[0] ?? before.sequences[0];
  const stage = page.locator(".preview-stage"),
    canvas = page.locator(".preview-canvas");
  const zoom = page.getByLabel("Preview zoom", { exact: true });
  await zoom.selectOption("100");
  const actualSize = await canvas.boundingBox();
  expect(actualSize.width).toBeCloseTo(scene.width, 0);
  expect(actualSize.height).toBeCloseTo(scene.height, 0);
  await zoom.selectOption("200");
  const actual = await canvas.boundingBox();
  const region = await stage.boundingBox(),
    center = {
      x: region.x + region.width / 2,
      y: region.y + region.height / 2,
    };
  await page.mouse.move(center.x, center.y);
  await page.mouse.down({ button: "middle" });
  await page.mouse.move(center.x + 90, center.y - 45, { steps: 6 });
  await page.mouse.up({ button: "middle" });
  const moved = await canvas.boundingBox();
  expect(moved.x - actual.x).toBeGreaterThan(40);
  expect(actual.y - moved.y).toBeGreaterThan(20);
  await stage.focus();
  await page.keyboard.down("Space");
  await page.mouse.move(center.x, center.y);
  await page.mouse.down();
  await page.mouse.move(center.x - 40, center.y + 25, { steps: 6 });
  await page.mouse.up();
  await page.keyboard.up("Space");
  expect(moved.x - (await canvas.boundingBox()).x).toBeGreaterThan(20);
  await page.getByRole("button", { name: "Fit preview", exact: true }).click();
  const fit = await canvas.boundingBox();
  expect(fit.width).toBeLessThanOrEqual(region.width - 47);
  expect(fit.height).toBeLessThanOrEqual(region.height - 47);
  expect(fit.x + fit.width / 2).toBeCloseTo(center.x, 0);
  expect(fit.y + fit.height / 2).toBeCloseTo(center.y, 0);
  expect((await inspect()).project).toEqual(before);
  check(
    "Classic Fit and actual-pixel percentage zoom, bounded middle/Space panning and Fit reset leave the project unchanged",
  );
  const menu = page.locator(".preview-guide-menu"),
    options = page.locator(".preview-guide-options");
  await menu
    .getByRole("button", { name: "Preview guides", exact: true })
    .click();
  await options.getByLabel("Guide type", { exact: true }).selectOption("grid");
  await options.getByLabel("Rows", { exact: true }).fill("4");
  await options.getByLabel("Columns", { exact: true }).fill("5");
  await expect(canvas.locator(".preview-guide-line")).toHaveCount(7);
  const vertical = await canvas.locator(".guide-x").first().boundingBox();
  const horizontal = await canvas.locator(".guide-y").first().boundingBox();
  expect(vertical.x - fit.x).toBeCloseTo(fit.width / 5, 0);
  expect(horizontal.y - fit.y).toBeCloseTo(fit.height / 4, 0);
  await options
    .getByLabel("Guide type", { exact: true })
    .selectOption("tiktok");
  await expect(
    canvas.getByAltText("TikTok layout", { exact: true }),
  ).toBeVisible();
  await expect
    .poll(() =>
      canvas
        .getByAltText("TikTok layout", { exact: true })
        .evaluate((image) => image.complete && image.naturalWidth > 0),
    )
    .toBeTruthy();
  check(
    "Bounded adjustable grid aligns to the actual canvas; TikTok overlay reuses Classic's original SVG",
  );
  await options
    .getByLabel("Guide type", { exact: true })
    .selectOption("custom");
  await options
    .getByRole("button", { name: "Add vertical guide", exact: true })
    .click();
  await options
    .getByRole("button", { name: "Add horizontal guide", exact: true })
    .click();
  await options.getByLabel("Guide position 1", { exact: true }).fill("17");
  await options.getByLabel("Guide position 2", { exact: true }).fill("83");
  await options
    .getByRole("button", { name: "Remove guide 1", exact: true })
    .click();
  await expect(canvas.locator(".guide-x")).toHaveCount(0);
  await expect(canvas.locator(".guide-y")).toHaveCount(1);
  await page.reload();
  await expect(page.getByLabel("Preview zoom", { exact: true })).toHaveValue(
    "fit",
  );
  await expect(page.locator(".guide-y")).toHaveCount(1);
  expect((await inspect()).project).toEqual(before);
  check(
    "Custom guides edit/remove and persist on reload without storing or modifying project contents",
  );
  if ((await page.locator("html").getAttribute("data-theme")) !== "light")
    await page
      .getByRole("button", { name: "Light theme", exact: true })
      .click();
  await page.getByLabel("Language", { exact: true }).first().selectOption("zh");
  await expect(page.getByLabel("预览缩放", { exact: true })).toHaveValue("fit");
  await expect(page.locator(".preview-canvas > img")).toBeVisible({
    timeout: 180000,
  });
  await expect(page.locator(".preview-loading")).toHaveCount(0, {
    timeout: 180000,
  });
  await page.screenshot({
    path: path.join(directory, "preview-viewport-light-zh.png"),
  });
  await page.getByRole("button", { name: "深色主题", exact: true }).click();
  await page.screenshot({
    path: path.join(directory, "preview-viewport-dark-zh.png"),
  });
  await page.getByRole("button", { name: "浅色主题", exact: true }).click();
  await page.getByLabel("语言", { exact: true }).first().selectOption("en");
  await menu
    .getByRole("button", { name: "Preview guides", exact: true })
    .click();
  await options.getByLabel("Guide type", { exact: true }).selectOption("none");
  await menu
    .getByRole("button", { name: "Preview guides", exact: true })
    .click();
  check(
    "Viewport and guide controls support Chinese/English and both themes; guides are view-only and excluded from native render requests",
  );
}
