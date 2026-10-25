import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash, randomUUID } from "node:crypto";
const hash = (value) => createHash("sha256").update(value).digest("hex");
const prefix = (label) =>
  new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?:\\s|$)`);
const source = `[Script Info]
ScriptType: v4.00+
PlayResX: 640
PlayResY: 360
WrapStyle: 2
[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Styled,Lato,52,&H403366CC,&H00000000,&H00000000,&H00000000,-1,-1,0,-1,125,125,3,0,1,0,0,7,20,20,30,1
Style: Normal,Lato,36,&H00FFFFFF,&H00000000,&H00000000,&H00000000,0,0,0,0,125,75,0,0,1,0,0,3,20,20,30,1
[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:00.00,0:00:02.00,Styled,,0,0,0,,{\\q2\\an7\\pos(200,100)\\frz-12}Styled ASS\\NKeeps position
Dialogue: 0,0:00:02.00,0:00:04.00,Normal,,0,0,0,,{\\q2}Different margins
`;
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
export async function verifyAssImport(browser, endpoint, directory, check) {
  const fixture = JSON.parse(
      await fs.readFile(path.join(directory, "b3-asr-fixture.json"), "utf8"),
    ),
    candidate = await identity(),
    filename = path.join(directory, "f23-ass-import-checkpoint.json");
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
      source,
      transactions: {},
      downloads: {},
      phase: "bootstrap",
    };
  expect(facts.source).toBe(source);
  const save = () =>
    fs.writeFile(filename, JSON.stringify(facts, null, 2) + "\n");
  const page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1100 },
  });
  page.setDefaultTimeout(30000);
  await page.addInitScript(() => {
    window.showSaveFilePicker = undefined;
  });
  const errors = [];
  page.on("pageerror", (e) => {
    if (errors.length < 32) errors.push(e.message.slice(0, 1024));
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
  const restore = async (stage, revision) => {
    facts.restores ??= {};
    let saved = facts.restores[stage];
    if (!saved) {
      saved = facts.restores[stage] = {
        request: {
          project_id: facts.project_id,
          expected_revision: (await inspect()).revision,
          target_revision: revision,
          idempotency_key: randomUUID(),
          asset_versions: {},
        },
      };
      await save();
    }
    expect(saved.request.target_revision).toBe(revision);
    if (!saved.receipt) {
      saved.receipt = await tool("opencut_project_restore", saved.request);
      await save();
    }
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
    const old = facts.transactions[stage];
    if (old?.receipt) return old.receipt;
    if (old?.request) {
      old.receipt = await tool(name, old.request);
      await save();
      return old.receipt;
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
      expect(envelope.error).toBeNull();
      facts.transactions[stage].receipt = envelope.data;
      await save();
      return envelope.data;
    } finally {
      page.off("request", listener);
    }
  };
  const planWrites = [];
  const planListener = (response) => {
    if (response.url().endsWith("/api/tools/opencut_subtitle_ass_plan")) {
      const pending = (async () => {
        facts.plans ??= [];
        if (facts.plans.length >= 4) return;
        facts.plans.push({
          request: response.request().postDataJSON(),
          envelope: await response.json(),
        });
        await save();
      })();
      planWrites.push(pending);
    }
  };
  page.on("response", planListener);
  let failure;
  try {
    await reopen();
    if (!facts.original) {
      facts.original = await inspect();
      await save();
    }
    if (facts.phase === "passed") {
      expect((await inspect()).project).toEqual(facts.original.project);
      check("F23 retained scoped ASS style evidence", facts);
      return facts;
    }
    const bold = facts.original.project.fonts.find(
        (f) => f.family === "Lato" && f.weight === 700 && f.style === "italic",
      ),
      normal = facts.original.project.fonts.find(
        (f) => f.family === "Lato" && f.weight === 400 && f.style === "normal",
      );
    expect(bold).toBeTruthy();
    expect(normal).toBeTruthy();
    const panel = page.locator(".editor-caption-workspace");
    if (!facts.transactions.import?.receipt) {
      facts.phase = "ASS preserve-style import";
      await save();
      await panel
        .getByRole("combobox", { name: prefix("Format") })
        .selectOption("ass");
      await panel
        .getByRole("textbox", { name: prefix("Subtitle content") })
        .fill(source);
      await transaction("import", "opencut_edit_apply", () =>
        panel
          .getByRole("button", { name: "Import subtitles", exact: true })
          .click(),
      );
    }
    await Promise.all(planWrites);
    if (!facts.imported) {
      facts.imported = await inspect(
        facts.transactions.import.receipt.revision,
      );
      const added = scene(facts.imported).tracks.filter(
        (t) => !scene(facts.original).tracks.some((old) => old.id === t.id),
      );
      expect(added).toHaveLength(1);
      facts.track_id = added[0].id;
      facts.element_id = added[0].elements[0].id;
      await save();
    }
    const track = scene(facts.imported).tracks.find(
      (t) => t.id === facts.track_id,
    );
    expect(track.kind).toBe("text");
    expect(track.elements).toHaveLength(2);
    const first = track.elements[0],
      second = track.elements[1];
    expect(first.content.text).toMatchObject({
      content: "Styled ASS\nKeeps position",
      font: { kind: "locked", font_id: bold.id },
      font_size: { unit: "canvas_pixels", value: 26 },
      font_weight: "bold",
      font_style: "italic",
      decoration: "line_through",
      align: "left",
      letter_spacing_px: 1.5,
      line_height_multiplier: 1.0,
      color: {
        encoding: "srgb",
        red: 204 / 255,
        green: 102 / 255,
        blue: 51 / 255,
        alpha: 191 / 255,
      },
    });
    expect(first.visual.transform).toMatchObject({
      scale_x: 1.25,
      scale_y: 1.25,
      rotation_degrees: 12,
    });
    expect(second.content.text).toMatchObject({
      content: "Different margins",
      font: { kind: "locked", font_id: normal.id },
      font_size: { unit: "canvas_pixels", value: 18 },
      align: "right",
    });
    expect(second.visual.transform).toMatchObject({
      scale_x: 1.25,
      scale_y: 0.75,
      rotation_degrees: 0,
    });
    expect(second.visual.transform.position_x_px).toBeCloseTo(150, 6);
    expect(second.visual.transform.position_y_px).toBeCloseTo(68.25, 6);
    const remainder = structuredClone(facts.imported.project);
    scene({ project: remainder }).tracks = scene(facts.original).tracks;
    expect(remainder).toEqual(facts.original.project);
    expect(facts.transactions.import.request.operations).toHaveLength(1);
    if (!facts.png_checked) {
      facts.phase = "independent styled ASS PNG";
      await save();
      if (!facts.isolation) {
        if (!facts.isolation_request) {
          const isolated = structuredClone(scene(facts.imported));
          isolated.captions = [];
          isolated.tracks.forEach((t) => {
            if (t.id !== facts.track_id) t.enabled = false;
          });
          const settings = structuredClone(facts.imported.project.settings);
          settings.settings.background = {
            kind: "color",
            color: { encoding: "srgb", red: 0, green: 0, blue: 0, alpha: 1 },
          };
          facts.isolation_request = {
            project_id: facts.project_id,
            expected_revision: (await inspect()).revision,
            idempotency_key: randomUUID(),
            label: "ASS independent PNG isolation",
            operations: [
              { type: "settings_set", settings },
              { type: "sequence_put", sequence: isolated },
            ],
          };
          await save();
        }
        facts.isolation = await tool(
          "opencut_edit_apply",
          facts.isolation_request,
        );
        await save();
      }
      if (!facts.transactions.png?.request) {
        const current = await inspect();
        const isolated = await inspect(facts.isolation.revision);
        if (
          JSON.stringify(current.project) !== JSON.stringify(isolated.project)
        )
          await restore("resume-isolation", facts.isolation.revision);
      }
      await reopen();
      if (!facts.transactions.png?.receipt) {
        await page
          .getByLabel("Playhead (seconds)", { exact: true })
          .fill("0.5");
        await page
          .getByRole("button", { name: "Go to time", exact: true })
          .click();
      }
      const submission = await transaction(
        "png",
        "opencut_render_submit",
        () =>
          page
            .getByRole("button", { name: "Render scene frame", exact: true })
            .click(),
      );
      let job;
      await expect
        .poll(
          async () => {
            job = await tool("opencut_job_view", { job_id: submission.job_id });
            return ["succeeded", "failed", "cancelled"].includes(job.state)
              ? job.state
              : "pending";
          },
          { timeout: 120000, intervals: [100, 250, 500] },
        )
        .not.toBe("pending");
      facts.png_job = job;
      await save();
      expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
      expect(job.result.kind).toBe("render.text_frame");
      const rendered = job.result.data;
      expect(rendered.identity).toMatchObject({
        project_id: facts.project_id,
        revision: facts.transactions.png.request.revision,
        sequence_id: facts.sequence_id,
        width: 320,
        height: 180,
        profile: "classic-v31-gpu-normal-text-frame-v2",
      });
      expect(rendered.texts).toHaveLength(1);
      const artifact = job.artifacts.find((a) => a.mime_type === "image/png"),
        output = path.join(directory, "f23-ass-import.png");
      expect(artifact).toBeTruthy();
      let png;
      try {
        png = await fs.readFile(output);
        if (hash(png) !== artifact.content_hash) png = null;
      } catch (error) {
        if (error.code !== "ENOENT") throw error;
      }
      if (!png) {
        await page.getByRole("button", { name: "Tasks", exact: true }).click();
        await page.getByLabel("Refresh tasks", { exact: true }).click();
        const card = page
          .locator(".task-card")
          .filter({ hasText: facts.transactions.png.request.revision });
        await expect(card).toHaveCount(1);
        const download = page.waitForEvent("download");
        await card
          .getByRole("button", { name: "Download", exact: true })
          .click();
        await (await download).saveAs(output);
        png = await fs.readFile(output);
      }
      expect(hash(png)).toBe(artifact.content_hash);
      facts.png_output = { path: output, sha256: hash(png) };
      await save();
      const fontsDirectory = process.env.OPENCUT_B1_FONT_SOURCES;
      expect(fontsDirectory).toBeTruthy();
      const manifest = JSON.parse(
          await fs.readFile(path.join(fontsDirectory, "manifest.json"), "utf8"),
        ),
        entry = manifest.files.find(
          (f) => f.sha256 === bold.content.content_hash,
        );
      expect(entry).toBeTruthy();
      const bytes = await fs.readFile(
        path.join(fontsDirectory, path.basename(entry.file)),
      );
      expect(hash(bytes)).toBe(bold.content.content_hash);
      facts.oracle = await page.evaluate(
        async ({ png, font }) => {
          const face = new FontFace(
            "ASSIndependent",
            Uint8Array.from(atob(font), (c) => c.charCodeAt(0)),
            { weight: "700", style: "italic" },
          );
          await face.load();
          document.fonts.add(face);
          try {
            const width = 320,
              height = 180,
              canvas = new OffscreenCanvas(width, height),
              ctx = canvas.getContext("2d");
            ctx.fillStyle = "black";
            ctx.fillRect(0, 0, width, height);
            ctx.font = "italic 700 26px ASSIndependent";
            ctx.textAlign = "left";
            ctx.textBaseline = "middle";
            ctx.letterSpacing = "1.5px";
            const theta = (12 * Math.PI) / 180,
              halfHeight = 26,
              center = {
                x: 100 - 1.25 * Math.sin(theta) * halfHeight,
                y: 50 + 1.25 * Math.cos(theta) * halfHeight,
              };
            const lines = ["Styled ASS", "Keeps position"].map((text, i) => {
              const m = ctx.measureText(text);
              return {
                text,
                width: m.width,
                ascent: m.actualBoundingBoxAscent,
                descent: m.actualBoundingBoxDescent,
                y: (i - 0.5) * 26,
              };
            });
            ctx.translate(center.x, center.y);
            ctx.scale(1.25, 1.25);
            ctx.rotate(theta);
            ctx.fillStyle = `rgba(204,102,51,${191 / 255})`;
            for (const line of lines) {
              ctx.fillText(line.text, 0, line.y);
              ctx.fillRect(
                0,
                line.y - (line.ascent - line.descent) * 0.35,
                line.width,
                26 * 0.07,
              );
            }
            const image = new Image();
            image.src = `data:image/png;base64,${png}`;
            await image.decode();
            const actual = new OffscreenCanvas(width, height),
              ac = actual.getContext("2d");
            ac.drawImage(image, 0, 0);
            const a = ac.getImageData(0, 0, width, height).data,
              e = canvas
                .getContext("2d")
                .getImageData(0, 0, width, height).data;
            let union = 0,
              intersection = 0,
              error = 0;
            for (let i = 0; i < a.length; i += 4) {
              const aa = Math.max(a[i], a[i + 1], a[i + 2]) > 32,
                ee = Math.max(e[i], e[i + 1], e[i + 2]) > 32;
              union += Number(aa || ee);
              intersection += Number(aa && ee);
              for (let c = 0; c < 4; c++)
                error += Math.abs(a[i + c] - e[i + c]);
            }
            return {
              width: image.naturalWidth,
              height: image.naturalHeight,
              center,
              lines,
              iou: intersection / union,
              meanChannelError: error / a.length,
            };
          } finally {
            document.fonts.delete(face);
          }
        },
        { png: png.toString("base64"), font: bytes.toString("base64") },
      );
      await save();
      expect(facts.oracle.width).toBe(320);
      expect(facts.oracle.height).toBe(180);
      expect(rendered.texts[0].layout.fontSizePx).toBe(26);
      expect(rendered.texts[0].layout.lineHeightPx).toBe(26);
      facts.oracle.lines.forEach((line, i) =>
        expect(rendered.texts[0].layout.lines[i].width).toBeCloseTo(
          line.width,
          4,
        ),
      );
      expect(facts.oracle.iou).toBeGreaterThan(0.97);
      expect(facts.oracle.meanChannelError).toBeLessThan(1);
      expect(first.visual.transform.position_x_px).toBeCloseTo(
        facts.oracle.center.x - 160,
        6,
      );
      expect(first.visual.transform.position_y_px).toBeCloseTo(
        facts.oracle.center.y - 90,
        6,
      );
      facts.png_checked = true;
      await save();
    }
    if (!facts.transactions.edit?.receipt) {
      await restore("after-png", facts.imported.revision);
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
        .fill("Edited ASS\nKeeps position");
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
    const expected = structuredClone(facts.imported.project);
    scene({ project: expected }).tracks.find(
      (t) => t.id === facts.track_id,
    ).elements[0].content.text.content = "Edited ASS\nKeeps position";
    expect(facts.edited.project).toEqual(expected);
    await reopen();
    for (const [direction, expectedProject] of [
      ["undo", facts.imported.project],
      ["redo", facts.edited.project],
    ]) {
      if (facts[direction + "_checked"]) continue;
      const history = page.locator(".editor-history");
      if (!facts.transactions[direction]?.request) {
        const plan = page.waitForResponse((r) =>
          r.url().endsWith(`/api/tools/opencut_edit_${direction}_plan`),
        );
        await history
          .getByRole("button", {
            name: direction === "undo" ? "Undo" : "Redo",
            exact: true,
          })
          .click();
        expect((await (await plan).json()).error).toBeNull();
      }
      await transaction(direction, `opencut_edit_${direction}_apply`, () =>
        history
          .getByRole("button", { name: `Apply ${direction}`, exact: true })
          .click(),
      );
      expect(
        (await inspect(facts.transactions[direction].receipt.revision)).project,
      ).toEqual(expectedProject);
      facts[direction + "_checked"] = true;
      await save();
    }
    await reopen();
    for (const format of ["srt", "ass"]) {
      if (!facts.downloads[format]?.path) {
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
        const target = path.join(directory, `f23-ass-import.${format}`);
        await (await download).saveAs(target);
        Object.assign(facts.downloads[format], {
          path: target,
          sha256: hash(await fs.readFile(target)),
        });
        await save();
      }
      const saved = facts.downloads[format],
        content = await fs.readFile(saved.path, "utf8");
      expect(hash(content)).toBe(saved.sha256);
      expect(content).toBe(saved.envelope.data.content);
      expect(saved.envelope.data.cue_count).toBe(2);
      expect(content).toContain(
        format === "ass"
          ? "Edited ASS\\NKeeps position"
          : "Edited ASS\nKeeps position",
      );
    }
    expect(errors).toEqual([]);
    facts.phase = "verified";
    await save();
  } catch (error) {
    failure = error;
    facts.active_revision =
      (await inspect().catch(() => null))?.revision ?? null;
    facts.original_revision = facts.original?.revision ?? null;
    facts.pending = {
      transactions: facts.transactions,
      restores: facts.restores ?? {},
      target_original_revision: facts.original_revision,
    };
    facts.failure = {
      message: String(error),
      phase: facts.phase,
      pageerrors: errors,
      ui: (
        await page
          .locator("body")
          .innerText()
          .catch(() => "")
      ).slice(0, 6000),
    };
    facts.first_failure ??= structuredClone(facts.failure);
    await save();
  } finally {
    try {
      if (!failure && facts.phase !== "passed" && facts.original) {
        facts.restored = await restore("original", facts.original.revision);
        expect(facts.restored.project).toEqual(facts.original.project);
        await save();
      }
    } catch (error) {
      facts.restore_failure = String(error);
      await save();
      failure ??= error;
    }
    page.off("response", planListener);
    await Promise.all(planWrites);
    await page.close();
  }
  if (failure) throw failure;
  facts.phase = "passed";
  await save();
  check(
    "F23 scoped ASS style import/edit/UndoRedo/export and one independent PNG",
    facts,
  );
  return facts;
}
