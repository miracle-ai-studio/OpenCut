import { editorSnapshot } from "./project-inspection.mjs";
import { verifyViewport } from "./verify-viewport.mjs";
import { chromium, expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { verifyGuide } from "./verify-presentation.mjs";
const endpoint = process.env.OPENCUT_WEB_URL;
const directory = process.env.OPENCUT_WEB_WORKSPACE;
const browser = await chromium.launch({
  headless: true,
  executablePath: process.env.OPENCUT_CHROME,
});
const page = await browser.newPage({
  locale: "en-US",
  viewport: { width: 1440, height: 1000 },
});
page.setDefaultTimeout(120000);
const checks = [],
  errors = [];
page.on("pageerror", (error) => errors.push(error.message));
const check = (name) => {
  checks.push(name);
  console.log(name);
};
async function call(name, input) {
  return page.evaluate(
    async ({ name, input }) => {
      const result = await fetch(`/api/tools/${name}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(input),
      }).then((response) => response.json());
      if (result.error) throw new Error(result.error.message);
      return result.data;
    },
    { name, input },
  );
}
const inspect = () =>
  editorSnapshot(call("opencut_project_inspect", { project_id: "speech", revision: null }));
const cues = (snapshot) => snapshot.project.base.sequences[0].caption_cues;
try {
  await page.goto(`${endpoint}/editor/speech?scene=main`);
  await verifyGuide(page, check);
  // Reset only this verifier's synthetic project, through the shared edit API.
  // Repeated acceptance runs must not stack earlier captions in QA screenshots.
  let seeded = await inspect();
  expect(seeded.project.base.name).toBe("Speech acceptance");
  while (cues(seeded).length) {
    await call("opencut_edit_apply", {
      project_id: "speech",
      expected_revision: seeded.revision,
      idempotency_key: crypto.randomUUID(),
      label: "Reset synthetic acceptance captions",
      operations: cues(seeded)
        .slice(0, 128)
        .map((cue) => ({
          type: "base.apply",
          operation: {
            type: "caption.delete",
            sequence_id: "main",
            cue_id: cue.id,
          },
        })),
    });
    seeded = await inspect();
  }
  await page.reload();
  await verifyViewport(page, inspect, directory, check);
  await expect(
    page.getByRole("region", { name: "Timeline", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Captions", exact: true }).click();
  const panel = page.getByRole("region", {
    name: "Automatic captions",
    exact: true,
  });
  await expect(panel).toBeVisible();
  await expect(panel.getByLabel("Language")).toHaveValue("auto");
  await expect(panel.getByLabel("Language").locator("option")).toHaveCount(10);
  await panel.getByLabel("Language").selectOption("en");
  await panel.getByLabel("Words per caption").selectOption("5");
  const before = await inspect();
  await panel
    .getByRole("button", { name: "Generate transcript", exact: true })
    .click();
  await expect(
    panel.getByRole("button", { name: "Transcribing…", exact: true }),
  ).toBeDisabled();
  await expect
    .poll(async () => cues(await inspect()).length, { timeout: 900000 })
    .toBeGreaterThan(cues(before).length);
  await expect(
    panel.getByRole("button", { name: "Generate transcript", exact: true }),
  ).toBeEnabled();
  const generated = await inspect();
  const added = cues(generated).filter(
    (cue) => !cues(before).some((prior) => prior.id === cue.id),
  );
  expect(added.length).toBeGreaterThan(0);
  expect(added.every((cue) => cue.text.split(/\s+/).length <= 5)).toBeTruthy();
  expect(
    added.some((cue) => cue.text.toLowerCase().includes("captions")),
  ).toBeTruthy();
  check(
    "One browser action performs real local ASR and atomically adds captions, retaining the selected language and word count",
  );
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect
    .poll(async () => JSON.stringify((await inspect()).project))
    .toBe(JSON.stringify(before.project));
  await page.getByRole("button", { name: "Redo", exact: true }).click();
  await expect
    .poll(async () => JSON.stringify((await inspect()).project))
    .toBe(JSON.stringify(generated.project));
  check(
    "Browser undo and redo recover exact documents for a generated-caption transaction",
  );
  await page
    .getByRole("button", { name: "Generate transcript", exact: true })
    .click();
  await expect(
    panel.getByRole("button", { name: "Cancel transcription", exact: true }),
  ).toBeVisible();
  await page.reload();
  await expect(
    page.getByRole("region", { name: "Timeline", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Captions", exact: true }).click();
  await expect(
    panel.getByRole("button", { name: "Transcribing…", exact: true }),
  ).toBeDisabled();
  await panel
    .getByRole("button", { name: "Cancel transcription", exact: true })
    .click();
  await expect(
    panel.getByRole("button", { name: "Generate transcript", exact: true }),
  ).toBeEnabled();
  expect((await inspect()).project).toEqual(generated.project);
  check(
    "Reload preserves durable transcription status; explicit cancellation adds no captions or project revision",
  );
  if ((await page.locator("html").getAttribute("data-theme")) !== "light")
    await page
      .getByRole("button", { name: "Light theme", exact: true })
      .click();
  await page.getByLabel("Language", { exact: true }).first().selectOption("zh");
  await expect(
    page.getByRole("region", { name: "自动字幕", exact: true }),
  ).toBeVisible();
  const frame = page.locator(".preview-canvas > img");
  const priorFrame = await frame.getAttribute("src").catch(() => null);
  await page.locator(".caption-list button").first().click();
  await expect
    .poll(() => frame.getAttribute("src").catch(() => null), {
      timeout: 180000,
    })
    .not.toBe(priorFrame);
  await expect(frame).toBeVisible({ timeout: 180000 });
  await expect(page.locator(".preview-loading")).toHaveCount(0, {
    timeout: 180000,
  });
  await page.screenshot({
    path: path.join(directory, "captions-light-zh.png"),
  });
  await page.getByRole("button", { name: "深色主题", exact: true }).click();
  await page.screenshot({ path: path.join(directory, "captions-dark-zh.png") });
  check(
    "Automatic-caption controls support Chinese/English and light/dark themes",
  );
  expect(errors).toEqual([]);
  await fs.writeFile(
    path.join(directory, "caption-browser-report.json"),
    JSON.stringify(
      { status: "passed", checks, consoleErrors: errors, endpoint },
      null,
      2,
    ),
  );
} catch (error) {
  await page.screenshot({
    path: path.join(directory, "captions-browser-failure.png"),
  });
  await fs.writeFile(
    path.join(directory, "caption-browser-report.json"),
    JSON.stringify(
      { status: "failed", checks, consoleErrors: errors, error: error.stack },
      null,
      2,
    ),
  );
  throw error;
} finally {
  await browser.close();
}
