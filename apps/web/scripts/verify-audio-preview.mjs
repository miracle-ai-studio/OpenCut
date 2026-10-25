import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import { verifyAudioPreviewGesture } from "./verify-audio-preview-gesture.mjs";

// Independent wire/PCM observations through the production HTTP producer. The
// downloaded pre-master block is not reported as device-consumed presentation.
export async function verifyAudioPreview(page, tool, fixed, sceneId, check) {
  const capability = await tool("opencut_audio_preview_capability", {});
  expect(capability.available, capability.unavailable_reason ?? "audio preview runtime").toBe(true);
  expect(capability.max_buffered_frames).toBe(8192);
  const scene = fixed.project.sequences.find(s => s.id === sceneId);
  const track = scene.tracks.find(t => t.elements.some(e => e.content.kind === "audio"));
  const element = track.elements.find(e => e.content.kind === "audio");
  expect(element.content.audio.gain).toEqual({ unit: "decibels", value: -6 });
  const before = await editorSnapshot(tool("opencut_project_inspect", { project_id: fixed.project.id, revision: null }));
  let session = await tool("opencut_audio_preview_create", { project_id: fixed.project.id, revision: fixed.revision,
    sequence_id: sceneId, sample_rate: 48000, start_time: { ticks: "1", timebase: 5 }, idempotency_key: crypto.randomUUID() });
  expect(session.identity.revision).toBe(fixed.revision);
  const control = async command => {
    session = await tool("opencut_audio_preview_control", { session_id: session.session_id, generation: session.generation,
      owner_id: session.owner_id, lease_id: session.lease_id, control_id: crypto.randomUUID(), command });
    return session;
  };
  const block = async () => {
    await control({ kind: "play" });
    const value = await page.evaluate(async s => {
      const response = await fetch("/api/preview/audio/stream", { method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ session_id: s.session_id, generation: s.generation, overlay_epoch: s.overlay_epoch, owner_id: s.owner_id, lease_id: s.lease_id }) });
      if (!response.ok) throw new Error(await response.text());
      if (response.headers.get("content-type") !== "application/vnd.opencut.audio-preview") throw new Error("Wrong PCM transport");
      const reader = response.body.getReader(); let bytes = new Uint8Array();
      try {
        for (;;) {
          const chunk = await reader.read(); if (chunk.done) throw new Error("Early PCM EOF");
          const joined = new Uint8Array(bytes.length + chunk.value.length); joined.set(bytes); joined.set(chunk.value, bytes.length); bytes = joined;
          if (bytes.length < 12) continue;
          const view = new DataView(bytes.buffer);
          if (String.fromCharCode(...bytes.subarray(0, 4)) !== "OCAP") throw new Error("Wrong PCM prefix");
          const h = view.getUint32(4, true), p = view.getUint32(8, true);
          if (h > 4096 || p > 32768 || !p) throw new Error("Unbounded PCM frame");
          if (bytes.length < 12 + h + p) continue;
          const header = JSON.parse(new TextDecoder().decode(bytes.subarray(12, 12 + h)));
          const payload = bytes.slice(12 + h, 12 + h + p);
          const hash = [...new Uint8Array(await crypto.subtle.digest("SHA-256", payload))].map(x => x.toString(16).padStart(2, "0")).join("");
          if (hash !== header.content_hash || p !== header.frame_count * 8) throw new Error("PCM integrity mismatch");
          const pcm = new DataView(payload.buffer); const channels = [[], []];
          for (let i = 0; i < header.frame_count; i++) for (let c = 0; c < 2; c++) channels[c].push(pcm.getFloat32(i * 8 + c * 4, true));
          return { header, channels };
        }
      } finally { await reader.cancel(); }
    }, session);
    expect(value.header.identity).toEqual(session.identity);
    expect(value.header).toMatchObject({ generation: session.generation, overlay_epoch: session.overlay_epoch, start_sample: "9600", sequence: "0", frame_count: 4096 });
    const observed = await tool("opencut_audio_preview_inspect", { session_id: session.session_id });
    expect(observed.consumed_frames).toBe("0");
    expect(observed.presentation_fresh).toBe(false);
    expect(BigInt(observed.buffered_frames)).toBeLessThanOrEqual(8192n);
    session = observed;
    return value;
  };
  try {
    const first = await block();
    const left = first.channels[0], right = first.channels[1];
    expect(left.every(Number.isFinite) && right.every(Number.isFinite)).toBe(true);
    expect(Math.max(...left.map((v, i) => Math.abs(v - right[i])))).toBeLessThan(1e-6);
    const rms = Math.sqrt(left.reduce((s, v) => s + v * v, 0) / left.length);
    const crossings = []; for (let i = 1; i < left.length; i++) if (left[i - 1] <= 0 && left[i] > 0) crossings.push(i - 1 - left[i - 1] / (left[i] - left[i - 1]));
    const frequency = (crossings.length - 1) * 48000 / (crossings.at(-1) - crossings[0]);
    expect(Math.abs(frequency - 660)).toBeLessThan(1);
    const expected = .125 * 10 ** (-6 / 20) / Math.sqrt(2);
    expect(rms).toBeGreaterThan(expected * .85); expect(rms).toBeLessThan(expected * 1.15);
    await control({ kind: "seek", sequence_time: { ticks: "1", timebase: 5 } });
    const repeated = await block();
    expect(repeated.channels).toEqual(first.channels);
    await control({ kind: "set_temporary_gain", gain: { track_id: track.id, element_id: element.id, decibels_tenths: -120,
      gesture_id: "preview-independent-gain", update_sequence: "1" } });
    await control({ kind: "seek", sequence_time: { ticks: "1", timebase: 5 } });
    const quieter = await block();
    const ratio = 10 ** (-6 / 20);
    expect(Math.max(...quieter.channels[0].map((v, i) => Math.abs(v - left[i] * ratio)))).toBeLessThan(1e-7);
    expect((await editorSnapshot(tool("opencut_project_inspect", { project_id: fixed.project.id, revision: null }))).revision).toBe(before.revision);
    check("real fixed-revision audio stream: independent660Hz/-6dB pre-master stereo; seek repeats exactPCM; temporary-12dB participates before mixing without edits; two-block backpressure never advances consumerTime", { preview: { identity: session.identity, frequency_hz: frequency, rms, seek_hash: first.header.content_hash, temporary_gain_hash: quieter.header.content_hash } });
  } finally { await control({ kind: "stop" }); }

  // A separate synthetic project exercises the actual prepared/pitch branch;
  // no original project revision is rewritten to build its expected signal.
  const copied = await tool("opencut_project_copy", { source_project_id: fixed.project.id, source_revision: fixed.revision,
    project_id: "preview-pitch-" + crypto.randomUUID(), name: "Independent preview pitch fixture", idempotency_key: crypto.randomUUID() });
  const pitchElement = structuredClone(element); pitchElement.content.retime.maintain_pitch = true;
  const committed = await tool("opencut_edit_apply", { project_id: copied.project.id, expected_revision: copied.revision,
    idempotency_key: crypto.randomUUID(), label: "Synthetic prepared preview acceptance", operations: [
      { type: "element_put", sequence_id: sceneId, track_id: track.id, element: pitchElement },
    ] });
  session = await tool("opencut_audio_preview_create", { project_id: copied.project.id, revision: committed.revision,
    sequence_id: sceneId, sample_rate: 48000, start_time: { ticks: "1", timebase: 5 }, idempotency_key: crypto.randomUUID() });
  try {
    const prepared = await block(), left = prepared.channels[0];
    const crossings = []; for (let i = 1; i < left.length; i++) if (left[i - 1] <= 0 && left[i] > 0) crossings.push(i - 1 - left[i - 1] / (left[i] - left[i - 1]));
    const frequency = (crossings.length - 1) * 48000 / (crossings.at(-1) - crossings[0]);
    expect(Math.abs(frequency - 440)).toBeLessThan(3);
    await control({ kind: "seek", sequence_time: { ticks: "1", timebase: 5 } });
    expect((await block()).channels).toEqual(prepared.channels);
    check("real prepared realtime branch: shared pinned SoundTouch preserves independent440Hz under1.5x retime and seek reuses exact preparedPCM", { prepared_preview: { identity: session.identity, frequency_hz: frequency, seek_hash: prepared.header.content_hash } });
  } finally { await control({ kind: "stop" }); }

  await verifyAudioPreviewOutput(page, tool, fixed, sceneId, check);
  await verifyAudioPreviewGesture(page, tool, fixed, sceneId, check);
}

export async function verifyAudioPreviewOutput(page, tool, fixed, sceneId, check) {
  const before = await editorSnapshot(tool("opencut_project_inspect", { project_id: fixed.project.id, revision: null }));
  // Exercise the product's AudioContext + AudioWorklet. Only consumption reports
  // emitted by the actual Worklet count as presentation, never downloaded PCM.
  await page.reload();
  const history = await tool("opencut_project_history", { project_id: fixed.project.id, limit: 50 });
  const index = history.revisions.findIndex(r => r.revision === fixed.revision);
  expect(index).toBeGreaterThanOrEqual(0);
  await page.locator(".editor-revisions > li").nth(index).getByRole("button", { name: "View revision", exact: true }).click();
  // Loading a fixed revision is asynchronous. Wait for that view to commit
  // before changing scenes, which deliberately cancels an older view request.
  await expect(page.getByRole("button", { name: "Return to current", exact: true })).toBeVisible();
  await page.getByLabel("Scene", { exact: true }).selectOption(sceneId);
  await page.getByLabel("Playhead (seconds)", { exact: true }).fill("0.1");
  await page.getByRole("button", { name: "Go to time", exact: true }).click();
  await expect(page.getByRole("button", { name: "Play scene audio", exact: true })).toBeEnabled();
  const consumerReports = [];
  const observe = response => { if (response.url().endsWith("/api/tools/opencut_audio_preview_control")) {
    const request = response.request().postDataJSON();
    if (request.command?.kind === "report_consumed") consumerReports.push(response.json());
  } };
  page.on("response", observe);
  try {
    const created = page.waitForResponse(r => r.url().endsWith("/api/tools/opencut_audio_preview_create"));
    await page.getByRole("button", { name: "Play scene audio", exact: true }).click();
    const envelope = await (await created).json(); expect(envelope.error).toBeNull();
    expect(envelope.data.identity.revision).toBe(fixed.revision);
    await expect.poll(async () => (await Promise.all(consumerReports)).some(r => r.data && BigInt(r.data.consumed_frames) > 0n), { timeout: 30000 }).toBe(true);
    await page.getByRole("button", { name: "Pause scene audio", exact: true }).click();
    await expect(page.getByText("Scene sound paused", { exact: true })).toBeVisible();
    const paused = await tool("opencut_audio_preview_inspect", { session_id: envelope.data.session_id });
    expect(paused.state).toBe("paused");
    const reports = (await Promise.all(consumerReports)).filter(r => !r.error && r.data);
    const final = reports.at(-1).data;
    expect(paused.presentation_time).toEqual(final.presentation_time);
    expect(BigInt(final.consumed_frames)).toBeGreaterThan(0n);
    const t = final.presentation_time; expect(Number(t.ticks) / t.timebase).toBeGreaterThan(.1);
    const resumed = page.waitForResponse(r => r.url().endsWith("/api/tools/opencut_audio_preview_control") && r.request().postDataJSON().command?.kind === "play");
    await page.getByRole("button", { name: "Play scene audio", exact: true }).click();
    const resume = await (await resumed).json(); expect(resume.error).toBeNull();
    expect(resume.data.session_id).toBe(paused.session_id);
    expect(BigInt(resume.data.generation)).toBeGreaterThan(BigInt(paused.generation));
    await expect.poll(async () => (await Promise.all(consumerReports)).some(r => r.data?.generation === resume.data.generation && BigInt(r.data.consumed_frames) > 0n), { timeout: 10000 }).toBe(true);
    await page.getByRole("button", { name: "Pause scene audio", exact: true }).click();
    await expect(page.getByText("Scene sound paused", { exact: true })).toBeVisible();
    expect((await editorSnapshot(tool("opencut_project_inspect", { project_id: fixed.project.id, revision: null }))).revision).toBe(before.revision);
    check("actual product AudioWorklet consumes productionPCM; Pause flushes and confirms final sampleTime on the same historical revision with zero edit writes", { audio_consumer: { identity: paused.identity, final_consumed_frames: final.consumed_frames, presentation_time: paused.presentation_time, state: paused.state } });
  } finally {
    page.off("response", observe);
    const returnButton = page.getByRole("button", { name: "Return to current", exact: true });
    if (await returnButton.isVisible()) await returnButton.click();
  }
}
