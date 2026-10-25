import { editorSnapshot } from "./project-inspection.mjs";
/** One remaining Classic ASS slice on the original text fixture, fonts and CAS.
 * Original Classic source is the independent reference; no product oracle use. */
import { chromium, expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { OpenCutClient } from "../../../packages/sdk-ts/dist/index.js";
const args = Object.fromEntries(
  process.argv
    .slice(2)
    .reduce((a, v, i, s) => (i % 2 ? a : [...a, [v.slice(2), s[i + 1]]]), []),
);
const reference = JSON.parse(await fs.readFile(args.reference, "utf8")),
  budget = JSON.parse(await fs.readFile(args.budget, "utf8"));
const file = path.join(
  args.workspace,
  "agent-classic-ass-current-v4-checkpoint.json",
);
const state = await fs
  .readFile(file, "utf8")
  .then(JSON.parse)
  .catch((e) => {
    if (e.code !== "ENOENT") throw e;
    return { candidate: args.candidate, stages: {}, requests: {} };
  });
expect(state.candidate).toBe(args.candidate);
const complete = await fs
  .readFile(args.report, "utf8")
  .then(JSON.parse)
  .catch((e) => {
    if (e.code !== "ENOENT") throw e;
    return null;
  });
if (
  complete?.status === "passed_scoped_classic_ass_remaining" &&
  complete.candidate === args.candidate
) {
  console.log("Reuse completed ASS slice.");
  process.exit(0);
}
const sha = (b) => createHash("sha256").update(b).digest("hex");
const save = async () => {
  await fs.writeFile(file + ".pending", JSON.stringify(state, null, 2) + "\n");
  await fs.rename(file + ".pending", file);
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
const client = new OpenCutClient({
  baseUrl: args.endpoint,
  token: (await fs.readFile(args.credential, "utf8")).trim(),
});
const project = "fb92adbb-b999-414e-b351-35f012b8e7b8",
  call = (tool, input) => client.call(tool, input),
  inspect = () =>
    editorSnapshot(call("opencut_project_inspect", { project_id: project, revision: null }));
const original = await stage("original-current-project", inspect),
  sequence = original.project.sequences[0].id;
const browser = await chromium.launch({
  headless: true,
  executablePath: process.env.OPENCUT_CHROME,
});
const page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1300 },
  }),
  oraclePage = await browser.newPage();
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
const open = async () => {
  await page.goto(`${args.endpoint}/editor/${project}?scene=${sequence}`);
  await expect(page.locator(".save-status")).toHaveText("Saved");
  const d = page.getByRole("dialog", {
    name: "Welcome to OpenCut",
    exact: true,
  });
  if (await d.isVisible())
    await d.getByRole("button", { name: "Close", exact: true }).click();
};
const edit = async (name, operations) =>
  stage(name, async () => {
    const before = await inspect();
    state.requests[name] ??= {
      project_id: project,
      expected_revision: before.revision,
      idempotency_key: "f23-" + name,
      label: name,
      operations,
    };
    await save();
    await call("opencut_edit_apply", state.requests[name]);
    return inspect();
  });
const undo = async (name) =>
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
const metrics = (actual, expected) => {
  let error = 0,
    intersection = 0,
    union = 0;
  for (let i = 0; i < expected.length; i += 4) {
    const a = Math.max(actual[i], actual[i + 1], actual[i + 2]) > 180,
      e = Math.max(expected[i], expected[i + 1], expected[i + 2]) > 180;
    if (a || e) union++;
    if (a && e) intersection++;
    error +=
      Math.abs(actual[i] - expected[i]) +
      Math.abs(actual[i + 1] - expected[i + 1]) +
      Math.abs(actual[i + 2] - expected[i + 2]);
  }
  return {
    glyph_iou: intersection / Math.max(1, union),
    mean_rgb_error: error / ((expected.length / 4) * 3),
    glyph_union: union,
  };
};
const passes = (m, video = false) =>
  m.glyph_iou >=
    (video ? budget.video_glyph_iou : budget.raw_preview_glyph_iou) &&
  m.mean_rgb_error <=
    (video ? budget.video_mean_rgb_error : budget.raw_preview_mean_rgb_error);
try {
  const font = await fs.readFile(reference.font.path);
  expect(sha(font)).toBe(reference.font.sha256);
  const expectedFact = await stage(
    "independent-original-classic-canvas",
    async () => {
      const result = await oraclePage.evaluate(
        async ({ reference, font }) => {
          const modules = reference.modules,
            cache = {};
          const encode = (bytes) => {
            let value = "";
            for (let i = 0; i < bytes.length; i += 8192)
              value += String.fromCharCode(...bytes.subarray(i, i + 8192));
            return btoa(value);
          };
          const normalize = (s) => {
            const parts = [];
            for (const p of s.split("/")) {
              if (p === "." || !p) continue;
              if (p === "..") parts.pop();
              else parts.push(p);
            }
            return parts.join("/");
          };
          const load = (id) => {
            if (cache[id]) return cache[id];
            if (!modules[id]) throw Error("Missing original module " + id);
            const exports = {};
            cache[id] = exports;
            new Function("exports", "require", modules[id])(exports, (s) =>
              load(
                s.startsWith("@/")
                  ? s.slice(2)
                  : normalize(id.split("/").slice(0, -1).join("/") + "/" + s),
              ),
            );
            return exports;
          };
          const builder = load("subtitles/build-subtitle-text-element"),
            primitives = load("text/primitives");
          const face = new FontFace(
            "Lato",
            Uint8Array.from(atob(font), (c) => c.charCodeAt(0)),
            { weight: "400", style: "normal" },
          );
          await face.load();
          document.fonts.add(face);
          const render = (caption, variant) => {
            const element = builder.buildSubtitleTextElement({
                index: 0,
                caption,
                canvasSize: {
                  width: reference.width,
                  height: reference.height,
                },
              }),
              p = structuredClone(element.params);
            if (variant === "no-wrap")
              p.content = p.content.replaceAll("\n", " ");
            if (variant === "no-background") p["background.enabled"] = false;
            if (variant === "wrong-position") p["transform.positionX"] += 30;
            const c = document.createElement("canvas");
            c.width = reference.width;
            c.height = reference.height;
            const ctx = c.getContext("2d");
            ctx.fillStyle = "black";
            ctx.fillRect(0, 0, c.width, c.height);
            const layout = primitives.measureTextLayout({
              text: p,
              canvasHeight: reference.height,
              ctx,
            });
            ctx.translate(
              reference.width / 2 + p["transform.positionX"],
              reference.height / 2 + p["transform.positionY"],
            );
            ctx.globalAlpha = p.opacity;
            primitives.drawMeasuredTextLayout({
              ctx,
              layout,
              textColor: p.color,
              background: {
                enabled: p["background.enabled"],
                cornerRadius: p["background.cornerRadius"],
                paddingX: p["background.paddingX"],
                paddingY: p["background.paddingY"],
                offsetX: p["background.offsetX"],
                offsetY: p["background.offsetY"],
              },
              backgroundColor: p["background.color"],
            });
            return {
              params: p,
              rgba: encode(ctx.getImageData(0, 0, c.width, c.height).data),
              png: c.toDataURL("image/png").split(",")[1],
            };
          };
          return reference.cases.map((c) => ({
            name: c.name,
            ...render(c.caption),
            negative:
              c.name === "default"
                ? render(c.caption, "no-wrap")
                : c.name === "top-left-background"
                  ? render(c.caption, "no-background")
                  : c.name === "bottom-right-margins"
                    ? render(c.caption, "wrong-position")
                    : null,
          }));
        },
        { reference, font: font.toString("base64") },
      );
      for (const row of result) {
        const m = metrics(
          Buffer.from(row.rgba, "base64"),
          Buffer.from(row.negative?.rgba ?? row.rgba, "base64"),
        );
        if (row.negative) expect(passes(m, true)).toBe(false);
        await fs.writeFile(
          path.join(args.workspace, "f23-reference-" + row.name + ".png"),
          Buffer.from(row.png, "base64"),
        );
      }
      const expectedFile = path.join(
        args.workspace,
        "f23-classic-expected-pixels.json",
      );
      await fs.writeFile(expectedFile, JSON.stringify(result) + "\n");
      return { file: expectedFile, count: result.length };
    },
  );
  const expected = JSON.parse(await fs.readFile(expectedFact.file, "utf8"));
  const isolated = await edit(
    "isolate-original-tracks",
    original.project.sequences.map((s) => ({
      type: "sequence_put",
      sequence: {
        ...structuredClone(s),
        tracks: s.tracks.map((t) => ({
          ...structuredClone(t),
          enabled: false,
        })),
      },
    })),
  );
  await open();
  const panel = page.locator(".editor-caption-workspace");
  await stage("prepare-original-compatibility-review", async () => {
    const before = await inspect();
    await panel
      .getByRole("combobox", { name: /^Format(?:\s|$)/ })
      .selectOption("ass");
    await panel
      .getByRole("radio", { name: "Classic compatibility", exact: true })
      .check();
    const content = panel.getByRole("textbox", {
      name: /^Subtitle content(?:\s|$)/,
    });
    await content.fill(await fs.readFile(reference.source, "utf8"));
    const prepare = () =>
      panel
        .getByRole("button", { name: "Import subtitles", exact: true })
        .click();
    await prepare();
    const review = panel.getByRole("status", {
      name: "ASS import review",
      exact: true,
    });
    await expect(review).toBeVisible();
    await content.fill(
      (await fs.readFile(reference.source, "utf8")) +
        "\n; changed after review",
    );
    await expect(review).toHaveCount(0);
    expect(await inspect()).toEqual(before);
    await content.fill(await fs.readFile(reference.source, "utf8"));
    const response = page.waitForResponse(
      (r) =>
        r.url().endsWith("/opencut_subtitle_classic_ass_plan") &&
        Array.isArray(r.request().postDataJSON().observations),
    );
    await prepare();
    const envelope = await (await response).json(),
      plan = envelope.data;
    expect(plan.transaction).toBeTruthy();
    expect(plan.cues).toHaveLength(reference.cases.length);
    expect(plan.skipped_cue_count).toBe(reference.parsed.skippedCueCount);
    for (const warning of reference.parsed.warnings)
      expect(plan.warnings).toContain(warning);
    await expect(review).toBeVisible();
    for (const warning of reference.parsed.warnings)
      await expect(review).toContainText(warning);
    state.requests.import = plan.transaction;
    state.plan = plan;
    await save();
    expect(await inspect()).toEqual(before);
    return {
      cue_count: plan.cues.length,
      skipped: plan.skipped_cue_count,
      warnings: plan.warnings,
      review_change_clears_plan: true,
      prepare_writes: 0,
    };
  });
  const imported = await stage(
    "save-reviewed-original-transaction",
    async () => {
      const current = await inspect();
      if (current.revision === state.requests.import.expected_revision) {
        if (
          !(await panel
            .getByRole("button", {
              name: "Save reviewed subtitles",
              exact: true,
            })
            .isVisible())
        ) {
          await call("opencut_edit_apply", state.requests.import);
          await open();
        } else {
          await panel
            .getByRole("button", {
              name: "Save reviewed subtitles",
              exact: true,
            })
            .click();
          await expect(
            panel.getByRole("button", {
              name: "Save reviewed subtitles",
              exact: true,
            }),
          ).toHaveCount(0);
          await expect(page.locator(".save-status")).toHaveText("Saved");
        }
      } else await call("opencut_edit_apply", state.requests.import);
      const after = await inspect();
      const receipt = await call(
        "opencut_edit_apply",
        state.requests.import,
      );
      expect(receipt.revision).toBe(after.revision);
      return after;
    },
  );
  await stage("native-planned-style-and-original-source-equality", async () => {
    const s = imported.project.sequences.find((s) => s.id === sequence),
      track = s.tracks.find(
        (t) =>
          !isolated.project.sequences[0].tracks.some((old) => old.id === t.id),
      );
    expect(track.elements).toHaveLength(reference.cases.length);
    for (let i = 0; i < track.elements.length; i++) {
      const e = track.elements[i],
        p = expected[i].params,
        text = e.content.text;
      expect(text.content).toBe(p.content);
      expect(text.font).toEqual({ kind: "locked", font_id: reference.font.id });
      expect(text.font_size.unit).toBe("classic_height90");
      expect(text.font_size.value).toBeCloseTo(p.fontSize, 9);
      expect(text.line_height_multiplier).toBe(p.lineHeight);
      expect(text.letter_spacing_px).toBe(p.letterSpacing);
      expect(text.align).toBe(p.textAlign);
      expect(text.font_weight).toBe(p.fontWeight);
      expect(text.font_style).toBe(p.fontStyle);
      expect(text.background.enabled).toBe(p["background.enabled"]);
      expect(e.visual.transform.position_x_px).toBeCloseTo(
        p["transform.positionX"],
        6,
      );
      expect(e.visual.transform.position_y_px).toBeCloseTo(
        p["transform.positionY"],
        6,
      );
    }
    const other = structuredClone(imported.project);
    other.sequences[0].tracks = isolated.project.sequences[0].tracks;
    expect(other).toEqual(isolated.project);
    return {
      cases: track.elements.map((e, i) => ({
        name: reference.cases[i].name,
        element_id: e.id,
        content: e.content.text.content,
        transform: e.visual.transform,
      })),
      original_source_style_equal: true,
      other_content_unchanged: true,
    };
  });
  await open();
  await stage("actual-continuous-preview-independent-pixels", async () => {
    await page.locator(".editor-timeline-ruler").focus();
    await page.keyboard.press("Home");
    await page.evaluate(
      ({ count, seconds }) => {
        const encode = (bytes) => {
          let value = "";
          for (let i = 0; i < bytes.length; i += 8192)
            value += String.fromCharCode(...bytes.subarray(i, i + 8192));
          return btoa(value);
        };
        const frames = {};
        globalThis.__f23Display = { frames, active: true };
        const loop = () => {
          const store = globalThis.__f23Display;
          if (!store.active) return;
          const c = document.querySelector("canvas.editor-scene-frame");
          if (c) {
            const time = Number(c.dataset.time),
              index = Math.floor(time / 0.5),
              offset = time - index * 0.5;
            if (
              Number.isFinite(time) &&
              index >= 0 &&
              index < count &&
              offset >= 0.1 &&
              offset <= 0.4 &&
              !frames[index]
            ) {
              const copy = document.createElement("canvas");
              copy.width = c.width;
              copy.height = c.height;
              const ctx = copy.getContext("2d");
              ctx.drawImage(c, 0, 0);
              frames[index] = {
                time,
                width: c.width,
                height: c.height,
                runtime: c.dataset.runtime,
                generation: c.dataset.generation,
                png: copy.toDataURL("image/png").split(",")[1],
                rgba: encode(ctx.getImageData(0, 0, c.width, c.height).data),
              };
            }
          }
          requestAnimationFrame(loop);
        };
        requestAnimationFrame(loop);
      },
      { count: reference.cases.length, seconds: reference.duration_seconds },
    );
    await page
      .getByRole("button", { name: "Play scene preview", exact: true })
      .click();
    try {
      await expect
        .poll(
          () =>
            page.evaluate(
              () => Object.keys(globalThis.__f23Display.frames).length,
            ),
          { timeout: budget.preview_capture_ms },
        )
        .toBe(reference.cases.length);
    } finally {
      await page.evaluate(() => {
        globalThis.__f23Display.active = false;
      });
      const pause = page.getByRole("button", {
        name: "Pause scene audio",
        exact: true,
      });
      if (await pause.isEnabled()) await pause.click();
    }
    const frames = await page.evaluate(() => globalThis.__f23Display.frames);
    const measurements = [];
    for (let i = 0; i < reference.cases.length; i++) {
      const f = frames[i];
      expect([f.width, f.height]).toEqual([reference.width, reference.height]);
      const m = metrics(
        Buffer.from(f.rgba, "base64"),
        Buffer.from(expected[i].rgba, "base64"),
      );
      await fs.writeFile(
        path.join(
          args.workspace,
          "f23-preview-" + reference.cases[i].name + ".png",
        ),
        Buffer.from(f.png, "base64"),
      );
      expect(
        passes(m),
        JSON.stringify({ case: reference.cases[i].name, ...m }),
      ).toBe(true);
      measurements.push({ name: reference.cases[i].name, time: f.time, ...m });
    }
    expect((await inspect()).project).toEqual(imported.project);
    return { frames: measurements, revision_unchanged: true };
  });
  const output = await stage("one-video-range-original-job", async () => {
    state.requests.export ??= {
      kind: "video_range",
      request: {
        project_id: project,
        revision: imported.revision,
        sequence_id: sequence,
        idempotency_key: "f23-combined-classic-ass-range",
        options: { format: "mp4", quality: "very_high", include_audio: false },
        range: { start_frame: "0", frame_count: String(reference.frames) },
      },
    };
    await save();
    state.accepted ??= await call(
      "opencut_render_submit",
      state.requests.export,
    );
    await save();
    let job;
    const until = Date.now() + budget.video_job_observation_ms;
    do {
      job = await call("opencut_job_view", { job_id: state.accepted.job_id });
      if (["succeeded", "failed", "cancelled"].includes(job.state)) break;
      if (Date.now() > until)
        throw Error(
          "Observe original Job " +
            state.accepted.job_id +
            ";timeout is not terminal",
        );
      await new Promise((r) => setTimeout(r, 400));
    } while (true);
    expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
    const output = path.join(args.workspace, "f23-" + job.job_id + ".mp4");
    let bytes = await fs.readFile(output).catch((e) => {
      if (e.code !== "ENOENT") throw e;
      return null;
    });
    if (!bytes) {
      bytes = Buffer.from(
        await (await client.downloadArtifact(job.artifacts[0])).arrayBuffer(),
      );
      await fs.writeFile(output, bytes);
    }
    expect(sha(bytes)).toBe(job.artifacts[0].content_hash);
    return {
      job_id: job.job_id,
      path: output,
      artifact: job.artifacts[0],
      identity: job.result.data.identity,
    };
  });
  await stage("video-timing-and-independent-expected-pixels", async () => {
    const probe = JSON.parse(
      execFileSync(
        "ffprobe",
        [
          "-v",
          "error",
          "-select_streams",
          "v:0",
          "-count_frames",
          "-show_streams",
          "-show_packets",
          "-of",
          "json",
          output.path,
        ],
        { encoding: "utf8" },
      ),
    );
    const v = probe.streams[0],
      packets = probe.packets.sort((a, b) => Number(a.pts) - Number(b.pts));
    expect(v.nb_read_frames).toBe(String(reference.frames));
    expect(v.avg_frame_rate).toBe("30/1");
    expect([v.width, v.height]).toEqual([320, 180]);
    const [n, d] = v.time_base.split("/").map(Number);
    packets.forEach((p, i) =>
      expect(Math.abs((Number(p.pts) * n) / d - i / 30)).toBeLessThan(1e-8),
    );
    const rgb = execFileSync(
      "ffmpeg",
      [
        "-v",
        "error",
        "-i",
        output.path,
        "-an",
        "-pix_fmt",
        "rgb24",
        "-f",
        "rawvideo",
        "pipe:1",
      ],
      {
        maxBuffer:
          reference.frames * reference.width * reference.height * 3 + 1024,
      },
    );
    expect(rgb.length).toBe(
      reference.frames * reference.width * reference.height * 3,
    );
    const rows = [];
    for (let i = 0; i < reference.cases.length; i++) {
      const index = reference.cases[i].sample_frame,
        offset = index * 320 * 180 * 3,
        rgba = Buffer.alloc(320 * 180 * 4);
      for (let j = 0; j < 320 * 180; j++) {
        rgba[j * 4] = rgb[offset + j * 3];
        rgba[j * 4 + 1] = rgb[offset + j * 3 + 1];
        rgba[j * 4 + 2] = rgb[offset + j * 3 + 2];
        rgba[j * 4 + 3] = 255;
      }
      const m = metrics(rgba, Buffer.from(expected[i].rgba, "base64"));
      expect(
        passes(m, true),
        JSON.stringify({ case: reference.cases[i].name, ...m }),
      ).toBe(true);
      rows.push({ name: reference.cases[i].name, frame: index, ...m });
    }
    return { frame_count: reference.frames, fps: "30/1", frames: rows };
  });
  await open();
  await undo("undo-reviewed-classic-import");
  await undo("undo-original-isolation");
  await stage("complete-original-project-restored", async () => {
    await open();
    expect((await inspect()).project).toEqual(original.project);
    return { complete_graph_fonts_assets_settings_equal: true, reloaded: true };
  });
  expect(errors).toEqual([]);
  const result = {
    status: "passed_scoped_classic_ass_remaining",
    candidate: args.candidate,
    project_id: project,
    reference: args.reference,
    budget: args.budget,
    stages: Object.keys(state.stages),
    review: state.stages["prepare-original-compatibility-review"],
    style: state.stages["native-planned-style-and-original-source-equality"],
    preview: state.stages["actual-continuous-preview-independent-pixels"],
    output,
    timing_pixels: state.stages["video-timing-and-independent-expected-pixels"],
    complete_original_project_restored: true,
    new_video_jobs: 1,
    new_asr_jobs: 0,
    new_sources: 0,
    new_fonts: 0,
    new_project_copies: 0,
    builds: 0,
    unit_tests: 0,
    page_errors: errors,
    scope:
      "Remaining Classic source-compatible ASS styles/wrapping/losses/diagnostics and actual UI/native/range output.Native491b76a2/Web7efed794 unchanged;original parser/builder/drawing code and exact existing Lato400 bytes provide independent expected pixels.Strict external ASS,prior English/NASA ASR and font/media imports retain their own evidence.Chinese human-reference/orthography and F20/F47/full A7/D8 remain open.",
  };
  await fs.writeFile(args.report, JSON.stringify(result, null, 2) + "\n");
  console.log(
    JSON.stringify({
      status: result.status,
      job_id: output.job_id,
      stages: result.stages.length,
    }),
  );
} catch (error) {
  state.last_error = error.stack;
  await save();
  await page
    .screenshot({
      path: path.join(args.workspace, "f23-classic-failure.png"),
      fullPage: true,
    })
    .catch(() => {});
  throw error;
} finally {
  await browser.close();
}
