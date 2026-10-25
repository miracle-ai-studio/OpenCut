import { expect } from "@playwright/test";
import { createHash } from "node:crypto";

// Authenticated production task/download. Expected frequency and level are
// independent of the renderer; the earlier full WAV adds a crop-order check.
export async function verifyAudioRange(page, tool, fixed, sceneId, fullBytes, fullResult, waitTerminal, check) {
  await page.reload();
  const history = await tool("opencut_project_history", { project_id: fixed.project.id, limit: 50 });
  const index = history.revisions.findIndex(r => r.revision === fixed.revision);
  expect(index).toBeGreaterThanOrEqual(0);
  await page.locator(".editor-revisions > li").nth(index)
    .getByRole("button", { name: "View revision", exact: true }).click();
  await expect(page.getByRole("button", { name: "Return to current", exact: true })).toBeVisible();
  await page.getByLabel("Scene", { exact: true }).selectOption(sceneId);
  await page.getByRole("button", { name: "Export audio WAV", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Export audio WAV", exact: true });
  const reset = dialog.getByRole("button", { name: "New audio export", exact: true });
  if (await reset.isVisible()) await reset.click();
  await dialog.getByRole("combobox", { name: /^Export scope/ }).selectOption("range");
  await dialog.getByLabel("Starting sample (0-based)", { exact: true }).fill("6000");
  await dialog.getByLabel("Frame count", { exact: true }).fill("24000");
  await dialog.getByRole("combobox", { name: /^Sample rate/ }).selectOption("48000");
  const [response] = await Promise.all([
    page.waitForResponse(r => r.url().endsWith("/api/tools/opencut_render_submit") && r.request().postDataJSON()?.kind === "audio"),
    dialog.getByRole("button", { name: "Start audio export", exact: true }).click(),
  ]);
  const envelope = await response.json(), request = response.request().postDataJSON();
  expect(envelope.error).toBeNull();
  expect(request).toMatchObject({ kind: "audio", request: { project_id: fixed.project.id, revision: fixed.revision,
    sequence_id: sceneId, sample_rate: 48000, range: { start_sample: "6000", frame_count: "24000" } } });
  const job = await waitTerminal(envelope.data.job_id);
  expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
  expect(job.result.kind).toBe("render.audio");
  const result = job.result.data;
  expect(result.identity).toMatchObject({ project_id: fixed.project.id, revision: fixed.revision,
    sequence_id: sceneId, sample_rate: 48000, channels: 2, start_sample: "6000", frame_count: "24000",
    whole_scene_frame_count: "64000", sample_format: "ieee_float32_le" });
  expect(result.execution).toMatchObject({ mastering_applied: false,
    mastering_profile: "classic-export-whole-scene-web-audio-v31" });
  expect(result.render_input_sha256).not.toBe(fullResult.render_input_sha256);
  expect((await tool("opencut_render_submit", request)).job_id).toBe(job.job_id);
  await dialog.getByRole("button", { name: "View tasks", exact: true }).click();
  const tasks = page.getByRole("dialog", { name: "Tasks", exact: true });
  await tasks.getByRole("button", { name: "Refresh tasks", exact: true }).click();
  const card = tasks.locator(".task-card").filter({ hasText: "Audio WAV export" });
  await expect(card.locator(".state")).toHaveText("succeeded");
  await page.evaluate(() => {
    window.savedRange = null;
    window.showSaveFilePicker = async () => ({ createWritable: async () => {
      const chunks = [];
      return new WritableStream({ write(chunk) { chunks.push(chunk); }, close() { window.savedRange = new Blob(chunks); } });
    } });
  });
  try {
    await card.getByRole("button", { name: "Download", exact: true }).click();
    await expect.poll(() => page.evaluate(() => window.savedRange?.size ?? 0)).toBe(192058);
    const bytes = Buffer.from(await page.evaluate(async () => Array.from(new Uint8Array(await window.savedRange.arrayBuffer()))));
    expect(bytes.toString("ascii", 0, 4)).toBe("RIFF");
    expect(bytes.readUInt32LE(4)).toBe(bytes.length - 8);
    expect(bytes.readUInt16LE(20)).toBe(3);
    expect(bytes.readUInt16LE(22)).toBe(2);
    expect(bytes.readUInt32LE(24)).toBe(48000);
    expect(bytes.readUInt32LE(46)).toBe(24000);
    expect(bytes.readUInt32LE(54)).toBe(24000 * 8);
    expect(result.artifact).toEqual({ mime_type: "audio/wav", size_bytes: "192058",
      content_hash: createHash("sha256").update(bytes).digest("hex") });
    expect(bytes.subarray(58)).toEqual(fullBytes.subarray(58 + 6000 * 8, 58 + 30000 * 8));
    let square = 0, previous = 0, finite = true, stereoDifference = 0;
    const crossings = [];
    for (let n = 0; n < 24000; n++) {
      const l = bytes.readFloatLE(58 + n * 8), r = bytes.readFloatLE(62 + n * 8);
      finite &&= Number.isFinite(l) && Number.isFinite(r);
      stereoDifference = Math.max(stereoDifference, Math.abs(l - r));
      square += l * l;
      if (n && previous <= 0 && l > 0) crossings.push(n - 1 - previous / (l - previous));
      previous = l;
    }
    expect(finite).toBe(true);
    expect(stereoDifference).toBeLessThan(.000001);
    const frequency = (crossings.length - 1) * 48000 / (crossings.at(-1) - crossings[0]);
    const expectedRms = .125 * 10 ** (-6 / 20) / Math.sqrt(2);
    expect(Math.abs(frequency - 660)).toBeLessThan(1);
    expect(Math.sqrt(square / 24000)).toBeGreaterThan(expectedRms * .85);
    expect(Math.sqrt(square / 24000)).toBeLessThan(expectedRms * 1.15);
  } finally {
    await page.evaluate(() => { delete window.savedRange; delete window.showSaveFilePicker; });
  }
  await tasks.getByRole("button", { name: "Close", exact: true }).click();
  await page.getByRole("button", { name: "Return to current", exact: true }).click();
  check("real historical V4 sample-range WAV preserves the captured revision, replays the same task, downloads exact192058-byte Float32 stereo PCM with independent660Hz/-6dB level and matches the complete mastered scene crop", { audio_range_wav: result });
}
