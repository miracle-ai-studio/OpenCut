import { expect } from "@playwright/test";

// Actual product gestures and Worklet consumption only. This checks control /
// revision binding; it does not claim independent post-master acoustic parity.
export async function verifyV4AudioPreviewGesture(page, tool, fixed, sceneId, check) {
  const scene = fixed.project.sequences.find(s => s.id === sceneId);
  const sourceTrack = scene?.tracks.find(t => t.elements.some(e => e.content.kind === "audio"));
  const source = sourceTrack?.elements.find(e => e.content.kind === "audio");
  expect(source, "The caller must provide a real imported audio source").toBeTruthy();
  expect(source.source_span.profile).toBe("classic_trimmed");
  // The shared real-media fixture supplies exactly two trimmed source seconds.
  // Extend only timeline placement by rate 1/10, never the resource read window.
  const duration = source.timeline_range.duration, rate = source.content.retime.rate;
  expect(BigInt(duration.ticks) * BigInt(rate.numerator)).toBe(
    2n * BigInt(duration.timebase) * BigInt(rate.denominator));
  const projectId = "preview-gesture-" + crypto.randomUUID();
  const copied = await tool("opencut_project_v4_copy", {
    source_project_id: fixed.project.id, source_revision: fixed.revision,
    project_id: projectId, name: "Scene sound gesture acceptance", idempotency_key: crypto.randomUUID(),
  });
  const first = structuredClone(source);
  first.name = "Gesture voice";
  first.enabled = true;
  first.channels = [];
  first.timeline_range = { start: { ticks: "0", timebase: 1 }, duration: { ticks: "20", timebase: 1 } };
  first.content.retime = { ...first.content.retime,
    rate: { numerator: 1, denominator: 10 }, maintain_pitch: false };
  first.content.audio = { ...first.content.audio, gain: { unit: "decibels", value: -6 }, muted: false };
  const second = structuredClone(first);
  second.id = crypto.randomUUID(); second.name = "Reference voice";
  second.content.audio.gain.value = -18;
  const secondTrack = { ...structuredClone(sourceTrack), id: crypto.randomUUID(), name: "Reference voice track",
    kind: "audio", channels: [], enabled: true, muted: false, locked: false,
    gain: { unit: "decibels", value: 0 }, elements: [second] };
  const operations = scene.tracks.map(track => ({ type: "track_put", sequence_id: sceneId,
    track: track.id === sourceTrack.id
      ? { ...structuredClone(track), channels: [], enabled: true, muted: false, locked: false,
          gain: { unit: "decibels", value: 0 }, elements: [first] }
      : { ...structuredClone(track), enabled: false } }));
  operations.push({ type: "track_put", sequence_id: sceneId, track: secondTrack });
  const inspect = () => tool("opencut_project_v4_inspect", { project_id: projectId, revision: null });
  const controls = [], controlRequests = [], writes = [], plans = [], creates = [];
  const witnessSlot = "__opencutGestureWitness_" + crypto.randomUUID().replaceAll("-", "");
  const completions = new WeakMap(), envelopes = new WeakMap(), failures = new WeakMap();
  const observation = { cdp_body_unavailable_completed: 0, failed_requests: 0, control_failures: [], witness_limit: 128 };
  const metadata = request => {
    const body = request.postDataJSON(), kind = body.command?.kind;
    return kind ? { key: "control:" + body.control_id, kind, session_id: body.session_id, generation: body.generation,
      update_sequence: body.command.gain?.update_sequence ?? null }
      : { key: "create:" + body.idempotency_key, kind: "create", project_id: body.project_id,
          revision: body.revision, sequence_id: body.sequence_id };
  };
  const recordFailure = (request, detail) => {
    if (failures.has(request)) { if (detail.status != null) failures.get(request).status = detail.status; return; }
    const value = { ...metadata(request), status: detail.status ?? null, completion: detail.completion,
      failure: request.failure() ? (/^net::[A-Z0-9_]+$/.test(request.failure().errorText) ? request.failure().errorText : "request_failed") : null };
    failures.set(request, value); observation.failed_requests++;
    if (observation.control_failures.length < 8) observation.control_failures.push(value);
  };
  const observeFinished = request => { completions.set(request, "finished"); };
  const observeFailed = request => {
    completions.set(request, "failed");
    if (["/v1/tools/opencut_audio_preview_v4_control", "/v1/tools/opencut_audio_preview_v4_create"].some(path => request.url().endsWith(path)))
      recordFailure(request, { status: null, completion: "failed" });
  };
  const readEnvelope = response => {
    if (envelopes.has(response)) return envelopes.get(response);
    const result = (async () => {
      const request = response.request(), finished = await response.finished();
      if (finished || request.failure() || completions.get(request) === "failed") {
        recordFailure(request, { status: response.status(), completion: "failed" });
        throw new Error("Original audio control request failed before its body completed");
      }
      try { return await response.json(); }
      catch (error) {
        // Only this exact Chromium body-cache failure may use the application's
        // already-parsed original body. A failed request is never accepted.
        if (!/Network\.getResponseBody[\s\S]*No data found for resource with given identifier/.test(String(error))) throw error;
        const expected = metadata(request);
        let witnessed;
        await expect.poll(async () => {
          witnessed = await page.evaluate(({ slot, key }) => {
            const state = globalThis[slot];
            if (!state || state.overflow) throw new Error("Audio response witness unavailable or over budget");
            return state.records.get(key) ?? null;
          }, { slot: witnessSlot, key: expected.key });
          if (witnessed?.stage === "rejected") throw new Error("Application rejected the original audio response body");
          return witnessed?.stage === "fulfilled";
        }, { timeout: 3000 }).toBe(true);
        expect(witnessed.metadata).toEqual(expected);
        expect(witnessed.status).toBe(response.status());
        expect(response.status()).toBe(200);
        expect(request.failure()).toBeNull();
        expect(completions.get(request)).not.toBe("failed");
        observation.cdp_body_unavailable_completed++;
        return witnessed.envelope;
      }
    })();
    envelopes.set(response, result); return result;
  };
  const observeRequest = request => {
    if (!request.url().includes("/v1/tools/")) return;
    const body = request.postDataJSON();
    if (request.url().endsWith("/opencut_audio_preview_v4_control")) controlRequests.push(body);
    if (body?.project_id !== projectId) return;
    if (request.url().endsWith("/opencut_edit_v4_apply")) writes.push(body);
    if (request.url().endsWith("/opencut_project_v4_audio_gain_plan")) plans.push(body);
    if (request.url().endsWith("/opencut_audio_preview_v4_create")) creates.push(body);
  };
  const observeResponse = response => {
    if (!response.url().endsWith("/v1/tools/opencut_audio_preview_v4_control")) return;
    const request = response.request().postDataJSON();
    controls.push({ request, envelope: readEnvelope(response).catch(error => ({ error: { message: String(error) } })) });
    if (controls.length > 128) controls.shift();
  };
  const matchingControl = (sessionId, kind, predicate = () => true) => page.waitForResponse(response => {
    if (!response.url().endsWith("/v1/tools/opencut_audio_preview_v4_control")) return false;
    const request = response.request().postDataJSON();
    return request.session_id === sessionId && request.command?.kind === kind && predicate(request);
  });
  const consume = async session => {
    await expect.poll(async () => {
      for (const event of controls) {
        if (event.request.session_id !== session.session_id || event.request.generation !== session.generation ||
            event.request.command?.kind !== "report_consumed") continue;
        const result = await event.envelope;
        if (!result.error && result.data?.generation === session.generation &&
            result.data.overlay_epoch === session.overlay_epoch && BigInt(result.data.consumed_frames) > 0n) return true;
      }
      return false;
    }, { timeout: 30000 }).toBe(true);
  };
  const accepted = async response => {
    const envelope = await readEnvelope(response); expect(envelope.error).toBeNull();
    expect(envelope.data.identity).toMatchObject({ project_id: projectId, sequence_id: sceneId });
    return envelope.data;
  };
  let originalHead, committed, oldSession, freshSession, firstPointerGeometry;
  try {
    originalHead = await tool("opencut_edit_v4_apply", { project_id: projectId, expected_revision: copied.revision,
      idempotency_key: crypto.randomUUID(), label: "Two real-source voices for gesture acceptance", operations });
    const fixture = await inspect(), fixtureScene = fixture.project.sequences.find(s => s.id === sceneId);
    expect(fixtureScene.tracks.filter(t => t.enabled).flatMap(t => t.elements)).toEqual([first, second]);
    expect(second.content.asset_id).toBe(first.content.asset_id);
    const pcmProof = await verifyTwoVoicePcm(page, tool, fixture, sceneId, sourceTrack.id, first.id);
    await page.goto(new URL(`/editor/${encodeURIComponent(projectId)}?scene=${encodeURIComponent(sceneId)}`, page.url()).href);
    const slider = page.getByRole("slider", { name: "Volume Gesture voice", exact: true });
    const reference = page.getByRole("slider", { name: "Volume Reference voice", exact: true });
    const surface = page.locator(".v4-timeline-element").filter({ has: slider }).locator(".v4-gain-surface");
    await expect(slider).toHaveAttribute("aria-valuenow", "-6");
    await expect(reference).toHaveAttribute("aria-valuenow", "-18");
    await expect(slider).toHaveAttribute("aria-disabled", "false");
    await expect(page.getByRole("button", { name: "Play scene audio", exact: true })).toBeEnabled();
    // Test observation only: preserve the production fetch call and its one
    // actual body read (SDK uses text→JSON.parse). No clone, second body read
    // or additional control request. The observer never changes returned bytes.
    await page.evaluate(({ slot }) => {
      const nativeFetch = globalThis.fetch, records = new Map();
      const state = { nativeFetch, records, overflow: false, wrapped: null };
      const put = (key, value) => {
        if (!records.has(key) && records.size >= 128) {
          const old = [...records].find(([, record]) => record.stage !== "pending");
          if (!old) { state.overflow = true; return; }
          records.delete(old[0]);
        }
        records.set(key, value);
      };
      const wrapped = async function(input, init) {
        const method = init?.method ?? (input instanceof Request ? input.method : "GET");
        const url = new URL(input instanceof Request ? input.url : String(input), location.href);
        const eligible = method === "POST" && url.origin === location.origin &&
          ["/v1/tools/opencut_audio_preview_v4_control", "/v1/tools/opencut_audio_preview_v4_create"].includes(url.pathname);
        let metadata;
        if (eligible && typeof init?.body === "string") {
          try {
            const body = JSON.parse(init.body), kind = body.command?.kind;
            metadata = kind ? { key: "control:" + body.control_id, kind, session_id: body.session_id, generation: body.generation,
              update_sequence: body.command.gain?.update_sequence ?? null }
              : { key: "create:" + body.idempotency_key, kind: "create", project_id: body.project_id,
                  revision: body.revision, sequence_id: body.sequence_id };
          } catch { /* An unobserved/invalid request can never supply a witness. */ }
        }
        if (metadata) {
          // A reused correlation key cannot prove which network request supplied
          // the parsed body. Observe it, but fail closed if fallback is needed.
          if (records.has(metadata.key)) state.overflow = true;
          else put(metadata.key, { stage: "pending", metadata, status: null });
        }
        let response;
        try { response = await Reflect.apply(nativeFetch, this, [input, init]); }
        catch (error) { if (metadata) put(metadata.key, { stage: "rejected", metadata, status: null }); throw error; }
        if (!metadata) return response;
        const observeEnvelope = envelope => {
          if (new TextEncoder().encode(JSON.stringify(envelope)).length > 16384) state.overflow = true;
          else put(metadata.key, { stage: "fulfilled", metadata, status: response.status, envelope });
        };
        const nativeText = response.text, nativeJson = response.json;
        response.text = async function(...args) {
          let raw;
          try { raw = await Reflect.apply(nativeText, this, args); }
          catch (error) { put(metadata.key, { stage: "rejected", metadata, status: response.status }); throw error; }
          // Observe exactly the bytes consumed by SDK transport; its own JSON
          // parsing and protocol validation still run after this returns.
          try {
            if (new TextEncoder().encode(raw).length > 16384) state.overflow = true;
            else observeEnvelope(JSON.parse(raw));
          } catch { put(metadata.key, { stage: "rejected", metadata, status: response.status }); }
          return raw;
        };
        response.json = async function(...args) {
          let envelope;
          try { envelope = await Reflect.apply(nativeJson, this, args); }
          catch (error) { put(metadata.key, { stage: "rejected", metadata, status: response.status }); throw error; }
          try { observeEnvelope(envelope); } catch { state.overflow = true; }
          return envelope;
        };
        return response;
      };
      state.wrapped = wrapped; globalThis[slot] = state; globalThis.fetch = wrapped;
    }, { slot: witnessSlot });
    page.on("requestfinished", observeFinished); page.on("requestfailed", observeFailed);
    page.on("request", observeRequest); page.on("response", observeResponse);
    const play = async revision => {
      await expect(page.getByRole("button", { name: "Play scene audio", exact: true })).toBeEnabled();
      const created = page.waitForResponse(r => r.url().endsWith("/v1/tools/opencut_audio_preview_v4_create") &&
        r.request().postDataJSON().project_id === projectId);
      const [response] = await Promise.all([created, page.getByRole("button", { name: "Play scene audio", exact: true }).click()]);
      const createdSession = await accepted(response);
      expect(createdSession.identity.revision).toBe(revision);
      // Play reanchors even a freshly Ready session. Consume belongs to the
      // accepted Play generation, never the earlier Create response generation.
      let playing;
      await expect.poll(async () => {
        for (const event of controls) {
          if (event.request.session_id !== createdSession.session_id || event.request.command?.kind !== "play") continue;
          const result = await event.envelope;
          if (!result.error && result.data?.state === "playing") { playing = result.data; return true; }
        }
        return false;
      }).toBe(true);
      expect(playing.identity).toEqual(createdSession.identity);
      await consume(playing);
      return playing;
    };
    const beginDrag = async session => {
      await expect(slider).toHaveAttribute("aria-disabled", "false");
      firstPointerGeometry ??= await slider.evaluate(element => {
        const rect = element.getBoundingClientRect();
        const hit = document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
        return { x: rect.x, y: rect.y, height: rect.height,
          center_hit_slider: hit === element || element.contains(hit) };
      });
      // Mouse coordinates do not auto-scroll a clipped timeline lane.
      await slider.scrollIntoViewIfNeeded();
      expect(await slider.evaluate(element => {
        const rect = element.getBoundingClientRect();
        const hit = document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
        return hit === element || element.contains(hit);
      }), "The actual pointer must hit the visible volume slider").toBe(true);
      const line = await slider.boundingBox(); expect(line.height).toBe(14);
      await page.mouse.move(line.x + line.width / 2, line.y + line.height / 2);
      await page.mouse.down();
      const response = matchingControl(session.session_id, "set_temporary_gain", request =>
        request.command.gain.element_id === first.id && request.command.gain.decibels_tenths === -120);
      const box = await surface.boundingBox();
      const percent = 1 - Math.sqrt((10 ** (-12 / 20) - .001) / 9.999);
      const [overlayResponse] = await Promise.all([response,
        page.mouse.move(box.x + box.width / 2, box.y + box.height * percent)]);
      const overlay = await accepted(overlayResponse);
      expect(overlay.state).toBe("playing");
      expect(overlay.temporary_gain).toMatchObject({ track_id: sourceTrack.id, element_id: first.id, decibels_tenths: -120 });
      expect(BigInt(overlay.overlay_epoch)).toBeGreaterThan(BigInt(session.overlay_epoch));
      await expect(slider).toHaveAttribute("aria-valuenow", "-12");
      await expect(reference).toHaveAttribute("aria-valuenow", "-18");
      await consume(overlay);
      expect((await inspect()).revision).toBe(originalHead.revision);
      expect(writes).toHaveLength(0); expect(plans).toHaveLength(0);
      return overlay;
    };
    oldSession = await play(originalHead.revision);
    const overlay = await beginDrag(oldSession);
    const clearedResponse = matchingControl(oldSession.session_id, "clear_temporary_gain", request =>
      request.command.gesture_id === overlay.temporary_gain.gesture_id);
    const [clear] = await Promise.all([clearedResponse, page.keyboard.press("Escape")]);
    await page.mouse.up();
    const restored = await accepted(clear);
    expect(restored.temporary_gain).toBeNull(); expect(restored.state).toBe("playing");
    await expect(slider).toHaveAttribute("aria-valuenow", "-6");
    await consume(restored);
    expect((await inspect()).revision).toBe(originalHead.revision);
    expect(writes).toHaveLength(0); expect(plans).toHaveLength(0);

    const secondOverlay = await beginDrag(restored);
    const plannedResponse = page.waitForResponse(r => r.url().endsWith("/v1/tools/opencut_project_v4_audio_gain_plan") &&
      r.request().postDataJSON().project_id === projectId);
    const appliedResponse = page.waitForResponse(r => r.url().endsWith("/v1/tools/opencut_edit_v4_apply") &&
      r.request().postDataJSON().project_id === projectId);
    const stoppedResponse = matchingControl(oldSession.session_id, "stop");
    const [planResponse, applyResponse, stopResponse] = await Promise.all([
      plannedResponse, appliedResponse, stoppedResponse, page.mouse.up(),
    ]);
    const plan = await planResponse.json(), apply = await applyResponse.json();
    expect(plan.error).toBeNull(); expect(apply.error).toBeNull();
    expect(plans).toHaveLength(1); expect(writes).toHaveLength(1);
    expect(plans[0]).toMatchObject({ expected_revision: originalHead.revision, sequence_id: sceneId,
      track_id: sourceTrack.id, element_id: first.id, command: { kind: "set_decibels", decibels_tenths: -120 } });
    expect(writes[0]).toEqual(plan.data.transaction);
    committed = apply.data;
    const stopped = await accepted(stopResponse); expect(stopped.state).toBe("stopped");
    await expect(page.locator(".v4-gain-status")).toContainText("Element audio changes saved.");
    await expect(slider).toHaveAttribute("aria-disabled", "false");
    const expected = structuredClone(fixture.project);
    expected.sequences.find(s => s.id === sceneId).tracks.find(t => t.id === sourceTrack.id).elements[0].content.audio.gain.value = -12;
    expect((await inspect()).project).toEqual(expected);
    // A head change must stop old playback; it must never silently create the
    // new revision's session. Only the following explicit user Play does so.
    expect(creates).toHaveLength(1);
    const oldControlCount = controlRequests.length;
    freshSession = await play(committed.revision);
    expect(freshSession.session_id).not.toBe(oldSession.session_id);
    expect(creates).toHaveLength(2);
    expect(controlRequests.slice(oldControlCount).filter(request => request.session_id === oldSession.session_id &&
      ["play", "report_consumed"].includes(request.command?.kind))).toHaveLength(0);
    const oldNow = await tool("opencut_audio_preview_v4_inspect", { session_id: oldSession.session_id });
    expect(oldNow.state).toBe("stopped"); expect(oldNow.consumed_frames).toBe(stopped.consumed_frames);
    const pauseResponse = matchingControl(freshSession.session_id, "pause");
    const [pausedResponse] = await Promise.all([pauseResponse,
      page.getByRole("button", { name: "Pause scene audio", exact: true }).click()]);
    const paused = await accepted(pausedResponse); expect(paused.state).toBe("paused");
    await expect(page.getByText("Scene sound paused", { exact: true })).toBeVisible();
    expect((await inspect()).revision).toBe(committed.revision);
    check("actual playing two-voice volume gesture: target-only overlay, Escape clears without edits, pointerup publishes one original transaction, old session stops and explicit Play binds the new revision", {
      audio_gesture: { project_id: projectId, source_asset_id: first.content.asset_id,
        target_element_id: first.id, reference_element_id: second.id, reference_decibels: -18,
        draft_decibels: -12, draft_generation: secondOverlay.generation, cancelled_generation: restored.generation,
        transaction_id: committed.transaction_id, old_session_id: oldSession.session_id,
        new_session_id: freshSession.session_id, final_revision: committed.revision,
        pre_master_two_voice_pcm: pcmProof,
        initial_pointer_geometry: firstPointerGeometry,
        independent_post_master_acoustics_verified: false,
        control_response_observation: observation },
    });
  } catch (error) {
    throw new Error(`${error instanceof Error ? error.message : String(error)}; audio control observation ${JSON.stringify(observation)}`, { cause: error });
  } finally {
    page.off("request", observeRequest); page.off("response", observeResponse);
    page.off("requestfinished", observeFinished); page.off("requestfailed", observeFailed);
    await page.evaluate(slot => {
      const state = globalThis[slot];
      if (state && globalThis.fetch === state.wrapped) globalThis.fetch = state.nativeFetch;
      state?.records.clear(); delete globalThis[slot];
    }, witnessSlot).catch(() => {});
    // Cancel a still-held pointer before navigating, including failure paths.
    await page.keyboard.press("Escape"); await page.mouse.up();
    await page.goto(new URL(`/editor/${encodeURIComponent(fixed.project.id)}`, page.url()).href);
    const head = await inspect();
    await tool("opencut_project_delete", { idempotency_key: crypto.randomUUID(),
      projects: [{ project_id: projectId, expected_revision: head.revision }] });
  }
}

// This separate observer never claims a Worklet lease or fabricates consumption.
// Both voices use the same real 0.125-amplitude fixture; changing only one voice
// has a different expected sum ratio from a global/master gain adjustment.
async function verifyTwoVoicePcm(page, tool, fixed, sceneId, trackId, elementId) {
  let session = await tool("opencut_audio_preview_v4_create", {
    project_id: fixed.project.id, revision: fixed.revision, sequence_id: sceneId,
    sample_rate: 48000, start_time: { ticks: "1", timebase: 5 }, idempotency_key: crypto.randomUUID(),
  });
  const control = async command => {
    session = await tool("opencut_audio_preview_v4_control", { session_id: session.session_id,
      generation: session.generation, owner_id: session.owner_id, lease_id: session.lease_id,
      control_id: crypto.randomUUID(), command });
  };
  const block = async () => {
    await control({ kind: "play" });
    const observed = await page.evaluate(async s => {
      const response = await fetch("/v1/preview/v4/audio/stream", { method: "POST",
        headers: { "Content-Type": "application/json" }, body: JSON.stringify({
          session_id: s.session_id, generation: s.generation, overlay_epoch: s.overlay_epoch,
          owner_id: s.owner_id, lease_id: s.lease_id,
        }) });
      if (!response.ok) throw new Error(await response.text());
      if (response.headers.get("content-type") !== "application/vnd.opencut.audio-preview-v4") throw new Error("Unexpected PCM content type");
      const reader = response.body.getReader(); let bytes = new Uint8Array();
      try {
        for (;;) {
          const chunk = await reader.read(); if (chunk.done) throw new Error("Early PCM EOF");
          const next = new Uint8Array(bytes.length + chunk.value.length); next.set(bytes); next.set(chunk.value, bytes.length); bytes = next;
          if (bytes.length < 12) continue;
          const view = new DataView(bytes.buffer);
          if (String.fromCharCode(...bytes.subarray(0, 4)) !== "OCA4") throw new Error("Unexpected PCM magic");
          const h = view.getUint32(4, true), p = view.getUint32(8, true);
          if (!h || h > 4096 || !p || p > 32768) throw new Error("Unbounded PCM frame");
          if (bytes.length < 12 + h + p) continue;
          const header = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes.subarray(12, 12 + h)));
          const payload = bytes.slice(12 + h, 12 + h + p);
          const hash = [...new Uint8Array(await crypto.subtle.digest("SHA-256", payload))].map(v => v.toString(16).padStart(2, "0")).join("");
          if (header.version !== 1 || !Number.isInteger(header.frame_count) || header.frame_count < 1 ||
              header.frame_count > 4096 || p !== header.frame_count * 8 || hash !== header.content_hash) throw new Error("Invalid PCM header/integrity");
          const pcm = new DataView(payload.buffer), channels = [[], []];
          for (let i = 0; i < header.frame_count; i++) for (let c = 0; c < 2; c++) {
            const value = pcm.getFloat32(i * 8 + c * 4, true);
            if (!Number.isFinite(value)) throw new Error("Nonfinite PCM sample");
            channels[c].push(value);
          }
          return { header, channels };
        }
      } finally { await reader.cancel(); }
    }, session);
    expect(observed.header.identity).toEqual(session.identity);
    expect(observed.header).toMatchObject({ session_id: session.session_id, generation: session.generation,
      overlay_epoch: session.overlay_epoch, start_sample: "9600", sequence: "0", frame_count: 4096 });
    session = await tool("opencut_audio_preview_v4_inspect", { session_id: session.session_id });
    expect(session.consumed_frames).toBe("0"); expect(session.presentation_fresh).toBe(false);
    return observed;
  };
  try {
    const baseline = await block(), left = baseline.channels[0];
    expect(Math.max(...left.map((v, i) => Math.abs(v - baseline.channels[1][i])))).toBeLessThan(1e-7);
    const rms = Math.sqrt(left.reduce((sum, v) => sum + v * v, 0) / left.length);
    const expectedRms = .125 * (10 ** (-6 / 20) + 10 ** (-18 / 20)) / Math.sqrt(2);
    expect(rms).toBeGreaterThan(expectedRms * .85); expect(rms).toBeLessThan(expectedRms * 1.15);
    await control({ kind: "seek", sequence_time: { ticks: "1", timebase: 5 } });
    await control({ kind: "set_temporary_gain", gain: { track_id: trackId, element_id: elementId,
      decibels_tenths: -120, gesture_id: "two-voice-pcm-proof", update_sequence: "1" } });
    const lowered = await block();
    const ratio = (10 ** (-12 / 20) + 10 ** (-18 / 20)) / (10 ** (-6 / 20) + 10 ** (-18 / 20));
    const maxError = Math.max(...lowered.channels.flatMap((channel, c) => channel.map((v, i) =>
      Math.abs(v - baseline.channels[c][i] * ratio))));
    expect(maxError).toBeLessThan(1e-7);
    expect((await tool("opencut_project_v4_inspect", { project_id: fixed.project.id, revision: null })).revision).toBe(fixed.revision);
    return { baseline_rms: rms, expected_rms: expectedRms, target_only_ratio: ratio,
      max_sample_error: maxError, baseline_hash: baseline.header.content_hash, overlay_hash: lowered.header.content_hash };
  } finally { await control({ kind: "stop" }); }
}
