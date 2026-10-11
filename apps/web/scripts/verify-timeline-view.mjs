import { expect } from "@playwright/test";

export async function verifyTimelineView(page, inspect, check) {
  const before = (await inspect()).project;
  const name = before.base.name;
  const slider = page.getByRole("slider", {
    name: "Timeline zoom",
    exact: true,
  });
  await slider.evaluate((input) => {
    Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      "value",
    ).set.call(input, "3");
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await expect(slider).toHaveValue("3");
  const viewport = page.locator(".timeline-scroll");
  await viewport.evaluate((element) => {
    element.scrollLeft = 280;
  });
  await expect
    .poll(() => viewport.evaluate((element) => element.scrollLeft))
    .toBe(280);
  await page.locator(".ruler-content").click({ position: { x: 360, y: 12 } });
  await expect(page.locator(".timecode")).toHaveText("00:01.50");
  const savedScroll = await viewport.evaluate((element) => element.scrollLeft);
  await page.reload();
  await expect(slider).toHaveValue("3");
  await expect
    .poll(() => viewport.evaluate((element) => element.scrollLeft))
    .toBe(savedScroll);
  await expect(page.locator(".timecode")).toHaveText("00:01.50");
  await expect(page.locator(".preview-canvas > img")).toBeVisible({
    timeout: 90000,
  });
  expect((await inspect()).project).toEqual(before);
  await page
    .getByRole("button", { name: "Back to projects", exact: true })
    .click();
  await page.getByRole("button", { name: "New project", exact: true }).click();
  const creation = page.getByRole("dialog", {
    name: "New project",
    exact: true,
  });
  await creation
    .getByLabel("Project name", { exact: true })
    .fill("Independent timeline view");
  await creation
    .getByRole("button", { name: "Create project", exact: true })
    .click();
  await expect(slider).toHaveValue("1");
  await expect(page.locator(".timecode")).toHaveText("00:00.00");
  await expect
    .poll(() => viewport.evaluate((element) => element.scrollLeft))
    .toBe(0);
  await page
    .getByRole("button", { name: "Back to projects", exact: true })
    .click();
  const card = page
    .locator(".project-card-wrap")
    .filter({
      has: page.getByText("Independent timeline view", { exact: true }),
    });
  await card.locator(".project-actions summary").click();
  await card
    .getByRole("button", { name: "Delete project", exact: true })
    .click();
  const deletion = page.getByRole("dialog", {
    name: "Delete projects",
    exact: true,
  });
  await deletion
    .getByLabel('Type "DELETE" to confirm', { exact: true })
    .fill("DELETE");
  await deletion
    .getByRole("button", { name: "Delete projects", exact: true })
    .click();
  await expect(deletion).toHaveCount(0);
  await page
    .locator(".project-card")
    .filter({ has: page.getByText(name, { exact: true }) })
    .click();
  await expect(slider).toHaveValue("3");
  await expect(page.locator(".timecode")).toHaveText("00:01.50");
  await expect
    .poll(() => viewport.evaluate((element) => element.scrollLeft))
    .toBe(savedScroll);
  expect((await inspect()).project).toEqual(before);
  check(
    "timeline zoom, scroll and exact playhead restore after reload and reopening, a new project gets an independent view, and project content remains unchanged",
  );
}
