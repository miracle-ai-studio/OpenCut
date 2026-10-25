import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash, randomUUID } from "node:crypto";

// Root owns the single synthetic workspace, model/server and serial browser.
// No media import, project creation or fabricated transcript is performed here.
const normalize = (text, language) => {
  const cleaned = text.normalize("NFKC").toLowerCase().replace(/[^\p{L}\p{N}\s]/gu, " ").trim();
  return language === "zh" ? [...cleaned.replace(/\s+/gu, "")] : cleaned.split(/\s+/u).filter(Boolean);
};
const distance = (reference, actual) => {
  let row = Array.from({ length: actual.length + 1 }, (_, i) => i);
  for (let i = 0; i < reference.length; i++) {
    const next = [i + 1];
    for (let j = 0; j < actual.length; j++) next.push(Math.min(next[j] + 1, row[j + 1] + 1, row[j] + Number(reference[i] !== actual[j])));
    row = next;
  }
  return row.at(-1);
};
const seconds = (time) => Number(time.ticks) / time.timebase;
const parseSrt = (content) => content.replace(/\r\n/g, "\n").trim().split(/\n\s*\n/u).map(block => {
  const [index, timing, ...text] = block.split("\n");
  const match = /^(\d{2,}):(\d{2}):(\d{2}),(\d{3}) --> (\d{2,}):(\d{2}):(\d{2}),(\d{3})$/u.exec(timing);
  expect(index).toMatch(/^\d+$/u); expect(match, timing).toBeTruthy();
  const timestamp = offset => Number(match[offset]) * 3600 + Number(match[offset + 1]) * 60 + Number(match[offset + 2]) + Number(match[offset + 3]) / 1000;
  return { text: text.join("\n"), start: timestamp(1), end: timestamp(5) };
});

export async function verifyAsr(browser, endpoint, directory, check) {
  const fixture = JSON.parse(await fs.readFile(path.join(directory, "b3-asr-fixture.json"), "utf8"));
  const { project_id: projectId, sequence_id: sequenceId, original_revision: originalRevision, samples } = fixture;
  expect(samples.map(sample => sample.language).sort()).toEqual(["en", "zh"]);
  for (const sample of samples) {
    expect(sample.expected_text).toBeTruthy(); expect(sample.revision).toBeTruthy();
    expect(sample.track_id).toBeTruthy(); expect(sample.font_id).toBeTruthy();
    expect(sample.model_sha256 ?? fixture.model_sha256).toMatch(/^[a-f0-9]{64}$/u);
  }
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
  page.setDefaultTimeout(30000);
  const evidence = [], network = [], errors = [];
  let phase = "open", changed = false;
  let checkpoint = {};
  const saveCheckpoint = async (facts = {}) => {
    checkpoint = { ...checkpoint, ...facts };
    await fs.writeFile(path.join(directory, "b3-asr-checkpoint.json"), JSON.stringify({ project_id: projectId, sequence_id: sequenceId, original_revision: originalRevision, phase, ...checkpoint, evidence }, null, 2) + "\n");
  };
  page.on("pageerror", error => { if (errors.length < 32) errors.push(error.message.slice(0, 1024)); });
  page.on("request", request => {
    const operation = new URL(request.url()).pathname.split("/").at(-1);
    if (operation.startsWith("opencut_") && network.length < 256) network.push({ operation, method: request.method() });
  });
  const tool = async (name, input) => {
    const envelope = await page.evaluate(async ({ name, input }) => (await fetch(`/api/tools/${name}`, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input),
    })).json(), { name, input });
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull(); return envelope.data;
  };
  const inspect = () => editorSnapshot(tool("opencut_project_inspect", { project_id: projectId, revision: null }));
  const scene = snapshot => snapshot.project.sequences.find(sequence => sequence.id === sequenceId);
  const restore = async revision => {
    const head = await inspect();
    await tool("opencut_project_restore", { project_id: projectId, expected_revision: head.revision, target_revision: revision, idempotency_key: randomUUID(), asset_versions: {} });
    return inspect();
  };
  const responseFor = name => page.waitForResponse(response => response.url().endsWith(`/api/tools/${name}`));
  const exchange = async (name, action) => {
    const pending = responseFor(name); await action(); const response = await pending, envelope = await response.json();
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull();
    return { request: response.request().postDataJSON(), result: envelope.data };
  };
  const reopen = async () => {
    await page.goto(`${endpoint}/editor/${encodeURIComponent(projectId)}`);
    await expect(page.getByRole("main", { name: "OpenCut editor", exact: true })).toBeVisible();
    const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
    if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
    await page.getByRole("combobox", { name: "Scene", exact: true }).selectOption(sequenceId);
  };
  try {
    await reopen();
    const panel = page.locator(".editor-caption-workspace");
    const capability = await tool("opencut_analysis_sequence_transcribe_capability", {});
    expect(capability.available, capability.unavailable_reason).toBe(true);
    for (const sample of samples) {
      phase = `${sample.language}: restore synthetic speech`;
      changed = true;
      const fixed = await restore(sample.revision), fixedScene = scene(fixed);
      checkpoint = {};
      await saveCheckpoint({ sample, active_revision: fixed.revision });
      const track = fixedScene.tracks.find(track => track.id === sample.track_id);
      expect(track).toMatchObject({ kind: "caption", enabled: true, locked: false });
      expect(fixedScene.captions.filter(cue => cue.track_id === track.id)).toEqual([]);
      expect(fixed.project.fonts.some(font => font.id === sample.font_id)).toBe(true);
      await reopen();
      await panel.getByRole("combobox", { name: /^Caption track(?:\s|$)/ }).selectOption(track.id);
      await panel.getByRole("combobox", { name: /^Pinned font(?:\s|$)/ }).selectOption(sample.font_id);
      await panel.getByRole("combobox", { name: /^Speech language(?:\s|$)/ }).selectOption(sample.language);
      phase = `${sample.language}: actual local inference`;
      const submitted = await exchange("opencut_analysis_sequence_transcribe", () => panel.getByRole("button", { name: "Transcribe saved scene", exact: true }).click());
      await saveCheckpoint({ submission: submitted, job_id: submitted.result.job_id });
      expect(submitted.request).toMatchObject({ project_id: projectId, revision: fixed.revision, sequence_id: sequenceId, language: sample.language });
      expect(submitted.request.idempotency_key).toBeTruthy();
      expect(submitted.result).toMatchObject({ kind: "analysis.sequence_transcribe", target: { project_id: projectId, revision: fixed.revision, sequence_id: sequenceId } });
      let job;
      await expect.poll(async () => {
        job = await tool("opencut_job_view", { job_id: submitted.result.job_id });
        return ["succeeded", "failed", "cancelled"].includes(job.state) ? job.state : "pending";
      }, { timeout: 600000, intervals: [500, 1000, 2000] }).not.toBe("pending");
      await saveCheckpoint({ terminal_job: job });
      expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
      expect(job.result).toMatchObject({ kind: "analysis.sequence_transcribe", schema_version: 1 });
      const result = job.result.data;
      expect(result).toMatchObject({ project_id: projectId, revision: fixed.revision, sequence_id: sequenceId });
      expect(result.audio.identity).toMatchObject({ project_id: projectId, revision: fixed.revision, sequence_id: sequenceId });
      expect(result.transcription).toMatchObject({ provider: "whisper.cpp", language: sample.language, model_sha256: sample.model_sha256 ?? fixture.model_sha256 });
      expect(result.transcription.model).toBeTruthy();
      const actualText = result.transcription.segments.map(segment => segment.text).join(" ");
      const reference = normalize(sample.expected_text, sample.language), actual = normalize(actualText, sample.language);
      expect(reference.length).toBeGreaterThan(0); expect(actual.length).toBeGreaterThan(0);
      const editDistance = distance(reference, actual), rate = editDistance / reference.length, threshold = sample.language === "en" ? .2 : .25;
      const accuracy = { metric: sample.language === "en" ? "WER" : "CER", normalization: "NFKC lowercase; retain Unicode letters/numbers; English whitespace words / Chinese non-whitespace codepoints", expected_text: sample.expected_text, actual_text: actualText, reference_units: reference.length, actual_units: actual.length, edit_distance: editDistance, rate, threshold };
      evidence.push({ language: sample.language, case: phase, synthetic_source: sample.speech_source ?? null, fixture_revision: sample.revision, active_revision: fixed.revision, request: submitted.request, accepted: submitted.result, job_id: job.job_id, result, accuracy });
      await saveCheckpoint({ inference_result: result, accuracy });
      expect(rate, JSON.stringify(accuracy)).toBeLessThanOrEqual(threshold);
      expect((await inspect()).revision).toBe(fixed.revision);
      phase = `${sample.language}: same original key replay`;
      const replay = await tool("opencut_analysis_sequence_transcribe", submitted.request);
      await saveCheckpoint({ replay });
      expect(replay.job_id).toBe(job.job_id);
      const replayedJob = await tool("opencut_job_view", { job_id: replay.job_id });
      expect(replayedJob.state).toBe("succeeded"); expect(replayedJob.result).toEqual(job.result);
      expect((await inspect()).revision).toBe(fixed.revision);
      evidence.push({ language: sample.language, case: phase, request: submitted.request, original_job_id: job.job_id, replay_job_id: replay.job_id, evidence_scope: "Original public idempotency request returns the same completed job and persisted result; no second inference job submitted." });
      await panel.getByRole("button", { name: "Refresh tasks", exact: true }).click();
      await panel.getByRole("combobox", { name: /^Scene transcription(?:\s|$)/ }).selectOption(job.job_id);
      phase = `${sample.language}: caption original-intent recovery`;
      const stagedPromise = responseFor("opencut_edit_recovery_stage"), ackPromise = responseFor("opencut_edit_recovery_ack");
      const generated = await exchange("opencut_subtitle_generate", () => panel.getByRole("button", { name: "Generate captions", exact: true }).click());
      await saveCheckpoint({ generation: generated });
      const stagedResponse = await stagedPromise, staged = await stagedResponse.json(), ack = await (await ackPromise).json();
      await saveCheckpoint({ recovery: { staged, ack } });
      expect(staged.error).toBeNull(); expect(ack.error).toBeNull();
      expect(generated.request).toMatchObject({ project_id: projectId, expected_revision: fixed.revision, sequence_id: sequenceId, track_id: track.id, analysis_job_id: job.job_id, style: { layout: "pinned_caption_v4", font: { kind: "locked", font_id: sample.font_id } } });
      expect(stagedResponse.request().postDataJSON()).toEqual({ context: { sequence_id: sequenceId, intent: { kind: "subtitle_generate", request: generated.request } } });
      expect(ack.data.receipt.caption).toEqual(generated.result);
      const committed = await inspect(); expect(committed.revision).toBe(generated.result.transaction.revision);
      expect(scene(committed).tracks).toEqual(fixedScene.tracks);
      const cues = scene(committed).captions.filter(cue => cue.track_id === track.id).sort((a, b) => seconds(a.timeline_range.start) - seconds(b.timeline_range.start));
      expect(cues.length).toBeGreaterThan(0); expect(cues.length).toBe(generated.result.cue_count);
      expect(normalize(cues.map(cue => cue.text).join(" "), sample.language)).toEqual(actual);
      for (const cue of cues) {
        expect(cue.style).toEqual(generated.request.style);
        expect(seconds(cue.timeline_range.start)).toBeGreaterThanOrEqual(0);
        expect(seconds(cue.timeline_range.duration)).toBeGreaterThan(0);
        expect(seconds(cue.timeline_range.start) + seconds(cue.timeline_range.duration)).toBeLessThanOrEqual(seconds(result.duration) + .001);
      }
      await saveCheckpoint({ committed_revision: committed.revision, cues });
      await reopen(); expect(scene(await inspect()).captions).toEqual(scene(committed).captions);
      await panel.getByRole("combobox", { name: /^Caption track(?:\s|$)/ }).selectOption(track.id);
      phase = `${sample.language}: actual downloaded SRT`;
      await panel.getByRole("combobox", { name: /^Format(?:\s|$)/ }).selectOption("srt");
      const downloadPromise = page.waitForEvent("download");
      const exported = await exchange("opencut_subtitle_export", () => panel.getByRole("button", { name: "Export subtitles", exact: true }).click());
      const download = await downloadPromise, saved = path.join(directory, `b3-asr-${sample.language}.srt`);
      await download.saveAs(saved); const bytes = await fs.readFile(saved);
      expect(bytes.length).toBeLessThanOrEqual(2 * 1024 * 1024);
      expect(bytes.toString("utf8")).toBe(exported.result.content);
      expect(exported.request).toEqual({ project_id: projectId, revision: committed.revision, sequence_id: sequenceId, track_id: track.id, format: "srt" });
      await saveCheckpoint({ export: exported, downloaded_file: saved, downloaded_sha256: createHash("sha256").update(bytes).digest("hex") });
      const srtCues = parseSrt(bytes.toString("utf8")); expect(srtCues.length).toBe(cues.length);
      srtCues.forEach((cue, index) => {
        expect(cue.text).toBe(cues[index].text);
        expect(Math.abs(cue.start - seconds(cues[index].timeline_range.start))).toBeLessThanOrEqual(.0011);
        expect(Math.abs(cue.end - seconds(cues[index].timeline_range.start) - seconds(cues[index].timeline_range.duration))).toBeLessThanOrEqual(.0011);
      });
      evidence.push({ language: sample.language, case: "generated captions persisted and exported", generation: generated, recovery_id: staged.data.recovery_id, acknowledged_receipt: ack.data.receipt, committed_revision: committed.revision, cues, export: { request: exported.request, cue_count: exported.result.cue_count, losses: exported.result.losses, sha256: createHash("sha256").update(bytes).digest("hex"), size: bytes.length } });
    }
    await saveCheckpoint({ samples_completed: samples.length });
    phase = "restore original owned revision";
    const restored = await restore(originalRevision); changed = false;
    expect(errors).toEqual([]);
    const report = { status: "passed", project_id: projectId, sequence_id: sequenceId, original_revision: originalRevision, final_revision: restored.revision, scope: "Synthetic English/Chinese local speech inference through real UI, fixed-revision audio/model identity, public same-key completed-job replay, V3 caption recovery, persistence and downloaded SRT. Human English/Chinese speech remains pending.", evidence, errors, bounded_network: network };
    await fs.writeFile(path.join(directory, "b3-asr.json"), JSON.stringify(report, null, 2) + "\n");
    check("B3 synthetic bilingual actual local ASR and caption recovery/SRT", report);
  } catch (error) {
    await saveCheckpoint({ failed: true }).catch(() => {});
    const failure = { checkpoint, status: "failed", phase, project_id: projectId, error: String(error).slice(0, 4096), errors, network, evidence };
    try { failure.ui = await page.evaluate(() => [...document.querySelectorAll('[role="alert"],[role="status"]')].slice(0, 24).map(node => node.textContent.slice(0, 1024))); } catch {}
    if (changed) {
      try { failure.restored_revision = (await restore(originalRevision)).revision; changed = false; }
      catch (restoreError) { failure.restore_error = String(restoreError).slice(0, 2048); }
    }
    await fs.writeFile(path.join(directory, "b3-asr-failure.json"), JSON.stringify(failure, null, 2) + "\n");
    await page.screenshot({ path: path.join(directory, "b3-asr-failure.png"), fullPage: false }).catch(() => {});
    throw error;
  } finally { await page.close(); }
}
