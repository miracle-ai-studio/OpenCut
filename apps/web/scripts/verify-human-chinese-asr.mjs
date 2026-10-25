import { editorSnapshot } from "./project-inspection.mjs";
/** One frozen, real Mandarin reference. Never retry inference to chase accuracy.
 * Keep original jobs/requests, distinguish inference from CER, restore fixture. */
import { chromium, expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { OpenCutClient } from "../../../packages/sdk-ts/dist/index.js";
const args = Object.fromEntries(
  process.argv
    .slice(2)
    .reduce((a, v, i, s) => (i % 2 ? a : [...a, [v.slice(2), s[i + 1]]]), []),
);
const source = JSON.parse(await fs.readFile(args.source, "utf8")),
  policy = JSON.parse(await fs.readFile(source.selection_policy, "utf8"));
const checkpoint = path.join(
  args.workspace,
  "agent-human-chinese-asr-v4-checkpoint.json",
);
const state = await fs
  .readFile(checkpoint, "utf8")
  .then(JSON.parse)
  .catch((e) => {
    if (e.code !== "ENOENT") throw e;
    return {
      candidate: args.candidate,
      source_sha256: source.sha256,
      stages: {},
      requests: {},
    };
  });
expect(state.candidate).toBe(args.candidate);
expect(state.source_sha256).toBe(source.sha256);
const done = await fs
  .readFile(args.report, "utf8")
  .then(JSON.parse)
  .catch((e) => {
    if (e.code !== "ENOENT") throw e;
    return null;
  });
if (done?.original_project_restored && done.candidate === args.candidate) {
  console.log("Reuse original Chinese observation: " + done.status);
  process.exit(0);
}
const save = async () => {
  await fs.writeFile(
    checkpoint + ".pending",
    JSON.stringify(state, null, 2) + "\n",
  );
  await fs.rename(checkpoint + ".pending", checkpoint);
};
const stage = async (name, run) => {
  if (state.stages[name]) return state.stages[name];
  console.log("Begin " + name);
  const value = await run();
  state.stages[name] = value;
  await save();
  console.log("Passed " + name);
  return value;
};
const token = (await fs.readFile(args.credential, "utf8")).trim(),
  client = new OpenCutClient({ baseUrl: args.endpoint, token });
const call = (tool, input) => client.call(tool, input),
  project = "fb92adbb-b999-414e-b351-35f012b8e7b8",
  inspect = () =>
    editorSnapshot(call("opencut_project_inspect", { project_id: project, revision: null }));
const sha = (b) => createHash("sha256").update(b).digest("hex");
const normalize = (s) =>
  [...s.normalize("NFKC").toLowerCase()].filter((c) => /[\p{L}\p{N}]/u.test(c));
const distance = (a, b) => {
  let row = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i++) {
    const next = [i];
    for (let j = 1; j <= b.length; j++)
      next[j] = Math.min(
        next[j - 1] + 1,
        row[j] + 1,
        row[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1),
      );
    row = next;
  }
  return row[b.length];
};
const waitJob = async (jobId, timeout = 240000) => {
  let view;
  const until = Date.now() + timeout;
  do {
    view = await call("opencut_job_view", { job_id: jobId });
    if (["succeeded", "failed", "cancelled"].includes(view.state)) break;
    if (Date.now() > until)
      throw Error(
        "Observe original job " +
          jobId +
          ";observation timeout is not terminal",
      );
    await new Promise((r) => setTimeout(r, 500));
  } while (true);
  expect(view.state, JSON.stringify(view.failure)).toBe("succeeded");
  return view;
};
const original = await stage("original-current-project", inspect),
  sequence = original.project.sequences[0].id;
const edit = (name, operations) =>
  stage(name, async () => {
    const current = await inspect();
    state.requests[name] ??= {
      project_id: project,
      expected_revision: current.revision,
      idempotency_key: "f23-human-" + name,
      label: name,
      operations,
    };
    await save();
    await call("opencut_edit_apply", state.requests[name]);
    return inspect();
  });
const media = await stage("import-one-frozen-human-source", async () => {
  const bytes = await fs.readFile(source.path);
  expect(sha(bytes)).toBe(source.sha256);
  state.requests.upload ??= {
    project_id: project,
    name: path.basename(source.path),
    size_bytes: String(bytes.length),
    idempotency_key: "f23-human-zh-frozen-upload",
  };
  await save();
  const upload = await call(
      "opencut_upload_begin",
      state.requests.upload,
    ),
    observed = await call("opencut_upload_inspect", {
      upload_id: upload.upload_id,
    });
  if (observed.state === "receiving") {
    let offset = Number(observed.next_offset);
    while (offset < bytes.length) {
      const chunk = bytes.subarray(offset, offset + 1024 * 1024);
      const result = await client.uploadChunk(upload.upload_id, String(offset), chunk);
      offset += chunk.length;
      expect(Number(result.next_offset)).toBe(offset);
    }
  }
  state.requests.complete ??= {
    upload_id: upload.upload_id,
    content_hash: source.sha256,
  };
  await save();
  state.importAccepted ??= await call(
    "opencut_upload_complete",
    state.requests.complete,
  );
  await save();
  const job = await waitJob(state.importAccepted.job_id);
  const result = await call("opencut_project_media_inspect", {
    project_id: project,
    media_id: job.result.data.media_id,
  });
  expect(result.asset.version.content_hash).toBe(source.sha256);
  return result;
});
await edit(
  "isolate-original-tracks",
  original.project.sequences.map((s) => ({
    type: "sequence_put",
    sequence: {
      ...structuredClone(s),
      tracks: s.tracks.map((t) => ({ ...structuredClone(t), enabled: false })),
    },
  })),
);
const placed = await stage("bind-place-original-human-source", async () => {
  const current = await inspect();
  state.requests.place ??= {
    project_id: project,
    expected_revision: current.revision,
    sequence_id: sequence,
    idempotency_key: "f23-human-fixed-placement",
    label: "F23 frozen human Chinese source",
    bindings: [
      {
        media_id: media.media_id,
        version: media.asset.version,
        asset_id: "f23-human-source",
        name: "FLEURS Mandarin test " + source.id,
        mode: "bind",
      },
    ],
    placements: [
      {
        element_id: "f23-human-audio",
        name: "FLEURS Mandarin test " + source.id,
        track_id: "f23-human-audio-track",
        asset_id: "f23-human-source",
        start: { ticks: "0", timebase: 1 },
        content: {
          kind: "audio",
          timing: {
            kind: "source_range",
            range: {
              start: { ticks: "0", timebase: 1 },
              duration: media.asset.duration,
            },
            rate: { numerator: 1, denominator: 1 },
            maintain_pitch: true,
          },
        },
      },
    ],
    new_tracks: [
      {
        id: "f23-human-audio-track",
        name: "F23 human Chinese reference",
        kind: "audio",
      },
    ],
    ripple: false,
  };
  await save();
  state.requests.placeTransaction ??= (
    await call("opencut_media_import_plan", state.requests.place)
  ).transaction;
  await save();
  await call("opencut_edit_apply", state.requests.placeTransaction);
  return inspect();
});
const browser = await chromium.launch({
    headless: true,
    executablePath: process.env.OPENCUT_CHROME,
  }),
  page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1300 },
    acceptDownloads: true,
  });
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
const open = async () => {
  await page.goto(`${args.endpoint}/editor/${project}?scene=${sequence}`);
  await expect(page.locator(".save-status")).toHaveText("Saved");
  const welcome = page.getByRole("dialog", {
    name: "Welcome to OpenCut",
    exact: true,
  });
  if (await welcome.isVisible())
    await welcome.getByRole("button", { name: "Close", exact: true }).click();
};
const undo = (name) =>
  stage(name, async () => {
    let receipt;
    if (state.requests[name])
      await call("opencut_edit_undo_apply", state.requests[name]);
    else {
      await page.route(
        "**/api/tools/opencut_edit_undo_apply",
        async (route) => {
          state.requests[name] = route.request().postDataJSON();
          await save();
          const response = await route.fetch();
          receipt = (await response.json()).data;
          await route.fulfill({ response });
        },
      );
      try {
        await page
          .getByRole("region", { name: "History", exact: true })
          .getByRole("button", { name: "Undo", exact: true })
          .click();
        const apply = page
          .getByRole("region", { name: "History plan", exact: true })
          .getByRole("button", { name: "Apply undo", exact: true });
        await expect(apply).toBeEnabled();
        await apply.click();
        await expect.poll(() => !!receipt).toBe(true);
      } finally {
        await page.unroute("**/api/tools/opencut_edit_undo_apply");
      }
    }
    await expect(page.locator(".save-status")).toHaveText("Saved");
    return inspect();
  });
try {
  await open();
  const panel = page.locator(".editor-caption-workspace");
  const accepted = await stage(
    "actual-ui-Chinese-language-submit",
    async () => {
      if (state.requests.asr) {
        const accepted = await call(
          "opencut_analysis_sequence_transcribe",
          state.requests.asr,
        );
        expect(accepted.target).toEqual({
          project_id: project,
          revision: placed.revision,
          sequence_id: sequence,
        });
        return { job_id: accepted.job_id };
      }
      await panel
        .getByRole("combobox", { name: /^Speech language(?:\s|$)/ })
        .selectOption("zh");
      const result = page.waitForResponse((r) =>
        r.url().endsWith("/opencut_analysis_sequence_transcribe"),
      );
      await page.route(
        "**/api/tools/opencut_analysis_sequence_transcribe",
        async (route) => {
          state.requests.asr = route.request().postDataJSON();
          await save();
          await route.continue();
        },
      );
      try {
        await panel
          .getByRole("button", { name: "Transcribe saved scene", exact: true })
          .click();
        const reply = (await (await result).json()).data;
        expect(state.requests.asr.language).toBe("zh");
        expect(reply.target).toEqual({
          project_id: project,
          revision: placed.revision,
          sequence_id: sequence,
        });
        return { job_id: reply.job_id };
      } finally {
        await page.unroute(
          "**/api/tools/opencut_analysis_sequence_transcribe",
        );
      }
    },
  );
  const observed = await stage(
    "complete-original-ASR-and-literal-CER",
    async () => {
      const view = await waitJob(accepted.job_id),
        result = view.result.data;
      expect(result.revision).toBe(placed.revision);
      expect(result.project_id).toBe(project);
      const transcription = result.transcription;
      expect(transcription.provider).toBe("whisper.cpp");
      expect(transcription.model_sha256).toBe(
        "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
      );
      expect(transcription.language).toBe("zh");
      const actual = transcription.segments.map((s) => s.text).join(" "),
        reference = normalize(source.reference),
        units = normalize(actual),
        errors = distance(reference, units),
        cer = errors / reference.length;
      const replay = await call(
        "opencut_analysis_sequence_transcribe",
        state.requests.asr,
      );
      expect(replay.job_id).toBe(accepted.job_id);
      expect((await inspect()).project).toEqual(placed.project);
      const row = {
        job_id: accepted.job_id,
        actual_text: actual,
        reference: source.reference,
        reference_units: reference.length,
        actual_units: units.length,
        edit_distance: errors,
        cer,
        threshold: policy.cer_threshold,
        passed: cer <= policy.cer_threshold,
        model: transcription.model,
        model_sha256: transcription.model_sha256,
        language: transcription.language,
        word_timing_status: transcription.word_timing_status,
        segments: transcription.segments,
        mix: result.audio,
        source_duration: result.duration,
        original_request_replay: true,
      };
      console.log(
        JSON.stringify({
          job_id: row.job_id,
          cer: row.cer,
          threshold: row.threshold,
          passed: row.passed,
        }),
      );
      return row;
    },
  );
  if (observed.passed) {
    const generated = await stage(
      "actual-Chinese-editable-caption-generation",
      async () => {
        await panel
          .getByRole("combobox", { name: /^Caption track(?:\s|$)/ })
          .selectOption("__new_subtitle_text_track__");
        const font = original.project.fonts.find(
          (f) =>
            f.family === "Ma Shan Zheng" &&
            f.weight === 400 &&
            f.style === "normal",
        );
        expect(font).toBeTruthy();
        await panel
          .getByRole("combobox", { name: /^Pinned font(?:\s|$)/ })
          .selectOption(font.id);
        await panel
          .getByRole("spinbutton", { name: /^Size \(px\)(?:\s|$)/ })
          .fill("10");
        await panel
          .getByRole("spinbutton", { name: /^Words per caption(?:\s|$)/ })
          .fill("4");
        await panel
          .getByRole("combobox", { name: /^Scene transcription(?:\s|$)/ })
          .selectOption(accepted.job_id);
        await panel
          .getByRole("button", { name: "Generate captions", exact: true })
          .click();
        await expect
          .poll(async () => {
            const p = await inspect();
            return p.project.sequences[0].tracks.filter(
              (t) =>
                !placed.project.sequences[0].tracks.some(
                  (old) => old.id === t.id,
                ),
            ).length;
          })
          .toBe(1);
        await expect(page.locator(".save-status")).toHaveText("Saved");
        const acknowledge = panel.getByRole("button", {
          name: "Acknowledge original result",
          exact: true,
        });
        if (await acknowledge.isVisible()) await acknowledge.click();
        const after = await inspect(),
          track = after.project.sequences[0].tracks.find(
            (t) =>
              !placed.project.sequences[0].tracks.some(
                (old) => old.id === t.id,
              ),
          );
        expect(track.kind).toBe("text");
        expect(track.elements.length).toBeGreaterThan(0);
        const text = track.elements
          .map((e) => e.content.text.content)
          .join(" ");
        expect(normalize(text)).toEqual(normalize(observed.actual_text));
        expect(
          track.elements.every(
            (e) =>
              e.content.text.font.kind === "locked" &&
              e.content.text.font.font_id === font.id,
          ),
        ).toBe(true);
        return {
          revision: after.revision,
          track_id: track.id,
          element_ids: track.elements.map((e) => e.id),
          text,
          font_id: font.id,
        };
      },
    );
    await stage("saved-Chinese-reopen-and-SRT-download", async () => {
      await open();
      const panel = page.locator(".editor-caption-workspace");
      await panel
        .getByRole("combobox", { name: /^Caption track(?:\s|$)/ })
        .selectOption(generated.track_id);
      await panel
        .getByRole("combobox", { name: /^Format(?:\s|$)/ })
        .selectOption("srt");
      const download = page.waitForEvent("download"),
        reply = page.waitForResponse((r) =>
          r.url().endsWith("/opencut_subtitle_text_export"),
        );
      await panel
        .getByRole("button", { name: "Export subtitles", exact: true })
        .click();
      const receipt = (await (await reply).json()).data,
        target = path.join(args.workspace, "f23-human-zh-caption.srt");
      await (await download).saveAs(target);
      const text = await fs.readFile(target, "utf8");
      expect(text).toBe(receipt.content);
      const body = text
        .trim()
        .split(/\r?\n\r?\n/)
        .map((block) => block.split(/\r?\n/).slice(2).join(" "))
        .join(" ");
      expect(normalize(body)).toEqual(normalize(observed.actual_text));
      return {
        path: target,
        sha256: sha(text),
        cue_count: receipt.cue_count,
        losses: receipt.losses,
      };
    });
    await undo("undo-generated-human-captions");
  }
  await undo("undo-human-binding-placement");
  await undo("undo-original-track-isolation");
  await stage("original-project-restored-and-reopened", async () => {
    await open();
    expect((await inspect()).project).toEqual(original.project);
    return { complete_graph_assets_fonts_settings_equal: true, reloaded: true };
  });
  expect(errors).toEqual([]);
  const report = {
    status: observed.passed
      ? "passed_scoped_human_chinese"
      : "observed_literal_CER_failure_original_job_retained",
    candidate: args.candidate,
    source: args.source,
    selection_policy: source.selection_policy,
    source_sha256: source.sha256,
    accuracy: observed,
    generation:
      state.stages["actual-Chinese-editable-caption-generation"] ?? null,
    download: state.stages["saved-Chinese-reopen-and-SRT-download"] ?? null,
    original_project_restored: true,
    new_sources: 1,
    new_source_bytes: source.size_bytes,
    new_import_jobs: 1,
    new_ASR_jobs: 1,
    new_video_jobs: 0,
    new_fonts: 0,
    new_project_copies: 0,
    builds: 0,
    unit_tests: 0,
    page_errors: errors,
    limits: [
      "One official human read-speech example;existing NASA interview/English scope stays separate.",
      "Literal CER only,NFKC letters/numbers normalization,original.25 threshold and original synthetic11/30 failure retained.No script conversion or replacement source to chase green.",
      "Segment-derived word timing is approximate;no human word-boundary,all-language,physical-output or A7/D8 claim.",
    ],
  };
  await fs.writeFile(args.report, JSON.stringify(report, null, 2) + "\n");
  console.log(
    JSON.stringify({
      status: report.status,
      job_id: accepted.job_id,
      cer: observed.cer,
    }),
  );
} catch (error) {
  state.last_error = error.stack;
  await save();
  await page
    .screenshot({
      path: path.join(args.workspace, "f23-human-zh-failure.png"),
      fullPage: true,
    })
    .catch(() => {});
  throw error;
} finally {
  await browser.close();
}
