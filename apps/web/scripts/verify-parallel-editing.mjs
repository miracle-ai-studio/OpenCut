import { editorSnapshot } from "./project-inspection.mjs";
// Preparation only: importing this module performs no IO or browser action.
// One document copy shares the existing synthetic workspace's CAS. No upload,
// media copy, font import, render/export Job, or replacement retry key is used.
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash, randomUUID } from "node:crypto";
import { verifyParallelEditing as verifyEditingLifecycle } from "./verify-editing-lifecycle.mjs";

const MODEL = "parallel-editing-authored-fixture-v1";
const W = 320, H = 180;
const sha = value => createHash("sha256").update(value).digest("hex");
const read = async file => JSON.parse(await fs.readFile(file, "utf8"));
const time = value => ({ ticks: String(Math.round(value * 120000)), timebase: 120000 });
const seconds = value => Number(value.ticks) / value.timebase;
const gray = value => ({ encoding: "srgb", red: value / 255, green: value / 255, blue: value / 255, alpha: 1 });
const rgba = value => [value, value, value, 255];

async function identity() {
  const root = process.env.OPENCUT_WEB_ROOT, binary = process.env.OPENCUT_WEB_BINARY;
  if (!root || !binary) throw Error("Explicit integrated OPENCUT_WEB_ROOT and OPENCUT_WEB_BINARY are required");
  const files = [];
  async function walk(relative = "") {
    for (const entry of await fs.readdir(path.join(root, relative), { withFileTypes: true })) {
      const name = path.join(relative, entry.name);
      if (entry.isSymbolicLink()) throw Error(`Unhashed web symlink ${name}`);
      if (entry.isDirectory()) await walk(name);
      else if (/\.(?:m?js|cjs|css)$/u.test(name)) files.push([name, sha(await fs.readFile(path.join(root, name)))]);
    }
  }
  await walk();
  files.sort(([a], [b]) => a.localeCompare(b));
  return { binary_sha256: sha(await fs.readFile(binary)), web_js_css_tree_sha256: sha(JSON.stringify(files)),
    helper_sha256: sha(await fs.readFile(new URL("./verify-editing-lifecycle.mjs", import.meta.url))),
    wrapper_sha256: sha(await fs.readFile(new URL(import.meta.url))) };
}

function scalar(elementId, parameter, first, last, dv = 0.1) {
  const id = `${elementId}-${parameter}`;
  return { id, kind: "scalar", target: { kind: "element", element_id: elementId },
    binding: { kind: "property", parameter }, extrapolation: { before: "hold", after: "hold" }, keys: [
      { id: `${id}-a`, time: time(0), value: first, left_handle: null,
        right_handle: { dt: time(0.4), dv }, segment_to_next: { kind: "bezier" }, tangent_mode: "broken" },
      { id: `${id}-b`, time: time(3), value: last, left_handle: { dt: time(-0.4), dv: -dv },
        right_handle: null, segment_to_next: { kind: "linear" }, tangent_mode: "broken" },
    ] };
}

// Independent cubic Bernstein polynomial and bisection; neither production
// channel sampler nor its implementation is imported to supply expectations.
function expectedCurve(channel, at) {
  const [a, b] = channel.keys, t0 = seconds(a.time), t3 = seconds(b.time);
  const cubic = (p, u) => p[0] * (1-u)**3 + 3*p[1]*(1-u)**2*u + 3*p[2]*(1-u)*u*u + p[3]*u**3;
  const x = [t0, t0 + seconds(a.right_handle.dt), t3 + seconds(b.left_handle.dt), t3];
  const y = [a.value, a.value + a.right_handle.dv, b.value + b.left_handle.dv, b.value];
  let lo = 0, hi = 1;
  for (let i = 0; i < 60; i++) { const mid = (lo+hi)/2; if (cubic(x, mid) < at) lo = mid; else hi = mid; }
  return cubic(y, (lo+hi)/2);
}

function graphic(id, x, y, width, height, color) {
  const contain = Math.min(W / width, H / height);
  return { id, name: id, enabled: true, timeline_range: { start: time(0), duration: time(3) },
    source_span: { profile: "classic_trimmed", trim_start: time(0), trim_end: time(0), source_duration: null }, channels: [],
    content: { kind: "graphic", graphic: { geometry: { kind: "rectangle", corner_radius_percent: 0 },
      source_size: { width, height }, fill: gray(color), stroke: { width_px: 0, align: "inside", color: gray(0) } } },
    visual: { profile: "classic_canvas_v31", opacity: 1, blend_mode: "normal", masks: [], effects: [],
      transform: { position_x_px: x-W/2, position_y_px: y-H/2, scale_x: 1/contain, scale_y: 1/contain, rotation_degrees: 0 } } };
}
function track(id, kind, elements) {
  return { id, name: id, kind, enabled: true, muted: false, locked: false, opacity: 1,
    gain: { unit: "decibels", value: 0 }, channels: [], elements };
}

function fixtureFor(project, sequenceId) {
  const original = project.sequences.find(s => s.id === sequenceId);
  if (!original) throw Error("The original fixed scene is missing; fixture cannot pass");
  const sound = original.tracks.flatMap(t => t.elements).find(e => e.content.kind === "audio" &&
    seconds(e.timeline_range.duration) >= 3 && seconds(e.timeline_range.start) === 0);
  if (!sound) throw Error("Existing D0 3-second audio element required; no new fixture media will be created");
  const audio = structuredClone(sound);
  audio.id = "f46-audio"; audio.name = "F46 existing CAS audio"; audio.enabled = true;
  audio.timeline_range = { start: time(0), duration: time(3) };
  audio.content.audio = { ...audio.content.audio, muted: false, gain: { unit: "decibels", value: 0 } };
  audio.channels = [scalar(audio.id, "gain_decibels", -18, -6, 2)];
  const a = graphic("f46-move-a", 52, 44, 40, 40, 224), b = graphic("f46-move-b", 132, 44, 40, 40, 128),
    hit = graphic("f46-alpha", 240, 50, 48, 48, 240), animated = graphic("f46-animated", 125, 135, 32, 32, 192),
    splitA = graphic("f46-split-a", 45, 135, 30, 30, 224), splitB = graphic("f46-split-b", 220, 135, 30, 30, 128);
  hit.visual.masks = [{ id: "f46-alpha-mask", enabled: true, inverted: false,
    shape: { kind: "ellipse", geometry: { center_x: 0, center_y: 0, width: 0.5, height: 0.5, rotation_degrees: 0, scale: 1 } },
    feather: { unit: "classic_pixels", value: 0 }, stroke: { width_px: 0, align: "center", color: gray(255) } }];
  const base = animated.visual.transform.scale_x;
  animated.channels = [scalar(animated.id, "scale_x", base, base*1.4, base*0.1),
    scalar(animated.id, "scale_y", base, base*1.6, base*0.1), scalar(animated.id, "opacity", 0.6, 0.9, 0.05)];
  splitA.channels = [scalar(splitA.id, "opacity", 0.3, 0.9)];
  splitB.channels = [scalar(splitB.id, "opacity", 0.8, 0.4, -0.1)];
  const sequence = structuredClone(original);
  // Retain the existing main-track ID and all asset/font records. Only this
  // copied scene is replaced; the source project/head is never edited.
  sequence.tracks = [track(original.main_track_id, "video", []),
    ...[a,b,hit,animated,splitA,splitB].map(e => track(`${e.id}-track`, "graphic", [e])), track("f46-audio-track", "audio", [audio])];
  sequence.captions = []; sequence.transitions = [];
  const settings = structuredClone(project.settings);
  if (settings.profile !== "classic_global") throw Error("F46 requires the existing Classic global fixture profile");
  settings.settings.canvas_size = { width: W, height: H };
  settings.settings.fps = { numerator: 30, denominator: 1 };
  settings.settings.background = { kind: "color", color: gray(32) };
  const probes = [
    { x: 40, y: 32, before: rgba(224), during: rgba(32) },
    { x: 88, y: 50, before: rgba(32), during: rgba(224) },
    { x: 120, y: 32, before: rgba(128), during: rgba(32) },
    { x: 168, y: 50, before: rgba(32), during: rgba(128) },
  ].map(p => ({ ...p, tolerance: 1 }));
  const input = { sequenceId, transform: { elementId: a.id, additionalElementIds: [b.id], timeSeconds: 0.5,
    deltaCanvas: { x: 24, y: 12 }, pixelProbes: probes },
    alpha: { timeSeconds: 0.5, width: W, height: H, probes: [
      { x: 40, y: 32, elementId: a.id, expected: rgba(224) },
      { x: 240, y: 50, elementId: hit.id, expected: rgba(240) },
      { x: 225, y: 35, elementId: null, expected: rgba(32) },
      { x: 120, y: 32, elementId: b.id, expected: rgba(128) },
      { x: 300, y: 150, elementId: null, expected: rgba(32) },
    ] },
    gain: { elementId: audio.id, channelId: audio.channels[0].id, timeSeconds: 0.5, dy: -8 },
    keyframes: { elementId: splitA.id, channelId: splitA.channels[0].id, keyId: splitA.channels[0].keys[0].id, valueDelta: 0.1 },
    split: { timeSeconds: 1.5, elements: [splitA,splitB].map(e => ({ elementId: e.id, channelId: e.channels[0].id,
      probeTimes: [0.25,1,2,2.75].map(time), expectedValues: [0.25,1,2,2.75].map(t => expectedCurve(e.channels[0], t)) })) },
    animatedScale: { elementId: animated.id, timeSeconds: 0.5, xChannelId: animated.channels[0].id,
      yChannelId: animated.channels[1].id, deltaCanvas: 12 } };
  return { model: MODEL, settings, sequence, input, resources: { assets: project.assets, fonts: project.fonts } };
}

export async function verifyParallelEditing(browser, endpoint, directory, check) {
  const sourceFile = path.resolve(directory, process.env.OPENCUT_PARALLEL_EDITING_SOURCE ?? "b4-graphic-output-checkpoint.json"),
    source = await read(sourceFile), candidate = await identity(), checkpoint = path.join(directory, "f46-parallel-editing-checkpoint.json");
  if (!source.project_id || !source.revision || !source.sequence_id)
    throw Error("Fixed D0 source project/revision/sequence checkpoint required; missing fixture is not acceptance");
  const sourceRef = { project_id: source.project_id, revision: source.revision, sequence_id: source.sequence_id,
    checkpoint: path.basename(sourceFile), sha256: sha(await fs.readFile(sourceFile)) };
  let facts;
  try { facts = await read(checkpoint); }
  catch (error) { if (error.code !== "ENOENT") throw error; }
  if (facts) {
    expect(facts.candidate, "Preserve old candidate evidence; do not resume it with a different binary/Web/helper").toEqual(candidate);
    expect(facts.source).toEqual(sourceRef);
    expect(facts.model).toBe(MODEL);
  } else facts = { schema_version: 1, model: MODEL, candidate, source: sourceRef, project_id: randomUUID(), operations: {} };
  let queue = Promise.resolve();
  const save = () => {
    const bytes = JSON.stringify(facts, null, 2) + "\n";
    queue = queue.then(async () => { await fs.writeFile(`${checkpoint}.next`, bytes); await fs.rename(`${checkpoint}.next`, checkpoint); });
    return queue;
  };
  await save();
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
  page.setDefaultTimeout(30000);
  const errors = [], onError = e => errors.push(e.message);
  page.on("pageerror", onError);
  const tool = async (name, input) => {
    const envelope = await page.evaluate(async ({ name, input }) => {
      const response = await fetch(`/api/tools/${name}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input) });
      return response.json();
    }, { name, input });
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull();
    return envelope.data;
  };
  const inspect = (project_id = facts.project_id, revision = null) => editorSnapshot(tool("opencut_project_inspect", { project_id, revision }));
  const operation = async (id, name, input) => {
    const saved = facts.operations[id];
    if (saved) { expect(saved.name).toBe(name); expect(saved.input).toEqual(input); }
    const record = facts.operations[id] ??= { name, input: structuredClone(input) };
    await save();
    if (!record.reply) { record.reply = await tool(name, record.input); await save(); }
    return record.reply;
  };
  try {
    facts.status = "running"; delete facts.failure; await save();
    await page.goto(`${endpoint.replace(/\/$/u, "")}/projects`);
    await expect(page.getByRole("button", { name: "Restore project package", exact: true })).toBeEnabled();
    facts.original ??= await inspect(source.project_id, source.revision);
    facts.source_entering_head ??= await inspect(source.project_id);
    const authored = fixtureFor(facts.original.project, source.sequence_id);
    if (facts.fixture) expect(facts.fixture).toEqual(authored);
    else facts.fixture = authored;
    facts.fixture_sha256 = sha(JSON.stringify(authored));
    await save();
    await operation("copy", "opencut_project_copy", { source_project_id: source.project_id, source_revision: source.revision,
      project_id: facts.project_id, name: "F46 parallel editing independent fixture", idempotency_key: `f46-copy-${facts.project_id}` });
    if (!facts.operations.setup) {
      const before = await inspect();
      facts.operations.setup = { name: "opencut_edit_apply", input: { project_id: facts.project_id,
        expected_revision: before.revision, idempotency_key: `f46-setup-${facts.project_id}`, label: "F46 independent editing fixture",
        operations: [{ type: "settings_set", settings: authored.settings }, { type: "sequence_put", sequence: authored.sequence }] } };
      await save();
    }
    const setup = await operation("setup", "opencut_edit_apply", facts.operations.setup.input);
    const fixed = await inspect(facts.project_id, setup.revision);
    expect(fixed.project.sequences.find(s => s.id === source.sequence_id)).toEqual(authored.sequence);
    expect(fixed.project.assets).toEqual(authored.resources.assets);
    expect(fixed.project.fonts).toEqual(authored.resources.fonts);
    facts.editing ??= { schema_version: 1, project_id: facts.project_id, sequence_id: source.sequence_id, stages: {}, operations: {} };
    await save();
    const result = await verifyEditingLifecycle({ page, base: endpoint, projectId: facts.project_id, fixture: authored.input,
      checkpoint: facts.editing, save: async value => { facts.editing = value; await save(); },
      check: async (_name, run) => run() });
    expect(result.status, "Missing fixture or partial stages cannot pass F46").toBe("passed");
    const required = ["shortcuts", "transform-cancel", "transform-commit", "transform-final-cancel", "animated-scale-x",
      "alpha-selection", "gain-curve", "keyframe-ui", "multi-split-curves"];
    for (const id of required) expect(result.stages[id]?.verified, id).toBe(true);
    const final = await inspect();
    expect(final.project).toEqual(fixed.project);
    expect(await inspect(source.project_id)).toEqual(facts.source_entering_head);
    expect(errors).toEqual([]);
    facts.final_revision = final.revision; facts.status = "passed";
    delete facts.failure_view;
    facts.scope = "Five real UI editing parts plus V2 scale-axis preservation, canceled final draft, original-key replay and exact graph Undo; independent interior pixels/Bezier samples, not full-frame AA, PCM, OS clipboard or concurrency stress.";
    await save();
    check("F46 parallel editing: transform pixels/alpha/gain/keyframes/multi-split", { checkpoint, candidate, project_id: facts.project_id,
      fixture_sha256: facts.fixture_sha256, stages: required, final_revision: facts.final_revision, scope: facts.scope });
    return facts;
  } catch (error) {
    facts.status = "failed"; facts.failure = String(error); facts.page_errors = errors;
    facts.failure_view = await page.getByRole("main").evaluateAll(nodes => nodes.map(node => node.innerText).join("\n"));
    await page.screenshot({ path: path.join(directory, "f46-parallel-editing-failure.png"), fullPage: true });
    await save(); throw error;
  } finally { page.off("pageerror", onError); await page.close(); }
}
