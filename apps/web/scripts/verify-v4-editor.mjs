import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { verifyV4AudioGain } from "./verify-v4-audio-gain.mjs";
import { verifyV4AudioPreview } from "./verify-v4-audio-preview.mjs";
import { verifyV4AudioRange } from "./verify-v4-audio-range.mjs";
import { verifyV4Timeline, verifyV4TimelineGestures } from "./verify-v4-timeline.mjs";
import { verifyV4Clipboard } from "./verify-v4-clipboard.mjs";

export async function applyMediaReview(page, writes, planning) {
  const review = page.getByRole("dialog", {
    name: "Review media changes",
    exact: true,
  });
  await expect(review).toBeVisible();
  const plan = await (await planning).json();
  expect(plan.error).toBeNull();
  const applying = page.waitForResponse((r) =>
    r.url().endsWith("/v1/tools/opencut_edit_v4_apply"),
  );
  await review
    .getByRole("button", { name: "Apply media changes", exact: true })
    .click();
  const receipt = await (await applying).json();
  expect(receipt.error).toBeNull();
  expect(writes.at(-1)).toEqual(plan.data.transaction);
  await expect(
    review.getByText("Media changes saved.", { exact: true }),
  ).toBeVisible();
  await review.getByRole("button", { name: "Done", exact: true }).click();
  await expect(review).toBeHidden();
  return { plan: plan.data, receipt: receipt.data };
}

export async function importProductMedia(page, filename, writes) {
  await expect(
    page.getByRole("button", { name: "Import media", exact: true }),
  ).toBeEnabled();
  const planning = page.waitForResponse((r) =>
    r.url().endsWith("/v1/tools/opencut_media_workspace_plan_v4"),
  );
  await page
    .getByLabel("Import media files", { exact: true })
    .setInputFiles(filename);
  const result = await applyMediaReview(page, writes, planning);
  await expect(
    page.locator(".v4-media-card").filter({ hasText: path.basename(filename) }),
  ).toBeVisible();
  return result;
}

export async function placeProductMedia(page, name, values, writes) {
  await page
    .getByRole("button", { name: `Place ${name}`, exact: true })
    .click();
  const dialog = page.getByRole("dialog", { name: "Place media", exact: true });
  await expect(dialog).toBeVisible();
  if (values.track)
    await dialog
      .getByRole("combobox", { name: /^Target track/ })
      .selectOption(values.track);
  for (const [label, value] of Object.entries(values.fields ?? {}))
    await dialog.getByLabel(label, { exact: true }).fill(value);
  if (values.keepPitch !== undefined)
    await dialog
      .getByLabel("Maintain pitch", { exact: true })
      .setChecked(values.keepPitch);
  if (values.sourceAudio !== undefined)
    await dialog
      .getByLabel("Include source audio", { exact: true })
      .setChecked(values.sourceAudio);
  const planning = page.waitForResponse((r) =>
    r.url().endsWith("/v1/tools/opencut_media_workspace_plan_v4"),
  );
  await dialog
    .getByRole("button", { name: "Review placement", exact: true })
    .click();
  return applyMediaReview(page, writes, planning);
}

export async function verifyMediaRecoveryAndReplacement(
  page,
  tool,
  directory,
  projectId,
  sceneId,
  writes,
  check,
) {
  await page.getByLabel("V4 scene", { exact: true }).selectOption(sceneId);
  const inspect = () =>
    tool("opencut_project_v4_inspect", {
      project_id: projectId,
      revision: null,
    });
  const before = await inspect();
  const planning = page.waitForResponse((r) =>
    r.url().endsWith("/v1/tools/opencut_media_workspace_plan_v4"),
  );
  await page
    .getByLabel("Import media files", { exact: true })
    .setInputFiles(path.join(directory, "sample.wav"));
  const review = page.getByRole("dialog", {
    name: "Review media changes",
    exact: true,
  });
  await expect(review).toBeVisible();
  expect((await (await planning).json()).error).toBeNull();
  await review
    .getByRole("button", { name: "Discard media plan", exact: true })
    .click();
  expect((await inspect()).revision).toBe(before.revision);
  const available = page.locator(".v4-available-media");
  await available.locator("summary").click();
  const recoverPlan = page.waitForResponse((r) =>
    r.url().endsWith("/v1/tools/opencut_media_workspace_plan_v4"),
  );
  await available
    .getByRole("button", { name: "Recover sample.wav", exact: true })
    .click();
  const recovered = await applyMediaReview(page, writes, recoverPlan);
  const audioAssetId = recovered.plan.decisions.bound_asset_ids[0];
  expect(recovered.receipt.base_revision).toBe(before.revision);
  await previewProductMedia(page, "sample.wav", "audio", 4);
  const first = await placeProductMedia(
    page,
    "sample.wav",
    {
      track: "new",
      fields: {
        "Timeline start (seconds)": "0",
        "Source start (seconds)": "0",
        "Source duration (seconds)": "2",
      },
      keepPitch: false,
    },
    writes,
  );
  expect(
    first.plan.transaction.operations.filter(
      (operation) => operation.type === "track_put",
    ),
  ).toHaveLength(1);
  let saved = await inspect();
  const newTrack = saved.project.sequences
    .find((scene) => scene.id === sceneId)
    .tracks.find((track) =>
      track.elements.some(
        (element) =>
          element.id === first.plan.decisions.placements[0].element_id,
      ),
    );
  const second = await placeProductMedia(
    page,
    "sample.wav",
    {
      track: newTrack.id,
      fields: {
        "Timeline start (seconds)": "2",
        "Source start (seconds)": "1",
        "Source duration (seconds)": "1",
      },
      keepPitch: false,
    },
    writes,
  );
  expect(second.plan.decisions.bound_asset_ids).toEqual([]);
  saved = await inspect();
  const beforeReplacement = saved.project.sequences
    .find((scene) => scene.id === sceneId)
    .tracks.find((track) => track.id === newTrack.id).elements;
  expect(beforeReplacement).toHaveLength(2);
  await page
    .getByRole("button", { name: "Replace sample.wav", exact: true })
    .click();
  const replacing = page.waitForResponse((r) =>
    r.url().endsWith("/v1/tools/opencut_media_workspace_plan_v4"),
  );
  await page
    .getByLabel("Import media files", { exact: true })
    .setInputFiles(path.join(directory, "replacement.wav"));
  const replaced = await applyMediaReview(page, writes, replacing);
  expect(replaced.plan.decisions.replaced_asset_ids).toEqual([audioAssetId]);
  const after = await inspect(),
    bytes = await fs.readFile(path.join(directory, "replacement.wav"));
  expect(
    after.project.assets.find((asset) => asset.id === audioAssetId),
  ).toMatchObject({
    name: "replacement.wav",
    version: {
      content_hash: createHash("sha256").update(bytes).digest("hex"),
      size: bytes.length,
    },
  });
  expect(
    after.project.sequences
      .find((scene) => scene.id === sceneId)
      .tracks.find((track) => track.id === newTrack.id).elements,
  ).toEqual(beforeReplacement);
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Place replacement.wav", exact: true }),
  ).toBeVisible();
  expect((await inspect()).project).toEqual(after.project);
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await page.getByRole("button", { name: "Apply undo", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Place sample.wav", exact: true }),
  ).toBeVisible();
  expect((await inspect()).project).toEqual(saved.project);
  await page.getByLabel("V4 scene", { exact: true }).selectOption(sceneId);
  await importProductMedia(page, path.join(directory, "sample.png"), writes);
  await previewProductMedia(page, "sample.png", "img");
  const image = await placeProductMedia(
    page,
    "sample.png",
    {
      track: "new_graphic",
      fields: {
        "Timeline start (seconds)": "0",
        "Image duration (seconds)": "1",
      },
    },
    writes,
  );
  expect(image.plan.decisions.placements).toHaveLength(1);
  const imageTrack = image.plan.transaction.operations.find(
    (operation) => operation.type === "track_put",
  );
  expect(imageTrack.track.kind).toBe("graphic");
  check(
    "real V4 UI discarded binding recovers uploaded audio, inserts and reuses one asset with an atomic new track, replaces bytes preserving dependents, reloads/undoes, and imports/previews/places PNG",
  );
  await verifySceneFrame(page, tool, projectId, sceneId, check);
}

async function verifySceneFrame(page, tool, projectId, sceneId, check) {
  const capability = await tool("opencut_project_v4_frame_capability", {});
  const required = !!(process.env.OPENCUT_CLASSIC_VISUAL_PACKAGE && process.env.OPENCUT_CLASSIC_VISUAL_CHROMIUM);
  if (required) expect(capability.available, capability.unavailable_reason ?? "visual runtime").toBe(true);
  if (!capability.available) return;
  // Reloading the media workspace returns to the default scene. Select the
  // fixture's scene explicitly and create a real HUD note for PNG exclusion.
  await page.getByLabel("V4 scene", { exact: true }).selectOption(sceneId);
  await page.getByRole("button", { name: "Add bookmark", exact: true }).click();
  await page.getByLabel("Bookmark time (seconds)", { exact: true }).fill("0");
  await page.getByLabel("Note duration (seconds, optional)", { exact: true }).fill("1");
  await page.getByRole("textbox", { name: "Canvas note", exact: true }).fill("Frame HUD remains local");
  await page.getByRole("button", { name: "Save bookmark", exact: true }).click();
  await expect(page.locator(".v4-marker")).toHaveCount(1);
  const state = await tool("opencut_project_v4_inspect", { project_id: projectId, revision: null });
  const rendered = page.getByRole("img", { name: "Rendered scene frame", exact: true });
  const render = async (seconds) => {
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill(seconds);
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
    await expect(rendered).toHaveCount(0);
    const submission = page.waitForResponse((r) => r.url().endsWith("/v1/tools/opencut_job_v4_frame_png"));
    await page.getByRole("button", { name: "Render scene frame", exact: true }).click();
    const response = await submission, envelope = await response.json(), request = response.request().postDataJSON();
    expect(envelope.error).toBeNull();
    expect(request).toMatchObject({ project_id: projectId, revision: state.revision, sequence_id: sceneId });
    const jobId = envelope.data.job_id;
    let job;
    await expect.poll(async () => {
      job = await tool("opencut_job_view", { job_id: jobId });
      return ["succeeded", "failed", "cancelled"].includes(job.state) ? job.state : "pending";
    }, { timeout: 120000, intervals: [100, 250, 500] }).not.toBe("pending");
    expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
    expect(job.extended_result).toMatchObject({ kind: "render.project_frame.v4", schema_version: 1 });
    const result = job.extended_result.data;
    expect(result.identity).toMatchObject({ project_id: projectId, revision: state.revision, sequence_id: sceneId,
      time: request.time, width: 640, height: 360, profile: "classic-v31-gpu-normal-frame" });
    expect(job.artifacts).toHaveLength(1);
    expect(job.artifacts[0]).toMatchObject({ mime_type: "image/png", content_hash: result.artifact.content_hash,
      size_bytes: result.artifact.size_bytes });
    await expect(rendered).toBeVisible({ timeout: 30000 });
    const pixels = await rendered.evaluate(async (image) => {
      await image.decode();
      const canvas = new OffscreenCanvas(image.naturalWidth, image.naturalHeight), ctx = canvas.getContext("2d");
      ctx.drawImage(image, 0, 0);
      return { width: canvas.width, height: canvas.height, center: [...ctx.getImageData(320, 180, 1, 1).data],
        margin: [...ctx.getImageData(20, 180, 1, 1).data] };
    });
    return { result, pixels };
  };
  const active = await render("0.5");
  await expect(page.locator(".v4-canvas-note")).toHaveCount(1);
  expect(active.pixels).toEqual({ width: 640, height: 360, center: [255, 0, 0, 255], margin: [0, 0, 0, 255] });
  expect(active.result.sources).toHaveLength(1);
  expect(active.result.sources[0]).toMatchObject({ width: 64, height: 48, requested_time: null, no_frame_reason: null });
  const outside = await render("1.25");
  expect(outside.pixels.center).toEqual([0, 0, 0, 255]);
  expect(outside.pixels.margin).toEqual([0, 0, 0, 255]);
  expect(outside.result.sources).toEqual([]);
  check("real V4 UI fixed scene PNG uses actual Classic GPU shaders: exact red image contain/black margins, visible HUD excluded from PNG and half-open clip exclusion have independent pixels; seeking invalidates the old picture", {
    scene_frame: { identity: active.result.identity, execution: active.result.execution, sources: active.result.sources,
      artifact: active.result.artifact, independent_pixels: active.pixels, outside_clip_pixels: outside.pixels }
  });
}

async function previewProductMedia(page, name, tag, duration) {
  await page
    .getByRole("button", { name: `Preview source ${name}`, exact: true })
    .click();
  const preview = page.getByRole("dialog", {
    name: "Source media preview",
    exact: true,
  });
  await expect(preview).toBeVisible();
  const media = preview.locator(tag);
  await expect(media).toBeVisible();
  if (tag === "img") {
    await expect
      .poll(() => media.evaluate((element) => element.naturalWidth))
      .toBe(64);
    expect(await media.evaluate((element) => element.naturalHeight)).toBe(48);
  } else {
    await expect
      .poll(() => media.evaluate((element) => element.readyState))
      .toBeGreaterThanOrEqual(1);
    expect(await media.evaluate((element) => element.duration)).toBeCloseTo(
      duration,
      1,
    );
  }
  expect(new URL(await media.getAttribute("src")).origin).toBe(
    new URL(page.url()).origin,
  );
  await preview.getByRole("button", { name: "Close", exact: true }).click();
}

function verifyFloatWav(bytes, result) {
  // Independent container/sample oracle. verify-web.py generates a 440 Hz,
  // amplitude 1/8 mono sine; this scene uses rate 3/2 and -6 dB without pitch
  // preservation. AAC encoding allows small amplitude/phase differences.
  const frames = 64000,
    rate = 48000;
  expect(bytes.length).toBe(58 + frames * 8);
  expect(bytes.toString("ascii", 0, 4)).toBe("RIFF");
  expect(bytes.readUInt32LE(4)).toBe(bytes.length - 8);
  expect(bytes.toString("ascii", 8, 16)).toBe("WAVEfmt ");
  expect(bytes.readUInt32LE(16)).toBe(18);
  expect(bytes.readUInt16LE(20)).toBe(3); // IEEE Float32, not integer PCM.
  expect(bytes.readUInt16LE(22)).toBe(2);
  expect(bytes.readUInt32LE(24)).toBe(rate);
  expect(bytes.readUInt32LE(28)).toBe(rate * 8);
  expect(bytes.readUInt16LE(32)).toBe(8);
  expect(bytes.readUInt16LE(34)).toBe(32);
  expect(bytes.readUInt16LE(36)).toBe(0);
  expect(bytes.toString("ascii", 38, 42)).toBe("fact");
  expect(bytes.readUInt32LE(42)).toBe(4);
  expect(bytes.readUInt32LE(46)).toBe(frames);
  expect(bytes.toString("ascii", 50, 54)).toBe("data");
  expect(bytes.readUInt32LE(54)).toBe(frames * 8);
  expect(result.identity).toMatchObject({
    sample_rate: rate,
    channels: 2,
    frame_count: String(frames),
    sample_format: "ieee_float32_le",
  });
  expect(result.artifact).toEqual({
    mime_type: "audio/wav",
    size_bytes: String(bytes.length),
    content_hash: createHash("sha256").update(bytes).digest("hex"),
  });
  let square = 0,
    peak = 0,
    previous = 0,
    finite = true,
    stereoDifference = 0;
  const crossings = [],
    start = Math.floor(rate * 0.125),
    end = Math.floor(rate * 1.2);
  for (let i = 0; i < frames; i++) {
    const left = bytes.readFloatLE(58 + i * 8),
      right = bytes.readFloatLE(62 + i * 8);
    finite &&= Number.isFinite(left) && Number.isFinite(right);
    stereoDifference = Math.max(stereoDifference, Math.abs(left - right));
    if (i >= start && i < end) {
      square += left * left;
      peak = Math.max(peak, Math.abs(left));
      if (i > start && previous <= 0 && left > 0)
        crossings.push(i - 1 - previous / (left - previous));
    }
    previous = left;
  }
  const amplitude = 0.125 * 10 ** (-6 / 20),
    rms = Math.sqrt(square / (end - start));
  expect(finite).toBe(true);
  expect(stereoDifference).toBeLessThan(0.000001);
  expect(rms).toBeGreaterThan((amplitude / Math.sqrt(2)) * 0.85);
  expect(rms).toBeLessThan((amplitude / Math.sqrt(2)) * 1.15);
  expect(peak).toBeGreaterThan(amplitude * 0.8);
  expect(peak).toBeLessThan(amplitude * 1.2);
  expect(crossings.length).toBeGreaterThan(650);
  const frequency =
    ((crossings.length - 1) * rate) / (crossings.at(-1) - crossings[0]);
  expect(Math.abs(frequency - 660)).toBeLessThan(1);
}

async function verifyQueuedAudioWav(
  page,
  tool,
  fixed,
  sequenceId,
  detached,
  check,
) {
  const capability = await tool("opencut_project_v4_audio_capability", {}),
    required = !!(
      process.env.OPENCUT_CLASSIC_AUDIO_PACKAGE &&
      process.env.OPENCUT_CLASSIC_AUDIO_CHROMIUM
    );
  if (required)
    expect(
      capability.available,
      capability.unavailable_reason ?? "runtime",
    ).toBe(true);
  if (!capability.available) return; // Optional when this suite has no runtime.
  expect(capability.submit_tool).toBe("opencut_job_v4_audio_wav");
  expect(capability.sample_rates).toContain(48000);
  const projectId = fixed.project.id;
  const submit = async () => {
    await page
      .getByRole("button", { name: "Export audio WAV", exact: true })
      .click();
    const dialog = page.getByRole("dialog", {
      name: "Export audio WAV",
      exact: true,
    });
    const reset = dialog.getByRole("button", {
      name: "New audio export",
      exact: true,
    });
    if (await reset.isVisible()) await reset.click();
    await dialog
      .getByRole("combobox", { name: /^Sample rate/ })
      .selectOption("48000");
    const submission = page.waitForResponse((r) =>
      r.url().endsWith("/v1/tools/opencut_job_v4_audio_wav"),
    );
    await dialog
      .getByRole("button", { name: "Start audio export", exact: true })
      .click();
    const response = await submission,
      envelope = await response.json(),
      request = response.request().postDataJSON();
    expect(envelope.error).toBeNull();
    expect(request).toMatchObject({
      project_id: projectId,
      sequence_id: sequenceId,
      sample_rate: 48000,
    });
    expect(request.idempotency_key).toBeTruthy();
    expect(envelope.data.kind).toBe("render.project_audio.v4");
    return { dialog, request, job: envelope.data };
  };
  const waitTerminal = async (jobId) => {
    let job;
    await expect
      .poll(
        async () => {
          job = await tool("opencut_job_view", { job_id: jobId });
          return ["succeeded", "failed", "cancelled"].includes(job.state)
            ? job.state
            : "pending";
        },
        { timeout: 120000, intervals: [100, 250, 500] },
      )
      .not.toBe("pending");
    return job;
  };
  const submitted = await submit();
  expect(submitted.request.revision).toBe(fixed.revision);
  // Advance head after acceptance; the queued task must retain the selected
  // revision, not accidentally render the later scene/track mutation.
  const audioTrack = fixed.project.sequences
    .find((s) => s.id === sequenceId)
    .tracks.find((track) => track.elements.some((e) => e.id === detached.id));
  const slower = structuredClone(detached);
  slower.content.retime.rate = { numerator: 1, denominator: 10 };
  slower.timeline_range.duration = { ticks: "20", timebase: 1 };
  const changed = await tool("opencut_edit_v4_apply", {
    project_id: projectId,
    expected_revision: fixed.revision,
    idempotency_key: crypto.randomUUID(),
    label: "Extend real audio cancellation fixture",
    operations: [
      {
        type: "element_put",
        sequence_id: sequenceId,
        track_id: audioTrack.id,
        element: slower,
      },
    ],
  });
  const job = await waitTerminal(submitted.job.job_id);
  expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
  expect(job.target).toMatchObject({
    project_id: projectId,
    revision: fixed.revision,
    sequence_id: sequenceId,
  });
  expect(job.extended_result).toMatchObject({
    kind: "render.project_audio.v4",
    schema_version: 1,
  });
  const result = job.extended_result.data;
  expect(result.identity).toMatchObject({
    project_id: projectId,
    revision: fixed.revision,
    sequence_id: sequenceId,
    profile: "classic-export-v31-web-audio",
  });
  expect(result.execution).toMatchObject({
    mastering_profile: "classic-export-whole-scene-web-audio-v31",
    mastering_applied: false,
    mediabunny_version: "1.41.0",
    soundtouch_version: "0.3.0",
  });
  for (const hash of [
    result.render_input_sha256,
    result.execution.host_sha256,
    result.execution.browser_sha256,
    result.execution.module_sha256,
  ])
    expect(hash).toMatch(/^[a-f0-9]{64}$/);
  expect(result.execution.host_sha256).toBe(
    createHash("sha256")
      .update(await fs.readFile(process.env.OPENCUT_WEB_BINARY))
      .digest("hex"),
  );
  expect(job.artifacts).toHaveLength(1);
  expect(
    (await tool("opencut_job_v4_audio_wav", submitted.request)).job_id,
  ).toBe(job.job_id);
  await expect(
    submitted.dialog.getByRole("button", { name: "View tasks", exact: true }),
  ).toBeEnabled();
  await submitted.dialog
    .getByRole("button", { name: "View tasks", exact: true })
    .click();
  const tasks = page.getByRole("dialog", { name: "Tasks", exact: true });
  await tasks
    .getByRole("button", { name: "Refresh tasks", exact: true })
    .click();
  const card = tasks.locator(".task-card").filter({ hasText: fixed.revision });
  await expect(
    card.getByText("Audio WAV export", { exact: true }),
  ).toBeVisible();
  await expect(
    card.getByRole("button", { name: "Add to library", exact: true }),
  ).toHaveCount(0);
  // Replace only the OS save dialog, never the SDK, authenticated artifact
  // request or response bytes. The same product streaming download runs.
  await page.evaluate(() => {
    window.savedV4Wav = null;
    window.showSaveFilePicker = async () => ({
      createWritable: async () => {
        const chunks = [];
        return new WritableStream({
          write(chunk) {
            chunks.push(chunk);
          },
          close() {
            window.savedV4Wav = new Blob(chunks);
          },
        });
      },
    });
  });
  await card.getByRole("button", { name: "Download", exact: true }).click();
  await expect
    .poll(() => page.evaluate(() => window.savedV4Wav?.size ?? 0))
    .toBe(Number(result.artifact.size_bytes));
  const bytes = Buffer.from(
    await page.evaluate(async () =>
      Array.from(new Uint8Array(await window.savedV4Wav.arrayBuffer())),
    ),
  );
  verifyFloatWav(bytes, result);
  await page.evaluate(() => {
    delete window.savedV4Wav;
    delete window.showSaveFilePicker;
  });
  await tasks.getByRole("button", { name: "Close", exact: true }).click();
  await page.reload();
  const cancellation = await submit();
  expect(cancellation.request.revision).toBe(changed.revision);
  expect(cancellation.job.job_id).not.toBe(job.job_id);
  // A 20s stereo 48k Float32 output is 7,680,058 bytes, within host admission.
  // Request cancellation immediately after actual acceptance to avoid racing
  // the UI's normal task polling interval; this is the same public cancel tool.
  await tool("opencut_job_request_cancel", { job_id: cancellation.job.job_id });
  const cancelled = await waitTerminal(cancellation.job.job_id);
  expect(cancelled.state).toBe("cancelled");
  expect(cancelled.artifacts).toEqual([]);
  expect(cancelled.extended_result).toBeNull();
  expect(cancelled.result).toBeNull();
  await cancellation.dialog
    .getByRole("button", { name: "View tasks", exact: true })
    .click();
  await tasks
    .getByRole("button", { name: "Refresh tasks", exact: true })
    .click();
  const cancelledCard = tasks
    .locator(".task-card")
    .filter({ hasText: changed.revision });
  await expect(cancelledCard.locator(".state")).toHaveText("cancelled");
  await expect(
    cancelledCard.getByRole("button", { name: "Download", exact: true }),
  ).toHaveCount(0);
  await tasks.getByRole("button", { name: "Close", exact: true }).click();
  await verifyV4AudioRange(page, tool, fixed, sequenceId, bytes, result, waitTerminal, check);
  await verifyV4AudioPreview(page, tool, fixed, sequenceId, check);
  check(
    "real V4 WAV UI submission and authenticated Tasks download retain the original revision, validate 58-byte Float32 stereo header and independent 660Hz/-6dB PCM; cancelled task publishes no artifact",
    {
      audio_wav: result,
      cancellation: {
        state: cancelled.state,
        artifact_count: cancelled.artifacts.length,
      },
    },
  );
}

// Real authenticated HTTP, SQLite revisions and product controls. No transport
// response is mocked and no V4 document is projected through the old UI model.
export async function verifyV4Editor(browser, endpoint, directory, check) {
  const page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1100 },
  });
  page.setDefaultTimeout(30000);
  const errors = [],
    writes = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("request", (r) => {
    if (r.url().endsWith("/v1/tools/opencut_edit_v4_apply"))
      writes.push(r.postDataJSON());
  });
  const tool = async (name, input) => {
    const envelope = await page.evaluate(
      async ({ name, input }) =>
        (
          await fetch(`/v1/tools/${name}`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(input),
          })
        ).json(),
      { name, input },
    );
    expect(envelope.error).toBeNull();
    return envelope.data;
  };
  const create = async (button, name) => {
    await page.getByRole("button", { name: button, exact: true }).click();
    const dialog = page.getByRole("dialog", { name: button, exact: true });
    await dialog.getByLabel("Project name", { exact: true }).fill(name);
    await dialog
      .getByRole("button", { name: "Create project", exact: true })
      .click();
    await expect(page).toHaveURL(/\/editor\/.+/);
    const guide = page.getByRole("dialog", {
      name: "Welcome to OpenCut",
      exact: true,
    });
    if (await guide.isVisible())
      await guide.getByRole("button", { name: "Close", exact: true }).click();
    return decodeURIComponent(new URL(page.url()).pathname.split("/").at(-1));
  };
  const seek = async (value) => {
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill(value);
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
  };
  try {
    await page.goto(`${endpoint}/editor`);
    await expect(
      page.getByRole("button", { name: "New project", exact: true }),
    ).toBeEnabled();
    const legacyId = await create("New project", "Acceptance legacy survivor");
    await expect(
      page.getByRole("button", { name: "Export", exact: true }),
    ).toBeVisible();
    await page
      .getByRole("button", { name: "Back to projects", exact: true })
      .click();
    const id = await create(
      "New project with scene notes",
      "Acceptance scene notes",
    );
    await expect(page.getByRole("main", { name: "V4 editor" })).toBeVisible();
    const initial = await tool("opencut_project_v4_inspect", {
      project_id: id,
      revision: null,
    });
    const sequenceId = initial.project.default_sequence_id;
    await page
      .getByRole("button", { name: "Add bookmark", exact: true })
      .click();
    await page.getByLabel("Bookmark time (seconds)", { exact: true }).fill("1");
    await page
      .getByLabel("Note duration (seconds, optional)", { exact: true })
      .fill("2");
    await page
      .getByRole("textbox", { name: "Canvas note", exact: true })
      .fill("Saved scene note");
    await page.getByLabel("Bookmark color", { exact: true }).fill("#ff0000");
    await page
      .getByRole("button", { name: "Save bookmark", exact: true })
      .click();
    await expect(page.locator(".v4-marker")).toHaveCount(1);
    let state = await tool("opencut_project_v4_inspect", {
      project_id: id,
      revision: null,
    });
    const note = state.project.sequences[0].bookmarks[0];
    expect(note).toMatchObject({
      sequence_id: sequenceId,
      time: { ticks: "1", timebase: 1 },
      duration: { ticks: "2", timebase: 1 },
      color: "#ff0000",
      note: "Saved scene note",
    });
    expect(writes.at(-1).expected_revision).toBe(initial.revision);
    const savedRevision = state.revision;
    check(
      "V4 bookmark controls persist exact revision-owned time, duration, color and note through real HTTP/SQLite",
    );
    for (const [time, visible] of [
      ["1", true],
      ["3", true],
      ["3.000001", false],
    ]) {
      await seek(time);
      await expect(page.locator(".v4-canvas-note")).toHaveCount(
        visible ? 1 : 0,
      );
    }
    await page.locator(".v4-marker").click();
    await seek("4");
    await expect(page.locator(".v4-canvas-note")).toHaveCount(0);
    await page
      .getByRole("button", { name: "Move to playhead", exact: true })
      .click();
    await expect(page.locator(".v4-marker")).toHaveAttribute(
      "aria-label",
      /00:04/,
    );
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await page.getByRole("button", { name: "Apply undo", exact: true }).click();
    await expect(page.locator(".v4-marker")).toHaveAttribute(
      "aria-label",
      /00:01/,
    );
    await page.getByRole("button", { name: "Redo", exact: true }).click();
    await page.getByRole("button", { name: "Apply redo", exact: true }).click();
    await expect(page.locator(".v4-marker")).toHaveAttribute(
      "aria-label",
      /00:04/,
    );
    check(
      "inclusive HUD endpoints and selected-note hiding match Classic; move/undo/redo use shared revision transactions",
    );
    // Seed a non-decimal rational and second scene through the real public API,
    // then exercise note-only editing and scene switching in the product.
    state = await tool("opencut_project_v4_inspect", {
      project_id: id,
      revision: null,
    });
    const scene = structuredClone(state.project.sequences[0]);
    scene.id = "acceptance-second-scene";
    scene.name = "Second scene";
    scene.bookmarks = [];
    scene.main_track_id = "acceptance-second-track";
    scene.tracks = [
      {
        ...scene.tracks[0],
        id: scene.main_track_id,
        elements: [],
        channels: [],
      },
    ];
    await tool("opencut_edit_v4_apply", {
      project_id: id,
      expected_revision: state.revision,
      idempotency_key: crypto.randomUUID(),
      label: "Seed independent rational and scene",
      operations: [
        {
          type: "bookmark",
          operation: {
            kind: "put",
            bookmark: {
              ...note,
              time: { ticks: "1", timebase: 3 },
              duration: { ticks: "2", timebase: 3 },
            },
          },
        },
        { type: "sequence_put", sequence: scene },
      ],
    });
    await page.reload();
    await expect(page.locator(".v4-marker")).toHaveCount(1);
    await page.locator(".v4-marker").click();
    await page
      .getByRole("button", { name: "Edit bookmark", exact: true })
      .click();
    await page
      .getByRole("textbox", { name: "Canvas note", exact: true })
      .fill("Rational preserved");
    await page
      .getByRole("button", { name: "Save bookmark", exact: true })
      .click();
    await expect(page.locator(".v4-marker")).toHaveAttribute(
      "aria-label",
      /Rational preserved/,
    );
    state = await tool("opencut_project_v4_inspect", {
      project_id: id,
      revision: null,
    });
    expect(state.project.sequences[0].bookmarks[0]).toMatchObject({
      time: { ticks: "1", timebase: 3 },
      duration: { ticks: "2", timebase: 3 },
    });
    await page.getByLabel("V4 scene", { exact: true }).selectOption(scene.id);
    await expect(page.locator(".v4-marker")).toHaveCount(0);
    await page.getByLabel("V4 scene", { exact: true }).selectOption(sequenceId);
    await expect(page.locator(".v4-marker")).toHaveCount(1);
    // The saved revision can be inspected independently after subsequent edits.
    expect(
      (
        await tool("opencut_project_v4_inspect", {
          project_id: id,
          revision: savedRevision,
        })
      ).project.sequences[0].bookmarks[0],
    ).toEqual(note);
    await page
      .getByRole("button", { name: "View revision", exact: true })
      .first()
      .click();
    await expect(
      page.getByRole("button", { name: "Add bookmark", exact: true }),
    ).toBeDisabled();
    await page
      .getByRole("button", { name: "Return to current", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Add bookmark", exact: true }),
    ).toBeEnabled();
    check(
      "note-only editing preserves 1/3 and 2/3 rational clocks; scene switching and saved-revision read-only controls survive reload",
    );
    // Actual upload/probe, binding and exact placement all use the product UI.
    const imported = await importProductMedia(
      page,
      path.join(directory, "sample.mp4"),
      writes,
    );
    expect(imported.plan.decisions.bound_asset_ids).toHaveLength(1);
    await previewProductMedia(page, "sample.mp4", "video", 4);
    state = await tool("opencut_project_v4_inspect", {
      project_id: id,
      revision: null,
    });
    const sourceScene = state.project.sequences.find(
      (s) => s.id === sequenceId,
    );
    const assetId = imported.plan.decisions.bound_asset_ids[0];
    const placed = await placeProductMedia(
      page,
      "sample.mp4",
      {
        track: sourceScene.main_track_id,
        fields: {
          "Timeline start (seconds)": "0",
          "Source start (seconds)": "0.5",
          "Source duration (seconds)": "2",
          "Speed numerator": "3",
          "Speed denominator": "2",
        },
        keepPitch: false,
        sourceAudio: true,
      },
      writes,
    );
    expect(placed.plan.decisions.bound_asset_ids).toEqual([]);
    expect(placed.plan.decisions.first_visual_fps).toEqual({
      numerator: 24,
      denominator: 1,
    });
    state = await tool("opencut_project_v4_inspect", {
      project_id: id,
      revision: null,
    });
    let source = state.project.sequences
      .find((s) => s.id === sequenceId)
      .tracks.flatMap((t) => t.elements)
      .find((e) => e.id === placed.plan.decisions.placements[0].element_id);
    const videoId = source.id,
      videoName = source.name;
    const uploadedBytes = await fs.readFile(path.join(directory, "sample.mp4"));
    expect(
      state.project.assets.find((a) => a.id === assetId).version,
    ).toMatchObject({
      content_hash: createHash("sha256").update(uploadedBytes).digest("hex"),
      size: uploadedBytes.length,
    });
    check(
      "real V4 UI upload/probe, source video preview and repeated-asset planner placement preserve exact trim/rate in one reviewed transaction",
    );
    await verifyV4AudioGain(page, tool, writes, id, sequenceId, source, check);
    const beforeSeparate = await tool("opencut_project_v4_inspect", {
      project_id: id,
      revision: null,
    });
    source = beforeSeparate.project.sequences.find(s => s.id === sequenceId)
      .tracks.flatMap(t => t.elements).find(e => e.id === videoId);
    expect(source.content.audio.gain).toEqual({ unit: "decibels", value: -6 });
    expect(
      beforeSeparate.project.assets.find((a) => a.id === assetId).metadata.audio
        .status,
    ).toBe("present");
    expect(
      beforeSeparate.project.assets.find((a) => a.id === assetId).version,
    ).toEqual(state.project.assets.find((a) => a.id === assetId).version);
    await page.reload();
    const videoButton = page.getByRole("button", {
      name: `${videoName} · video`,
      exact: true,
    });
    await videoButton.click();
    const controls = page.getByRole("region", {
      name: "Source audio controls",
      exact: true,
    });
    const applySourceAction = async (label) => {
      const planning = page.waitForResponse((r) =>
        r.url().endsWith("/v1/tools/opencut_project_v4_source_audio_plan"),
      );
      const applying = page.waitForResponse((r) =>
        r.url().endsWith("/v1/tools/opencut_edit_v4_apply"),
      );
      await controls.getByRole("button", { name: label, exact: true }).click();
      const plan = await (await planning).json(),
        receipt = await (await applying).json();
      expect(plan.error).toBeNull();
      expect(receipt.error).toBeNull();
      expect(writes.at(-1)).toEqual(plan.data.transaction);
      await expect(
        controls.getByText("Source audio changes saved.", { exact: true }),
      ).toBeVisible();
      return { plan: plan.data, receipt: receipt.data };
    };
    const separate = await applySourceAction("Separate source audio");
    expect(separate.receipt.base_revision).toBe(beforeSeparate.revision);
    await expect(
      controls.getByRole("button", {
        name: "Restore source audio",
        exact: true,
      }),
    ).toBeEnabled();
    const afterSeparate = await tool("opencut_project_v4_inspect", {
      project_id: id,
      revision: null,
    });
    expect(afterSeparate.revision).toBe(separate.receipt.revision);
    const elements = afterSeparate.project.sequences
      .find((s) => s.id === sequenceId)
      .tracks.flatMap((t) => t.elements);
    const separatedSource = elements.find((e) => e.id === videoId);
    const detached = elements.find(
      (e) => e.id === separate.plan.decision.detached.element_id,
    );
    expect(separatedSource.content.source_audio_enabled).toBe(false);
    expect(detached.content).toEqual({
      kind: "audio",
      asset_id: assetId,
      audio: source.content.audio,
      retime: source.content.retime,
    });
    expect(detached.timeline_range).toEqual(source.timeline_range);
    expect(detached.source_span).toEqual(source.source_span);
    expect(detached.channels).toEqual(source.channels);
    expect(
      (
        await tool("opencut_project_v4_inspect", {
          project_id: id,
          revision: beforeSeparate.revision,
        })
      ).project,
    ).toEqual(beforeSeparate.project);
    await page.reload();
    await videoButton.click();
    await expect(
      controls.getByText(
        "Restoring source audio keeps separated audio in the scene. Both may play together.",
        { exact: true },
      ),
    ).toBeVisible();
    const restore = await applySourceAction("Restore source audio");
    expect(restore.receipt.base_revision).toBe(afterSeparate.revision);
    await expect(
      controls.getByRole("button", {
        name: "Separate source audio",
        exact: true,
      }),
    ).toBeEnabled();
    const afterRestore = await tool("opencut_project_v4_inspect", {
      project_id: id,
      revision: null,
    });
    const restored = afterRestore.project.sequences
      .find((s) => s.id === sequenceId)
      .tracks.flatMap((t) => t.elements);
    expect(
      restored.find((e) => e.id === videoId).content.source_audio_enabled,
    ).toBe(true);
    expect(restored.find((e) => e.id === detached.id)).toEqual(detached);
    expect(
      (
        await tool("opencut_project_v4_inspect", {
          project_id: id,
          revision: afterSeparate.revision,
        })
      ).project,
    ).toEqual(afterSeparate.project);
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await page.getByRole("button", { name: "Apply undo", exact: true }).click();
    await expect(
      controls.getByRole("button", {
        name: "Restore source audio",
        exact: true,
      }),
    ).toBeEnabled();
    const afterUndo = await tool("opencut_project_v4_inspect", {
      project_id: id,
      revision: null,
    });
    const undone = afterUndo.project.sequences
      .find((s) => s.id === sequenceId)
      .tracks.flatMap((t) => t.elements);
    expect(
      undone.find((e) => e.id === videoId).content.source_audio_enabled,
    ).toBe(false);
    expect(undone.find((e) => e.id === detached.id)).toEqual(detached);
    check(
      "actual uploaded/probed video: UI Separate preserves source span, retime and -6dB gain at fixed revisions; reload/Restore keeps detached audio; selective Undo restores the disabled source",
    );
    await verifyQueuedAudioWav(
      page,
      tool,
      afterUndo,
      sequenceId,
      detached,
      check,
    );
    await verifyMediaRecoveryAndReplacement(
      page,
      tool,
      directory,
      id,
      scene.id,
      writes,
      check,
    );
    await verifyV4Timeline(
      page,
      tool,
      await tool("opencut_project_v4_inspect", { project_id: id, revision: null }),
      scene.id,
      check,
    );
    await verifyV4TimelineGestures(
      page,
      tool,
      await tool("opencut_project_v4_inspect", { project_id: id, revision: null }),
      scene.id,
      check,
    );
    await verifyV4Clipboard(
      page,
      tool,
      await tool("opencut_project_v4_inspect", { project_id: id, revision: null }),
      scene.id,
      check,
    );
    await page
      .getByRole("button", { name: "Back to projects", exact: true })
      .click();
    await expect(
      page
        .locator(".project-card")
        .filter({ hasText: "Acceptance legacy survivor" }),
    ).toBeVisible();
    await expect(
      page
        .locator(".project-card")
        .filter({ hasText: "Acceptance scene notes" }),
    ).toBeVisible();
    await page
      .locator(".project-card")
      .filter({ hasText: "Acceptance legacy survivor" })
      .click();
    await expect
      .poll(() => new URL(page.url()).pathname)
      .toBe(`/editor/${legacyId}`);
    await expect(
      page.getByRole("button", { name: "Export", exact: true }),
    ).toBeVisible();
    expect(errors).toEqual([]);
    check(
      "mixed V3/V4 project library retains the old project and reopens its existing editor/export surface without browser exceptions",
    );
  } finally {
    await page.close();
  }
}
