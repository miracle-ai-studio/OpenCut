import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";

// Root supplies the existing B1 workspace, candidate and serial browser. This
// verifier creates no project/media/font, starts no service, and claims no
// ASR acceptance. Caption pixels use an independent Canvas oracle over the
// previously rendered unchanged scene, never renderer layout or wrap code.
export async function verifyCaptions(browser, endpoint, directory, check) {
  const { project_id: projectId } = JSON.parse(
    await fs.readFile(path.join(directory, "b1-text-project.json"), "utf8"),
  );
  const page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1100 },
  });
  page.setDefaultTimeout(30000);
  const network = [],
    errors = [],
    evidence = [];
  let phase = "open",
    original,
    committed,
    imported;
  page.on("pageerror", (e) => {
    if (errors.length < 16) errors.push(e.message.slice(0, 1024));
  });
  page.on("request", (request) => {
    const operation = new URL(request.url()).pathname.split("/").at(-1);
    if (operation.startsWith("opencut_") && network.length < 256)
      network.push({ operation, method: request.method() });
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
  const inspect = () =>
    editorSnapshot(tool("opencut_project_inspect", {
      project_id: projectId,
      revision: null,
    }));
  const responseFor = (name) =>
    page.waitForResponse((response) =>
      response.url().endsWith(`/api/tools/${name}`),
    );
  const exchange = async (name, action) => {
    const pending = responseFor(name);
    await action();
    const response = await pending;
    const envelope = await response.json();
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull();
    return {
      request: response.request().postDataJSON(),
      result: envelope.data,
    };
  };
  try {
    await page.goto(`${endpoint}/editor/${encodeURIComponent(projectId)}`);
    await expect(
      page.getByRole("main", { name: "OpenCut editor", exact: true }),
    ).toBeVisible();
    original = await inspect();
    const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
    if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
    const sequenceId = original.project.default_sequence_id;
    const scene = (snapshot) =>
      snapshot.project.sequences.find((s) => s.id === sequenceId);
    const canvas = original.project.settings.settings.canvas_size;
    const panel = page.locator(".editor-caption-workspace");
    await expect(panel).toBeVisible();
    const font =
      original.project.fonts.find(
        (f) => f.weight === 400 && f.style === "normal",
      ) ?? original.project.fonts[0];
    expect(font, "Existing B1 pinned font is required").toBeTruthy();
    phase = "existing caption owning track";
    let track = scene(original).tracks.find(
      (t) => t.kind === "caption" && !t.locked,
    );
    if (!track) {
      await panel
        .getByRole("button", { name: "Add caption track", exact: true })
        .click();
      await expect
        .poll(async () =>
          scene(await inspect()).tracks.some(
            (t) => t.kind === "caption" && !t.locked,
          ),
        )
        .toBe(true);
      track = scene(await inspect()).tracks.find(
        (t) => t.kind === "caption" && !t.locked,
      );
    }
    await panel.getByLabel(/^Caption track/).selectOption(track.id);
    await panel.getByLabel(/^Pinned font/).selectOption(font.id);
    await panel.getByLabel("Size (px)", { exact: true }).fill("24");
    const before = await inspect();
    phase = "unchanged scene reference for independent caption pixels";
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill("0.5");
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
    const baselineSubmit = await exchange("opencut_render_submit", () => page.getByRole("button", { name: "Render scene frame", exact: true }).click());
    let baselineJob;
    await expect.poll(async () => {
      baselineJob = await tool("opencut_job_view", { job_id: baselineSubmit.result.job_id });
      return ["succeeded", "failed", "cancelled"].includes(baselineJob.state) ? baselineJob.state : "pending";
    }, { timeout: 120000, intervals: [100, 250, 500] }).not.toBe("pending");
    expect(baselineJob.state, JSON.stringify(baselineJob.failure)).toBe("succeeded");
    const baselineImage = page.getByRole("img", { name: "Rendered scene frame", exact: true });
    await expect(baselineImage).toBeVisible();
    const baseline = await baselineImage.evaluate(async node => {
      await node.decode();
      const canvas = document.createElement("canvas"); canvas.width = node.naturalWidth; canvas.height = node.naturalHeight;
      canvas.getContext("2d").drawImage(node, 0, 0); return canvas.toDataURL("image/png");
    });
    const fontDirectory = process.env.OPENCUT_B1_FONT_SOURCES ?? "/private/tmp/opencut-b1-font-sources-20261013";
    const fontManifest = JSON.parse(await fs.readFile(path.join(fontDirectory, "manifest.json"), "utf8"));
    const source = fontManifest.files.find(entry => entry.sha256 === font.content.content_hash);
    expect(source, "Reuse the independently pinned B1 font source").toBeTruthy();
    const fontBytes = await fs.readFile(path.join(fontDirectory, path.basename(source.file)));
    expect(createHash("sha256").update(fontBytes).digest("hex")).toBe(font.content.content_hash);
    const content = [
      "AV To",
      ...Array(Math.ceil((canvas.width * 0.8) / 85) + 5).fill("caption"),
    ].join(" ");
    const srt = `1\n00:00:00,100 --> 00:00:01,250\n${content}\n\n`;
    phase = "original SRT import through Web recovery";
    await panel.getByLabel(/^Format/).selectOption("srt");
    await panel
      .getByLabel("Open subtitle file", { exact: true })
      .setInputFiles({
        name: "b3-captions.srt",
        mimeType: "text/plain",
        buffer: Buffer.from(srt),
      });
    await expect(
      panel.getByRole("textbox", { name: /^Subtitle content(?:\s|$)/ }),
    ).toHaveValue(srt);
    const importNetworkStart = network.length;
    const stagedPromise = responseFor("opencut_edit_recovery_stage");
    const ackPromise = responseFor("opencut_edit_recovery_ack");
    imported = await exchange("opencut_subtitle_import", () =>
      panel
        .getByRole("button", { name: "Import subtitles", exact: true })
        .click(),
    );
    const stagedResponse = await stagedPromise,
      stagedEnvelope = await stagedResponse.json();
    expect(stagedEnvelope.error).toBeNull();
    expect(stagedResponse.request().postDataJSON()).toEqual({
      context: {
        sequence_id: sequenceId,
        intent: { kind: "subtitle_import", request: imported.request },
      },
    });
    expect(imported.request).toMatchObject({
      project_id: projectId,
      expected_revision: before.revision,
      sequence_id: sequenceId,
      track_id: track.id,
      content: srt,
      format: "srt",
      style: {
        layout: "pinned_caption_v4",
        font: { kind: "locked", font_id: font.id },
      },
    });
    expect(imported.result.cue_count).toBe(1);
    expect(
      network
        .slice(importNetworkStart)
        .some((n) => n.operation === "opencut_edit_apply"),
    ).toBe(false);
    const ackEnvelope = await (await ackPromise).json();
    expect(ackEnvelope.error).toBeNull();
    expect(ackEnvelope.data.receipt).toEqual(imported.result);
    committed = await inspect();
    expect(committed.revision).toBe(imported.result.transaction.revision);
    const captions = scene(committed).captions;
    expect(captions.length).toBe(scene(before).captions.length + 1);
    expect(scene(committed).tracks).toEqual(scene(before).tracks);
    for (const old of scene(before).captions)
      expect(captions).toContainEqual(old);
    const cue = captions.find(
      (c) => !scene(before).captions.some((old) => old.id === c.id),
    );
    expect(cue).toMatchObject({
      track_id: track.id,
      text: content,
      style: imported.request.style,
    });
    expect(cue.source_anchor).toBeNull();
    const seconds = (time) => Number(time.ticks) / time.timebase;
    expect(seconds(cue.timeline_range.start)).toBeCloseTo(0.1, 8);
    expect(seconds(cue.timeline_range.duration)).toBeCloseTo(1.15, 8);
    evidence.push({
      case: phase,
      original_revision: before.revision,
      request: imported.request,
      receipt: imported.result,
      staged_id: stagedEnvelope.data.recovery_id,
      acknowledged_receipt: ackEnvelope.data.receipt,
      cue,
    });
    phase = "reload persisted captions";
    await page.reload();
    await expect(panel).toBeVisible();
    expect(scene(await inspect()).captions).toEqual(captions);
    await panel.getByLabel(/^Caption track/).selectOption(track.id);
    phase = "ordinary atomic Undo Redo";
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await page.getByRole("button", { name: "Apply undo", exact: true }).click();
    await expect(page.getByRole("button", { name: "Apply undo", exact: true })).toHaveCount(0);
    await expect
      .poll(async () => scene(await inspect()).captions)
      .toEqual(scene(before).captions);
    const undone = await inspect();
    await page.getByRole("button", { name: "Redo", exact: true }).click();
    await page.getByRole("button", { name: "Apply redo", exact: true }).click();
    await expect(page.getByRole("button", { name: "Apply redo", exact: true })).toHaveCount(0);
    await expect
      .poll(async () => scene(await inspect()).captions)
      .toEqual(captions);
    const redone = await inspect();
    evidence.push({
      case: phase,
      import_revision: committed.revision,
      undo_revision: undone.revision,
      redo_revision: redone.revision,
    });
    phase = "V3 pinned-caption native PNG and layout receipt";
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill("0.5");
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
    const submitted = await exchange("opencut_render_submit", () =>
      page
        .getByRole("button", { name: "Render scene frame", exact: true })
        .click(),
    );
    expect(submitted.request).toMatchObject({
      project_id: projectId,
      revision: redone.revision,
      sequence_id: sequenceId,
    });
    let job;
    await expect
      .poll(
        async () => {
          job = await tool("opencut_job_view", {
            job_id: submitted.result.job_id,
          });
          return ["succeeded", "failed", "cancelled"].includes(job.state)
            ? job.state
            : "pending";
        },
        { timeout: 120000, intervals: [100, 250, 500] },
      )
      .not.toBe("pending");
    expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
    expect(job.result.kind).toBe("render.caption_frame");
    const result = job.result.data;
    expect(result).toMatchObject({
      schema_version: 3,
      identity: {
        project_id: projectId,
        revision: redone.revision,
        sequence_id: sequenceId,
        width: canvas.width,
        height: canvas.height,
        profile: "classic-v31-gpu-normal-caption-frame-v3",
      },
    });
    const wrap = result.captions.find((c) => c.elementId === cue.id),
      raster = result.texts.find((t) => t.elementId === cue.id);
    expect(wrap).toBeTruthy();
    expect(raster).toBeTruthy();
    expect(wrap.originalTextHash).toBe(
      createHash("sha256").update(content).digest("hex"),
    );
    expect(wrap.wrappedText.replace(/\s+/g, " ")).toBe(content);
    expect(wrap.wrappedText.split("\n").length).toBeGreaterThan(1);
    expect(raster.layout.lines.map((line) => line.content)).toEqual(
      wrap.wrappedText.split("\n"),
    );
    for (const line of raster.layout.lines)
      expect(line.width).toBeLessThanOrEqual(canvas.width * 0.8 + 0.001);
    expect(wrap.measurements.some((m) => m.width > canvas.width * 0.8)).toBe(
      true,
    );
    expect(raster.layout.transform.centerX).toBeCloseTo(canvas.width / 2, 5);
    expect(
      raster.layout.transform.centerY + raster.layout.block.height / 2,
    ).toBeCloseTo(canvas.height * 0.95, 5);
    expect(
      result.font_lock.some(
        (f) =>
          f.font_id === font.id && f.content_hash === font.content.content_hash,
      ),
    ).toBe(true);
    const image = page.getByRole("img", {
      name: "Rendered scene frame",
      exact: true,
    });
    await expect(image).toBeVisible();
    const imageObservation = await image.evaluate(async (image) => {
      await image.decode();
      return { width: image.naturalWidth, height: image.naturalHeight };
    });
    expect(imageObservation).toEqual({
      width: canvas.width,
      height: canvas.height,
    });
    const oracle = await image.evaluate(async (node, { baseline, fontBase64, content, width, height }) => {
      await node.decode();
      const face = new FontFace("IndependentCaptionOracle", Uint8Array.from(atob(fontBase64), c => c.charCodeAt(0)));
      await face.load(); document.fonts.add(face);
      try {
        const baseImage = new Image(); baseImage.src = baseline; await baseImage.decode();
        const expected = new OffscreenCanvas(width, height), ctx = expected.getContext("2d");
        ctx.drawImage(baseImage, 0, 0);
        const original = ctx.getImageData(0, 0, width, height).data;
        ctx.font = '24px "IndependentCaptionOracle"'; ctx.textBaseline = "middle"; ctx.textAlign = "center"; ctx.fillStyle = "white";
        const words = content.split(" "), lines = [];
        for (const word of words) {
          const last = lines.length - 1;
          if (last < 0 || ctx.measureText(lines[last] + " " + word).width > width * .8) lines.push(word);
          else lines[last] += " " + word;
        }
        const lineHeight = 24 * 1.2, top = height * .95 - lines.length * lineHeight;
        lines.forEach((text, i) => ctx.fillText(text, width / 2, top + (i + .5) * lineHeight));
        const expectedPixels = ctx.getImageData(0, 0, width, height).data;
        const actualCanvas = new OffscreenCanvas(width, height), actual = actualCanvas.getContext("2d"); actual.drawImage(node, 0, 0);
        const actualPixels = actual.getImageData(0, 0, width, height).data;
        let union = 0, intersection = 0, maxDifference = 0, outsideDifference = 0;
        for (let i = 0; i < original.length; i += 4) {
          let da = 0, de = 0, difference = 0;
          for (let c = 0; c < 3; c++) {
            da = Math.max(da, Math.abs(actualPixels[i+c] - original[i+c]));
            de = Math.max(de, Math.abs(expectedPixels[i+c] - original[i+c]));
            difference = Math.max(difference, Math.abs(actualPixels[i+c] - expectedPixels[i+c]));
          }
          const a = da > 32, e = de > 32;
          union += Number(a || e); intersection += Number(a && e);
          maxDifference = Math.max(maxDifference, difference);
          if (!a && !e) outsideDifference = Math.max(outsideDifference, difference);
        }
        return { lines, top, lineHeight, bottom: height * .95, union, intersection, iou: intersection / union, maxDifference, outsideDifference };
      } finally { document.fonts.delete(face); }
    }, { baseline, fontBase64: fontBytes.toString("base64"), content, width: canvas.width, height: canvas.height });
    expect(oracle.top).toBeGreaterThanOrEqual(0);
    expect(oracle.union).toBeGreaterThan(100);
    expect(oracle.iou).toBeGreaterThan(.995);
    expect(oracle.outsideDifference).toBeLessThanOrEqual(2);
    expect(wrap.wrappedText.split("\n")).toEqual(oracle.lines);
    evidence.push({
      case: phase,
      job_id: job.job_id,
      identity: result.identity,
      artifact: result.artifact,
      font_lock: result.font_lock,
      execution: result.execution,
      wrap,
      raster,
      image: imageObservation,
      independent_pixels: { ...oracle, baseline_job_id: baselineJob.job_id },
    });
    phase = "Web SRT and ASS export";
    const exports = [];
    for (const format of ["srt", "ass"]) {
      await panel.getByLabel(/^Format/).selectOption(format);
      const downloadPromise = page.waitForEvent("download");
      const exported = await exchange("opencut_subtitle_export", () =>
        panel
          .getByRole("button", { name: "Export subtitles", exact: true })
          .click(),
      );
      const download = await downloadPromise;
      const saved = path.join(directory, `b3-captions-export.${format}`);
      await download.saveAs(saved);
      const bytes = await fs.readFile(saved);
      expect(bytes.length).toBeLessThanOrEqual(2 * 1024 * 1024);
      expect(bytes.toString("utf8")).toBe(exported.result.content);
      expect(exported.request).toEqual({
        project_id: projectId,
        revision: redone.revision,
        sequence_id: sequenceId,
        track_id: track.id,
        format,
      });
      expect(exported.result.content).toContain(content);
      expect(exported.result.content).toContain(
        format === "srt"
          ? "00:00:00,100 --> 00:00:01,250"
          : "0:00:00.10,0:00:01.25",
      );
      expect(exported.result.losses.length).toBeGreaterThan(0);
      exports.push({
        format,
        request: exported.request,
        cue_count: exported.result.cue_count,
        losses: exported.result.losses,
        size: bytes.length,
        sha256: createHash("sha256").update(bytes).digest("hex"),
      });
    }
    evidence.push({ case: phase, exports });
    expect(
      network.some(
        (item) => item.operation === "opencut_analysis_sequence_transcribe",
      ),
    ).toBe(false);
    expect(errors).toEqual([]);
    const report = {
      status: "passed",
      project_id: projectId,
      sequence_id: sequenceId,
      final_revision: (await inspect()).revision,
      evidence,
      scope:
        "Real Web SRT import, V3 original-intent recovery, persistence, ordinary undo/redo, independent wrapped-caption glyph/position pixels over the unchanged scene and downloaded SRT/ASS. No real ASR.",
      bounded_network: network,
    };
    await fs.writeFile(
      path.join(directory, "b3-captions.json"),
      JSON.stringify(report, null, 2) + "\n",
    );
    check(
      "B3 Web original-caption recovery, native wrap/layout receipts and subtitle exchange",
      report,
    );
  } catch (error) {
    const failure = {
      status: "failed",
      phase,
      project_id: projectId,
      error: String(error).slice(0, 4096),
      errors,
      network,
      evidence,
    };
    try {
      failure.ui = await page.evaluate(() => ({
        url: location.pathname,
        diagnostics: [
          ...document.querySelectorAll('[role="alert"],[role="status"]'),
        ]
          .slice(0, 16)
          .map((n) => n.textContent.slice(0, 1024)),
      }));
    } catch {}
    await fs.writeFile(
      path.join(directory, "b3-captions-failure.json"),
      JSON.stringify(failure, null, 2) + "\n",
    );
    await page
      .screenshot({
        path: path.join(directory, "b3-captions-failure.png"),
        fullPage: false,
      })
      .catch(() => {});
    throw error;
  } finally {
    await page.close();
  }
}
