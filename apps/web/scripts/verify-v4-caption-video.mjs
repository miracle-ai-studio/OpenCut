import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import { createReadStream } from "node:fs";
import path from "node:path";
import { randomUUID, createHash } from "node:crypto";
import { installScenePreviewWitness } from "./verify-v4-scene-preview.mjs";

const fileSha256 = async file => {
  const hash = createHash("sha256");
  for await (const bytes of createReadStream(file)) hash.update(bytes);
  return hash.digest("hex");
};
const candidateIdentity = async () => {
  const binary = process.env.OPENCUT_WEB_BINARY, root = process.env.OPENCUT_WEB_ROOT;
  expect(binary, "OPENCUT_WEB_BINARY is required for candidate-bound job reuse").toBeTruthy();
  expect(root, "Explicit OPENCUT_WEB_ROOT is required for candidate-bound job reuse").toBeTruthy();
  const files = [];
  const visit = async relative => {
    const entries = await fs.readdir(path.join(root, relative), { withFileTypes: true });
    for (const entry of entries) {
      const name = path.join(relative, entry.name);
      if (entry.isDirectory()) await visit(name);
      else if (entry.isFile() && /\.(?:js|mjs|cjs|css)$/u.test(name)) files.push(name);
      else if (entry.isSymbolicLink()) throw new Error(`Web candidate tree contains an unhashed symlink: ${name}`);
    }
  };
  await visit(""); files.sort();
  expect(files.length, "Web candidate must contain built JS/CSS assets").toBeGreaterThan(0);
  const tree = createHash("sha256");
  for (const name of files) {
    const file = path.join(root, name), stat = await fs.stat(file);
    tree.update(JSON.stringify([name.split(path.sep).join("/"), stat.size, await fileSha256(file)]) + "\n");
  }
  return { binary_sha256: await fileSha256(binary), web_js_css_tree_sha256: tree.digest("hex"), web_js_css_files: files.length };
};

// One owned B3 SRT scene, no imports/project creation. Pixel and PCM checks are
// separate independent verifiers over the retained original source fixtures.
export async function verifyV4CaptionVideo(browser, endpoint, directory, check) {
  const captionsReport = JSON.parse(await fs.readFile(path.join(directory, "b3-captions.json"), "utf8"));
  const projectId = captionsReport.project_id, sequenceId = captionsReport.sequence_id, sourceRevision = captionsReport.final_revision;
  const checkpointPath = path.join(directory, "b3-caption-video-checkpoint.json");
  const candidate = await candidateIdentity();
  let prior = null;
  try { prior = JSON.parse(await fs.readFile(checkpointPath, "utf8")); } catch (error) { if (error.code !== "ENOENT") throw error; }
  if (prior && (prior.project_id !== projectId || prior.source_caption_revision !== sourceRevision)) prior = null;
  if (prior?.submission?.job_id && JSON.stringify(prior.candidate_identity) !== JSON.stringify(candidate)) {
    const mismatch = { status: "rejected", reason: "The retained export job belongs to another or unrecorded candidate. Root must decide whether to run a new candidate; the old job/checkpoint is preserved.", retained_job_id: prior.submission.job_id, retained_candidate: prior.candidate_identity ?? null, current_candidate: candidate };
    await fs.writeFile(path.join(directory, "b3-caption-video-candidate-mismatch.json"), JSON.stringify(mismatch, null, 2) + "\n");
    throw new Error(mismatch.reason);
  }
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 }, acceptDownloads: true });
  page.setDefaultTimeout(30000);
  let phase = "open", originalRevision, metadata = prior ?? {}, changed = false;
  const network = [], errors = [], frameCalls = [];
  let writeCount = 0;
  page.on("pageerror", error => { if (errors.length < 32) errors.push(error.message.slice(0, 1024)); });
  page.on("request", request => {
    const operation = new URL(request.url()).pathname.split("/").at(-1);
    if (operation.startsWith("opencut_") && network.length < 512) network.push({ operation });
    if (operation === "opencut_edit_v4_apply") writeCount++;
    if (operation === "opencut_visual_preview_v4_caption_frame" && frameCalls.length < 128) frameCalls.push(request.postDataJSON());
  });
  await page.addInitScript(() => Object.defineProperty(window, "showSaveFilePicker", { value: undefined, configurable: true }));
  await page.addInitScript(installScenePreviewWitness, { deferReadback: true });
  await page.addInitScript(() => {
    const captured = window.__captionVideoSamples = [];
    const transfer = ImageBitmapRenderingContext.prototype.transferFromImageBitmap;
    ImageBitmapRenderingContext.prototype.transferFromImageBitmap = function (bitmap) {
      const time = Number(this.canvas.dataset.time);
      const slot = time < .1 ? "before" : time >= .35 && time < 1 ? "during" : time >= 1.5 && time < 2.5 ? "after" : null;
      if (bitmap && this.canvas.classList.contains("v4-scene-frame") && slot && !captured.some(sample => sample.slot === slot))
        captured.push({ slot, time, generation: this.canvas.dataset.generation, bitmap: createImageBitmap(bitmap) });
      return transfer.call(this, bitmap);
    };
    window.__readCaptionVideoSamples = async () => {
      const output = [];
      for (const sample of captured) {
        const bitmap = await sample.bitmap;
        try {
          const surface = new OffscreenCanvas(bitmap.width, bitmap.height); surface.getContext("2d").drawImage(bitmap, 0, 0);
          const blob = await surface.convertToBlob({ type: "image/png" }), bytes = new Uint8Array(await blob.arrayBuffer());
          let binary = ""; for (const byte of bytes) binary += String.fromCharCode(byte);
          output.push({ slot: sample.slot, time: sample.time, generation: sample.generation, width: bitmap.width, height: bitmap.height, png: btoa(binary) });
        } finally { bitmap.close(); }
      }
      return output;
    };
  });
  const tool = async (name, input) => {
    const envelope = await page.evaluate(async ({ name, input }) => (await fetch(`/v1/tools/${name}`, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input),
    })).json(), { name, input });
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull(); return envelope.data;
  };
  const inspect = () => tool("opencut_project_v4_inspect", { project_id: projectId, revision: null });
  const restore = async revision => {
    const head = await inspect();
    await tool("opencut_project_v4_restore", { project_id: projectId, expected_revision: head.revision, target_revision: revision, idempotency_key: randomUUID(), asset_versions: {} });
    return inspect();
  };
  const checkpoint = async facts => {
    metadata = { ...metadata, ...facts, project_id: projectId, sequence_id: sequenceId, source_caption_revision: sourceRevision, candidate_identity: candidate };
    await fs.writeFile(checkpointPath, JSON.stringify({ ...metadata, phase }, null, 2) + "\n");
  };
  const reopen = async () => {
    await page.goto(`${endpoint}/editor/${encodeURIComponent(projectId)}`);
    await expect(page.getByRole("main", { name: "V4 editor", exact: true })).toBeVisible();
    const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
    if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
    await page.getByRole("combobox", { name: /^V4 scene(?:\s|$)/ }).selectOption(sequenceId);
  };
  try {
    await reopen(); originalRevision = (await inspect()).revision;
    if (!metadata.submission?.job_id) {
      phase = "restore verified caption source"; changed = true;
      const fixed = await restore(sourceRevision), scene = fixed.project.sequences.find(scene => scene.id === sequenceId);
      const titles = scene.tracks.filter(track => track.enabled).flatMap(track => track.elements).filter(element => element.enabled && element.content.kind === "text" && element.name.startsWith("B2 independent"));
      expect(titles).toHaveLength(2);
      titles.sort((a, b) => a.visual.transform.position_y_px - b.visual.transform.position_y_px);
      const captions = scene.captions.filter(cue => scene.tracks.some(track => track.id === cue.track_id && track.enabled));
      expect(captions).toHaveLength(1);
      expect(captions[0]).toEqual(captionsReport.evidence.find(item => item.cue)?.cue);
      const fontIds = new Set([...titles.map(title => title.content.text.font.font_id), ...captions.map(cue => cue.style.font.font_id)]);
      await checkpoint({ revision: fixed.revision, settings: fixed.project.settings.settings, titles: titles.map(title => ({ id: title.id, text: title.content.text, visual: title.visual })), captions, fonts: fixed.project.fonts.filter(font => fontIds.has(font.id)).map(font => ({ id: font.id, family: font.family, content: font.content })), bookmarks: scene.bookmarks, mp4_sample_times: [0, .5, 1.5] });
      await reopen();
      phase = "continuous caption preview before/during/after and seek generation";
      await page.getByLabel("Playhead (seconds)", { exact: true }).fill("0");
      await page.getByRole("button", { name: "Go to time", exact: true }).click();
      const live = page.locator("canvas.v4-scene-frame");
      await page.getByRole("button", { name: "Play scene preview", exact: true }).click();
      await expect(live).toBeVisible();
      const generation = await live.getAttribute("data-generation");
      await expect.poll(() => page.evaluate(() => window.__captionVideoSamples.some(sample => sample.slot === "before")), { timeout: 30000 }).toBe(true);
      await page.getByLabel("Playhead (seconds)", { exact: true }).fill("0.5");
      await page.getByLabel("Playhead (seconds)", { exact: true }).press("Enter");
      await expect.poll(() => live.getAttribute("data-generation")).not.toBe(generation);
      const middleGeneration = await live.getAttribute("data-generation");
      await expect.poll(() => page.evaluate(() => window.__captionVideoSamples.some(sample => sample.slot === "during"))).toBe(true);
      // Observe actual advancement within this generation before another seek.
      // Capturing only each seek's first frame does not establish continuous playback.
      await expect.poll(() => page.evaluate(generation => new Set(window.__scenePreviewWitness.frames
        .filter(frame => frame.generation === generation && frame.time >= .5)
        .map(frame => frame.time)).size, middleGeneration)).toBeGreaterThanOrEqual(3);
      await page.getByLabel("Playhead (seconds)", { exact: true }).fill("1.5");
      await page.getByLabel("Playhead (seconds)", { exact: true }).press("Enter");
      await expect.poll(() => live.getAttribute("data-generation")).not.toBe(middleGeneration);
      await expect.poll(() => page.evaluate(() => window.__captionVideoSamples.some(sample => sample.slot === "after"))).toBe(true);
      await page.getByRole("button", { name: "Pause scene audio", exact: true }).click();
      await expect(page.getByText("Scene sound paused", { exact: true })).toBeVisible();
      const pictures = await page.evaluate(() => window.__readCaptionVideoSamples());
      for (const picture of pictures) {
        picture.output = path.join(directory, `b3-caption-preview-${picture.slot}.png`);
        await fs.writeFile(picture.output, Buffer.from(picture.png, "base64")); delete picture.png;
      }
      const preview = await page.evaluate(() => { const witness = window.__scenePreviewWitness; return { frames: witness.frames, controls: witness.controls, progresses: witness.progresses, blocks: witness.blocks, overflows: witness.overflows, session_identity: witness.session?.identity }; });
      await checkpoint({ pictures, preview, caption_frame_calls: frameCalls, preview_passed: false });
      expect(pictures.map(picture => picture.slot).sort()).toEqual(["after", "before", "during"]);
      expect(frameCalls.length).toBeGreaterThan(0);
      expect(preview.session_identity).toMatchObject({ project_id: projectId, revision: fixed.revision, sequence_id: sequenceId });
      expect(preview.frames.filter(frame => frame.time > .3).length).toBeGreaterThan(2);
      expect(pictures.every(picture => picture.width === 320 && picture.height === 180)).toBe(true);
      expect((await inspect()).revision).toBe(fixed.revision); expect(writeCount).toBe(0);
      await checkpoint({ preview_passed: true });
      await reopen();
      phase = "new caption MP4 actual UI submit";
      await page.getByRole("button", { name: "Export video MP4", exact: true }).click();
      const dialog = page.getByRole("dialog", { name: "Export video MP4", exact: true });
      const pending = page.waitForResponse(response => response.url().endsWith("/v1/tools/opencut_job_v4_caption_video_mp4"));
      await dialog.getByRole("button", { name: "Start video export", exact: true }).click();
      const response = await pending, envelope = await response.json();
      expect(envelope.error, JSON.stringify(envelope.error)).toBeNull();
      await checkpoint({ submission: { source: "ui_submit", job_id: envelope.data.job_id, request: response.request().postDataJSON() } });
      expect(metadata.submission.request).toMatchObject({ project_id: projectId, revision: fixed.revision, sequence_id: sequenceId });
      await dialog.getByRole("button", { name: "View tasks", exact: true }).click();
    } else {
      phase = "reuse original caption export job; no second encoding";
      await page.getByRole("button", { name: "Tasks", exact: true }).click();
    }
    let job;
    await expect.poll(async () => {
      job = await tool("opencut_job_view", { job_id: metadata.submission.job_id });
      return ["succeeded", "failed", "cancelled"].includes(job.state) ? job.state : "pending";
    }, { timeout: 300000, intervals: [250, 500, 1000] }).not.toBe("pending");
    await checkpoint({ job });
    expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
    expect(job.target).toMatchObject({ project_id: projectId, revision: metadata.revision, sequence_id: sequenceId });
    expect(job.kind).toBe("render.project_caption_video.v4");
    expect(job.extended_result).toMatchObject({ kind: "render.project_caption_video.v4" });
    expect(job.extended_result.data).toMatchObject({ schema_version: 2, identity: { project_id: projectId, revision: metadata.revision, sequence_id: sequenceId, profile: "classic-v31-cfr-h264-aac-caption-v2", width: 320, height: 180, frame_count: "90", sample_rate: 48000, channels: 2 } });
    const artifact = job.artifacts.find(artifact => artifact.mime_type === "video/mp4"); expect(artifact).toBeTruthy();
    phase = "actual task artifact download";
    await page.getByLabel("Refresh tasks", { exact: true }).click();
    // The caption panel has its own text-labelled refresh button. Target the
    // Tasks panel's accessible icon and this export's fixed revision directly.
    const task = page.locator(".task-card").filter({ hasText: metadata.revision });
    await expect(task).toHaveCount(1);
    await expect(task).toContainText(metadata.revision); await expect(task).toContainText("succeeded");
    const downloading = page.waitForEvent("download"); await task.getByRole("button", { name: "Download", exact: true }).click();
    const download = await downloading, output = path.join(directory, "b3-caption-mixed.mp4");
    await download.saveAs(output); const bytes = await fs.readFile(output), sha256 = createHash("sha256").update(bytes).digest("hex");
    await checkpoint({ output, sha256, artifact, download_completed: true });
    expect(sha256).toBe(artifact.content_hash); expect(errors).toEqual([]);
    phase = "restore entering project revision";
    if (changed) { await restore(originalRevision); changed = false; }
    const report = { ...metadata, status: "passed", export_passed: true, restored_from_revision: originalRevision, final_revision: (await inspect()).revision, reused_job: prior?.submission?.job_id ?? null, pageerrors: errors, network, scope: "Actual wrapped-caption continuous playback/pause/seek generations, new caption frame operation, new caption MP4 UI job and downloaded artifact. Independent preview/MP4 pixels and PCM are verified separately." };
    await fs.writeFile(path.join(directory, "b3-caption-video-evidence.json"), JSON.stringify(report, null, 2) + "\n");
    check("B3 continuous captions and actual caption MP4 UI/download", report); return report;
  } catch (error) {
    const failure = { phase, project_id: projectId, error: String(error).slice(0, 4096), checkpoint: metadata, network, errors };
    try { failure.ui = await page.evaluate(() => document.body.innerText.slice(0, 8192)); } catch {}
    if (changed && originalRevision) {
      try { failure.restored_revision = (await restore(originalRevision)).revision; changed = false; }
      catch (restoreError) { failure.restore_error = String(restoreError).slice(0, 2048); }
    }
    await fs.writeFile(path.join(directory, "b3-caption-video-failure.json"), JSON.stringify(failure, null, 2) + "\n");
    await page.screenshot({ path: path.join(directory, "b3-caption-video-failure.png"), fullPage: false }).catch(() => {});
    throw error;
  } finally { await page.close(); }
}
