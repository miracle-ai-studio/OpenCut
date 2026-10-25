import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import {
  installScenePreviewWitness,
  trackScenePreviewPumps,
} from "./verify-scene-preview.mjs";

// Existing eight-second fixed scene, actual pointer/Escape controls and original
// Worker byte witness. Delays only one real accepted response; no fake cursor.
export async function verifyScenePreviewGain(
  browser,
  endpoint,
  directory,
  check,
  { pausedOnly = false } = {},
) {
  const fixture = JSON.parse(
    await fs.readFile(
      path.join(directory, "scene-preview-fixture.json"),
      "utf8",
    ),
  );
  for (const boundary of pausedOnly
    ? ["paused_gain"]
    : ["gain_during_seek", "escape_seek_pause"]) {
    const page = await browser.newPage({
      locale: "en-US",
      viewport: { width: 1440, height: 1100 },
    });
    const requests = [],
      writes = [],
      errors = [];
    await page.addInitScript(installScenePreviewWitness);
    const witness = trackScenePreviewPumps(page, {
      pcmWindows: [{ id: "first", start_sample: 0, frames: 4096 }],
    });
    page.on("pageerror", (e) => errors.push(e.message));
    page.on("request", (r) => {
      if (/opencut_(edit_apply|project_audio_gain_plan)$/.test(r.url()))
        writes.push(r.url());
    });
    let armed = false,
      held = false,
      release;
    const gate = new Promise((r) => {
      release = r;
    });
    const gateKind =
      boundary === "gain_during_seek" ? "seek" : "set_temporary_gain";
    await page
      .context()
      .route("**/api/tools/opencut_audio_preview_control", async (route) => {
        const input = route.request().postDataJSON();
        requests.push(input);
        if (armed && !held && input.command.kind === gateKind) {
          const response = await route.fetch();
          expect(response.status()).toBe(200);
          held = true;
          await gate;
          await route.fulfill({ response });
        } else await route.continue();
      });
    const tool = async (name, input) => {
      const reply = await page.evaluate(
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
      expect(reply.error).toBeNull();
      return reply.data;
    };
    const seek = async () => {
      const field = page.getByLabel("Playhead (seconds)", { exact: true });
      await field.fill("3.25");
      await field.press("Enter");
    };
    try {
      await page.goto(`${endpoint}/editor`);
      await expect(
        page.getByRole("button", { name: "New project", exact: true }),
      ).toBeEnabled();
      const fixed = await editorSnapshot(tool("opencut_project_inspect", {
        project_id: fixture.project_id,
        revision: null,
      }));
      expect(fixed.revision).toBe(fixture.revision);
      const scene = fixed.project.sequences.find((s) =>
        s.tracks.some((t) =>
          t.elements.some((e) => e.content.kind === "audio"),
        ),
      );
      const track = scene.tracks.find((t) =>
        t.elements.some((e) => e.content.kind === "audio"),
      );
      const audio = track.elements.find((e) => e.content.kind === "audio");
      expect(audio.content.audio.gain).toEqual({ unit: "decibels", value: 0 });
      await page.goto(`${endpoint}/editor/${fixture.project_id}`);
      const guide = page.getByRole("dialog", {
        name: "Welcome to OpenCut",
        exact: true,
      });
      if (await guide.isVisible())
        await guide.getByRole("button", { name: "Close", exact: true }).click();
      const slider = page.getByRole("slider", {
        name: `Volume ${audio.name}`,
        exact: true,
      });
      const surface = page
        .locator(".editor-timeline-element")
        .filter({ has: slider })
        .locator(".editor-gain-surface");
      const live = page.getByRole("img", {
        name: "Live scene preview",
        exact: true,
      });
      await page
        .getByRole("button", { name: "Play scene preview", exact: true })
        .click();
      await expect
        .poll(async () => Number(await live.getAttribute("data-time")), {
          timeout: 15000,
        })
        .toBeGreaterThan(0.05);
      const original = (await witness()).session;
      const before = requests.length;
      const pause = async () => {
        await page
          .getByRole("button", { name: "Pause scene audio", exact: true })
          .click();
        await expect(
          page.getByText("Scene sound paused", { exact: true }),
        ).toBeVisible();
      };
      let pausedReports = null,
        pausedAnchor = null;
      if (boundary === "paused_gain") {
        await pause();
        pausedReports = requests.filter(
          (r) => r.command.kind === "report_consumed",
        ).length;
        pausedAnchor = (
          await tool("opencut_audio_preview_inspect", {
            session_id: original.session_id,
          })
        ).presentation_time;
      } else armed = true;
      if (boundary === "gain_during_seek") {
        await seek();
        await expect.poll(() => held).toBe(true);
      }
      await slider.scrollIntoViewIfNeeded();
      const line = await slider.boundingBox();
      expect(line.height).toBe(14);
      await page.mouse.move(line.x + line.width / 2, line.y + line.height / 2);
      await page.mouse.down();
      const box = await surface.boundingBox();
      const percent = 1 - Math.sqrt((10 ** (-12 / 20) - 0.001) / 9.999);
      await page.mouse.move(
        box.x + box.width / 2,
        box.y + box.height * percent,
      );
      await expect(slider).toHaveAttribute("aria-valuenow", "-12");
      if (boundary === "escape_seek_pause") {
        await expect.poll(() => held).toBe(true);
        await page.keyboard.press("Escape");
        await page.mouse.up();
        await seek();
        await page
          .getByRole("button", { name: "Pause scene audio", exact: true })
          .click();
        release();
        await expect(
          page.getByText("Scene sound paused", { exact: true }),
        ).toBeVisible();
      } else {
        release();
        await expect
          .poll(
            async () =>
              (
                await tool("opencut_audio_preview_inspect", {
                  session_id: original.session_id,
                })
              ).temporary_gain?.decibels_tenths,
          )
          .toBe(-120);
      }
      const waitForOutput = async (tenths) => {
        await expect
          .poll(
            () =>
              page.evaluate((tenths) => {
                const w = globalThis.__scenePreviewWitness,
                  s = w.session;
                return (
                  !!s &&
                  (s.temporary_gain?.decibels_tenths ?? null) === tenths &&
                  w.progress?.token ===
                    `${s.session_id}:${s.generation}:${s.overlay_epoch}` &&
                  BigInt(w.progress.consumedFrames) > 0n
                );
              }, tenths),
            { timeout: 10000 },
          )
          .toBe(true);
      };
      const assertPcm = (window, db) => {
        expect(window.frames).toBe(4096);
        expect(window.values.every(Number.isFinite)).toBe(true);
        const channels = [0, 1].map((c) =>
          window.values.filter((_, i) => i % 2 === c),
        );
        const acoustic = channels.map((values) => {
          const crossings = [];
          for (let i = 1; i < values.length; i++)
            if (values[i - 1] <= 0 && values[i] > 0)
              crossings.push(
                i - 1 - values[i - 1] / (values[i] - values[i - 1]),
              );
          return {
            frequency_hz:
              ((crossings.length - 1) * 48000) /
              (crossings.at(-1) - crossings[0]),
            rms: Math.sqrt(
              values.reduce((n, v) => n + v * v, 0) / values.length,
            ),
          };
        });
        for (const s of acoustic) {
          expect(Math.abs(s.frequency_hz - 440)).toBeLessThan(1);
          expect(
            Math.abs(s.rms - (0.125 / Math.sqrt(2)) * 10 ** (db / 20)),
          ).toBeLessThan(0.0005);
        }
        expect(
          Math.max(...channels[0].map((v, i) => Math.abs(v - channels[1][i]))),
        ).toBeLessThan(1e-7);
        return acoustic;
      };
      let acoustic = null,
        restoredAcoustic = null,
        pixelFrames = 0,
        maxSkew = null;
      if (boundary === "gain_during_seek") {
        await waitForOutput(-120);
        let sampled;
        await expect
          .poll(
            async () => {
              sampled = await witness();
              return (
                sampled.session?.temporary_gain?.decibels_tenths === -120 &&
                sampled.selectedPcm?.length === 1 &&
                sampled.frames.filter(
                  (f) => f.generation === sampled.session.generation,
                ).length >= 4
              );
            },
            { timeout: 10000 },
          )
          .toBe(true);
        acoustic = assertPcm(sampled.selectedPcm[0], -12);
        const frames = sampled.frames.filter(
          (f) => f.generation === sampled.session.generation,
        );
        pixelFrames = frames.length;
        maxSkew = Math.max(...frames.map((f) => (f.actual - f.time) * 1000));
        for (const f of frames) {
          const expected =
            Math.floor(f.time) % 2 ? [0, 255, 0, 255] : [255, 0, 0, 255];
          f.color.forEach((v, i) =>
            expect(Math.abs(v - expected[i])).toBeLessThanOrEqual(4),
          );
          expect(f.actual - f.time).toBeGreaterThanOrEqual(-0.01);
          expect(f.actual - f.time).toBeLessThanOrEqual(0.15);
        }
      }
      if (boundary !== "escape_seek_pause") {
        await page.keyboard.press("Escape");
        await page.mouse.up();
        await expect
          .poll(
            async () =>
              (
                await tool("opencut_audio_preview_inspect", {
                  session_id: original.session_id,
                })
              ).temporary_gain,
          )
          .toBeNull();
        if (boundary === "gain_during_seek") {
          await waitForOutput(null);
          await expect(live).toBeVisible();
          let restored;
          await expect
            .poll(async () => {
              restored = await witness();
              return restored.selectedPcm?.length;
            })
            .toBe(1);
          restoredAcoustic = assertPcm(restored.selectedPcm[0], 0);
          await pause();
        }
      }
      await expect(slider).toHaveAttribute("aria-valuenow", "0");
      const final = await tool("opencut_audio_preview_inspect", {
        session_id: original.session_id,
      });
      expect(final.state).toBe("paused");
      expect(final.temporary_gain).toBeNull();
      expect(final.owner_id).toBe(original.owner_id);
      expect(final.lease_id).toBe(original.lease_id);
      expect(final.identity).toEqual(original.identity);
      const commands = requests
        .slice(before)
        .filter(
          (r) => !["report_consumed", "heartbeat"].includes(r.command.kind),
        );
      const expected =
        boundary === "gain_during_seek"
          ? ["seek", "set_temporary_gain", "clear_temporary_gain", "pause"]
          : boundary === "escape_seek_pause"
            ? ["set_temporary_gain", "seek", "clear_temporary_gain", "pause"]
            : ["pause", "set_temporary_gain", "clear_temporary_gain"];
      expect(commands.map((r) => r.command.kind)).toEqual(expected);
      for (let i = 1; i < commands.length; i++)
        expect(BigInt(commands[i].generation)).toBe(
          BigInt(commands[i - 1].generation) + 1n,
        );
      expect(
        commands.find((r) => r.command.kind === "set_temporary_gain").command
          .gain,
      ).toMatchObject({
        track_id: track.id,
        element_id: audio.id,
        decibels_tenths: -120,
      });
      if (boundary === "escape_seek_pause") {
        expect(final.presentation_time).toEqual({
          ticks: "156000",
          timebase: 48000,
        });
        expect(final.consumed_frames).toBe("0");
      }
      if (boundary === "paused_gain") {
        expect(
          requests.filter((r) => r.command.kind === "report_consumed").length,
        ).toBe(pausedReports);
        expect(final.presentation_time).toEqual(pausedAnchor);
        expect(final.consumed_frames).toBe("0");
      }
      const deviceStates = (await witness()).deviceStates;
      expect(deviceStates.every((s) => s === "closed")).toBe(true);
      const cdp = await browser.newBrowserCDPSession();
      try {
        expect(
          (await cdp.send("Target.getTargets")).targetInfos.filter(
            (t) => t.type === "worker" && /pump-worker/.test(t.url),
          ),
        ).toHaveLength(0);
      } finally {
        await cdp.detach();
      }
      expect(
        (
          await editorSnapshot(tool("opencut_project_inspect", {
            project_id: fixture.project_id,
            revision: null,
          }))
        ).revision,
      ).toBe(fixed.revision);
      expect(writes).toEqual([]);
      expect(errors).toEqual([]);
      check(
        `actual gain/${boundary} uses serial original authority, Escape without edits and closed paused device`,
        {
          scene_preview_gain: {
            boundary,
            project_id: fixture.project_id,
            revision: fixed.revision,
            commands: commands.map((r) => r.command.kind),
            generation_delta: String(
              BigInt(final.generation) - BigInt(original.generation),
            ),
            independent_pre_master_pcm: acoustic,
            restored_pre_master_pcm: restoredAcoustic,
            pixel_frames: pixelFrames,
            max_skew_ms: maxSkew,
            device_closed: true,
            worker_targets: 0,
            readonly: true,
            scope:
              "Original single mono440Hz source at-12dB duplicated to stereo. Actual Worklet supply and independent pixels; no separate reference voice, post-master capture/DAC, full control or long-runtime performance claim.",
          },
        },
      );
    } finally {
      release();
      await page.keyboard.press("Escape").catch(() => {});
      await page.mouse.up().catch(() => {});
      await page
        .context()
        .unroute("**/api/tools/opencut_audio_preview_control");
      await page.close();
    }
  }
}
