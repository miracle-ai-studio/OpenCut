import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import {
  loadGraphicOutputReference,
  measureMixedPixels,
} from "./graphic-output-oracle.mjs";

const read = async (file) => JSON.parse(await fs.readFile(file, "utf8"));
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const execute = promisify(execFile);
const decode = async (args) =>
  (
    await execute(
      process.env.OPENCUT_FFMPEG ?? "ffmpeg",
      ["-hide_banner", "-loglevel", "error", ...args],
      { encoding: "buffer", maxBuffer: 32 * 1024 * 1024 },
    )
  ).stdout;

// The restored graph must exactly equal the authored D0 mixed fixture. Reuse its
// hashed independent reference, never the earlier product's PNG/MP4 as expected.
export async function verifyV4PackageOutput(
  browser,
  endpoint,
  directory,
  check,
) {
  const recovered = await read(
    path.join(directory, "d1-package-checkpoint.json"),
  );
  const sourceDirectory = process.env.OPENCUT_D1_PACKAGE_SOURCE_DIR;
  const source = await read(
    path.join(sourceDirectory, "b4-graphic-output-checkpoint.json"),
  );
  const checkpoint = path.join(directory, "d1-package-output-checkpoint.json");
  let facts = {
    binary_sha256: sha(await fs.readFile(process.env.OPENCUT_WEB_BINARY)),
    jobs: {},
    pixels: [],
  };
  try {
    const old = await read(checkpoint);
    expect(old.binary_sha256).toBe(facts.binary_sha256);
    facts = old;
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  const save = async () =>
    fs.writeFile(checkpoint, JSON.stringify(facts, null, 2) + "\n");
  const page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1100 },
    acceptDownloads: true,
  });
  await page.addInitScript(() =>
    Object.defineProperty(window, "showSaveFilePicker", {
      value: undefined,
      configurable: true,
    }),
  );
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const tool = async (name, input) => {
    const reply = await page.evaluate(
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
    expect(reply.error, JSON.stringify(reply.error)).toBeNull();
    return reply.data;
  };
  const projectId = recovered.restored.target_project_id,
    revision = recovered.restored_render_revision;
  const sequence = source.sequence_id;
  const open = async () => {
    await page.goto(`${endpoint}/editor/${projectId}?scene=${sequence}`);
    await expect(
      page.getByRole("main", { name: "V4 editor", exact: true }),
    ).toBeVisible();
    const welcome = page.getByRole("dialog", {
      name: "Welcome to OpenCut",
      exact: true,
    });
    if (await welcome.isVisible())
      await welcome.getByRole("button", { name: "Close", exact: true }).click();
  };
  try {
    await open();
    const snapshot = await tool("opencut_project_v4_inspect", {
      project_id: projectId,
      revision: null,
    });
    expect(snapshot.revision).toBe(revision);
    expect(snapshot.project).toEqual({
      ...recovered.source.project,
      id: projectId,
    });
    expect(recovered.source.revision).toBe(source.revision);
    const reference = await loadGraphicOutputReference({
      directory: sourceDirectory,
      identity: source.pixel_reference.identity,
      generate: () => {
        throw Error("Existing independent reference must be retained");
      },
    });
    const rawExpected = await fs.readFile(
        path.join(reference.metadata.directory, "expected.rgba"),
      ),
      rawBase = await fs.readFile(
        path.join(reference.metadata.directory, "base.rgba"),
      );
    expect(sha(rawExpected)).toBe(reference.metadata.files["expected.rgba"]);
    expect(sha(rawBase)).toBe(reference.metadata.files["base.rgba"]);
    facts.reference_key = reference.metadata.key;
    for (const kind of ["png", "mp4"]) {
      const name =
        kind === "png"
          ? "opencut_job_v4_graphic_frame_png"
          : "opencut_job_v4_graphic_video_mp4";
      const record = (facts.jobs[kind] ??= {});
      if (!record.submission) {
        if (record.request)
          record.submission = await tool(name, record.request);
        else {
          await open();
          if (kind === "png") {
            await page
              .getByRole("textbox", { name: /^Playhead \(seconds\)/ })
              .fill("0.5");
            await page
              .getByRole("button", { name: "Go to time", exact: true })
              .click();
          } else
            await page
              .getByRole("button", { name: "Export video MP4", exact: true })
              .click();
          const request = page.waitForRequest((r) =>
              r.url().endsWith(`/v1/tools/${name}`),
            ),
            response = page.waitForResponse((r) =>
              r.url().endsWith(`/v1/tools/${name}`),
            );
          const click = page
            .getByRole("button", {
              name:
                kind === "png" ? "Render scene frame" : "Start video export",
              exact: true,
            })
            .click();
          record.request = (await request).postDataJSON();
          await save();
          await click;
          const reply = await (await response).json();
          expect(reply.error).toBeNull();
          record.submission = reply.data;
        }
        expect(record.request).toMatchObject({
          project_id: projectId,
          revision,
          sequence_id: sequence,
        });
        await save();
      }
      let job;
      await expect
        .poll(
          async () => {
            job = await tool("opencut_job_view", {
              job_id: record.submission.job_id,
            });
            return ["succeeded", "failed", "cancelled"].includes(job.state)
              ? job.state
              : "pending";
          },
          { timeout: 300000, intervals: [250, 500, 1000] },
        )
        .not.toBe("pending");
      record.job = job;
      await save();
      expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
      const artifact = job.artifacts.find(
        (a) => a.mime_type === (kind === "png" ? "image/png" : "video/mp4"),
      );
      expect(artifact).toBeTruthy();
      const output = path.join(directory, `d1-restored-mixed.${kind}`);
      let bytes;
      try {
        bytes = await fs.readFile(output);
      } catch (error) {
        if (error.code !== "ENOENT") throw error;
      }
      if (!bytes || sha(bytes) !== artifact.content_hash) {
        await open();
        await page.getByRole("button", { name: "Tasks", exact: true }).click();
        await page.getByLabel("Refresh tasks", { exact: true }).click();
        const cards = page.locator(".task-card").filter({ hasText: revision });
        await expect(cards.first()).toBeVisible();
        let found = false;
        for (let index = 0; index < (await cards.count()); index++) {
          const downloading = page.waitForEvent("download");
          await cards
            .nth(index)
            .getByRole("button", { name: "Download", exact: true })
            .click();
          const download = await downloading;
          if (
            download.suggestedFilename() === `opencut-${job.job_id}.${kind}`
          ) {
            await download.saveAs(output);
            found = true;
            break;
          }
          await download.delete();
        }
        expect(found).toBe(true);
        bytes = await fs.readFile(output);
      }
      expect(sha(bytes)).toBe(artifact.content_hash);
      record.output = output;
      record.sha256 = sha(bytes);
      await save();
      for (const time of kind === "png" ? [0.5] : [0, 0.5, 1.5, 2.9]) {
        const actual = await decode([
          ...(kind === "mp4" ? ["-ss", String(time)] : []),
          "-i",
          output,
          "-frames:v",
          "1",
          "-f",
          "rawvideo",
          "-pix_fmt",
          "rgba",
          "pipe:1",
        ]);
        const offset = Math.round(time * 30) * 320 * 180 * 4;
        const expected = (
          kind === "png" ? rawExpected : reference.expected
        ).subarray(offset, offset + 320 * 180 * 4);
        const base = (kind === "png" ? rawBase : reference.base).subarray(
          offset,
          offset + 320 * 180 * 4,
        );
        const result = {
          kind,
          time,
          ...measureMixedPixels(
            actual,
            expected,
            base,
            source.graphics.map((g) => g.spec),
            kind === "png" ? "preview" : "mp4",
          ),
        };
        facts.pixels = facts.pixels.filter(
          (p) => p.kind !== kind || p.time !== time,
        );
        facts.pixels.push(result);
        await save();
        expect(result.alphaErrors).toBe(0);
        expect(result.mean).toBeLessThanOrEqual(result.limits.mean);
        for (const region of result.regions) {
          expect(region.expectedPixels).toBeGreaterThan(100);
          expect(region.mean).toBeLessThanOrEqual(result.limits.regionMean);
          expect(region.iou).toBeGreaterThanOrEqual(result.limits.iou);
        }
      }
    }
    const pcm = await decode([
      "-i",
      facts.jobs.mp4.output,
      "-map",
      "0:a:0",
      "-ar",
      "48000",
      "-ac",
      "2",
      "-f",
      "f32le",
      "pipe:1",
    ]);
    const expected = await decode([
      "-i",
      path.join(sourceDirectory, "sample.wav"),
      "-ar",
      "48000",
      "-ac",
      "1",
      "-f",
      "f32le",
      "pipe:1",
    ]);
    expect(pcm.length / 8).toBeGreaterThanOrEqual(144000);
    expect(pcm.length / 8).toBeLessThanOrEqual(145024);
    const windows = [];
    for (const start of [9600, 72000, 120000])
      for (const channel of [0, 1]) {
        let square = 0,
          xx = 0,
          yy = 0,
          xy = 0;
        for (let i = 0; i < 4096; i++) {
          const a = pcm.readFloatLE(((start + i) * 2 + channel) * 4),
            e = expected.readFloatLE((start + i) * 4);
          square += (a - e) ** 2;
          xx += a * a;
          yy += e * e;
          xy += a * e;
        }
        const value = {
          start,
          channel,
          rmse: Math.sqrt(square / 4096),
          correlation: xy / Math.sqrt(xx * yy),
        };
        windows.push(value);
        expect(value.rmse).toBeLessThan(0.003);
        expect(value.correlation).toBeGreaterThan(0.99);
      }
    facts.pcm = {
      decoded_frames: pcm.length / 8,
      windows,
      reference_sha256: sha(
        await fs.readFile(path.join(sourceDirectory, "sample.wav")),
      ),
    };
    expect(errors).toEqual([]);
    facts.status = "passed";
    await save();
    check("D1 restored mixed PNG/MP4 independent pixels and PCM", {
      checkpoint,
      pixels: facts.pixels,
      pcm: facts.pcm,
      reference_key: facts.reference_key,
    });
  } catch (error) {
    facts.failure = String(error);
    await save();
    throw error;
  } finally {
    await page.close();
  }
}
