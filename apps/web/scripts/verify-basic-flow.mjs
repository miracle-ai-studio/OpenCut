import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { randomUUID, createHash } from "node:crypto";
import { editorSnapshot } from "./project-inspection.mjs";
import { placeProductMedia } from "./verify-editor.mjs";
import { verifySnapshots } from "./verify-snapshots.mjs";

// Reuse the retained short B1 composition and media. This complements the
// timeline suite; it does not import fixtures or exercise long-source audio.
export async function verifyBasicFlow(browser, endpoint, directory, check, options = {}) {
  const { project_id: projectId } = JSON.parse(await fs.readFile(path.join(directory, "b1-text-project.json"), "utf8"));
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
  page.setDefaultTimeout(30000);
  const errors = [], writes = [], jobs = [], evidence = [];
  let original, failure, phase = "open";
  page.on("pageerror", e => errors.push(e.message));
  page.on("request", request => {
    if (request.url().endsWith("/api/tools/opencut_edit_apply")) writes.push(request.postDataJSON());
  });
  page.on("response", async response => {
    if (response.url().endsWith("/api/tools/opencut_render_submit")) {
      const envelope = await response.json().catch(() => null);
      jobs.push({ request: response.request().postDataJSON(), envelope });
    }
  });
  const tool = async (name, input) => {
    const envelope = await page.evaluate(async ({ name, input }) => (await fetch(`/api/tools/${name}`, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input),
    })).json(), { name, input });
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull(); return envelope.data;
  };
  const inspect = () => editorSnapshot(tool("opencut_project_inspect", { project_id: projectId, revision: null }));
  const open = async () => {
    await page.goto(`${endpoint}/editor/${encodeURIComponent(projectId)}`);
    await expect(page.getByRole("main", { name: "OpenCut editor", exact: true })).toBeVisible({ timeout: 30000 });
    const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
    if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
  };
  const restore = async () => {
    const head = await inspect();
    if (head.revision !== original.revision) await tool("opencut_project_restore", {
      project_id: projectId, expected_revision: head.revision, target_revision: original.revision,
      idempotency_key: randomUUID(), asset_versions: {},
    });
    const restored = await inspect(); expect(restored.project).toEqual(original.project); return restored.revision;
  };
  try {
    await open(); original = await inspect();
    if (!options.snapshotsOnly) {
    phase = "rename";
    await page.getByRole("button", { name: `Rename project ${original.project.name}`, exact: true }).click();
    const rename = page.getByRole("dialog", { name: "Rename project", exact: true });
    const renamed = `${original.project.name} · basic flow`;
    await rename.getByRole("textbox", { name: "Project name", exact: true }).fill(renamed);
    await rename.getByRole("button", { name: "Save", exact: true }).click();
    await expect(rename).toBeHidden();
    const afterRename = await inspect();
    expect(afterRename.project).toEqual({ ...original.project, name: renamed });
    await open();
    await expect(page.getByRole("button", { name: `Rename project ${renamed}`, exact: true })).toBeVisible();
    evidence.push({ case: phase, revision: afterRename.revision });
    phase = "existing media placement";
    const beforeWrites = writes.length;
    const placed = await placeProductMedia(page, "sample.mp4", { track: "new", sourceAudio: false,
      fields: { "Timeline start (seconds)": "3", "Source start (seconds)": "0", "Source duration (seconds)": "1" },
    }, writes);
    expect(writes.length).toBe(beforeWrites + 1);
    const afterPlace = await inspect();
    expect(afterPlace.project.assets).toEqual(original.project.assets);
    const added = afterPlace.project.sequences.flatMap(s => s.tracks.flatMap(t => t.elements))
      .filter(e => !original.project.sequences.some(s => s.tracks.some(t => t.elements.some(old => old.id === e.id))));
    expect(added).toHaveLength(1);
    expect(added[0].content).toMatchObject({ kind: "video", source_audio_enabled: false });
    const seconds = time => Number(time.ticks) / time.timebase;
    expect(seconds(added[0].timeline_range.start)).toBe(3);
    expect(seconds(added[0].timeline_range.duration)).toBe(1);
    await open(); expect((await inspect()).project).toEqual(afterPlace.project);
    evidence.push({ case: phase, receipt: placed.receipt, plan: placed.plan });
    await restore(); await open();
    }
    phase = "snapshots";
    const capability = await tool("opencut_render_capability", {});
    expect(capability.frame.available, capability.frame.unavailable_reason).toBe(true);
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill("0.5");
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
    // Existing independently authored source is red at 0.5 s. Corner probes
    // avoid both fixed-font titles and the editor-only note overlay.
    await verifySnapshots(page, inspect, directory, name => evidence.push({ case: phase, check: name }), {
      probes: [{ x: 4, y: 4, rgb: [255, 0, 0], tolerance: 7 }, { x: 315, y: 175, rgb: [255, 0, 0], tolerance: 7 }],
    });
    expect(jobs).toHaveLength(2);
    for (const submitted of jobs) {
      expect(submitted.envelope.error).toBeNull();
      expect(submitted.request).toMatchObject({ kind: "frame", request: { project_id: projectId, revision: (await inspect()).revision, time: { ticks: "1", timebase: 2 } } });
      const job = await tool("opencut_job_view", { job_id: submitted.envelope.data.job_id });
      expect(job.state).toBe("succeeded"); expect(job.result.kind).toBe("render.frame");
      submitted.job = job;
    }
    expect(errors).toEqual([]);
  } catch (error) {
    failure = error;
    await fs.writeFile(path.join(directory, "basic-flow-failure.json"), JSON.stringify({ phase, error: String(error), errors,
      alerts: await page.getByRole("alert").allTextContents(), dialogs: await page.getByRole("dialog").allTextContents(),
      body: (await page.locator("body").innerText()).slice(-12000), jobs, evidence }, null, 2));
  } finally {
    if (original) {
      try { evidence.push({ case: "restoration", revision: await restore() }); }
      catch (error) { failure ??= error; }
    }
    await page.close();
  }
  if (failure) throw failure;
  const report = { status: "passed", project_id: projectId, binary_sha256: createHash("sha256").update(await fs.readFile(process.env.OPENCUT_WEB_BINARY)).digest("hex"), evidence, jobs, errors,
    scope: "Existing short composition; editor rename, reopen, existing media placement, original reviewed transaction, exact restoration, full-resolution PNG save/image clipboard and independent source pixels. Native save dialog is simulated; no long audio route." };
  report.scope = options.snapshotsOnly ? "Full-resolution PNG save/image clipboard, independent source pixels and cancelled save; retained composition unchanged. Native save dialog simulated; no long audio route." : report.scope;
  await fs.writeFile(path.join(directory, options.snapshotsOnly ? "basic-snapshots.json" : "basic-flow.json"), JSON.stringify(report, null, 2));
  check(options.snapshotsOnly ? "Retained scene snapshots" : "Basic editor rename, retained media placement and scene snapshots", report);
}
