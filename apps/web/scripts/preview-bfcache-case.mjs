/** Actual persisted browser history, not a synthetic pagehide/pageshow event. */
export async function verifyActualBfCache(h) {
  const { args, state, stage, save, slot, native, graph, fixture, document, observe, sameAuthority, expect } = h;
  const d = await document(fixture.project_id, { bfcache: true }), exclusions = [];
  const pending = new Map();
  d.page.on("request", request => pending.set(request, { url: request.url(), kind: request.resourceType() }));
  d.page.on("requestfinished", request => pending.delete(request));
  d.page.on("requestfailed", request => pending.delete(request));
  const cdp = await d.context.newCDPSession(d.page); await cdp.send("Page.enable");
  cdp.on("Page.backForwardCacheNotUsed", event => exclusions.push(event));
  await d.page.addInitScript(() => {
    globalThis.__bfqa = { id: crypto.randomUUID(), events: [], tools: [] };
    for (const name of ["pageshow", "pagehide"]) addEventListener(name, event => globalThis.__bfqa.events.push({ name, persisted: event.persisted }));
    const original = globalThis.fetch;
    globalThis.fetch = (input, init) => {
      const url = typeof input === "string" ? input : input.url;
      if (url.includes("/opencut_audio_preview_recovery_")) globalThis.__bfqa.tools.push(url.split("/").at(-1));
      return original(input, init);
    };
  });
  const snapshot = () => d.page.evaluate(() => structuredClone(globalThis.__bfqa));
  // A browser document avoids app bootstrap/cookie mutations and media loads.
  // robots.txt is served as a downloadable asset, so it is not a navigation.
  const leave = async () => { await d.page.goto("about:blank"); };
  const back = async () => { await d.page.goBack({ waitUntil: "commit" }); await expect(d.page.getByRole("region", { name: "Timeline", exact: true })).toBeVisible(); return snapshot(); };
  let playing;
  d.page.on("response", response => { if (response.url().endsWith("/opencut_audio_preview_recovery_dispatch")) void response.json().then(body => { if (body.data?.state === "playing") playing = body.data; }).catch(() => {}); });
  try {
    await d.open(); await expect(d.page.getByRole("button", { name: "Play scene audio", exact: true })).toBeEnabled();
    await stage("actual-editor-bfcache-eligibility", async () => {
      const before = await snapshot(), pendingBeforeNavigation = [...pending.values()];
      state.bfcache_pending_before_navigation = pendingBeforeNavigation; await save();
      await leave(); const after = await back();
      state.bfcache_probe = { same_document: after.id === before.id, events: after.events, exclusions, pending_before_navigation: pendingBeforeNavigation, playwright_default_disable_argument_removed: true, no_network_routing: true }; await save();
      expect(after.id).toBe(before.id); expect(after.events.some(e => e.name === "pagehide" && e.persisted)).toBe(true); expect(after.events.some(e => e.name === "pageshow" && e.persisted)).toBe(true);
      return state.bfcache_probe;
    });
    await stage("paused-audio-bfcache-retirement-and-fresh-play", async () => {
      await d.page.getByRole("button", { name: "Play scene audio", exact: true }).click(); await expect.poll(() => !!playing).toBe(true);
      await expect.poll(() => d.page.evaluate(() => globalThis.__recoveryPump.progress)).toBe(true);
      const first = playing;
      await d.page.getByRole("button", { name: "Pause scene audio", exact: true }).click(); await expect(d.page.getByText("Scene sound paused", { exact: true })).toBeVisible();
      const paused = await native(first.session_id); expect(paused.state).toBe("paused");
      const before = await snapshot(); await leave(); let stopped;
      await expect.poll(async () => { stopped = await native(first.session_id); return stopped.state; }, { timeout: 6000 }).toBe("stopped"); sameAuthority(first, stopped);
      const after = await back(); state.bfcache_audio_return = { original: observe(first), stopped: observe(stopped), before, after, exclusions }; await save();
      expect(after.id).toBe(before.id); expect(after.events.filter(e => e.name === "pageshow" && e.persisted).length).toBeGreaterThan(before.events.filter(e => e.name === "pageshow" && e.persisted).length);
      expect(after.tools.slice(before.tools.length).filter(name => name.endsWith("_stage") || name.endsWith("_dispatch"))).toEqual([]);
      await expect.poll(async () => (await slot(fixture.project_id)).record).toBeNull();
      expect(await d.page.evaluate(() => globalThis.__recoveryPump.starts)).toEqual([first.session_id]);
      // The cached object remains useful, but only an explicit new user action
      // may admit another native session. Do not adopt the retired old lease.
      playing = null; await d.page.evaluate(() => { globalThis.__recoveryPump.progress = false; });
      await expect(d.page.getByRole("button", { name: "Play scene audio", exact: true })).toBeEnabled(); await d.page.getByRole("button", { name: "Play scene audio", exact: true }).click();
      await expect.poll(() => !!playing).toBe(true); expect(playing.session_id).not.toBe(first.session_id);
      await expect.poll(() => d.page.evaluate(() => globalThis.__recoveryPump.progress)).toBe(true);
      const fresh = playing; await d.page.getByRole("button", { name: "Pause scene audio", exact: true }).click(); await expect(d.page.getByText("Scene sound paused", { exact: true })).toBeVisible(); await leave();
      await expect.poll(async () => (await native(fresh.session_id)).state, { timeout: 6000 }).toBe("stopped"); await expect.poll(async () => (await slot(fixture.project_id)).record).toBeNull(); expect(graph()).toEqual(state.bfcache_baseline);
      return { actual_persisted_return: true, same_cached_document_after_audio: true, old_session_id: first.session_id, old_session_stopped_on_departure: true, no_implicit_stage_dispatch_or_pcm_restart: true, explicit_fresh_play_session_id: fresh.session_id, fresh_actual_consumption: true, fresh_session_stopped: true, project_and_all_jobs_unchanged: true };
    });
  } finally { await cdp.detach().catch(() => {}); await d.close(); }
}
