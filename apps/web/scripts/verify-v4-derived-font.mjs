import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash, randomUUID } from "node:crypto";
const hash = (value) => createHash("sha256").update(value).digest("hex");
const TARGET = "cf25b163-cc52-46f7-ab1a-9719bb1e6180";
const TEXT = "Karla derived\nAV To 0123";
const HASH = "020253eac2f832850ce691eb650c60cae7ce29927b7f7c50bc7975f9376af91a";
const SIZE = 45884;
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
      if (entry.isSymbolicLink()) throw Error("Candidate symlink");
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
export async function verifyV4DerivedFont(
  browser,
  endpoint,
  directory,
  check,
  options = {},
) {
  const fixture = JSON.parse(
      await fs.readFile(path.join(directory, "b1-text-project.json"), "utf8"),
    ),
    candidate = await identity(),
    filename = path.join(directory, "f20-derived-font-checkpoint.json");
  let facts;
  try {
    facts = JSON.parse(await fs.readFile(filename, "utf8"));
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  if (facts) expect(facts.candidate).toEqual(candidate);
  else {
    expect(options.offlineOnly).not.toBe(true);
    facts = {
      candidate,
      project_id: fixture.project_id,
      element_id: TARGET,
      transactions: {},
      phase: "bootstrap",
    };
  }
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
  page.on("pageerror", (error) => {
    if (errors.length < 32) errors.push(error.message.slice(0, 1024));
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
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull();
    return envelope.data;
  };
  const inspect = (revision = null) =>
    tool("opencut_project_v4_inspect", {
      project_id: facts.project_id,
      revision,
    });
  const scene = (snapshot) =>
    snapshot.project.sequences.find((s) => s.id === facts.sequence_id);
  const target = (snapshot) =>
    scene(snapshot)
      .tracks.flatMap((t) => t.elements)
      .find((e) => e.id === TARGET);
  const reopen = async () => {
    await page.goto(
      `${endpoint}/editor/${encodeURIComponent(facts.project_id)}${facts.sequence_id ? `?scene=${encodeURIComponent(facts.sequence_id)}` : ""}`,
    );
    await expect(
      page.getByRole("main", { name: "V4 editor", exact: true }),
    ).toBeVisible();
    const guide = page.getByRole("dialog", {
      name: "Welcome to OpenCut",
      exact: true,
    });
    if (await guide.isVisible())
      await guide.getByRole("button", { name: "Close", exact: true }).click();
  };
  // Persist exact requests before dispatch; resumed stages reuse their original key.
  const mutation = async (stage, name, input) => {
    let saved = facts.transactions[stage];
    if (!saved) {
      saved = facts.transactions[stage] = { operation: name, request: input };
      await save();
    }
    expect(saved.operation).toBe(name);
    if (!saved.receipt) {
      saved.receipt = await tool(name, saved.request);
      await save();
    }
    return saved.receipt;
  };
  const uiMutation = async (stage, name, action) => {
    const old = facts.transactions[stage];
    if (old?.request) return mutation(stage, name, old.request);
    const route = `**/v1/tools/${name}`;
    await page.route(
      route,
      async (r) => {
        facts.transactions[stage] = {
          operation: name,
          request: r.request().postDataJSON(),
        };
        await save();
        await r.continue();
      },
      { times: 1 },
    );
    const pending = page.waitForResponse((r) =>
      r.url().endsWith(`/v1/tools/${name}`),
    );
    await action();
    const envelope = await (await pending).json();
    facts.transactions[stage].envelope = envelope;
    await save();
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull();
    facts.transactions[stage].receipt = envelope.data;
    await save();
    return envelope.data;
  };
  const restore = async (stage, revision) =>
    mutation(stage, "opencut_project_v4_restore", {
      project_id: facts.project_id,
      expected_revision: (await inspect()).revision,
      target_revision: revision,
      idempotency_key: randomUUID(),
      asset_versions: {},
    });
  const proof = (revision) =>
    tool("opencut_font_derived_v1_proof", {
      project_id: facts.project_id,
      revision,
      font_id: facts.font_id,
    });
  try {
    await reopen();
    if (!facts.original) {
      facts.original = await inspect();
      facts.sequence_id = facts.original.project.default_sequence_id;
      await save();
      await reopen();
    }
    if (facts.phase === "passed") {
      expect((await inspect()).project).toEqual(facts.original.project);
      check("F20 retained derived font online/offline evidence", facts);
      return facts;
    }
    if (!options.offlineOnly) {
      if (!facts.transactions.import?.receipt) {
        facts.phase = "online derived import";
        await save();
        const catalog = await tool("opencut_font_derived_v1_catalog", {});
        facts.catalog_face = catalog.fonts.find(
          (f) =>
            f.family === "Karla" && f.weight === 400 && f.style === "normal",
        );
        await save();
        expect(facts.catalog_face).toBeTruthy();
        expect(facts.catalog_face.content_hash).toBe(HASH);
        expect(facts.catalog_face.size_bytes).toBe(String(SIZE));
        await page
          .getByRole("button", { name: "Import font", exact: true })
          .click();
        const form = page.getByRole("form", {
          name: "Import font",
          exact: true,
        });
        await form
          .getByRole("combobox", { name: prefix("Font family") })
          .selectOption("Karla");
        await form
          .getByRole("combobox", { name: prefix("Font style") })
          .selectOption("400:normal");
        await form
          .getByRole("textbox", { name: prefix("Text to check") })
          .fill(TEXT);
        await expect(
          form.getByRole("button", { name: "Import font", exact: true }),
        ).toBeEnabled();
        await uiMutation("import", "opencut_font_derived_v1_import", () =>
          form
            .getByRole("button", { name: "Import font", exact: true })
            .click(),
        );
        await expect(form).toBeHidden();
      }
      const imported = await inspect(
        facts.transactions.import.receipt.revision,
      );
      facts.imported_revision = imported.revision;
      facts.font_id = facts.transactions.import.request.font_id;
      facts.font = imported.project.fonts.find((f) => f.id === facts.font_id);
      await save();
      expect(facts.font).toMatchObject({
        family: "Karla",
        weight: 400,
        style: "normal",
        content: { content_hash: HASH, size: SIZE },
        origin: {
          kind: "generated",
          provider: "opencut-fonttools-static-v1",
          generation_id: facts.catalog_face.derivation_id,
        },
      });
      expect({ ...imported.project, fonts: facts.original.project.fonts }).toEqual(
        facts.original.project,
      );
      expect(imported.project.fonts.filter((font) => font.id !== facts.font_id)).toEqual(
        facts.original.project.fonts,
      );
      facts.proof = await proof(imported.revision);
      await save();
      expect(facts.proof.source.face).toEqual(facts.catalog_face);
      expect(JSON.parse(facts.proof.source.proof_json).derivation_id).toBe(
        facts.catalog_face.derivation_id,
      );
      facts.replayed = await tool(
        "opencut_font_derived_v1_import",
        facts.transactions.import.request,
      );
      await save();
      expect(facts.replayed).toEqual(facts.transactions.import.receipt);
      if (!facts.transactions.edit?.receipt) {
        await reopen();
        await page
          .locator(`[data-element-id="${TARGET}"] .v4-element-select`)
          .click({ position: { x: 12, y: 12 } });
        await page
          .getByRole("button", { name: "Edit text", exact: true })
          .click();
        const form = page.getByRole("form", { name: "Edit text", exact: true });
        await form
          .getByRole("textbox", { name: prefix("Text content") })
          .fill(TEXT);
        await form
          .getByRole("combobox", { name: prefix("Font family") })
          .selectOption(facts.font_id);
        await form
          .getByRole("button", { name: "Save text", exact: true })
          .click();
        const review = page.getByRole("region", {
          name: "Review text changes",
          exact: true,
        });
        await expect(review).toBeVisible();
        await uiMutation("edit", "opencut_edit_v4_apply", () =>
          review
            .getByRole("button", { name: "Apply text changes", exact: true })
            .click(),
        );
        await expect(review).toBeHidden();
      }
      await reopen();
      facts.edited = await inspect(facts.transactions.edit.receipt.revision);
      await save();
      expect(target(facts.edited).content.text.content).toBe(TEXT);
      expect(target(facts.edited).content.text.font.font_id).toBe(
        facts.font_id,
      );
      expect(scene(facts.edited).captions).toEqual(
        scene(facts.original).captions,
      );
      if (!facts.transactions.isolation?.receipt) {
        const current = await inspect(),
          isolated = structuredClone(scene(current)),
          settings = structuredClone(current.project.settings);
        settings.settings.canvas_size = { width: 320, height: 180 };
        settings.settings.canvas_size_mode = "custom";
        settings.settings.last_custom_canvas_size = { width: 320, height: 180 };
        settings.settings.background = {
          kind: "color",
          color: { encoding: "srgb", red: 0, green: 0, blue: 0, alpha: 1 },
        };
        isolated.captions = [];
        for (const track of isolated.tracks) {
          track.enabled = track.elements.some((e) => e.id === TARGET);
          for (const element of track.elements) {
            element.enabled = element.id === TARGET;
            if (element.id === TARGET) {
              Object.assign(element.content.text, {
                content: TEXT,
                font_size: { unit: "canvas_pixels", value: 24 },
                font_weight: "normal",
                font_style: "normal",
                decoration: "none",
                align: "center",
                letter_spacing_px: 0,
                line_height_multiplier: 1.2,
                color: {
                  encoding: "srgb",
                  red: 1,
                  green: 1,
                  blue: 1,
                  alpha: 1,
                },
                background: {
                  ...element.content.text.background,
                  enabled: false,
                },
              });
              Object.assign(element.visual.transform, {
                position_x_px: 0,
                position_y_px: 0,
                scale_x: 1,
                scale_y: 1,
                rotation_degrees: 0,
              });
              element.visual.opacity = 1;
            }
          }
        }
        await mutation("isolation", "opencut_edit_v4_apply", {
          project_id: facts.project_id,
          expected_revision: current.revision,
          idempotency_key: randomUUID(),
          label: "Derived font independent PNG isolation",
          operations: [
            { type: "settings_set", settings },
            { type: "sequence_put", sequence: isolated },
          ],
        });
      }
      facts.render_revision = facts.transactions.isolation.receipt.revision;
      facts.phase = "awaiting host download-disabled restart";
      await save();
      expect((await inspect()).revision).toBe(facts.render_revision);
      facts.online_page_errors = errors;
      await save();
      expect(errors).toEqual([]);
      check(
        "F20 online UI derived import, original-key replay and reopened text edit",
        {
          project_id: facts.project_id,
          font: facts.font,
          receipt: facts.transactions.import.receipt,
          proof: facts.proof,
          render_revision: facts.render_revision,
          checkpoint: filename,
        },
      );
      return facts;
    }
    if (!facts.png_checked) {
      expect(facts.render_revision).toBeTruthy();
      expect((await inspect()).revision).toBe(facts.render_revision);
      facts.phase = "offline fixed-revision PNG";
      await save();
      const staticCatalog = await tool("opencut_font_v2_catalog", {});
      facts.offline_catalog = staticCatalog;
      await save();
      expect(staticCatalog.download_available).toBe(false);
      await reopen();
      if (!facts.transactions.png?.request) {
        const e = target(await inspect());
        const seconds =
          Number(e.timeline_range.start.ticks) /
            Number(e.timeline_range.start.timebase) +
          0.01;
        await page
          .getByLabel("Playhead (seconds)", { exact: true })
          .fill(String(seconds));
        await page
          .getByRole("button", { name: "Go to time", exact: true })
          .click();
      }
      const submission = await uiMutation(
        "png",
        "opencut_job_v4_text_frame_png",
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
      expect(job.extended_result.kind).toBe("render.project_text_frame.v4");
      const rendered = job.extended_result.data;
      expect(rendered.identity).toMatchObject({
        project_id: facts.project_id,
        revision: facts.render_revision,
        sequence_id: facts.sequence_id,
        width: 320,
        height: 180,
        profile: "classic-v31-gpu-normal-text-frame-v2",
      });
      expect(rendered.texts).toHaveLength(1);
      expect(rendered.font_lock).toEqual([
        { font_id: facts.font_id, content_hash: HASH, size: SIZE },
      ]);
      const artifact = job.artifacts.find((a) => a.mime_type === "image/png"),
        output = path.join(directory, "f20-derived-font.png");
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
      facts.png_output = { path: output, sha256: hash(png) };
      await save();
      expect(hash(png)).toBe(artifact.content_hash);
      const font = await fs.readFile(
        "/private/tmp/opencut-font-derive-evidence/karla-regular.ttf",
      );
      expect(hash(font)).toBe(HASH);
      expect(font.length).toBe(SIZE);
      facts.oracle = await page.evaluate(
        async ({ png, font, text }) => {
          const face = new FontFace(
            "DerivedIndependent",
            Uint8Array.from(atob(font), (c) => c.charCodeAt(0)),
            { weight: "400", style: "normal" },
          );
          await face.load();
          document.fonts.add(face);
          try {
            const w = 320,
              h = 180,
              c = new OffscreenCanvas(w, h),
              ctx = c.getContext("2d");
            ctx.fillStyle = "black";
            ctx.fillRect(0, 0, w, h);
            ctx.font = "normal 400 24px DerivedIndependent";
            ctx.textAlign = "center";
            ctx.textBaseline = "middle";
            ctx.fillStyle = "white";
            const lines = text.split("\n").map((s, i) => ({
              text: s,
              width: ctx.measureText(s).width,
              y: 90 + (i - 0.5) * 28.8,
            }));
            for (const line of lines) ctx.fillText(line.text, 160, line.y);
            const image = new Image();
            image.src = `data:image/png;base64,${png}`;
            await image.decode();
            const actual = new OffscreenCanvas(w, h),
              ac = actual.getContext("2d");
            ac.drawImage(image, 0, 0);
            const a = ac.getImageData(0, 0, w, h).data,
              e = ctx.getImageData(0, 0, w, h).data;
            let union = 0,
              intersection = 0,
              error = 0;
            for (let i = 0; i < a.length; i += 4) {
              const aa = a[i] > 32,
                ee = e[i] > 32;
              union += Number(aa || ee);
              intersection += Number(aa && ee);
              for (let k = 0; k < 4; k++)
                error += Math.abs(a[i + k] - e[i + k]);
            }
            return {
              width: image.naturalWidth,
              height: image.naturalHeight,
              lines,
              iou: intersection / union,
              meanChannelError: error / a.length,
            };
          } finally {
            document.fonts.delete(face);
          }
        },
        {
          png: png.toString("base64"),
          font: font.toString("base64"),
          text: TEXT,
        },
      );
      await save();
      expect(facts.oracle.width).toBe(320);
      expect(facts.oracle.height).toBe(180);
      expect(facts.oracle.iou).toBeGreaterThan(0.97);
      expect(facts.oracle.meanChannelError).toBeLessThan(1);
      expect(rendered.texts[0].layout.fontSizePx).toBe(24);
      expect(rendered.texts[0].layout.lineHeightPx).toBeCloseTo(28.8, 6);
      facts.oracle.lines.forEach((line, i) =>
        expect(rendered.texts[0].layout.lines[i].width).toBeCloseTo(
          line.width,
          4,
        ),
      );
      facts.png_checked = true;
      await save();
    }
    await restore("final-restore", facts.original.revision);
    expect((await inspect()).project).toEqual(facts.original.project);
    facts.historical_proof = await proof(facts.imported_revision);
    await save();
    expect(facts.historical_proof).toEqual(facts.proof);
    expect(errors).toEqual([]);
    facts.phase = "passed";
    await save();
    check(
      "F20 offline derived font PNG independent external-font Canvas, complete restoration and retained historical proof",
      facts,
    );
    return facts;
  } catch (error) {
    facts.failure = {
      phase: facts.phase,
      message: String(error).slice(0, 4096),
    };
    await save();
    throw error;
  } finally {
    await page.close();
  }
}
