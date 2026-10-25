// Human review and preview of the project created through actual Agent domains.
import { chromium, expect as baseExpect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { OpenCutClient } from "../../../packages/sdk-ts/dist/index.js";
import { editorSnapshot } from "./project-inspection.mjs";
import { installScenePreviewWitness } from "./verify-scene-preview.mjs";

const expect = baseExpect.configure({ timeout: 30000 });

const args = Object.fromEntries(process.argv.slice(2).reduce((out, key, i, all) => i % 2 ? out : [...out, [key.slice(2), all[i + 1]]], []));
for (const key of ["workspace", "endpoint", "binary", "chromium", "output"]) if (!args[key]) throw Error(`Missing --${key}`);
await fs.mkdir(args.output, { recursive: true });
const sha = bytes => createHash("sha256").update(bytes).digest("hex");
const candidate = sha(await fs.readFile(args.binary));
const project = args.project ?? "agent-domain-workflow";
const checkpoint = path.join(args.output, "browser-checkpoint.json");
const state = await fs.readFile(checkpoint, "utf8").then(JSON.parse).catch(error => {
  if (error.code !== "ENOENT") throw error;
  return { candidate, project_id: project, stages: {}, requests: {} };
});
if (state.candidate !== candidate) {
  expect(args["previous-candidate"]).toBe(state.candidate);
  expect(state.stages.preview).toBeUndefined();
  state.stage_candidates ??= Object.fromEntries(Object.keys(state.stages).map(name => [name, state.candidate]));
  (state.candidate_transitions ??= []).push({ from: state.candidate, to: candidate,
    retained_stages: Object.keys(state.stages), reason: "Ordinary decoder idle-read protocol fix" });
  state.candidate = candidate;
}
expect(state.project_id).toBe(project);
const save = async () => {
  await fs.writeFile(checkpoint + ".pending", JSON.stringify(state, null, 2) + "\n");
  await fs.rename(checkpoint + ".pending", checkpoint);
};
const client = new OpenCutClient({ baseUrl: args.endpoint,
  token: (await fs.readFile(path.join(args.workspace, "service/api-token"), "utf8")).trim() });
const inspect = () => editorSnapshot(client.callDomain("project", "inspect", { project_id: project, revision: null }));
const errors = [];
const browser = await chromium.launch({ headless: true, executablePath: args.chromium });
let page;
try {
  const context = await browser.newContext({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
  page = await context.newPage(); page.setDefaultTimeout(30000);
  page.on("pageerror", error => errors.push(error.message));
  await page.addInitScript(installScenePreviewWitness, { deferReadback: true });
  await page.addInitScript(() => {
    const samples = window.__workflowSamples = [];
    const original = ImageBitmapRenderingContext.prototype.transferFromImageBitmap;
    ImageBitmapRenderingContext.prototype.transferFromImageBitmap = function (bitmap) {
      const time = Number(this.canvas.dataset.time);
      const generation = this.canvas.dataset.generation;
      if (bitmap && this.canvas.classList.contains("editor-scene-frame") && samples.length < 2 &&
          ((samples.length === 0 && time > .1 && time < 2.9) ||
           (samples.length === 1 && window.__workflowWantSecond && generation !== samples[0].generation && time >= 1.5 && time <= 2.5)))
        samples.push({ time, generation, bitmap: createImageBitmap(bitmap) });
      return original.call(this, bitmap);
    };
    window.__readWorkflowSamples = async () => {
      const output = [];
      for (const sample of samples) {
        const bitmap = await sample.bitmap;
        try {
          const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
          canvas.getContext("2d").drawImage(bitmap, 0, 0);
          const bytes = new Uint8Array(await (await canvas.convertToBlob({ type: "image/png" })).arrayBuffer());
          let binary = ""; for (const value of bytes) binary += String.fromCharCode(value);
          output.push({ time: sample.time, generation: sample.generation, png: btoa(binary) });
        } finally { bitmap.close(); }
      }
      return output;
    };
  });
  const open = async () => {
    await page.goto(args.endpoint + "/editor/" + encodeURIComponent(project));
    await expect(page.getByRole("main", { name: "OpenCut editor", exact: true })).toBeVisible();
    const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
    if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
  };
  await open();
  state.initial ??= await inspect(); await save();
  if (!args["read-only"] && !state.stages.rename) {
    if (state.requests.rename) {
      state.stages.rename = await client.callDomain("edit", "apply", state.requests.rename);
      await save(); await open();
    } else {
      await context.route("**/api/tools/opencut_edit_apply", async route => {
        state.requests.rename = route.request().postDataJSON(); await save(); await route.continue();
      });
      const before = await inspect();
      await page.getByRole("button", { name: `Rename project ${before.project.name}`, exact: true }).click();
      const dialog = page.getByRole("dialog", { name: "Rename project", exact: true });
      await dialog.getByRole("textbox", { name: "Project name", exact: true }).fill("Agent domain workflow reviewed");
      const response = page.waitForResponse(reply => reply.url().endsWith("/api/tools/opencut_edit_apply"));
      await dialog.getByRole("button", { name: "Save", exact: true }).click();
      const receipt = await (await response).json(); expect(receipt.error).toBeNull();
      state.stages.rename = receipt.data; await save();
      await context.unroute("**/api/tools/opencut_edit_apply");
    }
  }
  const reviewed = await inspect();
  if (!args["read-only"]) expect(reviewed.project.name).toBe("Agent domain workflow reviewed");
  if (!state.stages.preview) {
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill("0");
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
    await page.getByRole("button", { name: "Play scene preview", exact: true }).click();
    const live = page.locator("canvas.editor-scene-frame"); await expect(live).toBeVisible();
    await expect.poll(() => page.evaluate(() => window.__workflowSamples.length)).toBe(1);
    const generation = await live.getAttribute("data-generation");
    await page.evaluate(() => { window.__workflowWantSecond = true; });
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill("1.5");
    await page.getByLabel("Playhead (seconds)", { exact: true }).press("Enter");
    await expect.poll(() => live.getAttribute("data-generation")).not.toBe(generation);
    await expect.poll(() => page.evaluate(() => window.__workflowSamples.length)).toBe(2);
    await page.getByRole("button", { name: "Pause scene audio", exact: true }).click();
    await expect(page.getByText("Scene sound paused", { exact: true })).toBeVisible();
    const pictures = await page.evaluate(() => window.__readWorkflowSamples());
    for (let i = 0; i < pictures.length; i++) {
      pictures[i].output = path.join(args.output, `workflow-preview-${i}.png`);
      await fs.writeFile(pictures[i].output, Buffer.from(pictures[i].png, "base64")); delete pictures[i].png;
    }
    const observed = await page.evaluate(() => {
      const w = window.__scenePreviewWitness;
      return { frames: w.frames, progresses: w.progresses, overflows: w.overflows };
    });
    expect(observed.frames.length).toBeGreaterThan(2); expect(observed.overflows).toBe(0);
    state.stages.preview = { pictures, observed, browser: await browser.version() };
    (state.stage_candidates ??= {}).preview = candidate; await save();
  }
  await open(); expect(await inspect()).toEqual(reviewed); expect(errors).toEqual([]);
  const scene = reviewed.project.sequences[0];
  const composition = { project_id: project, revision: reviewed.revision, sequence_id: scene.id,
    settings: reviewed.project.settings.settings, fonts: reviewed.project.fonts, captions: scene.captions,
    titles: [], pictures: state.stages.preview.pictures, preview_passed: true };
  await fs.writeFile(path.join(args.output, "workflow-composition-evidence.json"), JSON.stringify(composition, null, 2) + "\n");
  await fs.writeFile(path.join(args.output, "browser-report.json"), JSON.stringify({ status: "passed", binary_sha256: candidate,
    project_id: project, snapshot: reviewed, stages: state.stages, page_errors: errors,
    scope: "Real editor open, optional human rename, saved reopen and three-second preview/seek/pause. No physical-audio or long-media claim." }, null, 2) + "\n");
  if (state.failure) { state.retained_verifier_failure ??= state.failure; delete state.failure; await save(); }
  console.log("Agent project, SDK domain readback, human review, preview/seek/pause and reopen passed");
} catch (error) {
  const observed = await page?.evaluate(() => {
    const w = window.__scenePreviewWitness, canvas = document.querySelector("canvas.editor-scene-frame");
    return { alerts: [...document.querySelectorAll('[role="alert"]')].map(e => e.textContent),
      samples: window.__workflowSamples?.map(({ time, generation }) => ({ time, generation })),
      canvas: canvas ? { width: canvas.width, height: canvas.height, data: { ...canvas.dataset } } : null,
      frames: w?.frames.slice(-16), progress: w?.progresses.slice(-8),
      body: document.body.innerText.slice(-4000) };
  }).catch(() => null);
  state.failure = { message: String(error), page_errors: errors, observed }; await save(); throw error;
} finally { await browser.close(); }
