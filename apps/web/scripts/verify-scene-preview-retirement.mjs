import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { nativeTree, processes } from "./verify-scene-preview-continuity.mjs";

// Real document departure, not React unmount or a synthetic pagehide event.
// Reuse the original fixture and whole owned-tree census; never exclude old actors.
export async function verifyScenePreviewRetirement(browser, endpoint, directory, check) {
  const fixture = JSON.parse(await fs.readFile(path.join(directory, "scene-preview-fixture.json"), "utf8"));
  const service = JSON.parse(await fs.readFile(path.join(directory, "service/state.json"), "utf8"));
  const evidence = { phases: [], readonly: true, scope: "Known playing/paused session, full document navigation; no abrupt process kill or unknown admission proof." };
  const snapshot = () => {
    const rows = nativeTree(service.pid, processes());
    if (rows.length > 64) throw Error("Retirement census exceeded64 owned processes");
    return { rss: rows.reduce((sum, r) => sum + r.rss, 0), processes: rows };
  };
  try {
    for (const phase of ["paused", "playing"]) {
      const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
      let session;
      page.on("response", async response => {
        if (!response.url().endsWith("/api/tools/opencut_audio_preview_control")) return;
        try { const r = await response.json(); if (r.data) session = r.data; } catch { /* discarded document */ }
      });
      const tool = async (name, input) => {
        const r = await page.evaluate(async ({ name, input }) => (await fetch(`/api/tools/${name}`, {
          method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input),
        })).json(), { name, input });
        expect(r.error).toBeNull(); return r.data;
      };
      const record = { phase, before: null, after: null, state_after_departure: null };
      evidence.phases.push(record);
      try {
        await page.goto(`${endpoint}/editor/${fixture.project_id}`);
        const play = page.getByRole("button", { name: "Play scene preview", exact: true });
        await expect(play).toBeEnabled();
        const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
        if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
        await play.click();
        await expect.poll(async () => Number(await page.getByRole("img", { name: "Live scene preview", exact: true }).getAttribute("data-time")), { timeout: 15000 }).toBeGreaterThan(0);
        await expect.poll(() => session?.state, { timeout: 15000 }).toBe("playing");
        if (phase === "paused") {
          await page.getByRole("button", { name: "Pause scene audio", exact: true }).click();
          await expect(page.getByText("Scene sound paused", { exact: true })).toBeVisible();
          await expect.poll(() => session?.state).toBe("paused");
        }
        const id = session.session_id;
        record.before = snapshot();
        expect(record.before.processes.some(r => r.role === "node")).toBe(true);
        const owned = new Map(record.before.processes.filter(r => r.pid !== service.pid).map(r => [r.pid, r.start]));
        await page.goto(`${endpoint}/editor`);
        await expect(page.getByRole("button", { name: "New project", exact: true })).toBeEnabled();
        await expect.poll(async () => {
          const s = await tool("opencut_audio_preview_inspect", { session_id: id });
          record.state_after_departure = s.state; return s.state;
        }, { timeout: 3000 }).toBe("stopped");
        await expect.poll(() => processes().filter(r => owned.get(r.pid) === r.start).length, { timeout: 10000 }).toBe(0);
        record.after = snapshot();
        const cdp = await browser.newBrowserCDPSession();
        try { expect((await cdp.send("Target.getTargets")).targetInfos.filter(t => t.type === "worker" && /pump-worker/.test(t.url))).toHaveLength(0); }
        finally { await cdp.detach(); }
        const head = await editorSnapshot(tool("opencut_project_inspect", { project_id: fixture.project_id, revision: null }));
        expect(head.revision).toBe(fixture.revision);
      } finally { record.after ??= snapshot(); await page.close(); }
    }
    check("playing/paused document departure stops the original session and releases actual owned native processes and PCM Worker", { scene_preview_retirement: evidence });
  } finally {
    await fs.writeFile(path.join(directory, "scene-preview-retirement-evidence.json"), JSON.stringify(evidence, null, 2));
  }
}
