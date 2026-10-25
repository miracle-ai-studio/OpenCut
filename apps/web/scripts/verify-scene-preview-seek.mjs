import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import {
  installScenePreviewWitness,
  trackScenePreviewPumps,
} from "./verify-scene-preview.mjs";

// Reuse original eight-second scene/media. Hold one real acknowledged response
// body, not a fake session/clock, to exercise each actual transport boundary.
export async function verifyScenePreviewSeek(
  browser,
  endpoint,
  directory,
  check,
  {pauseAfterSeek = false} = {},
) {
  const fixture = JSON.parse(
    await fs.readFile(
      path.join(directory, "scene-preview-fixture.json"),
      "utf8",
    ),
  );
  for (const boundary of pauseAfterSeek ? ["accepted_seek"] : ["final_report", "accepted_seek"]) {
    const page = await browser.newPage({
      locale: "en-US",
      viewport: { width: 1440, height: 1100 },
    });
    const controls = [],
      writes = [],
      errors = [];
    await page.addInitScript(installScenePreviewWitness);
    const witness = trackScenePreviewPumps(page, {
      pcmWindows: [{ id: "first", start_sample: 0, frames: 4096 }],
      controlGate: boundary === "final_report" ? {kind: "report_consumed", stoppedReport: true} : null,
    });
    page.on("pageerror", (e) => errors.push(e.message));
    page.on("request", (r) => {
      if (r.url().endsWith("opencut_edit_apply"))
        writes.push(r.postDataJSON());
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
    let armed = false,
      held = false,
      release;
    let heldWorker;
    const releaseBody = async () => {
      release();
      if (heldWorker) await heldWorker.evaluate(() => globalThis.__releaseScenePreviewControl?.()).catch(() => {});
    };
    const gate = new Promise((resolve) => {
      release = resolve;
    });
    await page
      .context()
      .route("**/api/tools/opencut_audio_preview_control", async (route) => {
        const input = route.request().postDataJSON();
        controls.push(input);
        // The final report is owned by the Worker body gate; never hold it a
        // second time in the browser network route before its body can arrive.
        const matches = boundary === "accepted_seek" && input.command.kind === "seek";
        if (armed && !held && matches) {
          held = true;
          const response = await route.fetch();
          await gate;
          await route.fulfill({ response });
        } else await route.continue();
      });
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
      await page.goto(`${endpoint}/editor/${fixture.project_id}`);
      const guide = page.getByRole("dialog", {
        name: "Welcome to OpenCut",
        exact: true,
      });
      if (await guide.isVisible())
        await guide.getByRole("button", { name: "Close", exact: true }).click();
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
      const original = (await witness()).session,
        before = controls.length;
      armed = true;
      if (boundary === "final_report") {
        heldWorker = page.workers().find(worker => /pump-worker/.test(worker.url()));
        expect(heldWorker).toBeTruthy();
        await heldWorker.evaluate(() => { globalThis.__scenePumpWitness.controlGate.armed = true; });
      }
      const seek = async (value) => {
        const field = page.getByLabel("Playhead (seconds)", { exact: true });
        await field.fill(value);
        await field.press("Enter");
      };
      await seek("1.25");
      await expect.poll(async () => boundary === "final_report" ? heldWorker.evaluate(() => globalThis.__scenePumpWitness.controlGate.held) : held).toBe(true);
      await seek("2.25");
      const lastIntentBegan = Date.now();
      await seek("3.25");
      await expect(live).toBeHidden();
      if (pauseAfterSeek) await page.getByRole("button", {name: "Pause scene audio", exact: true}).click();
      const released = Date.now();
      await releaseBody();
      if (pauseAfterSeek) {
        await expect(page.getByText("Scene sound paused", {exact: true})).toBeVisible();
        const final = await tool("opencut_audio_preview_inspect", {session_id: original.session_id});
        expect(final.state).toBe("paused");
        expect(final.owner_id).toBe(original.owner_id); expect(final.lease_id).toBe(original.lease_id);
        expect(final.identity).toEqual(original.identity);
        // Two seeks and Pause each reanchor in the Rust state machine.
        expect(BigInt(final.generation)-BigInt(original.generation)).toBe(3n);
        expect(final.presentation_time).toEqual({ticks: "156000", timebase: 48000});
        expect(final.consumed_frames).toBe("0");
        const requests = controls.slice(before);
        expect(requests.filter(r => r.command.kind === "seek").map(r => r.command.sequence_time)).toEqual([{ticks: "5", timebase: 4}, {ticks: "13", timebase: 4}]);
        expect(requests.at(-1)).toMatchObject({generation: (BigInt(final.generation)-1n).toString(), command: {kind: "pause"}});
        expect(requests.some(r => ["stop", "play"].includes(r.command.kind))).toBe(false);
        expect((await witness()).deviceStates).toEqual(["closed"]);
        const cdp = await browser.newBrowserCDPSession();
        try { expect((await cdp.send("Target.getTargets")).targetInfos.filter(t => t.type === "worker" && /pump-worker/.test(t.url))).toHaveLength(0); }
        finally { await cdp.detach(); }
        expect((await editorSnapshot(tool("opencut_project_inspect", {project_id: fixture.project_id, revision: null}))).revision).toBe(fixed.revision);
        expect(writes).toEqual([]); expect(errors).toEqual([]);
        check("actual pause during accepted rapid seek preserves latest target and authority without restarting a PCM Worker", {scene_preview_seek_pause: {project_id: fixture.project_id, revision: fixed.revision, generation_delta: "3", authoritative_sample: final.presentation_time.ticks, consumed_frames: final.consumed_frames, device_closed: true, worker_targets: 0, readonly: true}});
        continue;
      }
      await expect(live).toBeVisible({ timeout: 10000 });
      await expect
        .poll(async () => Number(await live.getAttribute("data-time")))
        .toBeGreaterThanOrEqual(3.25);
      const firstPictureMs = Date.now() - released;
      const latestSeekMs = Date.now() - lastIntentBegan;
      expect(latestSeekMs).toBeLessThanOrEqual(1000);
      let current;
      await expect
        .poll(async () => {
          current = await witness();
          return current.frames.filter(
            (f) => f.generation === current.session?.generation,
          ).length;
        })
        .toBeGreaterThanOrEqual(4);
      const final = current.session,
        requests = controls.slice(before),
        seeks = requests.filter((r) => r.command.kind === "seek");
      expect(final.session_id).toBe(original.session_id);
      expect(final.identity).toEqual(original.identity);
      expect(final.owner_id).toBe(original.owner_id);
      expect(final.lease_id).toBe(original.lease_id);
      expect(seeks.map((r) => r.command.sequence_time)).toEqual(
        boundary === "final_report"
          ? [{ ticks: "13", timebase: 4 }]
          : [
              { ticks: "5", timebase: 4 },
              { ticks: "13", timebase: 4 },
            ],
      );
      expect(BigInt(final.generation) - BigInt(original.generation)).toBe(
        boundary === "final_report" ? 1n : 2n,
      );
      expect(requests.some((r) => r.command.kind === "stop")).toBe(false);
      const newFrames = current.frames.filter(
        (f) => f.generation === final.generation,
      );
      for (const f of newFrames) {
        const expected =
          Math.floor(f.time) % 2 ? [0, 255, 0, 255] : [255, 0, 0, 255];
        f.color.forEach((v, i) =>
          expect(Math.abs(v - expected[i])).toBeLessThanOrEqual(4),
        );
        expect(f.actual - f.time).toBeGreaterThanOrEqual(-0.01);
        expect(f.actual - f.time).toBeLessThanOrEqual(0.15);
      }
      const captured = current.selectedPcm[0];
      expect(captured.frames).toBe(4096);
      const channels = [0, 1].map((c) =>
        captured.values.filter((_, i) => i % 2 === c),
      );
      const stats = channels.map((samples) => {
        const crossings = [];
        for (let i = 1; i < samples.length; i++)
          if (samples[i - 1] <= 0 && samples[i] > 0)
            crossings.push(
              i - 1 - samples[i - 1] / (samples[i] - samples[i - 1]),
            );
        return {
          frequency_hz:
            ((crossings.length - 1) * 48000) /
            (crossings.at(-1) - crossings[0]),
          rms: Math.sqrt(
            samples.reduce((n, v) => n + v * v, 0) / samples.length,
          ),
        };
      });
      for (const s of stats) {
        expect(Math.abs(s.frequency_hz - 440)).toBeLessThan(1);
        expect(Math.abs(s.rms - 0.125 / Math.sqrt(2))).toBeLessThan(0.001);
      }
      expect(
        Math.max(...channels[0].map((v, i) => Math.abs(v - channels[1][i]))),
      ).toBeLessThan(1e-7);
      await page
        .getByRole("button", { name: "Pause scene audio", exact: true })
        .click();
      await expect(
        page.getByText("Scene sound paused", { exact: true }),
      ).toBeVisible();
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
        `real rapid seek coalesces latest target during${boundary}, original fixed authority, independent stereoPCM/pixels and synchronized resume`,
        {
          scene_preview_seek: {
            boundary,
            project_id: fixture.project_id,
            revision: fixed.revision,
            accepted_seek_commands: seeks.length,
            first_picture_after_release_ms: firstPictureMs,
            latest_seek_ms: latestSeekMs,
            generation_delta: String(
              BigInt(final.generation) - BigInt(original.generation),
            ),
            independent_pcm: stats,
            pixel_frames: newFrames.length,
            max_skew_ms: Math.max(
              ...newFrames.map((f) => (f.actual - f.time) * 1000),
            ),
            readonly: true,
            scope:
              "Held real control response body. Same mono440Hz original source duplicated to stereo; no distinct-channel/gain/DAC/full controls or sustained performance proof.",
          },
        },
      );
    } finally {
      await releaseBody();
      await page
        .context()
        .unroute("**/api/tools/opencut_audio_preview_control");
      await page.close();
    }
  }
}
