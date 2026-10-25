import { editorSnapshot } from "./project-inspection.mjs";
// Unified-verification entry only. Importing this file never starts a browser,
// subprocess, fixture import or renderer. Requests/jobs survive interrupted runs.
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash, randomUUID } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import {
  SCENE_STACK_ORACLE_MODEL,
  sceneStackCases,
  referenceSceneCase,
  sceneReferenceProbes,
  gaussianSceneRgba,
} from "./scene-stack-oracle.mjs";
import { observeYuv420Rgba } from "./graphic-output-oracle.mjs";
import { bindBackgroundSource } from "./scene-stack-background-source.mjs";
const execute = promisify(execFile),
  sha = (b) => createHash("sha256").update(b).digest("hex"),
  read = async (f) => JSON.parse(await fs.readFile(f, "utf8"));
const W = 320,
  H = 180,
  PROFILE = "classic-v31-gpu-scene-frame-v1",
  time = (n) => ({ ticks: String(Math.round(n * 120000)), timebase: 120000 });
const gray = (n) => ({
  encoding: "srgb",
  red: n / 255,
  green: n / 255,
  blue: n / 255,
  alpha: 1,
});
async function candidateIdentity() {
  if (!process.env.OPENCUT_WEB_ROOT || !process.env.OPENCUT_WEB_BINARY)
    throw Error(
      "Use the integrated explicit OPENCUT_WEB_ROOT and OPENCUT_WEB_BINARY",
    );
  const files = [];
  async function walk(relative = "") {
    for (const e of await fs.readdir(
      path.join(process.env.OPENCUT_WEB_ROOT, relative),
      { withFileTypes: true },
    )) {
      const name = path.join(relative, e.name);
      if (e.isSymbolicLink()) throw Error(`Unhashed web symlink ${name}`);
      if (e.isDirectory()) await walk(name);
      else if (/\.(?:js|mjs|cjs|css)$/u.test(name))
        files.push([
          name,
          sha(await fs.readFile(path.join(process.env.OPENCUT_WEB_ROOT, name))),
        ]);
    }
  }
  await walk();
  files.sort(([a], [b]) => a.localeCompare(b));
  return {
    binary_sha256: sha(await fs.readFile(process.env.OPENCUT_WEB_BINARY)),
    web_js_css_tree_sha256: sha(JSON.stringify(files)),
  };
}
async function fixedFont(project) {
  const directory = process.env.OPENCUT_B1_FONT_SOURCES;
  if (!directory)
    return {
      reason:
        "TextMask independent pixels require OPENCUT_B1_FONT_SOURCES from the existing licensed fixture; no system-font fallback",
    };
  const manifest = await read(path.join(directory, "manifest.json"));
  for (const font of project.fonts.filter(
    (f) => f.weight === 400 && f.style === "normal" && f.face_index === 0,
  )) {
    const original = manifest.files.find(
      (f) => f.sha256 === font.content.content_hash && f.file.endsWith(".ttf"),
    );
    if (!original) continue;
    const bytes = await fs.readFile(
      path.join(directory, path.basename(original.file)),
    );
    expect(sha(bytes)).toBe(font.content.content_hash);
    return { font, bytes };
  }
  return {
    reason:
      "No reused fixed 400/normal face has matching independent original font bytes",
  };
}
// A separate raw Canvas glyph raster from original font bytes and authored text.
// No renderer imports, plans, glyph proofs, layouts or observed product pixels.
async function textMaskAlpha(page, bytes) {
  return page.evaluate(async (base64) => {
    const face = new FontFace(
      "SceneStackIndependent",
      Uint8Array.from(atob(base64), (c) => c.charCodeAt(0)),
      { weight: "400", style: "normal" },
    );
    await face.load();
    document.fonts.add(face);
    try {
      const canvas = new OffscreenCanvas(320, 180),
        ctx = canvas.getContext("2d");
      if (!ctx) throw Error("Independent glyph Canvas unavailable");
      ctx.font = 'normal 400 48px "SceneStackIndependent"';
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.letterSpacing = "0px";
      ctx.fillStyle = "white";
      ctx.fillText("HI", 160, 90);
      const raw = ctx.getImageData(0, 0, 320, 180).data;
      return Array.from({ length: 320 * 180 }, (_, i) => raw[i * 4 + 3]);
    } finally {
      document.fonts.delete(face);
    }
  }, bytes.toString("base64"));
}
function authoredSequence(original, cases) {
  const sequence = structuredClone(original),
    main = structuredClone(
      sequence.tracks.find((t) => t.id === sequence.main_track_id),
    );
  if (!main || main.kind !== "video")
    throw Error("Reused fixture needs its original main video track");
  main.elements = [];
  main.channels = [];
  main.enabled = true;
  main.locked = false;
  main.muted = true;
  const track = {
    id: "scene-stack-graphics",
    name: "Scene independent source cases",
    kind: "graphic",
    enabled: true,
    muted: false,
    locked: false,
    opacity: 1,
    gain: { unit: "decibels", value: 0 },
    channels: [],
    elements: [],
  };
  for (const c of cases) {
    const rect = c.rect ?? { left: 0, top: 0, width: W, height: H },
      contain = Math.min(W / rect.width, H / rect.height);
    track.elements.push({
      id: `scene-stack-${c.id}`,
      name: c.id,
      timeline_range: { start: time(c.start), duration: time(c.duration) },
      source_span: {
        profile: "classic_trimmed",
        trim_start: time(0),
        trim_end: time(0),
        source_duration: null,
      },
      enabled: true,
      content: {
        kind: "graphic",
        graphic: {
          geometry: { kind: "rectangle", corner_radius_percent: 0 },
          source_size: { width: rect.width, height: rect.height },
          fill: gray(c.front),
          stroke: { width_px: 0, align: "inside", color: gray(0) },
        },
      },
      channels: [],
      visual: {
        profile: "classic_canvas_v31",
        opacity: 1,
        blend_mode: "normal",
        transform: {
          position_x_px: rect.left + rect.width / 2 - W / 2,
          position_y_px: rect.top + rect.height / 2 - H / 2,
          scale_x: 1 / contain,
          scale_y: 1 / contain,
          rotation_degrees: 0,
        },
        masks: structuredClone(c.masks),
        effects: c.element_blur
          ? [
              {
                id: `scene-stack-effect-${c.id}`,
                enabled: true,
                effect: { kind: "classic_blur", intensity: c.element_blur },
              },
            ]
          : [],
      },
    });
  }
  const effectCase = cases.find((c) => c.scene_blur),
    adjustment = {
      id: "scene-stack-adjustment",
      name: "Scene adjustment boundary",
      kind: "effect",
      enabled: true,
      muted: false,
      locked: false,
      opacity: 1,
      gain: { unit: "decibels", value: 0 },
      channels: [],
      elements: effectCase
        ? [
            {
              id: "scene-stack-adjustment-element",
              name: "Independent scene blur",
              enabled: true,
              timeline_range: {
                start: time(effectCase.start),
                duration: time(effectCase.duration),
              },
              source_span: {
                profile: "classic_trimmed",
                trim_start: time(0),
                trim_end: time(0),
                source_duration: null,
              },
              visual: null,
              channels: [],
              content: {
                kind: "effect_layer",
                effect: {
                  id: "scene-stack-adjustment-blur",
                  enabled: true,
                  effect: {
                    kind: "classic_blur",
                    intensity: effectCase.scene_blur,
                  },
                },
              },
            },
          ]
        : [],
    };
  sequence.tracks = [main, track, adjustment];
  sequence.captions = [];
  sequence.transitions = [];
  // Sequence replacement must retain annotations; bookmark edits have their
  // own frame-clock operations. They do not affect these output probes.
  return sequence;
}
function observations(raw, probes) {
  expect(raw.length).toBe(W * H * 4);
  return probes.map((p) => {
    const actual = Array.from(
      raw.subarray((p.y * W + p.x) * 4, (p.y * W + p.x) * 4 + 4),
    );
    return {
      ...p,
      actual,
      max_channel_error: Math.max(
        ...actual.map((v, c) => Math.abs(v - p.expected[c])),
      ),
    };
  });
}
function assertPixels(samples, tolerance, label) {
  expect(samples.length).toBeGreaterThanOrEqual(8);
  for (const p of samples)
    expect(
      p.max_channel_error,
      `${label} (${p.x},${p.y}) expected=${p.expected} actual=${p.actual}`,
    ).toBeLessThanOrEqual(tolerance);
}
async function decode(file, seconds = null) {
  return (
    await execute(
      process.env.OPENCUT_FFMPEG ?? "ffmpeg",
      [
        "-hide_banner",
        "-loglevel",
        "error",
        ...(seconds === null ? [] : ["-ss", String(seconds)]),
        "-i",
        file,
        "-frames:v",
        "1",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgba",
        "pipe:1",
      ],
      { encoding: "buffer", maxBuffer: 32 * 1024 * 1024 },
    )
  ).stdout;
}
async function pollJob(tool, id) {
  let final;
  await expect
    .poll(
      async () => {
        final = await tool("opencut_job_view", { job_id: id });
        return ["succeeded", "failed", "cancelled"].includes(final.state)
          ? final.state
          : "pending";
      },
      { timeout: 300000, intervals: [250, 500, 1000] },
    )
    .not.toBe("pending");
  expect(final.state, JSON.stringify(final.failure)).toBe("succeeded");
  return final;
}
/** `page` accepts an existing Page. A Browser is also accepted for verify-suite's
 * existing signature; its fourth check callback is adapted without changing the
 * suite. For Page callers, optional `tool` is the authenticated tool function.
 * Optional phases: OPENCUT_SCENE_STACK_WEBM=1 and ..._BACKGROUND=1. */
export async function verifySceneStack(page, endpoint, directory, tool) {
  let owned = false,
    check = () => {};
  if (typeof page.newPage === "function") {
    const browser = page;
    check = typeof tool === "function" ? tool : check;
    page = await browser.newPage({
      locale: "en-US",
      viewport: { width: 1440, height: 1100 },
      acceptDownloads: true,
    });
    tool = undefined;
    owned = true;
  }
  page.setDefaultTimeout(30000);
  const errors = [],
    onError = (e) => errors.push(e.message);
  page.on("pageerror", onError);
  const checkpoint = path.join(directory, "d2c-scene-stack-checkpoint.json"),
    sourceFile = path.join(directory, "d2b-mask-checkpoint.json"),
    source = await read(sourceFile),
    candidate = await candidateIdentity();
  let facts = {
    schema_version: 1,
    candidate,
    project_id: randomUUID(),
    sequence_id: source.sequence_id,
    source: {
      project_id: source.project_id,
      revision: source.revision,
      checkpoint_sha256: sha(await fs.readFile(sourceFile)),
    },
    stages: {},
    jobs: {},
    pending_scopes: [],
  };
  try {
    const old = await read(checkpoint);
    expect(old.candidate).toEqual(candidate);
    expect(old.source.project_id).toBe(source.project_id);
    expect(old.source.revision).toBe(source.revision);
    facts = old;
  } catch (e) {
    if (e.code !== "ENOENT") throw e;
  }
  const save = async () => {
    await fs.writeFile(
      `${checkpoint}.next`,
      JSON.stringify(facts, null, 2) + "\n",
    );
    await fs.rename(`${checkpoint}.next`, checkpoint);
  };
  const operation = async (stage, name, input) => {
    const record =
      facts.stages[stage] ??
      (facts.stages[stage] = { name, input: structuredClone(input) });
    expect(record.name).toBe(name);
    await save();
    if (!record.data) {
      record.data = await tool(name, record.input);
      await save();
    }
    return record.data;
  };
  facts.status = "running";
  delete facts.failure;
  await save();
  try {
    await page.goto(`${endpoint}/projects`);
    await expect(
      page.getByRole("button", {
        name: "Restore project package",
        exact: true,
      }),
    ).toBeEnabled();
    tool ??= async (name, input) => {
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
    const inspect = (id = facts.project_id, revision = null) =>
      editorSnapshot(tool("opencut_project_inspect", { project_id: id, revision }));
    const original =
      facts.original ?? (await inspect(source.project_id, source.revision));
    facts.original = original;
    await save();
    await operation("copy", "opencut_project_copy", {
      source_project_id: source.project_id,
      source_revision: source.revision,
      project_id: facts.project_id,
      name: "Scene stack independent acceptance",
      idempotency_key: `scene-stack-copy-${facts.project_id}`,
    });
    const independentFont = await fixedFont(original.project);
    facts.pending_scopes = [];
    if (!independentFont.font)
      facts.pending_scopes.push({
        scope: "TextMask independent glyph pixels",
        reason: independentFont.reason,
      });
    const cases = sceneStackCases(
      independentFont.font?.id ?? "unavailable",
    ).filter((c) => !c.needs_font || independentFont.font);
    const fixtureScene = authoredSequence(
        original.project.sequences.find((s) => s.id === facts.sequence_id),
        cases,
      ),
      settings = structuredClone(original.project.settings);
    expect(settings.profile).toBe("classic_global");
    settings.settings.canvas_size = { width: W, height: H };
    settings.settings.fps = { numerator: 30, denominator: 1 };
    settings.settings.background = { kind: "color", color: gray(32) };
    if (facts.authored_cases)
      expect(
        cases,
        "The resumed fixture's fixed font/case inventory changed; retain the original fixture inputs",
      ).toEqual(facts.authored_cases);
    facts.authored_cases = cases;
    facts.fixture_sequence = fixtureScene;
    if (!facts.stages.setup) {
      const before = await inspect();
      facts.stages.setup = { name: "opencut_edit_apply", input: {
        project_id: facts.project_id,
        expected_revision: before.revision,
        idempotency_key: `scene-stack-setup-${facts.project_id}`,
        label: "Independent static Scene reference cases",
        operations: [
          { type: "settings_set", settings },
          { type: "sequence_put", sequence: fixtureScene },
        ],
      } };
      await save();
    }
    const setup = await operation("setup", "opencut_edit_apply", facts.stages.setup.input);
    facts.revision = setup.revision;
    const actual = await inspect(facts.project_id, facts.revision);
    expect(
      actual.project.sequences.find((s) => s.id === facts.sequence_id),
    ).toEqual(fixtureScene);
    expect(actual.project.assets).toEqual(original.project.assets);
    expect(actual.project.fonts).toEqual(original.project.fonts);
    const fontAlpha = independentFont.font
      ? await textMaskAlpha(page, independentFont.bytes)
      : null;
    const oracleSources = [
      "scene-stack-oracle.mjs",
      "mask-oracle.mjs",
      "blend-oracle.mjs",
      "graphic-output-oracle.mjs",
    ];
    const referenceKey = sha(
      JSON.stringify({
        model: SCENE_STACK_ORACLE_MODEL,
        cases,
        font_hash: independentFont.font?.content.content_hash ?? null,
        font_alpha_sha256: fontAlpha ? sha(Uint8Array.from(fontAlpha)) : null,
        sources: await Promise.all(
          oracleSources.map(async (n) => [
            n,
            sha(await fs.readFile(new URL(n, import.meta.url))),
          ]),
        ),
      }),
    );
    facts.oracle = {
      model: SCENE_STACK_ORACLE_MODEL,
      key: referenceKey,
      authored_source: true,
      product_plans_used_for_expected: false,
      hard_complex_edge_aa_excluded: true,
      png_tolerance: 2,
      video_tolerance: 6,
      scope:
        "Pure authored geometry, off-center binary/analytic split probes, independent small-sigma Gaussian and original-font Canvas glyphs; not all AA/crop/animation/performance combinations",
    };
    await save();
    const references = new Map();
    for (const c of cases) {
      const expected = referenceSceneCase(c, c.needs_font ? fontAlpha : null),
        reference = sceneReferenceProbes(
          c,
          expected,
          c.needs_font ? fontAlpha : null,
        );
      references.set(c.id, { expected, ...reference });
      const stage = `png-${c.id}`,
        rec = facts.jobs[stage] ?? (facts.jobs[stage] = {});
      const submission = await operation(
        `job-${stage}`,
        "opencut_render_submit",
        { kind: "frame", request: {
          project_id: facts.project_id,
          revision: facts.revision,
          sequence_id: facts.sequence_id,
          time: time(c.at),
          idempotency_key: `scene-stack-png-${facts.project_id}-${c.id}`,
        } },
      );
      rec.submission = submission;
      await save();
      const job = await pollJob(tool, submission.job_id);
      rec.job = job;
      await save();
      expect(job.result?.kind).toBe("render.frame");
      expect(job.result.data.identity).toMatchObject({
        project_id: facts.project_id,
        revision: facts.revision,
        sequence_id: facts.sequence_id,
        width: W,
        height: H,
        profile: PROFILE,
      });
      const file = await downloadArtifact(
          page,
          endpoint,
          directory,
          job,
          "image/png",
          rec,
          save,
        ),
        raw = await decode(file);
      rec.reference = {
        key: referenceKey,
        expected_rgba_sha256: sha(expected),
        negative_controls: reference.negative_controls,
      };
      rec.observation = {
        artifact_sha256: rec.output.sha256,
        rgba_sha256: sha(raw),
        samples: observations(raw, reference.points),
        passed: false,
      };
      await save();
      assertPixels(rec.observation.samples, 2, c.id);
      rec.observation.passed = true;
      await save();
    }
    if (process.env.OPENCUT_SCENE_STACK_WEBM === "1") {
      const rec = facts.jobs.webm ?? (facts.jobs.webm = {});
      const submission = await operation(
        "job-webm",
        "opencut_render_submit",
        { kind: "video", request: {
          project_id: facts.project_id,
          revision: facts.revision,
          sequence_id: facts.sequence_id,
          idempotency_key: `scene-stack-webm-muted-${facts.project_id}`,
          options: {
            format: "webm",
            quality: "very_high",
            include_audio: false,
          },
        } },
      );
      rec.submission = submission;
      await save();
      const job = await pollJob(tool, submission.job_id);
      rec.job = job;
      await save();
      expect(job.result?.kind).toBe("render.video");
      expect(job.result.data.identity).toMatchObject({
        project_id: facts.project_id,
        revision: facts.revision,
        sequence_id: facts.sequence_id,
        width: W,
        height: H,
        visual_profile: "scene_v1",
        options: { format: "webm", quality: "very_high", include_audio: false },
        audio_frame_count: null,
      });
      const file = await downloadArtifact(
          page,
          endpoint,
          directory,
          job,
          "video/webm",
          rec,
          save,
        ),
        probe = JSON.parse(
          (
            await execute(
              process.env.OPENCUT_FFPROBE ?? "ffprobe",
              [
                "-v",
                "error",
                "-count_frames",
                "-show_streams",
                "-show_format",
                "-of",
                "json",
                file,
              ],
              { encoding: "utf8", maxBuffer: 4 * 1024 * 1024 },
            )
          ).stdout,
        );
      rec.probe = probe;
      await save();
      const video = probe.streams.filter((s) => s.codec_type === "video");
      expect(video).toHaveLength(1);
      expect(
        probe.streams.filter((s) => s.codec_type === "audio"),
      ).toHaveLength(0);
      expect(video[0]).toMatchObject({
        codec_name: "vp9",
        width: W,
        height: H,
        nb_read_frames: String(cases.length * 15),
      });
      rec.observations = [];
      for (const c of cases) {
        const ref = references.get(c.id),
          projected = await observeYuv420Rgba(ref.expected),
          probes = ref.points.map((p) => ({
            ...p,
            expected: Array.from(
              projected.subarray((p.y * W + p.x) * 4, (p.y * W + p.x) * 4 + 4),
            ),
          })),
          raw = await decode(file, c.at);
        const observation = {
          case: c.id,
          seconds: c.at,
          reference_key: referenceKey,
          artifact_sha256: rec.output.sha256,
          rgba_sha256: sha(raw),
          samples: observations(raw, probes),
          passed: false,
        };
        rec.observations.push(observation);
        await save();
        assertPixels(observation.samples, 6, `VP9 ${c.id}`);
        observation.passed = true;
        await save();
      }
    } else
      facts.pending_scopes.push({
        scope: "WebM very-high quality, no audio stream, actual ffprobe/decode",
        reason:
          "Enable OPENCUT_SCENE_STACK_WEBM=1 during unified verification; no job is submitted by default",
      });
    if (process.env.OPENCUT_SCENE_STACK_BACKGROUND === "1")
      await backgroundPhase({
        page,
        endpoint,
        directory,
        facts,
        original,
        tool,
        operation,
        inspect,
        save,
        settings,
      });
    else
      facts.pending_scopes.push({
        scope: "MainTrackBlur independent source-frame pixels",
        reason:
          "Optional OPENCUT_SCENE_STACK_BACKGROUND=1 requires reused sample.mp4 hash match and spatially discriminating source pixels",
      });
    expect(errors).toEqual([]);
    facts.pageerrors = errors;
    facts.status = facts.pending_scopes.length ? "passed_scoped" : "passed";
    facts.whole_scene_migration_complete = false;
    await save();
    check("Scene stack scoped independent PNG pixels and negative controls", {
      checkpoint,
      project_id: facts.project_id,
      revision: facts.revision,
      candidate,
      oracle: facts.oracle,
      case_count: cases.length,
      pending_scopes: facts.pending_scopes,
      whole_scene_migration_complete: false,
    });
    return facts;
  } catch (error) {
    facts.status = "failed";
    facts.failure = String(error);
    facts.pageerrors = errors;
    await save();
    throw error;
  } finally {
    page.off("pageerror", onError);
    if (owned) await page.close();
  }
}
async function downloadArtifact(
  page,
  endpoint,
  directory,
  job,
  mime,
  rec,
  save,
) {
  const artifact = job.artifacts.find((a) => a.mime_type === mime);
  expect(artifact).toBeTruthy();
  expect(artifact.artifact_id).toBe(
    `artifact:${job.job_id}:${artifact.content_hash}`,
  );
  const file = path.join(
    directory,
    `scene-stack-${job.job_id}.${mime === "image/png" ? "png" : "webm"}`,
  );
  let bytes;
  try {
    bytes = await fs.readFile(file);
  } catch (e) {
    if (e.code !== "ENOENT") throw e;
  }
  if (!bytes || sha(bytes) !== artifact.content_hash) {
    const response = await page.request.get(
      `${endpoint}/api/artifacts/${encodeURIComponent(artifact.artifact_id)}`,
    );
    expect(response.ok()).toBe(true);
    bytes = await response.body();
    expect(sha(bytes)).toBe(artifact.content_hash);
    await fs.writeFile(`${file}.next`, bytes);
    await fs.rename(`${file}.next`, file);
  }
  expect(sha(bytes)).toBe(artifact.content_hash);
  rec.output = { file, sha256: sha(bytes), artifact };
  await save();
  return file;
}
async function backgroundPhase({
  page,
  endpoint,
  directory,
  facts,
  original,
  tool,
  operation,
  inspect,
  save,
  settings,
}) {
  const video = original.project.sequences
      .find((s) => s.id === facts.sequence_id)
      .tracks.flatMap((t) => t.elements)
      .find((e) => e.content.kind === "video"),
    sample = process.env.OPENCUT_SCENE_STACK_BACKGROUND_SOURCE ?? path.join(directory, "sample.mp4");
  if (!video) {
    facts.pending_scopes.push({
      scope: "MainTrackBlur",
      reason: "Original reused fixture has no video",
    });
    return;
  }
  let asset = original.project.assets.find(
    (a) => a.id === video.content.asset_id,
  );
  if (!process.env.OPENCUT_SCENE_STACK_BACKGROUND_SOURCE && sha(await fs.readFile(sample)) !== asset?.version.content_hash) {
    facts.pending_scopes.push({
      scope: "MainTrackBlur",
      reason:
        "Original sample.mp4 bytes do not match the reused fixed video asset; refusing another media copy",
    });
    return;
  }
  const raw = (
    await execute(
      process.env.OPENCUT_FFMPEG ?? "ffmpeg",
      [
        "-hide_banner",
        "-loglevel",
        "error",
        "-ss",
        "0.25",
        "-i",
        sample,
        "-vf",
        `scale=${W}:${H}:force_original_aspect_ratio=increase:flags=bilinear,crop=${W}:${H}`,
        "-frames:v",
        "1",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgba",
        "pipe:1",
      ],
      { encoding: "buffer", maxBuffer: 32 * 1024 * 1024 },
    )
  ).stdout;
  expect(raw.length).toBe(W * H * 4);
  const expected = gaussianSceneRgba(raw, 60),
    points = [];
  let contrast = 0;
  for (const y of [12, 24, 40, 140, 156, 168])
    for (const x of [12, 32, 64, 256, 288, 308]) {
      const p = (y * W + x) * 4,
        delta = Math.max(
          ...[0, 1, 2].map((k) => Math.abs(raw[p + k] - expected[p + k])),
        );
      if (delta > 12) {
        points.push({
          x,
          y,
          expected: Array.from(expected.subarray(p, p + 4)),
        });
        contrast = Math.max(contrast, delta);
      }
    }
  if (points.length < 8) {
    facts.pending_scopes.push({
      scope: "MainTrackBlur",
      reason:
        "Reused source is spatially flat or has fewer than eight independent blur-discriminating exterior probes; no center-constant acceptance",
    });
    return;
  }
  if (process.env.OPENCUT_SCENE_STACK_BACKGROUND_SOURCE)
    asset = await bindBackgroundSource({ page, endpoint, file: sample, facts, operation, inspect, pollJob, tool, save });
  const sequence = structuredClone(facts.fixture_sequence),
    main = sequence.tracks.find((t) => t.id === sequence.main_track_id),
    element = structuredClone(video);
  element.id = "scene-stack-background-video";
  element.content.asset_id = asset.id;
  element.channels = [];
  element.timeline_range = { start: time(0), duration: time(0.5) };
  element.source_span = {
    profile: "classic_trimmed",
    trim_start: time(0),
    trim_end: time(0),
    source_duration: null,
  };
  element.content.source_audio_enabled = false;
  element.content.audio.muted = true;
  element.content.retime = {
    rate: { numerator: 1, denominator: 1 },
    maintain_pitch: false,
    mapping: { kind: "classic_rounded_ticks", timebase: 120000 },
  };
  element.visual = {
    profile: "classic_canvas_v31",
    opacity: 1,
    blend_mode: "normal",
    effects: [],
    masks: [],
    transform: {
      position_x_px: 0,
      position_y_px: 0,
      scale_x: 0.25,
      scale_y: 0.25,
      rotation_degrees: 0,
    },
  };
  main.elements = [element];
  sequence.tracks = [main];
  const background = structuredClone(settings);
  background.settings.background = { kind: "main_track_blur", intensity: 60 };
  if (!facts.stages["background-setup"]) {
    const before = await inspect();
    facts.stages["background-setup"] = { name: "opencut_edit_apply", input: {
      project_id: facts.project_id,
      expected_revision: before.revision,
      idempotency_key: `scene-stack-background-${facts.project_id}`,
      label: "Reused source independent backdrop blur",
      operations: [
        { type: "settings_set", settings: background },
        { type: "sequence_put", sequence },
      ],
    } };
    await save();
  }
  const setup = await operation("background-setup", "opencut_edit_apply", facts.stages["background-setup"].input);
  const revision = setup.revision,
    rec = facts.jobs.background ?? (facts.jobs.background = {}),
    submission = await operation(
      "job-background",
      "opencut_render_submit",
      { kind: "frame", request: {
        project_id: facts.project_id,
        revision,
        sequence_id: facts.sequence_id,
        time: time(0.25),
        idempotency_key: `scene-stack-background-png-${facts.project_id}`,
      } },
    );
  rec.submission = submission;
  await save();
  const job = await pollJob(tool, submission.job_id);
  rec.job = job;
  const file = await downloadArtifact(
      page,
      endpoint,
      directory,
      job,
      "image/png",
      rec,
      save,
    ),
    actual = await decode(file);
  rec.oracle = {
    original_source_sha256: asset.version.content_hash,
    expected_rgba_sha256: sha(expected),
    negative_unblurred_max_delta: contrast,
  };
  rec.observation = { samples: observations(actual, points), passed: false };
  await save();
  assertPixels(rec.observation.samples, 2, "MainTrackBlur");
  rec.observation.passed = true;
  await save();
}
