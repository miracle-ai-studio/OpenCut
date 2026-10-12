import { expect } from "@playwright/test";

const time = (ticks, timebase = 1) => ({ ticks: String(ticks), timebase });
const equalTime = (actual, ticks, timebase = 1) => {
  expect(BigInt(actual.ticks) * BigInt(timebase)).toBe(BigInt(ticks) * BigInt(actual.timebase));
};
const prefix = "OpenCut timeline references v4\n";
const uuid = () => crypto.randomUUID();

// Reuses authorized, already imported CAS bytes. The authored clock fixture is
// independent of decoded PCM; neither this test nor reference clipboard text
// establishes cross-application clipboard, system IME or file-paste acceptance.
export async function verifyV4Clipboard(page, tool, fixed, sceneId, check) {
  const sourceScene = fixed.project.sequences.find(scene => scene.id === sceneId);
  const uploaded = sourceScene?.tracks.flatMap(track => track.elements).find(element => element.content.kind === "audio");
  expect(uploaded, "Clipboard acceptance needs the existing uploaded audio").toBeTruthy();
  const asset = fixed.project.assets.find(value => value.id === uploaded.content.asset_id);
  expect(asset?.duration, "Use the observed four-second source, without changing its metadata").toBeTruthy();
  equalTime(asset.duration, 4);
  expect(uploaded.channels).toHaveLength(0);
  expect(fixed.project.settings.profile).toBe("classic_global");

  const returnUrl = page.url(), returnScene = await page.getByLabel("V4 scene", { exact: true }).inputValue();
  const returnHead = await tool("opencut_project_v4_inspect", { project_id: fixed.project.id, revision: null });
  const projectId = "clipboard-acceptance-" + uuid(), destinationId = uuid();
  const firstId = uuid(), secondId = uuid(), firstTrackId = uuid(), secondTrackId = uuid();
  const lockedId = uuid(), mainId = uuid(), seedLabel = "Clipboard fixed authored source";
  const writes = [], copies = [], plans = [], historyRequests = [], historyFailures = [];
  let copied = false, failure, originalClipboard, historyAttempt;
  const inspect = () => tool("opencut_project_v4_inspect", { project_id: projectId, revision: null });
  const findScene = (snapshot, id) => snapshot.project.sequences.find(scene => scene.id === id);
  const findElement = (snapshot, id, scene = destinationId) => findScene(snapshot, scene).tracks.flatMap(track => track.elements).find(element => element.id === id);
  const toolbar = page.locator('[aria-label="Timeline editing"]');
  const clipboard = page.locator('[aria-label="Timeline clipboard"]');
  const review = () => page.getByRole("dialog", { name: "Review paste", exact: true });
  const modifier = await page.evaluate(() => /Mac/.test(navigator.platform)) ? "Meta" : "Control";
  const historyName = name => /^opencut_edit_v4_(undo|redo)_(plan|apply)$/.test(name);
  const observe = request => {
    const name = request.url().split("/").at(-1);
    if (!["opencut_edit_v4_apply", "opencut_timeline_v4_copy", "opencut_timeline_v4_paste_plan"].includes(name) && !historyName(name)) return;
    const body = request.postDataJSON();
    if (body?.project_id !== projectId) return;
    if (historyName(name)) {
      historyRequests.push({ name, input: body });
      if (historyRequests.length > 16) historyRequests.shift();
      return;
    }
    (name === "opencut_edit_v4_apply" ? writes : name === "opencut_timeline_v4_copy" ? copies : plans).push(body);
  };
  const observeFailure = request => {
    const name = request.url().split("/").at(-1);
    if (!historyName(name) || request.postDataJSON()?.project_id !== projectId) return;
    historyFailures.push({ name, input: request.postDataJSON(), error: request.failure()?.errorText });
    if (historyFailures.length > 8) historyFailures.shift();
  };
  const responseFor = name => page.waitForResponse(response => response.url().endsWith(`/v1/tools/${name}`) &&
    response.request().postDataJSON()?.project_id === projectId);
  const envelope = async response => { const result = await response.json(); expect(result.error).toBeNull(); return result.data; };
  const ready = async id => {
    await expect(page.getByLabel("V4 scene", { exact: true })).toHaveValue(id);
    await expect(page.getByRole("button", { name: "Return to current", exact: true })).toBeHidden();
    await expect(toolbar.getByRole("button", { name: "Add track", exact: true })).toBeEnabled();
  };
  const select = async ids => {
    for (let index = 0; index < ids.length; index++) {
      // Pointer selection precedes the historical-view gesture prohibition.
      // Use the real card interior, away from trim edges and the lower gain line;
      // modified Enter is not a portable browser button-activation gesture.
      const target = page.locator(`.v4-timeline-element[data-element-id="${ids[index]}"] .v4-element-select`);
      await target.scrollIntoViewIfNeeded();
      const hit = await target.evaluate(element => {
        const box = element.getBoundingClientRect();
        // Intersect every actual clipping ancestor and the viewport. Reserve
        // more than the physical 6 px trim handle at each authored card edge.
        let left = Math.max(0, box.left + 7), right = Math.min(innerWidth, box.right - 7);
        let top = Math.max(0, box.top + 2), bottom = Math.min(innerHeight, box.bottom - 2);
        for (let ancestor = element.parentElement; ancestor; ancestor = ancestor.parentElement) {
          const style = getComputedStyle(ancestor), bounds = ancestor.getBoundingClientRect();
          if (/^(auto|scroll|hidden|clip)$/.test(style.overflowX)) {
            left = Math.max(left, bounds.left + ancestor.clientLeft);
            right = Math.min(right, bounds.left + ancestor.clientLeft + ancestor.clientWidth);
          }
          if (/^(auto|scroll|hidden|clip)$/.test(style.overflowY)) {
            top = Math.max(top, bounds.top + ancestor.clientTop);
            bottom = Math.min(bottom, bounds.top + ancestor.clientTop + ancestor.clientHeight);
          }
        }
        const label = element.closest(".v4-track")?.querySelector(":scope > .v4-track-name");
        if (label) {
          const bounds = label.getBoundingClientRect();
          if (bounds.bottom > top && bounds.top < bottom) left = Math.max(left, bounds.right + 1);
        }
        const x = (left + right) / 2, y = (top + bottom) / 2;
        const position = { x: x - box.left - element.clientLeft, y: y - box.top - element.clientTop };
        const atPoint = document.elementFromPoint(x, y);
        return { position, visibleWidth: right - left, visibleHeight: bottom - top,
          contains: atPoint !== null && element.contains(atPoint) };
      });
      expect(hit.visibleWidth, `Clipboard source ${ids[index]} has no visible interior outside trim/sticky regions`).toBeGreaterThan(0);
      expect(hit.visibleHeight, `Clipboard source ${ids[index]} has no visible button interior`).toBeGreaterThan(0);
      expect(hit.contains, `Clipboard source ${ids[index]} interior is occluded`).toBe(true);
      await target.click({ position: hit.position, modifiers: index ? [modifier] : [] });
      await expect(target).toHaveAttribute("aria-pressed", "true");
    }
    await expect(toolbar).toContainText(`${ids.length} selected`);
  };
  const seek = async seconds => {
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill(seconds);
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
  };
  const plan = async action => {
    const before = await inspect(), count = writes.length;
    const [response] = await Promise.all([responseFor("opencut_timeline_v4_paste_plan"), action()]);
    const data = await envelope(response), request = response.request().postDataJSON();
    expect(request).toMatchObject({ project_id: projectId, expected_revision: before.revision, sequence_id: destinationId });
    expect(data.transaction).toMatchObject({ project_id: projectId, expected_revision: before.revision, idempotency_key: request.idempotency_key, label: request.label });
    await expect(review().getByRole("status")).toHaveText("Review copied elements before saving.");
    expect((await inspect()).revision).toBe(before.revision); expect(writes).toHaveLength(count);
    return { before, data, request };
  };
  const apply = async planned => {
    const historyBefore = await tool("opencut_project_history", { project_id: projectId, limit: 50 });
    const count = writes.length;
    const [response] = await Promise.all([responseFor("opencut_edit_v4_apply"), review().getByRole("button", { name: "Apply copied elements", exact: true }).click()]);
    const receipt = await envelope(response);
    expect(response.request().postDataJSON()).toEqual(planned.data.transaction);
    expect(writes.slice(count)).toEqual([planned.data.transaction]);
    expect(receipt.base_revision).toBe(planned.before.revision);
    await expect(review().getByRole("status")).toHaveText("Pasted elements saved.");
    await review().getByRole("button", { name: "Done", exact: true }).click(); await expect(review()).toBeHidden();
    await ready(destinationId);
    const snapshot = await inspect(); expect(snapshot.revision).toBe(receipt.revision);
    const historyAfter = await tool("opencut_project_history", { project_id: projectId, limit: 50 });
    expect(historyAfter.revisions).toHaveLength(historyBefore.revisions.length + 1);
    return { snapshot, receipt };
  };
  const assertClone = (actual, source, ticks, denominator = 1) => {
    expect(actual.id).not.toBe(source.id); equalTime(actual.timeline_range.start, ticks, denominator);
    // Every authored field is compared, including raw source/retime timebases.
    const expected = structuredClone(source); expected.id = actual.id; expected.timeline_range.start = actual.timeline_range.start;
    expect(actual.channels).toHaveLength(source.channels.length);
    const sourceIds = new Set([source.id, ...source.channels.flatMap(channel => [channel.id, ...channel.keys.flatMap(key => [key.id, ...(key.segment_id ? [key.segment_id] : [])])])]);
    const newIds = [actual.id];
    expected.channels.forEach((channel, index) => {
      const rebound = actual.channels[index]; expect(sourceIds.has(rebound.id)).toBe(false); newIds.push(rebound.id);
      channel.id = rebound.id;
      expect(rebound.target).toEqual({ kind: "element", element_id: actual.id }); channel.target = rebound.target;
      channel.keys.forEach((key, keyIndex) => {
        const reboundKey = rebound.keys[keyIndex]; expect(sourceIds.has(reboundKey.id)).toBe(false); newIds.push(reboundKey.id); key.id = reboundKey.id;
        if (key.segment_id) { expect(reboundKey.segment_id).toBeTruthy(); expect(sourceIds.has(reboundKey.segment_id)).toBe(false); newIds.push(reboundKey.segment_id); key.segment_id = reboundKey.segment_id; }
      });
    });
    expect(new Set(newIds).size).toBe(newIds.length); expect(actual).toEqual(expected);
    equalTime(actual.timeline_range.duration, 4, 3); equalTime(actual.source_span.trim_start, 1); equalTime(actual.source_span.trim_end, 1);
    equalTime(actual.source_span.source_duration, 4); expect(actual.content.retime.rate).toEqual({ numerator: 3, denominator: 2 });
  };
  const historyAction = async action => {
    const area = page.locator(".v4-history"), lower = action.toLowerCase(), before = await inspect();
    historyAttempt = { action, head: before.revision };
    const navigation = await tool("opencut_project_v4_history_navigation", { project_id: projectId, revision: before.revision });
    historyAttempt.navigation = navigation;
    expect(navigation.revision).toBe(before.revision);
    expect(navigation[lower], `${action} must have a target at the captured head`).toBeTruthy();
    await expect(area.getByRole("button", { name: /Apply (undo|redo)/ })).toBeHidden();
    await expect(area.getByRole("button", { name: action, exact: true })).toBeEnabled();
    const [planResponse] = await Promise.all([responseFor(`opencut_edit_v4_${lower}_plan`), area.getByRole("button", { name: action, exact: true }).click()]);
    const input = planResponse.request().postDataJSON(), planned = await planResponse.json();
    historyAttempt.plan_response = { status: planResponse.status(), input, envelope: planned };
    expect(planned.error).toBeNull();
    expect(input).toMatchObject({ project_id: projectId, expected_revision: before.revision });
    expect(input.idempotency_key).toBeTruthy();
    if (lower === "undo") expect(input.target_transaction_id).toBe(navigation.undo.transaction_id);
    else expect(input.undo_transaction_id).toBe(navigation.redo.undo_transaction_id);
    expect(planned.data).toMatchObject({ direction: lower, expected_revision: before.revision, target_transaction_id: navigation[lower].transaction_id });
    if (lower === "redo") expect(planned.data.undo_transaction_id).toBe(input.undo_transaction_id);
    const proposal = area.getByRole("region", { name: "History plan", exact: true });
    await expect(proposal).toContainText(`${action} plan`);
    expect(planned.data.conflicts, `${action} plan conflicts at ${before.revision}`).toEqual([]);
    expect(planned.data.changes.length).toBeGreaterThan(0);
    expect((await inspect()).revision).toBe(before.revision);
    const applyButton = proposal.getByRole("button", { name: `Apply ${lower}`, exact: true });
    // Wait for this plan's pending state to clear before watching submission;
    // a disabled conflict proposal must not become a misleading response timeout.
    await expect(applyButton).toBeEnabled();
    const count = historyRequests.length;
    const applyResponsePromise = responseFor(`opencut_edit_v4_${lower}_apply`);
    const committedPromise = applyResponsePromise.then(async response => {
      const committed = await response.json();
      historyAttempt.apply_response = { status: response.status(), input: response.request().postDataJSON(), envelope: committed };
      expect(committed.error).toBeNull();
      expect(response.request().postDataJSON()).toEqual(input);
      expect(committed.data).toMatchObject({ direction: lower, target_transaction_id: planned.data.target_transaction_id,
        transaction: { base_revision: before.revision } });
      return committed;
    });
    // Register before clicking so fast refreshes cannot be missed, but accept
    // only the navigation request bound to this exact apply receipt revision.
    const refreshedPromise = page.waitForResponse(async response => {
      if (!response.url().endsWith("/v1/tools/opencut_project_v4_history_navigation") ||
          response.request().postDataJSON()?.project_id !== projectId) return false;
      const committed = await committedPromise;
      return response.request().postDataJSON().revision === committed.data.transaction.revision;
    });
    const [committed, refreshed] = await Promise.all([
      committedPromise,
      refreshedPromise,
      applyButton.click(),
    ]);
    expect(historyRequests.slice(count)).toEqual([{ name: `opencut_edit_v4_${lower}_apply`, input }]);
    const refreshedNavigation = await envelope(refreshed);
    expect(refreshedNavigation.revision).toBe(committed.data.transaction.revision);
    expect(refreshed.request().postDataJSON().revision).toBe(committed.data.transaction.revision);
    await expect(applyButton).toBeHidden();
    await expect(area.getByRole("button", { name: action === "Undo" ? "Redo" : "Undo", exact: true })).toBeEnabled();
    expect((await inspect()).revision).toBe(committed.data.transaction.revision);
  };

  page.on("request", observe);
  page.on("requestfailed", observeFailure);
  try {
    // Permission is explicit and limited to this browser origin; no external app
    // is queried. Restore the prior text when the browser exposes it.
    await page.context().grantPermissions(["clipboard-read", "clipboard-write"], { origin: new URL(returnUrl).origin });
    originalClipboard = await page.evaluate(() => navigator.clipboard.readText());
    const copy = await tool("opencut_project_v4_copy", { source_project_id: fixed.project.id, source_revision: fixed.revision,
      project_id: projectId, name: "Fixed-source clipboard browser acceptance", idempotency_key: uuid() }); copied = true;
    const first = structuredClone(uploaded); first.id = firstId; first.name = "Clipboard historical primary"; first.enabled = true;
    first.timeline_range = { start: time(1, 3), duration: time(160000, 120000) };
    first.source_span = { profile: "classic_trimmed", trim_start: time(1), trim_end: time(1), source_duration: time(4) };
    first.content.retime.rate = { numerator: 3, denominator: 2 }; first.content.audio.gain = { unit: "decibels", value: -6 };
    first.channels = [{ kind: "scalar", id: uuid(), target: { kind: "element", element_id: firstId },
      binding: { kind: "property", parameter: "gain_decibels" }, extrapolation: { before: "hold", after: "linear" }, keys: [
        { id: uuid(), segment_id: uuid(), time: time(0), value: -6, left_handle: null, right_handle: { dt: time(1, 3), dv: -2 }, segment_to_next: { kind: "bezier" }, tangent_mode: "broken" },
        { id: uuid(), time: time(1), value: -12, left_handle: { dt: time(-1, 3), dv: 2 }, right_handle: null, segment_to_next: { kind: "linear" }, tangent_mode: "broken" },
      ] }];
    const second = structuredClone(first); second.id = secondId; second.name = "Clipboard historical reference";
    second.timeline_range.start = time(5, 6); second.channels = [];
    const makeTrack = (id, name, kind, elements = [], locked = false) => ({ id, name, kind, enabled: true, muted: false, locked,
      opacity: 1, gain: { unit: "decibels", value: 0 }, channels: [], elements });
    const locked = makeTrack(lockedId, "Clipboard locked destination", "audio", [], true);
    const destination = { id: destinationId, name: "Clipboard destination scene", main_track_id: mainId,
      tracks: [makeTrack(mainId, "Clipboard destination main", "video"), locked], captions: [], transitions: [], bookmarks: [] };
    const operations = [];
    for (const track of copy.project.sequences.find(scene => scene.id === sceneId).tracks) {
      if (track.locked) operations.push({ type: "track_lock_set", sequence_id: sceneId, track_id: track.id, locked: false });
      operations.push({ type: "track_put", sequence_id: sceneId, track: { ...structuredClone(track), locked: false, enabled: false } });
    }
    operations.push({ type: "track_put", sequence_id: sceneId, track: makeTrack(firstTrackId, "Clipboard source primary track", "audio", [first]) },
      { type: "track_put", sequence_id: sceneId, track: makeTrack(secondTrackId, "Clipboard source reference track", "audio", [second]) },
      { type: "sequence_put", sequence: destination });
    const seed = await tool("opencut_edit_v4_apply", { project_id: projectId, expected_revision: copy.revision, idempotency_key: uuid(), label: seedLabel, operations });
    const later = structuredClone(first); later.name = "Clipboard later primary"; later.content.audio.gain.value = -3;
    await tool("opencut_edit_v4_apply", { project_id: projectId, expected_revision: seed.revision, idempotency_key: uuid(), label: "Clipboard later source edit",
      operations: [{ type: "element_put", sequence_id: sceneId, track_id: firstTrackId, element: later }] });
    await page.goto(new URL(`/editor/${encodeURIComponent(projectId)}?scene=${encodeURIComponent(sceneId)}`, returnUrl).href); await ready(sceneId);
    const revisionRow = page.locator(".v4-revisions li").filter({ hasText: seedLabel });
    await revisionRow.getByRole("button", { name: "View revision", exact: true }).click();
    await expect(page.getByRole("button", { name: "Return to current", exact: true })).toBeVisible();
    await select([firstId, secondId]);
    const beforeCopy = await inspect(), copyWrites = writes.length;
    const [copyResponse] = await Promise.all([responseFor("opencut_timeline_v4_copy"), clipboard.getByRole("button", { name: "Copy elements", exact: true }).click()]);
    const references = await envelope(copyResponse);
    expect(copyResponse.request().postDataJSON()).toEqual({ project_id: projectId, revision: seed.revision, sequence_id: sceneId,
      targets: [{ track_id: firstTrackId, element_id: firstId }, { track_id: secondTrackId, element_id: secondId }] });
    expect(Object.keys(references).sort()).toEqual(["items", "schema_version", "selection_digest", "source_project_id", "source_revision", "source_sequence_id"].sort());
    expect(references.source_revision).toBe(seed.revision); expect(references.selection_digest).toMatch(/^v2:[a-f0-9]{64}$/);
    expect((await inspect()).revision).toBe(beforeCopy.revision); expect(writes).toHaveLength(copyWrites);
    await expect(clipboard.getByRole("button", { name: "Paste copied elements", exact: true })).toBeDisabled();
    await expect(clipboard.getByRole("button", { name: "Duplicate elements", exact: true })).toBeDisabled();
    await page.getByRole("button", { name: "Return to current", exact: true }).click(); await ready(sceneId);
    await page.getByLabel("V4 scene", { exact: true }).selectOption(destinationId); await ready(destinationId); await seek("0");
    const discarded = await plan(() => clipboard.getByRole("button", { name: "Paste copied elements", exact: true }).click());
    expect(discarded.request.clipboard).toEqual(references);
    await review().getByRole("button", { name: "Discard paste", exact: true }).click(); await expect(review()).toBeHidden();
    expect((await inspect()).revision).toBe(discarded.before.revision); expect(writes).toHaveLength(copyWrites);
    const pastedPlan = await plan(() => clipboard.getByRole("button", { name: "Paste copied elements", exact: true }).click());
    expect(pastedPlan.request.clipboard).toEqual(references); equalTime(pastedPlan.request.at, 0);
    expect(pastedPlan.data.decision.new_track_ids).toHaveLength(2);
    const pasted = await apply(pastedPlan);
    const newIdFor = sourceId => pastedPlan.request.placements.flatMap(group => group.elements).find(item => item.source_element_id === sourceId).new_element_id;
    const pastedFirstId = newIdFor(firstId), pastedSecondId = newIdFor(secondId);
    const pastedFirst = findElement(pasted.snapshot, pastedFirstId), pastedSecond = findElement(pasted.snapshot, pastedSecondId);
    assertClone(pastedFirst, first, 0); assertClone(pastedSecond, second, 1, 2);
    expect(findElement(pasted.snapshot, firstId, sceneId)).toEqual(later);
    expect(findScene(pasted.snapshot, sceneId)).toEqual(findScene(pastedPlan.before, sceneId));
    expect(findScene(pasted.snapshot, destinationId).tracks.find(track => track.id === lockedId)).toEqual(locked);
    const destinationTracks = findScene(pasted.snapshot, destinationId).tracks;
    expect(destinationTracks.indexOf(destinationTracks.find(track => track.id === mainId))).toBeLessThan(destinationTracks.indexOf(destinationTracks.find(track => track.id === lockedId)));
    for (const id of pastedPlan.data.decision.new_track_ids) expect(destinationTracks.findIndex(track => track.id === id)).toBeLessThan(destinationTracks.findIndex(track => track.id === lockedId));
    expect(pasted.snapshot.project.settings).toEqual(copy.project.settings); expect(pasted.snapshot.project.assets).toEqual(copy.project.assets);
    await historyAction("Undo"); expect((await inspect()).project).toEqual(pastedPlan.before.project);
    await historyAction("Redo"); expect((await inspect()).project).toEqual(pasted.snapshot.project);
    // The scene selector changes session state, while an explicit URL scene
    // still determines a reopened editor. Reopen the destination deliberately;
    // this checks persisted project bytes, not active-scene preference storage.
    const destinationUrl = new URL(`/editor/${encodeURIComponent(projectId)}`, returnUrl);
    destinationUrl.searchParams.set("scene", destinationId);
    await page.goto(destinationUrl.href); await ready(destinationId);
    expect(new URL(page.url()).searchParams.get("scene")).toBe(destinationId);
    expect((await inspect()).project).toEqual(pasted.snapshot.project);
    check("clipboard UI fixed historical Read, cross-scene same-project paste, readonly discard and one exact public commit; 3:2 retime/nonzero trims/half-second group spacing and stable animation rebind survive locked-track insertion, reopen and selective Undo/Redo", {
      clipboard: { source_revision: seed.revision, selection_digest: references.selection_digest, destination_scene_id: destinationId, pasted_ids: [pastedFirstId, pastedSecondId], transaction_id: pasted.receipt.transaction_id } });

    await select([pastedFirstId]);
    const duplicatedPlan = await plan(() => clipboard.getByRole("button", { name: "Duplicate elements", exact: true }).click());
    expect(duplicatedPlan.request.label).toBe("Duplicate timeline elements"); equalTime(duplicatedPlan.request.at, 0);
    expect(duplicatedPlan.request.clipboard.source_revision).toBe(duplicatedPlan.before.revision);
    const duplicated = await apply(duplicatedPlan), duplicateId = duplicatedPlan.data.decision.selection[0].element_id;
    expect(duplicatedPlan.data.decision.selection).toHaveLength(1); assertClone(findElement(duplicated.snapshot, duplicateId), pastedFirst, 0);
    expect(findElement(duplicated.snapshot, pastedFirstId)).toEqual(pastedFirst); expect(findElement(duplicated.snapshot, pastedSecondId)).toEqual(pastedSecond);
    await historyAction("Undo"); expect((await inspect()).project).toEqual(duplicatedPlan.before.project);
    await historyAction("Redo"); expect((await inspect()).project).toEqual(duplicated.snapshot.project);
    check("formal Duplicate copies the fixed authored element with fresh channel/key/segment identities, leaves its source unchanged, commits the reviewed transaction once and Undo/Redo restores the complete graph", {
      clipboard_duplicate: { source_element_id: pastedFirstId, duplicate_element_id: duplicateId, transaction_id: duplicated.receipt.transaction_id } });

    await select([duplicateId]); await page.locator(".v4-timeline-panel").focus();
    const [nativeCopyResponse] = await Promise.all([responseFor("opencut_timeline_v4_copy"), page.keyboard.press(`${modifier}+c`)]);
    const nativeReferences = await envelope(nativeCopyResponse);
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe(prefix + JSON.stringify(nativeReferences));
    await seek("4"); await page.locator(".v4-timeline-panel").focus();
    const nativePlan = await plan(() => page.keyboard.press(`${modifier}+v`));
    expect(nativePlan.request.clipboard).toEqual(nativeReferences); equalTime(nativePlan.request.at, 4);
    const nativePasted = await apply(nativePlan), nativeId = nativePlan.data.decision.selection[0].element_id;
    assertClone(findElement(nativePasted.snapshot, nativeId), findElement(duplicated.snapshot, duplicateId), 4);
    const copyCount = copies.length, planCount = plans.length, writeCount = writes.length, input = page.getByLabel("Playhead (seconds)", { exact: true });
    await input.fill("clipboard text stays in input"); await input.focus(); await page.keyboard.press(`${modifier}+a`); await page.keyboard.press(`${modifier}+c`);
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe("clipboard text stays in input");
    await input.fill(""); await page.keyboard.press(`${modifier}+v`); await expect(input).toHaveValue("clipboard text stays in input");
    // Browser-dispatched composition state verifies the explicit guard only;
    // this is not an assertion about a configured operating-system IME.
    const panel = page.locator(".v4-timeline-panel"); await panel.focus();
    await panel.dispatchEvent("compositionstart", { data: "中" });
    try { await page.keyboard.press(`${modifier}+c`); }
    finally { await panel.dispatchEvent("compositionend", { data: "中" }); }
    await input.fill("4");
    expect(copies).toHaveLength(copyCount); expect(plans).toHaveLength(planCount); expect(writes).toHaveLength(writeCount);
    expect((await inspect()).revision).toBe(nativePasted.snapshot.revision);
    check("explicit origin clipboard permissions: actual Ctrl/Cmd+C writes reference-only text and native Ctrl/Cmd+V plans one reviewed exact commit; focused input copy/paste and browser composition-state guard do not invoke timeline tools", {
      clipboard_browser: { native_paste_transaction_id: nativePasted.receipt.transaction_id, source_element_id: duplicateId, pasted_element_id: nativeId,
        file_paste: "not exercised", cross_application_clipboard: "not exercised", operating_system_ime: "not exercised" } });
  } catch (error) {
    failure = error;
    if (error instanceof Error) {
      try { error.message += `\nClipboard UI statuses: ${JSON.stringify(await clipboard.locator('[role="status"], [role="alert"]').allTextContents())}`; }
      catch { /* Preserve the actual failure if the page is unavailable. */ }
      try {
        error.message += `\nClipboard history diagnostic: ${JSON.stringify({ attempt: historyAttempt, requests: historyRequests, failed_requests: historyFailures,
          history_text: (await page.locator(".v4-history").innerText()).slice(0, 4096),
          statuses: await page.locator('.v4-history [role="status"], .v4-history [role="alert"]').allTextContents(),
          alerts: (await page.getByRole("alert").allTextContents()).slice(0, 8),
          buttons: await page.locator(".v4-history button").evaluateAll(buttons => buttons.map(button => ({ name: button.textContent, disabled: button.disabled }))) })}`;
      } catch { /* Diagnostics cannot replace the original failure. */ }
    }
    throw error;
  } finally {
    page.off("request", observe);
    page.off("requestfailed", observeFailure);
    const errors = [];
    if (originalClipboard !== undefined) try { await page.evaluate(text => navigator.clipboard.writeText(text), originalClipboard); } catch (error) { errors.push(error); }
    if (copied) try { const head = await inspect(); await tool("opencut_project_delete", { idempotency_key: uuid(), projects: [{ project_id: projectId, expected_revision: head.revision }] }); } catch (error) { errors.push(error); }
    try {
      const destination = new URL(returnUrl); destination.searchParams.set("scene", returnScene); await page.goto(destination.href);
      const back = page.getByRole("button", { name: "Return to current", exact: true }); if (await back.isVisible()) await back.click();
      await ready(returnScene);
      expect((await tool("opencut_project_v4_inspect", { project_id: fixed.project.id, revision: null })).revision).toBe(returnHead.revision);
    } catch (error) { errors.push(error); }
    if (errors.length) {
      const detail = errors.map(error => String(error)).join("\n");
      if (failure instanceof Error) failure.message += `\nClipboard fixture cleanup: ${detail}`;
      else if (!failure) throw new Error(`Clipboard fixture cleanup: ${detail}`);
    }
  }
}
