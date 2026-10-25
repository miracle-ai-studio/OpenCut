/** Additional fault cases for the retained preview recovery harness. */
import fs from "node:fs/promises";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { nativeTree, processes } from "./verify-scene-preview-continuity.mjs";

export async function verifyPreviewOutages(h) {
  const { args, candidate, state, stage, save, refreshClient, call, slot, native, graph, fixture, document, observe, sameAuthority, expect } = h;
  const expected = state.outage_baseline;
  const originalRequests = (page, target) => {
    page.on("request", r => { if (/\/opencut_audio_preview_(?:recovery_(?:stage|dispatch|ack)|create)$/.test(r.url())) target.push({ tool: r.url().split("/").at(-1), input: r.postDataJSON() }); });
  };
  const capture = async (d, command, accept) => {
    const records = new Map(); let playing;
    await d.context.route("**/api/tools/opencut_audio_preview_recovery_stage", async route => {
      if (d.offline) return route.abort("internetdisconnected");
      const original = route.request().postDataJSON(), response = await route.fetch(), body = await response.json(); expect(body.error).toBeNull();
      records.set(body.data.recovery_id, { original, record: body.data }); await route.fulfill({ response });
    });
    await d.context.route("**/api/tools/opencut_audio_preview_recovery_dispatch", async route => {
      if (d.offline) return route.abort("internetdisconnected");
      const input = route.request().postDataJSON(), response = await route.fetch(), body = await response.json(); expect(body.error).toBeNull();
      const saved = records.get(input.recovery_id); expect(saved).toBeTruthy();
      if (saved.original.request.kind === "control" && saved.original.request.request.command.kind === command) await accept({ ...saved, input, receipt: body.data }, route, response);
      else { if (body.data.state === "playing") playing = body.data; await route.fulfill({ response }); }
    });
    await d.open(); await expect(d.page.getByRole("button", { name: "Play scene audio", exact: true })).toBeEnabled(); await d.page.getByRole("button", { name: "Play scene audio", exact: true }).click();
    await expect.poll(() => !!playing).toBe(true); await expect.poll(() => d.page.evaluate(() => globalThis.__recoveryPump.progress)).toBe(true);
    return playing;
  };
  await stage("offline-accepted-pause-reload", async () => {
    expect((await slot(fixture.project_id)).record).toBeNull();
    const d = await document(fixture.project_id); let accepted;
    try {
      const playing = await capture(d, "pause", async (value, route) => {
        accepted = value; state.offline_pause = { record: value.record, receipt: observe(value.receipt) }; await save();
        // route.fetch uses the test API transport; do not let it bypass the
        // page's genuine offline state for any subsequent recovery request.
        d.offline = true; await d.context.setOffline(true); await route.abort("internetdisconnected");
      });
      await d.page.getByRole("button", { name: "Pause scene audio", exact: true }).click(); await expect.poll(() => !!accepted).toBe(true);
      expect(await d.page.evaluate(async () => { try { await fetch("/app/session"); return true; } catch { return false; } })).toBe(false);
      const paused = await native(playing.session_id); expect(paused.state).toBe("paused"); sameAuthority(playing, paused);
      expect((await slot(fixture.project_id)).record).toMatchObject({ recovery_id: accepted.record.recovery_id, status: "completed" });
      const restored = []; await d.context.unroute("**/api/tools/opencut_audio_preview_recovery_stage"); await d.context.unroute("**/api/tools/opencut_audio_preview_recovery_dispatch");
      originalRequests(d.page, restored); d.offline = false; await d.context.setOffline(false); await d.page.reload();
      await expect(d.page.getByRole("region", { name: "Timeline", exact: true })).toBeVisible(); await expect.poll(async () => (await slot(fixture.project_id)).record).toBeNull();
      expect(restored).toEqual([{ tool: "opencut_audio_preview_recovery_ack", input: { project_id: fixture.project_id, recovery_id: accepted.record.recovery_id, disposition: "retire" } }]);
      const stopped = await native(playing.session_id); expect(stopped.state).toBe("stopped"); sameAuthority(playing, stopped);
      expect(await d.page.evaluate(() => globalThis.__recoveryPump.starts)).toEqual([]); expect(graph()).toEqual(expected);
      return { session_id: playing.session_id, recovery_id: accepted.record.recovery_id, browser_really_offline: true, accepted_pause_survived_lost_response: true, reconnect_reload_only_retired_original: true, no_pcm_or_command_restart: true, original_owner_lease_and_identity: true };
    } finally { await d.context.setOffline(false).catch(() => {}); await d.close(); }
  });
  const env = { ...process.env };
  for (const k of Object.keys(env)) if (/^OPENCUT_(CLASSIC_|SOURCE_ART_|FONT_DERIVE_|FONT_COLOR_STATIC_)/.test(k)) delete env[k];
  env.OPENCUT_FONT_DERIVE_CACHE = "/private/tmp/opencut-b3-font-cache"; env.OPENCUT_ALLOW_PUBLIC_FONT_DOWNLOAD = "1";
  const server = (...words) => {
    const raw = execFileSync(candidate.binary, ["--json", "--database", path.join(args.workspace, "workspace.sqlite"), "server", ...words, "--directory", path.join(args.workspace, "service")], { encoding: "utf8", env, timeout: 55000 });
    const body = JSON.parse(raw); expect(body.error).toBeNull(); return body.data;
  };
  const start = () => server("start", "--config", path.join(path.dirname(args.candidate), "service-open-config.json"), "--timeout-seconds", "40");
  await stage("owned-host-sigkill-accepted-seek", async () => {
    expect(state.host_fault).toBeUndefined();
    const d = await document(fixture.project_id); let accepted, release, killed = false, ready;
    const gate = new Promise(resolve => { release = resolve; });
    try {
      const playing = await capture(d, "seek", async (value, route) => {
        accepted = value; state.host_seek = { record: value.record, receipt: observe(value.receipt) }; await save();
        await gate; await route.abort("failed").catch(() => {});
      });
      await d.page.getByLabel("Playhead (seconds)", { exact: true }).fill("1.25"); await d.page.getByLabel("Playhead (seconds)", { exact: true }).press("Enter"); await expect.poll(() => !!accepted).toBe(true);
      const before = server("status"); expect(before.ready).toBe(true); expect(before.executable).toBe(candidate.binary);
      const activeJobs = Number(execFileSync("python3", ["-B", "-c", "import sqlite3,sys;c=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro',uri=True);print(c.execute(\"select count(*) from jobs where state in ('queued','running')\").fetchone()[0])", path.join(args.workspace, "workspace.sqlite")], { encoding: "utf8" })); expect(activeJobs).toBe(0);
      const owned = nativeTree(before.pid, processes()); expect(owned.some(p => p.role === "node")).toBe(true); expect(owned.length).toBeGreaterThan(1);
      const births = new Map(owned.map(p => [p.pid, p.start]));
      const cookie = (await d.context.cookies(args.endpoint)).find(c => c.name === "opencut_session").value;
      state.host_fault = { service: { pid: before.pid, instance: before.instance, executable: before.executable }, owned_processes: owned, accepted: observe(accepted.receipt), recovery: accepted.record }; await save();
      process.kill(before.pid, "SIGKILL"); killed = true; state.host_fault.sigkill_sent = true; await save(); release();
      await expect.poll(() => processes().filter(p => births.get(p.pid) === p.start).length, { timeout: 10000 }).toBe(0);
      state.host_fault.all_recorded_processes_exited = true; await save();
      ready = start(); expect(ready.ready).toBe(true); expect(ready.instance).not.toBe(before.instance); expect(ready.executable).toBe(candidate.binary);
      state.host_fault.restarted_service = { pid: ready.pid, instance: ready.instance, ready: true }; await save(); await refreshClient();
      expect((await slot(fixture.project_id)).record).toMatchObject({ recovery_id: accepted.record.recovery_id, status: "completed" });
      await expect(call("opencut_audio_preview_recovery_dispatch", { project_id: fixture.project_id, recovery_id: accepted.record.recovery_id })).rejects.toMatchObject({ code: "PREVIEW_RECOVERY_RETIRED" });
      await expect(native(playing.session_id)).rejects.toMatchObject({ code: "NOT_FOUND" });
      const oldCookie = await d.context.request.post(args.endpoint + "/api/tools/opencut_access_inspect", { data: {} }); expect(oldCookie.status()).toBe(403); expect((await oldCookie.json()).error.code).toBe("NOT_AUTHORIZED");
      state.host_fault.old_browser_cookie_rejected = true; await save();
      const restored = []; await d.context.unroute("**/api/tools/opencut_audio_preview_recovery_stage"); await d.context.unroute("**/api/tools/opencut_audio_preview_recovery_dispatch"); originalRequests(d.page, restored);
      await d.page.reload(); await expect(d.page.getByRole("region", { name: "Timeline", exact: true })).toBeVisible();
      await expect.poll(async () => (await slot(fixture.project_id)).record).toBeNull();
      expect((await d.context.cookies(args.endpoint)).find(c => c.name === "opencut_session").value).not.toBe(cookie);
      expect(restored).toEqual([{ tool: "opencut_audio_preview_recovery_ack", input: { project_id: fixture.project_id, recovery_id: accepted.record.recovery_id, disposition: "retire" } }]);
      expect(await d.page.evaluate(() => globalThis.__recoveryPump.starts)).toEqual([]); expect(graph()).toEqual(expected);
      return { old_service: before.instance, new_service: ready.instance, captured_native_processes: owned.length, all_captured_processes_exited: true, actual_sigkill: true, accepted_seek_persisted: true, old_incarnation_dispatch_refused: "PREVIEW_RECOVERY_RETIRED", old_native_session_absent: true, old_cookie_rejected_new_cookie_on_reload: true, reload_only_acknowledged_retirement: true, original_project_and_all_jobs_unchanged: true };
    } finally {
      release();
      if (killed && !ready) { ready = start(); state.host_fault.finally_restored_ready = ready.ready; await save(); await refreshClient(); }
      await d.close();
    }
  });
}
