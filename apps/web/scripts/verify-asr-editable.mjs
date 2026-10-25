import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash, randomUUID } from "node:crypto";
const hash = (value) => createHash("sha256").update(value).digest("hex");
const prefix = (label) =>
  new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?:\\s|$)`);
async function identity() {
  const root = process.env.OPENCUT_WEB_ROOT,
    binary = process.env.OPENCUT_WEB_BINARY;
  expect(root).toBeTruthy();
  expect(binary).toBeTruthy();
  const files = [];
  async function visit(dir = "") {
    for (const entry of await fs.readdir(path.join(root, dir), {
      withFileTypes: true,
    })) {
      const name = path.join(dir, entry.name);
      if (entry.isSymbolicLink()) throw new Error("Candidate symlink");
      if (entry.isDirectory()) await visit(name);
      else if (/\.(js|mjs|cjs|css)$/u.test(name))
        files.push([name, hash(await fs.readFile(path.join(root, name)))]);
    }
  }
  await visit();
  files.sort((a, b) => a[0].localeCompare(b[0]));
  expect(files.length).toBeGreaterThan(0);
  return {
    binary_sha256: hash(await fs.readFile(binary)),
    web_js_css_tree_sha256: hash(JSON.stringify(files)),
  };
}
const normalized = (text) => text.replace(/\s+/gu, " ").trim();
const seconds = (t) => Number(t.ticks) / t.timebase;
export async function verifyAsrEditable(browser, endpoint, directory, check) {
  const fixture = JSON.parse(
    await fs.readFile(path.join(directory, "b3-asr-fixture.json"), "utf8"),
  );
  const sample = fixture.samples.find((s) => s.language === "en");
  expect(sample).toBeTruthy();
  let old;
  for (const filename of ["b3-asr.json", "b3-asr-failure.json"]) {
    try {
      const report = JSON.parse(
        await fs.readFile(path.join(directory, filename), "utf8"),
      );
      if (report.evidence?.some((e) => e.language === "en" && e.result)) {
        old = report;
        break;
      }
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
  }
  const reference = old?.evidence.find((e) => e.language === "en" && e.result),
    oldCues = old?.evidence.find((e) => e.language === "en" && e.cues)?.cues;
  expect(reference).toBeTruthy();
  expect(oldCues?.length).toBeGreaterThan(0);
  const filename = path.join(directory, "f23-asr-editable-checkpoint.json"),
    candidate = await identity();
  let facts;
  try {
    facts = JSON.parse(await fs.readFile(filename, "utf8"));
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  if (facts) expect(facts.candidate).toEqual(candidate);
  else
    facts = {
      candidate,
      project_id: fixture.project_id,
      sequence_id: fixture.sequence_id,
      source: sample,
      transactions: {},
      downloads: {},
      phase: "bootstrap",
      scope:
        "English synthetic speech new editable TextV4 entry integration; no new WER/human/Chinese quality claim; proportional word timings are estimated",
    };
  expect(facts.project_id).toBe(fixture.project_id);
  expect(facts.source).toEqual(sample);
  const save = () =>
    fs.writeFile(filename, JSON.stringify(facts, null, 2) + "\n");
  const page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1100 },
  });
  page.setDefaultTimeout(30000);
  await page.addInitScript(() =>
    Object.defineProperty(window, "showSaveFilePicker", {
      value: undefined,
      configurable: true,
    }),
  );
  const errors = [];
  page.on("pageerror", (error) => {
    if (errors.length < 32) errors.push(error.message.slice(0, 1024));
  });
  const tool = async (name, input) => {
    const envelope = await page.evaluate(
      async ({ name, input }) =>
        (
          await fetch(`/api/tools/${name}`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(input),
          })
        ).json(),
      { name, input },
    );
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull();
    return envelope.data;
  };
  const inspect = (revision = null) =>
    editorSnapshot(tool("opencut_project_inspect", {
      project_id: facts.project_id,
      revision,
    }));
  const scene = (snapshot) =>
    snapshot.project.sequences.find((s) => s.id === facts.sequence_id);
  const restore = async (revision, stage) => {
    facts.restores ??= {};
    const saved = (facts.restores[stage] ??= {});
    if (!saved.request) {
      saved.request = {
        project_id: facts.project_id,
        expected_revision: (await inspect()).revision,
        target_revision: revision,
        idempotency_key: randomUUID(),
        asset_versions: {},
      };
      await save();
    }
    expect(saved.request.target_revision).toBe(revision);
    if (!saved.receipt) {
      saved.receipt = await tool("opencut_project_restore", saved.request);
      await save();
    }
    expect((await inspect()).revision).toBe(saved.receipt.revision);
    return inspect();
  };
  const reopen = async () => {
    await page.goto(
      `${endpoint}/editor/${encodeURIComponent(facts.project_id)}?scene=${encodeURIComponent(facts.sequence_id)}`,
    );
    await expect(
      page.getByRole("main", { name: "OpenCut editor", exact: true }),
    ).toBeVisible();
    const guide = page.getByRole("dialog", {
      name: "Welcome to OpenCut",
      exact: true,
    });
    if (await guide.isVisible())
      await guide.getByRole("button", { name: "Close", exact: true }).click();
  };
  const transaction = async (stage, name, action) => {
    const saved = facts.transactions[stage];
    if (saved?.receipt) return saved.receipt;
    if (saved?.request) {
      saved.receipt = await tool(name, saved.request);
      await save();
      return saved.receipt;
    }
    let journal;
    const listener = (request) => {
      if (request.url().endsWith(`/api/tools/${name}`)) {
        facts.transactions[stage] = {
          operation: name,
          request: request.postDataJSON(),
        };
        journal = save();
      }
    };
    page.on("request", listener);
    try {
      const pending = page.waitForResponse((r) =>
        r.url().endsWith(`/api/tools/${name}`),
      );
      await action();
      const response = await pending;
      await journal;
      const envelope = await response.json();
      facts.transactions[stage].envelope = envelope;
      await save();
      expect(envelope.error, JSON.stringify(envelope.error)).toBeNull();
      facts.transactions[stage].receipt = envelope.data;
      await save();
      return envelope.data;
    } finally {
      page.off("request", listener);
    }
  };
  const plans = [];
  const planListener = async (response) => {
    if (
      response.url().endsWith("/api/tools/opencut_subtitle_text_plan") &&
      plans.length < 4
    ) {
      const item = {
        request: response.request().postDataJSON(),
        envelope: await response.json(),
      };
      plans.push(item);
      facts.generation_plans = plans;
      await save();
    }
  };
  page.on("response", planListener);
  let primaryError,
    retainedPassed = false;
  try {
    await reopen();
    if (!facts.original) {
      facts.original = await inspect();
      await save();
    }
    if (facts.phase === "passed") {
      retainedPassed = true;
      expect((await inspect()).project).toEqual(facts.original.project);
      check("F23 retained English synthetic ASR editable entry", facts);
      return facts;
    }
    const panel = page.locator(".editor-caption-workspace");
    if (!facts.transactions.generate?.receipt) {
      if (!facts.fixed) {
        facts.fixed = await restore(sample.revision, "source");
        await save();
      }
      const capability = await tool(
        "opencut_analysis_sequence_transcribe_capability",
        {},
      );
      facts.capability = capability;
      await save();
      expect(capability.available, capability.unavailable_reason).toBe(true);
      const current = await inspect();
      if (current.revision !== facts.fixed.revision)
        throw new Error(
          "Saved ASR fixed revision is no longer the head; cannot reuse its inference for generation after a public restore. Root must resolve the fixed-revision checkpoint; do not rerun ASR.",
        );
      const source = facts.fixed.project.assets.find(
        (m) => m.version.content_hash === sample.source_sha256,
      );
      facts.source_media = source;
      await save();
      expect(source).toBeTruthy();
      expect(source.version.content_hash).toBe(sample.source_sha256);
      await reopen();
      await panel
        .getByRole("combobox", { name: prefix("Pinned font") })
        .selectOption(sample.font_id);
      await expect(
        panel.getByRole("combobox", { name: prefix("Caption track") }),
      ).toHaveValue("__new_subtitle_text_track__");
      await panel
        .getByRole("combobox", { name: prefix("Speech language") })
        .selectOption("en");
      facts.phase = "local English inference";
      await save();
      const submission = await transaction(
        "asr",
        "opencut_analysis_sequence_transcribe",
        () =>
          panel
            .getByRole("button", {
              name: "Transcribe saved scene",
              exact: true,
            })
            .click(),
      );
      expect(facts.transactions.asr.request).toMatchObject({
        project_id: facts.project_id,
        revision: facts.fixed.revision,
        sequence_id: facts.sequence_id,
        language: "en",
      });
      let job;
      await expect
        .poll(
          async () => {
            job = await tool("opencut_job_view", { job_id: submission.job_id });
            if (["succeeded", "failed", "cancelled"].includes(job.state)) {
              facts.job = job;
              await save();
            }
            return ["succeeded", "failed", "cancelled"].includes(job.state)
              ? job.state
              : "pending";
          },
          { timeout: 600000, intervals: [500, 1000, 2000] },
        )
        .not.toBe("pending");
      expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
      const result = job.result.data;
      expect(result.transcription).toMatchObject({
        provider: "whisper.cpp",
        language: "en",
        model_sha256: sample.model_sha256 ?? fixture.model_sha256,
      });
      expect(result.transcription.segments).toEqual(
        reference.result.transcription.segments,
      );
      expect((await inspect()).revision).toBe(facts.fixed.revision);
      facts.phase = "generate editable text from fixed successful job";
      await save();
      await panel
        .getByRole("button", { name: "Refresh tasks", exact: true })
        .click();
      await panel
        .getByRole("combobox", { name: prefix("Scene transcription") })
        .selectOption(submission.job_id);
      await panel
        .getByRole("spinbutton", { name: prefix("Words per caption") })
        .fill("3");
      await transaction("generate", "opencut_edit_apply", () =>
        panel
          .getByRole("button", { name: "Generate captions", exact: true })
          .click(),
      );
    }
    if (!facts.generated) {
      facts.generated = await inspect(
        facts.transactions.generate.receipt.revision,
      );
      const added = scene(facts.generated).tracks.filter(
        (t) => !scene(facts.fixed).tracks.some((old) => old.id === t.id),
      );
      expect(added).toHaveLength(1);
      facts.track_id = added[0].id;
      facts.element_id = added[0].elements[0].id;
      await save();
    }
    const generatedTrack = scene(facts.generated).tracks.find(
      (t) => t.id === facts.track_id,
    );
    const unchanged = structuredClone(facts.generated.project);
    scene({ project: unchanged }).tracks = scene(facts.fixed).tracks;
    expect(unchanged).toEqual(facts.fixed.project);
    expect(generatedTrack.kind).toBe("text");
    expect(generatedTrack.elements).toHaveLength(oldCues.length);
    generatedTrack.elements.forEach((e, i) => {
      expect(e.content.kind).toBe("text");
      expect(normalized(e.content.text.content)).toBe(
        normalized(oldCues[i].text),
      );
      expect(seconds(e.timeline_range.start)).toBeCloseTo(
        seconds(oldCues[i].timeline_range.start),
        6,
      );
      expect(seconds(e.timeline_range.duration)).toBeCloseTo(
        seconds(oldCues[i].timeline_range.duration),
        6,
      );
    });
    if (!facts.transactions.edit?.receipt) {
      if ((await inspect()).revision !== facts.generated.revision)
        await restore(facts.generated.revision, "before_edit");
      await reopen();
      await page
        .locator(`[data-element-id="${facts.element_id}"] .editor-element-select`)
        .click({ position: { x: 12, y: 12 } });
      await page
        .getByRole("button", { name: "Edit text", exact: true })
        .click();
      const form = page.getByRole("form", { name: "Edit text", exact: true });
      await form
        .getByRole("textbox", { name: prefix("Text content") })
        .fill("Edited spoken subtitle\nSecond line");
      await form
        .getByRole("combobox", { name: prefix("Text decoration") })
        .selectOption("underline");
      await form
        .getByRole("textbox", { name: prefix("Letter spacing (pixels)") })
        .fill("1.5");
      const plan = page.waitForResponse((r) =>
        r.url().endsWith("/api/tools/opencut_text_plan"),
      );
      await form
        .getByRole("button", { name: "Save text", exact: true })
        .click();
      expect((await (await plan).json()).error).toBeNull();
      await transaction("edit", "opencut_edit_apply", () =>
        page
          .getByRole("region", { name: "Review text changes", exact: true })
          .getByRole("button", { name: "Apply text changes", exact: true })
          .click(),
      );
    }
    if (!facts.edited) {
      facts.edited = await inspect(facts.transactions.edit.receipt.revision);
      await save();
    }
    const expected = structuredClone(facts.generated.project),
      element = scene({ project: expected })
        .tracks.find((t) => t.id === facts.track_id)
        .elements.find((e) => e.id === facts.element_id);
    Object.assign(element.content.text, {
      content: "Edited spoken subtitle\nSecond line",
      decoration: "underline",
      letter_spacing_px: 1.5,
    });
    expect(facts.edited.project).toEqual(expected);
    if ((await inspect()).revision !== facts.edited.revision)
      await restore(facts.edited.revision, "before_download");
    await reopen();
    expect((await inspect()).project).toEqual(facts.edited.project);
    for (const format of ["srt", "ass"]) {
      if (facts.downloads[format]?.path) {
        const saved = facts.downloads[format],
          content = await fs.readFile(saved.path, "utf8");
        expect(hash(content)).toBe(saved.sha256);
        expect(content).toBe(saved.envelope.data.content);
        expect(saved.envelope.data.cue_count).toBe(oldCues.length);
        expect(content).toContain(
          format === "ass"
            ? "Edited spoken subtitle\\NSecond line"
            : "Edited spoken subtitle\nSecond line",
        );
        saved.checked = true;
        await save();
        continue;
      }
      await panel
        .getByRole("combobox", { name: prefix("Caption track") })
        .selectOption(facts.track_id);
      await panel
        .getByRole("combobox", { name: prefix("Format") })
        .selectOption(format);
      const response = page.waitForResponse((r) =>
          r.url().endsWith("/api/tools/opencut_subtitle_text_export"),
        ),
        download = page.waitForEvent("download");
      await panel
        .getByRole("button", { name: "Export subtitles", exact: true })
        .click();
      const received = await response,
        envelope = await received.json();
      facts.downloads[format] = {
        request: received.request().postDataJSON(),
        envelope,
      };
      await save();
      expect(envelope.error).toBeNull();
      const output = path.join(directory, `f23-asr-editable.${format}`);
      await (await download).saveAs(output);
      const content = await fs.readFile(output, "utf8");
      Object.assign(facts.downloads[format], {
        path: output,
        sha256: hash(content),
      });
      await save();
      expect(content).toBe(envelope.data.content);
      expect(envelope.data.cue_count).toBe(oldCues.length);
      expect(content).toContain(
        format === "ass"
          ? "Edited spoken subtitle\\NSecond line"
          : "Edited spoken subtitle\nSecond line",
      );
      facts.downloads[format].checked = true;
      await save();
    }
    expect(errors).toEqual([]);
    facts.phase = "verified";
    await save();
  } catch (error) {
    primaryError = error;
    facts.active_revision =
      (await inspect().catch(() => null))?.revision ?? null;
    facts.original_revision = facts.original?.revision ?? null;
    facts.pending_recovery = {
      asr: facts.transactions.asr ?? null,
      generation: facts.transactions.generate ?? null,
      generation_plans: facts.generation_plans ?? [],
      edit: facts.transactions.edit ?? null,
      target_original_revision: facts.original_revision,
    };
    facts.failure = {
      phase: facts.phase,
      message: String(error),
      pageerrors: errors,
      ui: (
        await page
          .locator("body")
          .innerText()
          .catch(() => "")
      ).slice(0, 6000),
    };
    await save();
  } finally {
    try {
      if (!primaryError && !retainedPassed && facts.original) {
        facts.restored = await restore(facts.original.revision, "original");
        expect(facts.restored.project).toEqual(facts.original.project);
        await save();
      }
    } catch (error) {
      facts.restore_failure = String(error);
      await save();
      primaryError ??= error;
    }
    page.off("response", planListener);
    await page.close();
  }
  if (primaryError) throw primaryError;
  facts.phase = "passed";
  await save();
  check(
    "F23 English synthetic ASR to editable TextV4/save/reopen/SRT/ASS",
    facts,
  );
  return facts;
}
