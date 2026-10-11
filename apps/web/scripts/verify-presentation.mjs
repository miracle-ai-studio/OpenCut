import { expect } from "@playwright/test";
import path from "node:path";
export async function verifyGuide(page, check) {
  const guide = page.locator(".editor-guide");
  await expect(guide).toBeVisible();
  await expect(
    guide.getByRole("heading", { name: "Welcome to OpenCut", exact: true }),
  ).toBeVisible();
  await guide.getByRole("button", { name: "Next", exact: true }).click();
  await expect(
    guide.getByRole("heading", {
      name: "Editing on your computer",
      exact: true,
    }),
  ).toBeVisible();
  await guide.getByLabel("Guide language", { exact: true }).selectOption("zh");
  await expect(
    guide.getByRole("heading", { name: "在你的电脑上剪辑", exact: true }),
  ).toBeVisible();
  await guide.getByRole("button", { name: "返回", exact: true }).click();
  await expect(
    guide.getByRole("heading", { name: "欢迎使用 OpenCut", exact: true }),
  ).toBeVisible();
  await guide.getByRole("button", { name: "下一步", exact: true }).click();
  await guide.getByRole("button", { name: "下一步", exact: true }).click();
  await expect(
    guide.getByRole("heading", { name: "与社区共同构建", exact: true }),
  ).toBeVisible();
  await guide.getByLabel("引导语言", { exact: true }).selectOption("en");
  await guide.getByRole("button", { name: "Finish", exact: true }).click();
  await expect(guide).toHaveCount(0);
  const notice = page.getByRole("complementary", {
    name: "Original OpenCut update",
    exact: true,
  });
  await expect(notice).toBeVisible();
  await expect(
    notice.getByRole("link", { name: "Read release notes" }),
  ).toHaveAttribute("href", "/changelog/0.3.0");
  await notice
    .getByRole("button", { name: "Dismiss update", exact: true })
    .click();
  await expect(notice).toHaveCount(0);
  await page.reload();
  await expect(
    page.getByRole("region", { name: "Timeline", exact: true }),
  ).toBeVisible();
  await expect(guide).toHaveCount(0);
  await expect(notice).toHaveCount(0);
  await page.getByRole("button", { name: "Editor guide", exact: true }).click();
  await expect(
    guide.getByRole("heading", { name: "Welcome to OpenCut", exact: true }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(guide).toHaveCount(0);
  check(
    "Classic three-step guide supports both languages, backward navigation, finish and replay/Escape; original release notification opens the published local record and does not repeat after reload",
  );
}
export async function verifyPanelLayouts(page, inspect, directory, check) {
  const before = (await inspect()).project;
  const group = page.locator("#stage-panels"),
    vertical = page.locator("#workspace");
  const sizes = () =>
    page.evaluate(() => ({
      library: document.getElementById("library").getBoundingClientRect().width,
      preview: document.getElementById("preview").getBoundingClientRect().width,
      properties: document.getElementById("properties").getBoundingClientRect()
        .width,
      stage: document.getElementById("stage").getBoundingClientRect().height,
      timeline: document.getElementById("timeline").getBoundingClientRect()
        .height,
    }));
  const initial = await sizes();
  const h = await group
    .locator(":scope > .resize-handle")
    .first()
    .boundingBox();
  await page.mouse.move(h.x + h.width / 2, h.y + h.height / 2);
  await page.mouse.down();
  await page.mouse.move(h.x + h.width / 2 + 55, h.y + h.height / 2, {
    steps: 8,
  });
  await page.mouse.up();
  const v = await vertical.locator(":scope > .resize-handle").boundingBox();
  await page.mouse.move(v.x + v.width / 2, v.y + v.height / 2);
  await page.mouse.down();
  await page.mouse.move(v.x + v.width / 2, v.y + v.height / 2 - 45, {
    steps: 8,
  });
  await page.mouse.up();
  await expect
    .poll(async () => Math.abs((await sizes()).library - initial.library))
    .toBeGreaterThan(30);
  await expect
    .poll(async () => Math.abs((await sizes()).stage - initial.stage))
    .toBeGreaterThan(25);
  await expect
    .poll(() =>
      page.evaluate(() => {
        const value = JSON.parse(
          localStorage.getItem("opencut.layout") ?? "null",
        );
        return Boolean(value?.stage && value?.workspace);
      }),
    )
    .toBe(true);
  const saved = await sizes();
  await page.reload();
  await expect(
    page.getByRole("region", { name: "Timeline", exact: true }),
  ).toBeVisible();
  for (const name of Object.keys(saved))
    await expect
      .poll(async () => Math.abs((await sizes())[name] - saved[name]))
      .toBeLessThan(3);
  expect((await inspect()).project).toEqual(before);
  await page.screenshot({
    path: path.join(directory, "editor-restored-panels.png"),
    fullPage: true,
  });
  check(
    "Classic horizontal and vertical panel sizes persist after real pointer resizing and reload, while the complete project document stays unchanged",
  );
}
