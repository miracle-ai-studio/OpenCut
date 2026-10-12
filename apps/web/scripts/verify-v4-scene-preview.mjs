import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { importProductMedia, placeProductMedia } from "./verify-v4-editor.mjs";

export function installScenePreviewWitness({ deferReadback = false } = {}) {
    const deferred = [], sampledSeconds = new Set();
    const w = (window.__scenePreviewWitness = {
      session: null,
      progress: null,
      frames: [],
      progresses: [],
      bindings: [],
      pcm: [],
      firstSoundAt: null,
      blocks: 0,
    });
    const originalFetch = window.fetch;
    window.fetch = async (...args) => {
      const response = await originalFetch(...args);
      const url = String(args[0]);
      if (/opencut_audio_preview_v4_(create|control)$/.test(url))
        response
          .clone()
          .json()
          .then((r) => {
            if (r.data) w.session = r.data;
          })
          .catch(() => {});
      if (url.endsWith("opencut_visual_preview_v4_frame"))
        response.clone().json().then(r => {
          if (r.data?.bound && w.bindings.length < 1024)
            w.bindings.push({ time: r.data.time_seconds, pts: r.data.bound.decoded.filter(d => d.kind === "readyVideo").map(d => d.actualTimestampSeconds) });
        }).catch(() => {});
      return response;
    };
    const Original = window.AudioWorkletNode;
    window.AudioWorkletNode = class extends Original {
      constructor(...args) {
        super(...args);
        const post = this.port.postMessage.bind(this.port);
        this.port.postMessage = (data, ...rest) => {
          if (data.kind === "reset")
            w.progress = {
              token: data.token,
              consumedFrames: "0",
              stalled: true,
            };
          if (data.kind === "block") {
            w.blocks++;
            if (w.pcm.length < 4096)
              w.pcm.push(
                ...new Float32Array(data.pcm)
                  .subarray(0, Math.min(8192, (4096 - w.pcm.length) * 2))
                  .filter((_, i) => i % 2 === 0),
              );
          }
          return post(data, ...rest);
        };
        this.port.addEventListener("message", ({ data }) => {
          if (data.kind === "progress" && data.token === w.progress?.token) {
            w.progress = data;
            if (Number(data.consumedFrames) > 0 && w.progresses.length < 8192)
              w.progresses.push({ frames: data.consumedFrames, stalled: data.copied === 0, at: performance.now() });
            if (Number(data.consumedFrames) > 0 && !w.firstSoundAt)
              w.firstSoundAt = Date.now();
          }
        });
      }
    };
    const put = CanvasRenderingContext2D.prototype.putImageData;
    CanvasRenderingContext2D.prototype.putImageData = function (data, ...args) {
      const result = put.call(this, data, ...args);
      if (
        this.canvas.classList.contains("v4-scene-frame") &&
        w.frames.length < 512
      ) {
        const t = Number(this.canvas.dataset.time),
          s = w.session,
          p = w.progress;
        const start = s
          ? Number(s.actual_start_sample) / s.identity.sample_rate
          : 0;
        const actual =
          s && p
            ? start + Number(p.consumedFrames) / s.identity.sample_rate
            : 0;
        const pixel =
          (Math.floor(data.height / 2) * data.width +
            Math.floor(data.width / 2)) *
          4;
        w.frames.push({
          width: data.width,
          height: data.height,
          pts: w.bindings.findLast(b => b.time === t)?.pts ?? [],
          time: t,
          actual,
          confirmed: s
            ? Number(s.authoritative_time.ticks) / s.authoritative_time.timebase
            : 0,
          generation: this.canvas.dataset.generation,
          color: [...data.data.subarray(pixel, pixel + 4)],
          at: performance.now(),
          runtime: this.canvas.dataset.runtime,
        });
      }
      return result;
    };
    const transfer = ImageBitmapRenderingContext.prototype.transferFromImageBitmap;
    ImageBitmapRenderingContext.prototype.transferFromImageBitmap = function (bitmap) {
      if (bitmap && this.canvas.classList.contains("v4-scene-frame") && w.frames.length < 512) {
        // Read one independently selected center pixel before ownership moves.
        // Avoid a full-frame GPU readback in the measurement itself.
        const s = w.session, p = w.progress, t = Number(this.canvas.dataset.time);
        const record = { width: bitmap.width, height: bitmap.height,
          pts: w.bindings.findLast(b => b.time === t)?.pts ?? [], time: t,
          actual: s && p ? Number(s.actual_start_sample) / s.identity.sample_rate + Number(p.consumedFrames) / s.identity.sample_rate : 0,
          confirmed: s ? Number(s.authoritative_time.ticks) / s.authoritative_time.timebase : 0,
          generation: this.canvas.dataset.generation, color: null,
          at: performance.now(), runtime: this.canvas.dataset.runtime };
        const read = (source) => {
          const sample = new OffscreenCanvas(1, 1), context = sample.getContext("2d", { willReadFrequently: true });
          context.drawImage(source, 0, 0, 1, 1);
          record.color = [...context.getImageData(0, 0, 1, 1).data];
          sample.width = 0; sample.height = 0;
        };
        if (deferReadback) {
          // Freeze one actual center pixel per authored second, at most 16.
          // Materialize its CPU bytes after the measured playback window; a
          // synchronous GPU read every paint otherwise dominates this test.
          const second = Math.floor(t);
          if (!sampledSeconds.has(second) && deferred.length < 16) {
            sampledSeconds.add(second);
            deferred.push({ record, bitmap: createImageBitmap(bitmap, Math.floor(bitmap.width / 2), Math.floor(bitmap.height / 2), 1, 1), read });
          }
        } else {
          const sample = new OffscreenCanvas(1, 1), context = sample.getContext("2d", { willReadFrequently: true });
          context.drawImage(bitmap, Math.floor(bitmap.width / 2), Math.floor(bitmap.height / 2), 1, 1, 0, 0, 1, 1);
          record.color = [...context.getImageData(0, 0, 1, 1).data];
          sample.width = 0; sample.height = 0;
        }
        w.frames.push(record);
      }
      return transfer.call(this, bitmap);
    };
    window.__readScenePreviewSamples = async () => {
      for (const item of deferred) {
        const bitmap = await item.bitmap;
        try { item.read(bitmap); } finally { bitmap.close(); }
      }
      deferred.length = 0;
    };
}

// Owned independent color schedule and 440Hz PCM; source fixture is generated
// by verify-web.py. The witness observes real Worklet consumption and canvas
// readback, rather than deriving expected pixels from the service frame plan.
export async function verifyV4ScenePreview(
  browser,
  endpoint,
  directory,
  check,
  { width = 160, height = 90 } = {},
) {
  const page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1100 },
  });
  page.setDefaultTimeout(30000);
  const errors = [],
    writes = [],
    streamObservations = [],
    requests = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("request", (r) => {
    if (r.url().endsWith("opencut_edit_v4_apply"))
      writes.push(r.postDataJSON());
    requests.push(new URL(r.url()).pathname);
  });
  page.on("response", (r) => {
    if (r.url().includes("/preview/v4/audio/stream"))
      streamObservations.push({ kind: "response", status: r.status() });
  });
  page.on("requestfailed", (r) => {
    if (r.url().includes("/preview/v4/audio/stream"))
      streamObservations.push({ kind: "failed", error: r.failure() });
  });
  await page.addInitScript(installScenePreviewWitness);
  const tool = async (name, input) => {
    const r = await page.evaluate(
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
    expect(r.error).toBeNull();
    return r.data;
  };
  const inspect = (id) =>
    tool("opencut_project_v4_inspect", { project_id: id, revision: null });
  const live = page.getByRole("img", {
    name: "Live scene preview",
    exact: true,
  }); // canvas has img role below
  const witness = () =>
    page.evaluate(() => structuredClone(window.__scenePreviewWitness));
  let id;
  try {
    await page.goto(`${endpoint}/editor`);
    await expect(
      page.getByRole("button", { name: "New project", exact: true }),
    ).toBeEnabled();
    const known = await fs.readFile(path.join(directory, "scene-preview-fixture.json"), "utf8").then(JSON.parse).catch(e => { if (e.code === "ENOENT") return null; throw e; });
    id = known?.project_id ?? `scene-preview-${crypto.randomUUID()}`;
    if (!known) {
    await tool("opencut_project_v4_create", {
      project: {
        id,
        name: "Continuous preview acceptance",
        sequence_id: "main",
        width,
        height,
        fps: { numerator: 24, denominator: 1 },
      },
      idempotency_key: `${id}-create`,
    });
    }
    await page.goto(`${endpoint}/editor/${id}`);
    await expect(
      page.getByRole("main", { name: "V4 editor", exact: true }),
    ).toBeVisible();
    const guide = page.getByRole("dialog", {
      name: "Welcome to OpenCut",
      exact: true,
    });
    if (await guide.isVisible())
      await guide.getByRole("button", { name: "Close", exact: true }).click();
    if (!known) {
    await importProductMedia(page, path.join(directory, "sample.mp4"), writes);
    await placeProductMedia(
      page,
      "sample.mp4",
      {
        track: "new",
        sourceAudio: false,
        fields: {
          "Timeline start (seconds)": "0",
          "Source start (seconds)": "0",
          "Source duration (seconds)": "8",
        },
      },
      writes,
    );
    await importProductMedia(page, path.join(directory, "sample.wav"), writes);
    await placeProductMedia(
      page,
      "sample.wav",
      {
        track: "new",
        keepPitch: true,
        fields: {
          "Timeline start (seconds)": "0",
          "Source start (seconds)": "0",
          "Source duration (seconds)": "8",
        },
      },
      writes,
    );
    }
    const fixed = await inspect(id),
      beforeWrites = writes.length;
    if (known) expect(fixed.revision).toBe(known.revision);
    const began = Date.now();
    await page
      .getByRole("button", { name: "Play scene preview", exact: true })
      .click();
    await expect(live).toBeVisible();
    expect(await live.evaluate(c => [c.width, c.height])).toEqual([width, height]);
    const firstPicture = Date.now() - began;
    expect(firstPicture).toBeLessThanOrEqual(10000);
    await expect
      .poll(
        async () => {
          const w = await witness();
          return Math.max(0, ...w.frames.map((f) => f.time));
        },
        { timeout: 20000 },
      )
      .toBeGreaterThan(4);
    await page
      .getByRole("button", { name: "Pause scene audio", exact: true })
      .click();
    await expect(
      page.getByText("Scene sound paused", { exact: true }),
    ).toBeVisible();
    await expect(live).toBeVisible();
    const first = await witness();
    const cold = Math.max(firstPicture, first.firstSoundAt - began);
    expect(first.firstSoundAt).not.toBeNull();
    expect(cold).toBeLessThanOrEqual(10000);
    const moving = first.frames.filter((f) => f.time > 0 && f.time < 4);
    expect(new Set(moving.map((f) => f.time)).size).toBeGreaterThanOrEqual(20);
    for (const frame of moving) {
      const expected =
        Math.floor(frame.time) % 2 === 0 ? [255, 0, 0, 255] : [0, 255, 0, 255];
      frame.color.forEach((v, c) =>
        expect(
          Math.abs(v - expected[c]),
          `color at ${frame.time}`,
        ).toBeLessThanOrEqual(4),
      );
      // Worklet messages arrive on the main thread; one 128-frame quantum plus
      // message scheduling is a bounded observation tolerance, not a DAC claim.
      expect(frame.actual - frame.time).toBeGreaterThanOrEqual(-0.01);
      expect(frame.actual - frame.time).toBeLessThanOrEqual(0.15);
    }
    const pcm = first.pcm,
      crossings = [];
    for (let i = 1; i < pcm.length; i++)
      if (pcm[i - 1] <= 0 && pcm[i] > 0)
        crossings.push(i - 1 - pcm[i - 1] / (pcm[i] - pcm[i - 1]));
    const hz =
      ((crossings.length - 1) * 48000) / (crossings.at(-1) - crossings[0]);
    expect(Math.abs(hz - 440)).toBeLessThan(1);
    const rms = Math.sqrt(pcm.reduce((s, v) => s + v * v, 0) / pcm.length);
    expect(rms).toBeGreaterThan(0.08);
    expect(rms).toBeLessThan(0.1);
    const paused = first.session.authoritative_time;
    await page.waitForTimeout(250);
    expect((await witness()).session.authoritative_time).toEqual(paused);
    check(
      "continuous normal preview: independent alternating red/green pixels, 440Hz PCM and actual Worklet-consumer skew <=150ms; pause freezes clock",
      {
        scene_preview: {
          width,
          height,
          cold_ms: cold,
          distinct_frames: new Set(moving.map((f) => f.time)).size,
          max_skew_ms: Math.max(
            ...moving.map((f) => (f.actual - f.time) * 1000),
          ),
          frequency_hz: hz,
          rms,
          runtime: JSON.parse(moving[0].runtime),
        },
      },
    );

    // Resume first picture is gated on the new server generation.
    const resumed = Date.now();
    await page
      .getByRole("button", { name: "Play scene preview", exact: true })
      .click();
    await expect(live).toBeVisible();
    expect(Date.now() - resumed).toBeLessThanOrEqual(1000);
    const oldGeneration = await live.getAttribute("data-generation");
    // Delay one old-generation bind reply until after seek. It must never paint.
    let held = false;
    let release;
    const released = new Promise((resolve) => {
      release = resolve;
    });
    await page.route(
      "**/v1/tools/opencut_visual_preview_v4_frame",
      async (route) => {
        const input = route.request().postDataJSON();
        if (
          !held &&
          input.decoded &&
          input.session.generation === oldGeneration
        ) {
          held = true;
          const response = await route.fetch();
          await released;
          await route.fulfill({ response });
        } else await route.continue();
      },
    );
    await expect.poll(() => held).toBe(true);
    const beforeSeek = (await witness()).frames.length;
    const seekBegan = Date.now();
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill("2.25");
    await page.getByLabel("Playhead (seconds)", { exact: true }).press("Enter");
    await expect(live).toBeHidden();
    release();
    await expect(live).toBeVisible();
    expect(Date.now() - seekBegan).toBeLessThanOrEqual(1000);
    expect(await live.getAttribute("data-generation")).not.toBe(oldGeneration);
    const after = (await witness()).frames.slice(beforeSeek);
    expect(after.every((f) => f.generation !== oldGeneration)).toBe(true);
    expect(after[0].time).toBeGreaterThanOrEqual(2.25);
    expect(after[0].color[0]).toBeGreaterThan(250);
    await page.unroute("**/v1/tools/opencut_visual_preview_v4_frame");
    await page
      .getByRole("button", { name: "Pause scene audio", exact: true })
      .click();
    await expect(
      page.getByText("Scene sound paused", { exact: true }),
    ).toBeVisible();
    expect((await inspect(id)).revision).toBe(fixed.revision);
    expect(writes.length).toBe(beforeWrites);
    expect(
      requests.filter((p) => p.endsWith("opencut_job_v4_frame_png")),
    ).toEqual([]);
    check(
      "hot resume/seek first picture <=1s, late old-generation bind cannot paint; no project edits or durable frame jobs",
      { seek_ms: Date.now() - seekBegan },
    );
    await fs.writeFile(
      path.join(directory, "scene-preview-fixture.json"),
      JSON.stringify({ project_id: id, revision: fixed.revision }),
    );
    expect(errors).toEqual([]);
  } catch (error) {
    await fs.writeFile(
      path.join(directory, "scene-preview-failure.json"),
      JSON.stringify(
        {
          errors,
          requests,
          streamObservations,
          witness: await witness(),
          alerts: await page.getByRole("alert").allTextContents(),
        },
        null,
        2,
      ),
    );
    await page.screenshot({
      path: path.join(directory, "scene-preview-failure.png"),
      fullPage: true,
    });
    throw error;
  } finally {
    await page.close();
  }
}

// Supplemental checks reuse the accepted project/media, not another import
// fixture. Invoked serially after scene-preview in the same owned workspace.
export async function verifyV4ScenePreviewBoundaries(
  browser,
  endpoint,
  directory,
  check,
) {
  const fixture = JSON.parse(
    await fs.readFile(
      path.join(directory, "scene-preview-fixture.json"),
      "utf8",
    ),
  );
  const page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1100 },
  });
  const errors = [],
    reads = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("request", (r) => {
    if (new URL(r.url()).pathname === "/v1/project-v4-resources/content")
      reads.push(r.url());
  });
  try {
    await page.goto(`${endpoint}/editor/${fixture.project_id}`);
    const play = page.getByRole("button", {
      name: "Play scene preview",
      exact: true,
    });
    await expect(play).toBeEnabled();
    const guide = page.getByRole("dialog", {
      name: "Welcome to OpenCut",
      exact: true,
    });
    if (await guide.isVisible())
      await guide.getByRole("button", { name: "Close", exact: true }).click();
    await play.click();
    const live = page.getByRole("img", {
      name: "Live scene preview",
      exact: true,
    });
    await expect(live).toBeVisible();
    await expect.poll(() => live.getAttribute("data-time"), { timeout: 10000 }).not.toBe("0");
    await page
      .getByRole("button", { name: "Pause scene audio", exact: true })
      .click();
    await expect(
      page.getByText("Scene sound paused", { exact: true }),
    ).toBeVisible();
    await expect(live).toBeVisible();
    expect(reads.length).toBe(1);
    await page
      .getByRole("combobox", { name: "Language", exact: true })
      .selectOption("zh");
    const chinesePlay = page.getByRole("button", {
      name: "播放场景声画",
      exact: true,
    });
    await expect(chinesePlay).toBeEnabled();
    await expect(
      page.getByRole("img", { name: "连续声画预览", exact: true }),
    ).toBeVisible();
    await chinesePlay.click();
    await expect(
      page.getByRole("img", { name: "连续声画预览", exact: true }),
    ).toBeVisible();
    await page
      .getByRole("button", { name: "暂停场景声音", exact: true })
      .click();
    await expect(
      page.getByText("场景声音已暂停", { exact: true }),
    ).toBeVisible();
    expect(reads.length).toBe(1);
    check(
      "Chinese preview entry/canvas and hot resume share the verified encoded video cache; only one scoped resource download",
      { preview_cache: { resource_downloads: reads.length } },
    );
    await page.route("**/v1/tools/opencut_visual_preview_v4_frame", (route) =>
      route.fulfill({
        status: 403,
        contentType: "application/json",
        body: JSON.stringify({
          api_version: "1",
          request_id: crypto.randomUUID(),
          data: null,
          error: {
            code: "NOT_AUTHORIZED",
            message: "Preview authorization withdrawn",
            details: {},
          },
          warnings: [],
        }),
      }),
    );
    await expect(
      page.getByRole("img", { name: "连续声画预览", exact: true }),
    ).toBeHidden({ timeout: 2500 });
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "Preview authorization withdrawn" }),
    ).toBeVisible();
    await page.waitForTimeout(250);
    await expect(
      page.getByRole("img", { name: "连续声画预览", exact: true }),
    ).toBeHidden();
    check(
      "paused preview revalidates authority within one second and clears cached pixels after a versioned permission rejection; genuine original-credential revocation is covered by the native host test",
    );
    expect(errors).toEqual([]);
  } catch (error) {
    await page.screenshot({
      path: path.join(directory, "scene-preview-boundaries-failure.png"),
      fullPage: true,
    });
    throw error;
  } finally {
    await page.close();
  }
}
