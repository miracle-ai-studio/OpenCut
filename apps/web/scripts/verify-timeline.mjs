import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import os from "node:os";
import path from "node:path";

const time = (ticks, timebase = 1) => ({ ticks: String(ticks), timebase });
const labelPrefix = label => new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?:\\s|$)`);
const equalTime = (actual, ticks, timebase = 1) => {
  expect(BigInt(actual.ticks) * BigInt(timebase)).toBe(BigInt(ticks) * BigInt(actual.timebase));
};

// Actual imported bytes remain bound to the copied project. This helper tests
// authored edits and durable/public receipts; it makes no new decode/PCM claim.
export async function verifyTimeline(page, tool, fixed, sceneId, check) {
  const sourceScene = fixed.project.sequences.find(scene => scene.id === sceneId);
  const source = sourceScene?.tracks.flatMap(track => track.elements).find(element => element.content.kind === "audio");
  expect(source, "Timeline acceptance requires the caller's real uploaded audio").toBeTruthy();
  expect(source.source_span.profile).toBe("classic_trimmed");
  const rate = source.content.retime.rate;
  expect(BigInt(source.timeline_range.duration.ticks) * BigInt(rate.numerator)).toBe(
    2n * BigInt(source.timeline_range.duration.timebase) * BigInt(rate.denominator));
  const sourceAsset = fixed.project.assets.find(asset => asset.id === source.content.asset_id);
  expect(sourceAsset?.duration, "The uploaded media must have an observed four-second extent").toBeTruthy();
  equalTime(sourceAsset.duration, 4);
  equalTime(source.source_span.source_duration, 4);
  expect(source.channels).toHaveLength(0);

  const returnUrl = page.url();
  const returnSceneId = await page.getByLabel("Scene", { exact: true }).inputValue();
  const returnHead = await editorSnapshot(tool("opencut_project_inspect", { project_id: fixed.project.id, revision: null }));
  const projectId = "timeline-acceptance-" + crypto.randomUUID();
  const firstId = crypto.randomUUID(), secondId = crypto.randomUUID();
  const firstTrackId = crypto.randomUUID(), secondTrackId = crypto.randomUUID();
  const firstName = "Timeline primary voice", secondName = "Timeline reference voice";
  const trackName = "Timeline primary track", referenceTrackName = "Timeline reference track";
  const routePattern = "**/api/tools/opencut_edit_apply";
  const writes = [], plans = [];
  let copied = false, failure, lostResponse, recoveryPlan, routeHandler;
  const observe = request => {
    if (request.url().endsWith("/api/tools/opencut_edit_timeline_plan")) {
      const body = request.postDataJSON();
      if (body?.project_id === projectId) plans.push(body);
    }
    if (request.url().endsWith("/api/tools/opencut_edit_apply")) {
      const body = request.postDataJSON();
      if (body?.project_id === projectId) writes.push(body);
    }
  };
  const inspect = () => editorSnapshot(tool("opencut_project_inspect", { project_id: projectId, revision: null }));
  const raw = (name, input) => page.evaluate(async ({ name, input }) => {
    const response = await fetch(`/api/tools/${name}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input) });
    return response.json();
  }, { name, input });
  const scene = snapshot => snapshot.project.sequences.find(value => value.id === sceneId);
  const element = (snapshot, id) => scene(snapshot).tracks.flatMap(track => track.elements).find(value => value.id === id);
  const clip = name => page.locator(".editor-element-select").filter({ hasText: `${name} · audio` });
  const trackButton = name => page.locator(".editor-track-name").filter({ hasText: `${name} · audio` });
  const toolbar = page.locator('[aria-label="Timeline editing"]');
  const review = () => page.getByRole("dialog", { name: "Review timeline changes", exact: true });
  const waitReturnCurrent = async () => {
    const back = page.getByRole("button", { name: "Return to current", exact: true });
    if (await back.isVisible()) await back.click();
    await expect(back).toBeHidden();
    await expect(page.getByLabel("Scene", { exact: true })).toHaveValue(sceneId);
    await expect(toolbar.getByRole("button", { name: "Add track", exact: true })).toBeEnabled();
  };
  const select = async names => {
    const modifier = await page.evaluate(() => /Mac/.test(navigator.platform)) ? "Meta" : "Control";
    for (let index = 0; index < names.length; index++) {
      const target = clip(names[index]);
      await target.scrollIntoViewIfNeeded();
      // Keep clear of the trim handles and prove this actual item is hittable.
      const hit = await target.evaluate(element => {
        const box = element.getBoundingClientRect();
        const position = { x: Math.min(12, box.width / 2), y: Math.min(12, box.height / 2) };
        const atPoint = document.elementFromPoint(
          box.left + element.clientLeft + position.x,
          box.top + element.clientTop + position.y);
        return { position, width: box.width, height: box.height,
          contains: atPoint !== null && element.contains(atPoint) };
      });
      expect(hit.width, `Timeline clip ${names[index]} has no click width`).toBeGreaterThan(0);
      expect(hit.height, `Timeline clip ${names[index]} has no click height`).toBeGreaterThan(0);
      expect(hit.contains, `Timeline clip ${names[index]} leading point is occluded`).toBe(true);
      await target.click({ position: hit.position, modifiers: index ? [modifier] : [] });
    }
    await expect(toolbar).toContainText(`${names.length} selected`);
  };
  const startForm = async (action, fill = async () => {}) => {
    await expect(toolbar.getByRole("button", { name: action, exact: true })).toBeEnabled();
    await toolbar.getByRole("button", { name: action, exact: true }).click();
    const form = page.getByRole("dialog", { name: action, exact: true });
    await expect(form).toBeVisible();
    await fill(form);
    return form;
  };
  const planForm = async (action, fill) => {
    const before = await inspect();
    const form = await startForm(action, fill);
    const [response] = await Promise.all([
      page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_edit_timeline_plan")),
      form.getByRole("button", { name: "Review timeline changes", exact: true }).click(),
    ]);
    const envelope = await response.json();
    expect(envelope.error).toBeNull();
    expect(response.request().postDataJSON()).toMatchObject({ project_id: projectId, expected_revision: before.revision, sequence_id: sceneId });
    await expect(review()).toBeVisible();
    await expect(review().getByRole("status")).toHaveText("Review timeline changes before saving.");
    expect(envelope.data.transaction.expected_revision).toBe(before.revision);
    return { plan: envelope.data, request: response.request().postDataJSON(), before };
  };
  const applyPlan = async planned => {
    const count = writes.length;
    const [response] = await Promise.all([
      page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_edit_apply")),
      review().getByRole("button", { name: "Apply timeline changes", exact: true }).click(),
    ]);
    const envelope = await response.json();
    expect(envelope.error).toBeNull();
    expect(response.request().postDataJSON()).toEqual(planned.plan.transaction);
    expect(writes).toHaveLength(count + 1);
    expect(writes.at(-1)).toEqual(planned.plan.transaction);
    expect(envelope.data.base_revision).toBe(planned.before.revision);
    await expect(review().getByRole("status")).toHaveText("Timeline changes saved.");
    await review().getByRole("button", { name: "Done", exact: true }).click();
    await expect(review()).toBeHidden();
    await waitReturnCurrent();
    await expect.poll(async () => (await inspect()).revision).toBe(envelope.data.revision);
    return { snapshot: await inspect(), receipt: envelope.data };
  };
  const planTrack = async action => {
    const before = await inspect();
    const [response] = await Promise.all([
      page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_edit_timeline_plan")),
      toolbar.getByRole("button", { name: action, exact: true }).click(),
    ]);
    const envelope = await response.json(); expect(envelope.error).toBeNull();
    await expect(review()).toBeVisible();
    return applyPlan({ plan: envelope.data, before });
  };

  page.on("request", observe);
  try {
    const copy = await tool("opencut_project_copy", { source_project_id: fixed.project.id, source_revision: fixed.revision,
      project_id: projectId, name: "Independent timeline editing acceptance", idempotency_key: crypto.randomUUID() });
    copied = true;
    const first = structuredClone(source); first.id = firstId; first.name = firstName; first.enabled = true;
    first.timeline_range = { start: time(0), duration: time(4, 3) };
    first.content.retime.rate = { numerator: 3, denominator: 2 };
    // This explicit synthetic source window remains inside the observed four
    // seconds. It does not alter media metadata or assert decoded samples.
    first.source_span = { profile: "classic_trimmed", trim_start: time(1), trim_end: time(1), source_duration: time(4) };
    const second = structuredClone(first); second.id = secondId; second.name = secondName;
    const makeTrack = (id, name, voice) => ({ id, name, kind: "audio", enabled: true, muted: false, locked: false,
      opacity: 1, gain: { unit: "decibels", value: 0 }, channels: [], elements: [voice] });
    expect(copy.project.settings.profile).toBe("classic_global");
    const settings = structuredClone(copy.project.settings);
    settings.settings.fps = { numerator: 30, denominator: 1 };
    const operations = [{ type: "settings_set", settings }];
    for (const track of sourceScene.tracks) {
      if (track.locked) operations.push({ type: "track_lock_set", sequence_id: sceneId, track_id: track.id, locked: false });
      operations.push({ type: "track_put", sequence_id: sceneId, track: { ...structuredClone(track), enabled: false, locked: false } });
    }
    operations.push({ type: "track_put", sequence_id: sceneId, track: makeTrack(firstTrackId, trackName, first) },
      { type: "track_put", sequence_id: sceneId, track: makeTrack(secondTrackId, referenceTrackName, second) });
    const seeded = await tool("opencut_edit_apply", { project_id: projectId, expected_revision: copy.revision,
      idempotency_key: crypto.randomUUID(), label: "Synthetic two-track timeline fixture with real imported media", operations });
    await page.goto(new URL(`/editor/${encodeURIComponent(projectId)}?scene=${encodeURIComponent(sceneId)}`, returnUrl).href);
    await waitReturnCurrent(); await expect(clip(firstName)).toBeVisible(); await expect(clip(secondName)).toBeVisible();
    expect((await inspect()).revision).toBe(seeded.revision);

    await select([firstName, secondName]);
    const fillMove = async form => {
      for (const [name, id] of [[firstName, firstTrackId], [secondName, secondTrackId]]) {
        await form.getByRole("combobox", { name: labelPrefix(`Target track for ${name}`) }).selectOption(id);
        await form.getByLabel(`New start for ${name} (seconds)`, { exact: true }).fill("1");
      }
    };
    const beforeCancel = await inspect(), noWrite = writes.length;
    const draft = await startForm("Move selection", fillMove);
    await draft.getByRole("button", { name: "Cancel", exact: true }).click();
    await expect(draft).toBeHidden();
    expect((await inspect()).revision).toBe(beforeCancel.revision); expect(writes).toHaveLength(noWrite);
    const discarded = await planForm("Move selection", fillMove);
    await review().getByRole("button", { name: "Discard timeline changes", exact: true }).click();
    expect((await inspect()).revision).toBe(beforeCancel.revision); expect(writes).toHaveLength(noWrite);
    const movedPlan = await planForm("Move selection", fillMove);
    expect(movedPlan.plan.transaction.operations).toEqual(discarded.plan.transaction.operations);
    const moved = await applyPlan(movedPlan);
    for (const id of [firstId, secondId]) {
      equalTime(element(moved.snapshot, id).timeline_range.start, 1);
      expect(element(moved.snapshot, id).source_span).toEqual(first.source_span);
      expect(element(moved.snapshot, id).content).toEqual(first.content);
    }

    await trackButton(trackName).click();
    const locked = await planTrack("Lock track");
    expect(scene(locked.snapshot).tracks.find(track => track.id === firstTrackId).locked).toBe(true);
    await select([firstName]); await expect(toolbar.getByRole("button", { name: "Move selection", exact: true })).toBeDisabled();
    const lockedHead = await inspect(), lockWrites = writes.length;
    const rejected = await raw("opencut_edit_timeline_plan", { project_id: projectId, expected_revision: lockedHead.revision,
      sequence_id: sceneId, idempotency_key: crypto.randomUUID(), label: "Reject locked move", command: { kind: "move_selection", ripple: false,
        moves: [{ target: { track_id: firstTrackId, element_id: firstId }, target_track_id: firstTrackId, new_start: time(0) }] } });
    expect(rejected.error.code).toBe("CONSTRAINT_VIOLATION"); expect((await inspect()).revision).toBe(lockedHead.revision); expect(writes).toHaveLength(lockWrites);
    await trackButton(trackName).click(); await planTrack("Unlock track");
    const collisionHead = await inspect(), collisionWrites = writes.length;
    const collision = await raw("opencut_edit_timeline_plan", { project_id: projectId, expected_revision: collisionHead.revision,
      sequence_id: sceneId, idempotency_key: crypto.randomUUID(), label: "Reject existing right identity", command: { kind: "split", ripple: false, retain: "both",
        targets: [{ target: { track_id: firstTrackId, element_id: firstId }, right_element_id: secondId }], at: time(3, 2) } });
    expect(collision.error.code).toBe("INVALID_ARGUMENT"); expect((await inspect()).revision).toBe(collisionHead.revision); expect(writes).toHaveLength(collisionWrites);

    const recoveryPanel = () => page.getByRole("region", { name: "Recover original edit", exact: true });
    const reloadOriginalReview = async (planned, expectedReceipt, planCount) => {
      const [response] = await Promise.all([
        page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_edit_recovery_inspect") &&
          response.request().postDataJSON().project_id === projectId), page.reload(),
      ]);
      const inspection = await response.json(); expect(inspection.error).toBeNull();
      const recovery = inspection.data.record;
      expect(recovery.context.intent).toEqual({ kind: "public_apply", request: planned.plan.transaction });
      expect(recovery.context.sequence_id).toBe(sceneId);
      expect(recovery.receipt).toEqual(expectedReceipt ? { kind: "public_apply", transaction: expectedReceipt } : null);
      expect(new URL(page.url()).pathname).toBe(`/editor/${projectId}`);
      await expect(page.getByLabel("Scene", { exact: true })).toHaveValue(sceneId);
      await expect(recoveryPanel().getByRole("status")).toHaveText(expectedReceipt ? "Original edit saved."
        : "The original edit needs recovery before another change.");
      expect(plans).toHaveLength(planCount);
      return recovery;
    };
    const doneRecovery = async (needRelease = false) => {
      if (needRelease) await recoveryPanel().getByRole("button", { name: "Release resolved edit", exact: true }).click();
      await recoveryPanel().getByRole("button", { name: "Done", exact: true }).click();
      await expect(recoveryPanel()).toHaveCount(0); await waitReturnCurrent();
      const inspected = await tool("opencut_edit_recovery_inspect", { project_id: projectId, recovery_id: null });
      expect(inspected.record).toBeNull();
    };

    // The response is genuinely lost after its original transaction commits.
    // A whole-page reload must restore the authoritative original receipt,
    // without another plan or another apply.
    await select([firstName]);
    recoveryPlan = await planForm("Move selection", form => form.getByLabel(`New start for ${firstName} (seconds)`, { exact: true }).fill("0"));
    const historyBefore = await tool("opencut_project_history", { project_id: projectId, limit: 50 });
    const recoveryWrites = writes.length, recoveryPlans = plans.length;
    routeHandler = async route => {
      const response = await route.fetch(); lostResponse = { request: route.request().postDataJSON(), envelope: await response.json() };
      await route.abort("failed");
    };
    await page.route(routePattern, routeHandler, { times: 1 });
    await review().getByRole("button", { name: "Apply timeline changes", exact: true }).click();
    await expect(review().getByRole("status")).toHaveText("Timeline save is unconfirmed. Retry the original operation to recover its receipt.");
    expect(lostResponse.envelope.error).toBeNull(); expect(lostResponse.request).toEqual(recoveryPlan.plan.transaction);
    await page.unroute(routePattern, routeHandler); routeHandler = null;
    const savedRecovery = await reloadOriginalReview(recoveryPlan, lostResponse.envelope.data, recoveryPlans);
    await expect(recoveryPanel().getByRole("status")).toHaveText("Original edit saved.");
    await expect(recoveryPanel().getByRole("button", { name: "Retry original edit", exact: true })).toHaveCount(0);
    expect(writes.slice(recoveryWrites)).toEqual([lostResponse.request]);
    await doneRecovery(true);
    const historyAfter = await tool("opencut_project_history", { project_id: projectId, limit: 50 });
    expect(historyAfter.revisions).toHaveLength(historyBefore.revisions.length + 1);
    equalTime(element(await inspect(), firstId).timeline_range.start, 0);

    // A dispatch aborted before it reaches the service is equally unknown to
    // the browser. After reload, explicit Retry must use that exact durable
    // transaction and key, not reconstruct or replan its intent.
    await select([secondName]);
    const pendingPlan = await planForm("Move selection", form => form.getByLabel(`New start for ${secondName} (seconds)`, { exact: true }).fill("2"));
    const pendingHistory = await tool("opencut_project_history", { project_id: projectId, limit: 50 });
    const pendingWrites = writes.length, pendingPlans = plans.length;
    let abortedRequest;
    routeHandler = async route => { abortedRequest = route.request().postDataJSON(); await route.abort("failed"); };
    await page.route(routePattern, routeHandler, { times: 1 });
    await review().getByRole("button", { name: "Apply timeline changes", exact: true }).click();
    await expect(review().getByRole("status")).toHaveText("Timeline save is unconfirmed. Retry the original operation to recover its receipt.");
    expect(abortedRequest).toEqual(pendingPlan.plan.transaction);
    expect((await inspect()).revision).toBe(pendingPlan.before.revision);
    await page.unroute(routePattern, routeHandler); routeHandler = null;
    await reloadOriginalReview(pendingPlan, null, pendingPlans);
    await expect(recoveryPanel().getByRole("status")).toHaveText("The original edit needs recovery before another change.");
    const [replayResponse] = await Promise.all([
      page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_edit_apply") &&
        response.request().postDataJSON().project_id === projectId),
      recoveryPanel().getByRole("button", { name: "Retry original edit", exact: true }).click(),
    ]);
    const replay = await replayResponse.json(); expect(replay.error).toBeNull();
    expect(replayResponse.request().postDataJSON()).toEqual(abortedRequest);
    expect(replayResponse.request().postDataJSON().idempotency_key).toBe(pendingPlan.request.idempotency_key);
    expect(replay.data.base_revision).toBe(pendingPlan.before.revision);
    expect(plans).toHaveLength(pendingPlans);
    expect(writes.slice(pendingWrites)).toEqual([abortedRequest, abortedRequest]);
    await expect(recoveryPanel().getByRole("status")).toHaveText("Original edit saved.");
    await doneRecovery();
    const recoveredSnapshot = await inspect();
    expect(recoveredSnapshot.revision).toBe(replay.data.revision);
    equalTime(element(recoveredSnapshot, secondId).timeline_range.start, 2);
    const recoveredReference = structuredClone(element(recoveredSnapshot, secondId));
    const pendingHistoryAfter = await tool("opencut_project_history", { project_id: projectId, limit: 50 });
    expect(pendingHistoryAfter.revisions).toHaveLength(pendingHistory.revisions.length + 1);
    check("real two-track timeline UI: Ctrl/Meta Move and cancellation, lock/ID rejection; actual page reload restores an accepted-but-lost original receipt without another apply, while pre-service network abort reloads and Retry sends the identical original transaction/key without planning and publishes once", {
      timeline_transactions: { project_id: projectId, fixed_media_revision: fixed.revision, synthetic_fps: 30, synthetic_rate: { numerator: 3, denominator: 2 }, synthetic_source_trim_start: time(1), synthetic_source_trim_end: time(1), moved_transaction_id: moved.receipt.transaction_id,
        accepted_lost_transaction_id: savedRecovery.receipt.transaction.transaction_id, accepted_lost_revision: savedRecovery.receipt.transaction.revision,
        retried_transaction_id: replay.data.transaction_id, retried_revision: replay.data.revision,
        retried_original_key: pendingPlan.request.idempotency_key, target_element_ids: [firstId, secondId] } });

    await select([firstName]);
    const left = await applyPlan(await planForm("Trim element", async form => {
      await form.getByRole("combobox", { name: labelPrefix("Trim edge") }).selectOption("left"); await form.getByLabel("Boundary (scene seconds)", { exact: true }).fill("0.2");
    }));
    let actual = element(left.snapshot, firstId);
    equalTime(actual.timeline_range.start, 1, 5); equalTime(actual.timeline_range.duration, 17, 15);
    equalTime(actual.source_span.trim_start, 13, 10); equalTime(actual.source_span.trim_end, 1);
    await select([firstName]);
    const right = await applyPlan(await planForm("Trim element", async form => {
      await form.getByRole("combobox", { name: labelPrefix("Trim edge") }).selectOption("right"); await form.getByLabel("Boundary (scene seconds)", { exact: true }).fill("1.2");
    }));
    actual = element(right.snapshot, firstId); equalTime(actual.timeline_range.duration, 1);
    equalTime(actual.source_span.trim_start, 13, 10); equalTime(actual.source_span.trim_end, 6, 5);
    await select([firstName]);
    const splitPlan = await planForm("Split selection", async form => {
      await form.getByLabel("Boundary (scene seconds)", { exact: true }).fill("0.5");
      await form.getByRole("combobox", { name: labelPrefix("Keep after split") }).selectOption("both");
    });
    const rightId = splitPlan.request.command.targets[0].right_element_id;
    const splitResult = await applyPlan(splitPlan);
    const leftPart = element(splitResult.snapshot, firstId), rightPart = element(splitResult.snapshot, rightId);
    equalTime(leftPart.timeline_range.duration, 3, 10); equalTime(leftPart.source_span.trim_end, 9, 4);
    equalTime(rightPart.timeline_range.start, 1, 2); equalTime(rightPart.timeline_range.duration, 7, 10);
    equalTime(rightPart.source_span.trim_start, 7, 4); equalTime(rightPart.source_span.trim_end, 6, 5);
    for (const value of [leftPart, rightPart]) { expect(value.content).toEqual(first.content); equalTime(value.source_span.source_duration, 4); }
    await select([leftPart.name]);
    const deletePlan = await planForm("Delete selection", form => form.getByLabel("Ripple affected tracks", { exact: true }).check());
    expect(deletePlan.plan.decision.ripple_shifted_ids).toEqual([rightId]);
    const deleted = await applyPlan(deletePlan);
    expect(element(deleted.snapshot, firstId)).toBeUndefined();
    equalTime(element(deleted.snapshot, rightId).timeline_range.start, 1, 5);
    expect(element(deleted.snapshot, rightId).source_span).toEqual(rightPart.source_span);
    expect(element(deleted.snapshot, secondId)).toEqual(recoveredReference);

    await page.reload(); await waitReturnCurrent(); await expect(clip(rightPart.name)).toBeVisible();
    expect((await inspect()).project).toEqual(deleted.snapshot.project);
    const history = page.locator(".editor-history");
    await expect(history.getByRole("button", { name: "Undo", exact: true })).toBeEnabled();
    await history.getByRole("button", { name: "Undo", exact: true }).click();
    const [undoResponse] = await Promise.all([
      page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_edit_undo_apply")),
      history.getByRole("button", { name: "Apply undo", exact: true }).click(),
    ]);
    const undo = await undoResponse.json(); expect(undo.error).toBeNull();
    // applyHistory awaits refresh; absence of its plan and the new enabled
    // navigation are UI observations, rather than racing its HTTP receipt.
    await expect(history.getByRole("button", { name: "Apply undo", exact: true })).toBeHidden();
    await expect(history.getByRole("button", { name: "Redo", exact: true })).toBeEnabled();
    await expect(clip(leftPart.name)).toBeVisible();
    expect((await inspect()).project).toEqual(splitResult.snapshot.project);
    await history.getByRole("button", { name: "Redo", exact: true }).click();
    const [redoResponse] = await Promise.all([
      page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_edit_redo_apply")),
      history.getByRole("button", { name: "Apply redo", exact: true }).click(),
    ]);
    const redo = await redoResponse.json(); expect(redo.error).toBeNull();
    await expect(history.getByRole("button", { name: "Apply redo", exact: true })).toBeHidden();
    await expect(history.getByRole("button", { name: "Undo", exact: true })).toBeEnabled();
    await expect(history.getByRole("button", { name: "Redo", exact: true })).toBeDisabled();
    await expect(clip(leftPart.name)).toHaveCount(0);
    expect((await inspect()).project).toEqual(deleted.snapshot.project);
    await page.reload(); await waitReturnCurrent(); await expect(clip(leftPart.name)).toHaveCount(0); await expect(clip(rightPart.name)).toBeVisible();
    expect((await inspect()).project).toEqual(deleted.snapshot.project);
    check("real timeline UI bilateral trim/split/delete+ripple preserves independent 1.5x source-clock expectations; original public plans apply once, reopening and selective Undo/Redo recover the complete project without touching the independent reference track", {
      timeline_source_clock: { source_asset_id: source.content.asset_id, right_element_id: rightId, split_time: time(1, 2),
        right_trim_start: time(7, 4), right_trim_end: time(6, 5), right_duration: time(7, 10), rippled_start: time(1, 5),
        delete_transaction_id: deleted.receipt.transaction_id, redo_revision: redo.data.transaction.revision } });
  } catch (error) {
    failure = error;
    if (error instanceof Error) {
      try {
        const fieldNames = await page.locator('[role="dialog"] label').evaluateAll(labels => labels
          .filter(label => label.getClientRects().length)
          .map(label => (label.querySelector("span")?.textContent ?? label.getAttribute("aria-label") ?? "").trim().slice(0, 200))
          .filter(Boolean).slice(0, 50));
        if (fieldNames.length) error.message += `\nTimeline dialog field names: ${JSON.stringify(fieldNames)}`;
      } catch { /* Preserve the original failure if the page is unavailable. */ }
    }
    throw error;
  }
  finally {
    const cleanupErrors = [];
    page.off("request", observe);
    if (routeHandler) try { await page.unroute(routePattern, routeHandler); } catch (error) { cleanupErrors.push(error); }
    if (copied) try {
      const head = await inspect();
      await tool("opencut_project_delete", { idempotency_key: crypto.randomUUID(), projects: [{ project_id: projectId, expected_revision: head.revision }] });
    } catch (error) { cleanupErrors.push(error); }
    try {
      const destination = new URL(returnUrl); destination.searchParams.set("scene", returnSceneId);
      await page.goto(destination.href);
      const back = page.getByRole("button", { name: "Return to current", exact: true });
      if (await back.isVisible()) await back.click();
      await expect(back).toBeHidden();
      await expect(page.getByLabel("Scene", { exact: true })).toHaveValue(returnSceneId);
      await expect(toolbar.getByRole("button", { name: "Add track", exact: true })).toBeEnabled();
      expect((await editorSnapshot(tool("opencut_project_inspect", { project_id: fixed.project.id, revision: null }))).revision).toBe(returnHead.revision);
    } catch (error) { cleanupErrors.push(error); }
    if (cleanupErrors.length) {
      const detail = cleanupErrors.map(error => String(error)).join("\n");
      if (failure instanceof Error) failure.message += `\nTimeline cleanup failed without hiding the original failure:\n${detail}`;
      else if (!failure) throw new Error(`Timeline cleanup failed:\n${detail}`);
    }
  }
}

// Physical gestures over the same real CAS media as the form acceptance. The
// copied project is metadata only; source clocks below are independent hand
// expectations for 30 fps, rate 3/2 and a [1,3] source-second window.
export async function verifyTimelineGestures(page, tool, fixed, sceneId, check) {
  const original = fixed.project.sequences.find(scene => scene.id === sceneId);
  const source = original?.tracks.flatMap(track => track.elements).find(element => element.content.kind === "audio");
  expect(source).toBeTruthy(); expect(source.source_span.profile).toBe("classic_trimmed");
  const asset = fixed.project.assets.find(value => value.id === source.content.asset_id);
  expect(asset?.duration).toBeTruthy();
  equalTime(asset.duration, 4);
  const returnUrl = page.url(), returnSceneId = await page.getByLabel("Scene", { exact: true }).inputValue();
  const returnHead = await editorSnapshot(tool("opencut_project_inspect", { project_id: fixed.project.id, revision: null }));
  const projectId = "timeline-gestures-" + crypto.randomUUID(), ids = [crypto.randomUUID(), crypto.randomUUID()];
  const trackIds = [crypto.randomUUID(), crypto.randomUUID(), crypto.randomUUID()];
  const names = ["Gesture primary voice", "Gesture reference voice"];
  const plans = [], writes = [];
  const observe = request => {
    if (!request.url().includes("/api/tools/")) return;
    const body = request.postDataJSON(); if (body?.project_id !== projectId) return;
    if (request.url().endsWith("/opencut_edit_timeline_plan")) plans.push(body);
    if (request.url().endsWith("/opencut_edit_apply")) writes.push(body);
  };
  const panel = () => page.locator(".editor-timeline-panel"), viewport = () => panel().locator(".editor-timeline-viewport");
  const toolbar = () => panel().locator('[aria-label="Timeline editing"]');
  const clip = name => panel().locator(".editor-element-select").filter({ hasText: `${name} · audio` });
  const card = name => panel().locator(".editor-timeline-element").filter({
    has: page.locator(".editor-element-select").filter({ hasText: `${name} · audio` }),
  });
  const review = () => page.getByRole("dialog", { name: "Review timeline changes", exact: true });
  const inspect = () => editorSnapshot(tool("opencut_project_inspect", { project_id: projectId, revision: null }));
  const scene = snapshot => snapshot.project.sequences.find(value => value.id === sceneId);
  const element = (snapshot, id) => scene(snapshot).tracks.flatMap(track => track.elements).find(value => value.id === id);
  const ready = async () => {
    await expect(page.getByLabel("Scene", { exact: true })).toHaveValue(sceneId);
    await expect(toolbar().getByRole("button", { name: "Add track", exact: true })).toBeEnabled();
  };
  const hit = async locator => {
    await locator.scrollIntoViewIfNeeded();
    await locator.evaluate(element => {
      element.scrollIntoView({ block: "center", inline: "nearest", behavior: "instant" });
      const viewport = element.closest(".editor-timeline-viewport");
      const view = viewport.getBoundingClientRect(), box = element.getBoundingClientRect();
      const label = element.closest(".editor-track")?.querySelector(".editor-track-name")?.getBoundingClientRect();
      const left = Math.max(view.left + viewport.clientLeft, label?.right ?? view.left) + 40;
      const right = view.left + viewport.clientLeft + viewport.clientWidth - 40;
      // Native scrolling does not account for the sticky track label. Move the
      // target itself into view; never move the hit point outside a narrow handle.
      if (box.left < left) viewport.scrollLeft += box.left - left;
      else if (box.right > right) viewport.scrollLeft += box.right - right;
    });
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    const point = await locator.evaluate(element => {
      const rect = node => {
        const box = node.getBoundingClientRect();
        return { left: box.left, top: box.top, right: box.right, bottom: box.bottom, width: box.width, height: box.height };
      };
      const viewport = element.closest(".editor-timeline-viewport"), box = rect(element), view = rect(viewport);
      const label = element.closest(".editor-track")?.querySelector(".editor-track-name"), sticky = label ? rect(label) : null;
      const visible = { left: 0, top: 0, right: innerWidth, bottom: innerHeight }, clips = [];
      for (let parent = element.parentElement; parent; parent = parent.parentElement) {
        const style = getComputedStyle(parent), clipX = /auto|scroll|hidden|clip/.test(style.overflowX), clipY = /auto|scroll|hidden|clip/.test(style.overflowY);
        if (!clipX && !clipY) continue;
        const bounds = rect(parent), left = bounds.left + parent.clientLeft, top = bounds.top + parent.clientTop;
        clips.push({ node: parent.className, bounds, overflowX: style.overflowX, overflowY: style.overflowY });
        if (clipX) { visible.left = Math.max(visible.left, left); visible.right = Math.min(visible.right, left + parent.clientWidth); }
        if (clipY) { visible.top = Math.max(visible.top, top); visible.bottom = Math.min(visible.bottom, top + parent.clientHeight); }
      }
      visible.left = Math.max(visible.left, sticky?.right ?? view.left);
      const left = Math.max(box.left, visible.left), right = Math.min(box.right, visible.right);
      const top = Math.max(box.top, visible.top), bottom = Math.min(box.bottom, visible.bottom);
      const x = Math.min(right - 1, Math.max(left + 1, box.left + Math.min(12, box.width / 2)));
      const y = Math.min(bottom - 1, Math.max(top + 1, box.top + Math.min(12, box.height / 2)));
      const actual = document.elementFromPoint(x, y);
      return { x, y, position: { x: x - box.left, y: y - box.top }, inside: right > left && bottom > top &&
        x >= left && x < right && y >= top && y < bottom && actual !== null && element.contains(actual),
        probe: { target: { node: element.tagName, classes: element.className, trim: element.getAttribute("data-trim-edge"), box },
          view, sticky, visible, clips, scroll: { left: viewport.scrollLeft, top: viewport.scrollTop,
            maxLeft: viewport.scrollWidth - viewport.clientWidth },
          actual: actual ? { node: actual.tagName, classes: actual.className, box: rect(actual) } : null } };
    });
    expect(point.inside, `A physical timeline gesture must hit its current visible element: ${JSON.stringify(point)}`).toBe(true);
    return point;
  };
  const select = async () => {
    const modifier = await page.evaluate(() => /Mac/.test(navigator.platform)) ? "Meta" : "Control";
    for (let index = 0; index < names.length; index++) {
      const point = await hit(clip(names[index]));
      await clip(names[index]).click({ position: point.position, modifiers: index ? [modifier] : [] });
    }
    await expect(toolbar()).toContainText("2 selected");
  };
  const drag = async (locator, dx, dy, cancel = false) => {
    const point = await hit(locator);
    const { view, sticky, scroll } = point.probe;
    const path = { left: Math.min(point.x, point.x + dx), right: Math.max(point.x, point.x + dx) };
    const leftZone = Math.max(view.left + 172, sticky?.right ?? view.left) + 24, rightZone = view.right - 24;
    expect(scroll.left <= 0 || path.left >= leftZone,
      `A controlled drag must stay outside the left auto-scroll zone: ${JSON.stringify({ point, dx, path, leftZone })}`).toBe(true);
    expect(scroll.left >= scroll.maxLeft || path.right <= rightZone,
      `A controlled drag must stay outside the right auto-scroll zone: ${JSON.stringify({ point, dx, path, rightZone })}`).toBe(true);
    await page.mouse.move(point.x, point.y); await page.mouse.down();
    await page.mouse.move(point.x + dx, point.y + dy, { steps: 4 });
    if (cancel) await page.keyboard.press("Escape");
    await page.mouse.up();
  };
  const plannedGesture = async action => {
    const before = await inspect(), count = writes.length;
    const [response] = await Promise.all([
      page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_edit_timeline_plan") && response.request().postDataJSON().project_id === projectId),
      action(),
    ]);
    const result = await response.json(); expect(result.error).toBeNull();
    await expect(review()).toBeVisible();
    expect(writes).toHaveLength(count); expect((await inspect()).revision).toBe(before.revision);
    expect(result.data.transaction.expected_revision).toBe(before.revision);
    return { before, plan: result.data, request: response.request().postDataJSON() };
  };
  const apply = async planned => {
    const count = writes.length;
    const [response] = await Promise.all([
      page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_edit_apply") && response.request().postDataJSON().project_id === projectId),
      review().getByRole("button", { name: "Apply timeline changes", exact: true }).click(),
    ]);
    const result = await response.json(); expect(result.error).toBeNull();
    expect(response.request().postDataJSON()).toEqual(planned.plan.transaction); expect(writes).toHaveLength(count + 1);
    await expect(review().getByRole("status")).toHaveText("Timeline changes saved.");
    await review().getByRole("button", { name: "Done", exact: true }).click(); await ready();
    await expect.poll(async () => (await inspect()).revision).toBe(result.data.revision);
    return inspect();
  };
  let copied = false, failure;
  page.on("request", observe);
  try {
    const copy = await tool("opencut_project_copy", { source_project_id: fixed.project.id, source_revision: fixed.revision,
      project_id: projectId, name: "Independent physical timeline gestures", idempotency_key: crypto.randomUUID() });
    copied = true;
    const settings = structuredClone(copy.project.settings); expect(settings.profile).toBe("classic_global");
    settings.settings.fps = { numerator: 30, denominator: 1 };
    const operations = [{ type: "settings_set", settings }];
    for (const track of original.tracks) {
      if (track.locked) operations.push({ type: "track_lock_set", sequence_id: sceneId, track_id: track.id, locked: false });
      operations.push({ type: "track_put", sequence_id: sceneId, track: { ...structuredClone(track), enabled: false, locked: false } });
    }
    for (let index = 0; index < 3; index++) {
      const voice = structuredClone(source); voice.id = ids[index]; voice.name = names[index]; voice.enabled = true;
      voice.timeline_range = { start: time(0), duration: time(4, 3) };
      voice.content.retime = { ...voice.content.retime, rate: { numerator: 3, denominator: 2 }, maintain_pitch: false };
      voice.source_span = { profile: "classic_trimmed", trim_start: time(1), trim_end: time(1), source_duration: time(4) };
      operations.push({ type: "track_put", sequence_id: sceneId, track: { id: trackIds[index], name: `Gesture track ${index + 1}`, kind: "audio",
        enabled: true, muted: false, locked: false, opacity: 1, gain: { unit: "decibels", value: 0 }, channels: [], elements: index < 2 ? [voice] : [] } });
    }
    await tool("opencut_edit_apply", { project_id: projectId, expected_revision: copy.revision, idempotency_key: crypto.randomUUID(),
      label: "Physical gesture fixture using already imported CAS bytes", operations });
    await page.goto(new URL(`/editor/${encodeURIComponent(projectId)}?scene=${encodeURIComponent(sceneId)}`, returnUrl).href); await ready();
    await select();
    const beforeCancel = await inspect(), planCount = plans.length, writeCount = writes.length;
    await drag(clip(names[0]), 80, 0, true);
    expect(plans).toHaveLength(planCount); expect(writes).toHaveLength(writeCount);
    expect((await inspect()).revision).toBe(beforeCancel.revision);
    const move = await plannedGesture(() => drag(clip(names[0]), 80, 80));
    expect(move.request.command).toMatchObject({ kind: "move_selection", ripple: false, moves: [
      { target: { track_id: trackIds[0], element_id: ids[0] }, target_track_id: trackIds[1] },
      { target: { track_id: trackIds[1], element_id: ids[1] }, target_track_id: trackIds[2] },
    ] });
    for (const value of move.request.command.moves) equalTime(value.new_start, 1);
    const moved = await apply(move);
    for (let index = 0; index < 2; index++) {
      equalTime(element(moved, ids[index]).timeline_range.start, 1);
      equalTime(element(moved, ids[index]).source_span.trim_start, 1);
      expect(scene(moved).tracks[index + original.tracks.length + 1].elements.map(value => value.id)).toContain(ids[index]);
    }
    // Zoom and native scrolling alter presentation only. Prove a current
    // viewport hit, then restore 1x using the keyboard-operated range control.
    const zoom = panel().getByRole("slider", { name: "Timeline zoom" });
    await zoom.focus(); await zoom.press("End"); await expect(zoom).toHaveValue("8");
    await viewport().evaluate(element => { element.scrollLeft = 400; element.dispatchEvent(new Event("scroll")); });
    const zoomHead = (await inspect()).revision; await hit(clip(names[0]));
    await zoom.focus(); await zoom.press("Home");
    for (let index = 0; index < 9; index++) await zoom.press("ArrowRight");
    await expect(zoom).toHaveValue("1"); expect((await inspect()).revision).toBe(zoomHead);
    const left = await apply(await plannedGesture(() => drag(card(names[0]).locator('[data-trim-edge="left"]'), 16, 0)));
    let actual = element(left, ids[0]);
    equalTime(actual.timeline_range.start, 6, 5); equalTime(actual.timeline_range.duration, 17, 15);
    equalTime(actual.source_span.trim_start, 13, 10); equalTime(actual.source_span.trim_end, 1);
    const right = await apply(await plannedGesture(() => drag(card(names[0]).locator('[data-trim-edge="right"]'), -32 / 3, 0)));
    actual = element(right, ids[0]); equalTime(actual.timeline_range.duration, 1);
    equalTime(actual.source_span.trim_start, 13, 10); equalTime(actual.source_span.trim_end, 6, 5);
    // Ruler seek and keyboard commands use exact service-confirmed requests;
    // Q/W are reviewed then discarded, S is the sole split publication.
    await clip(names[0]).click({ position: (await hit(clip(names[0]))).position });
    await viewport().evaluate(element => { element.scrollLeft = 0; });
    const ruler = panel().getByRole("slider", { name: "Scene playhead" });
    await ruler.scrollIntoViewIfNeeded();
    await viewport().evaluate(element => { element.scrollLeft = 0; element.scrollTop = 0; });
    await ruler.click({ position: { x: 136, y: 12 } });
    await expect(ruler).toHaveAttribute("aria-valuenow", "1.7");
    for (const [key, retain] of [["q", "right"], ["w", "left"]]) {
      const planned = await plannedGesture(async () => { await panel().focus(); await page.keyboard.press(key); });
      expect(planned.request.command.retain).toBe(retain); equalTime(planned.request.command.at, 17, 10);
      await review().getByRole("button", { name: "Discard timeline changes", exact: true }).click(); await ready();
    }
    const split = await plannedGesture(async () => { await panel().focus(); await page.keyboard.press("s"); });
    const splitId = split.request.command.targets[0].right_element_id, splitResult = await apply(split);
    equalTime(element(splitResult, ids[0]).timeline_range.duration, 1, 2);
    equalTime(element(splitResult, splitId).timeline_range.duration, 1, 2);
    equalTime(element(splitResult, splitId).source_span.trim_start, 41, 20);
    const history = page.locator(".editor-history");
    await history.getByRole("button", { name: "Undo", exact: true }).click();
    await history.getByRole("button", { name: "Apply undo", exact: true }).click();
    await expect(history.getByRole("button", { name: "Apply undo", exact: true })).toBeHidden();
    await expect(history.getByRole("button", { name: "Redo", exact: true })).toBeEnabled(); expect((await inspect()).project).toEqual(right.project);
    await history.getByRole("button", { name: "Redo", exact: true }).click();
    await history.getByRole("button", { name: "Apply redo", exact: true }).click();
    await expect(history.getByRole("button", { name: "Apply redo", exact: true })).toBeHidden();
    await expect(history.getByRole("button", { name: "Undo", exact: true })).toBeEnabled(); expect((await inspect()).project).toEqual(splitResult.project);
    check("physical fixed-revision timeline core gestures: multi-track Move+Escape, bilateral trim, zoom/scroll hit and ruler S/Q/W; original Review transactions publish once and Undo/Redo preserves independently expected 1.5x source windows", {
      timeline_gestures: { project_id: projectId, move_revision: moved.revision, split_revision: splitResult.revision,
        source_rate: { numerator: 3, denominator: 2 }, fps: 30, final_right_source_start: time(41, 20), right_element_id: splitId } });
    // A physical empty-lane rubberband crosses the actual content rectangles;
    // Escape restores the prior selection and cannot create a transaction.
    const lane = panel().locator(".editor-track").filter({ has: page.locator(".editor-track-name", { hasText: "Gesture track 2" }) }).locator(".editor-track-lane");
    await lane.scrollIntoViewIfNeeded();
    await lane.evaluate(element => {
      element.scrollIntoView({ block: "center", inline: "nearest", behavior: "instant" });
      element.closest(".editor-timeline-viewport").scrollLeft = 0;
    });
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    const selectedBefore = await panel().locator('.editor-timeline-element[data-selected="true"]').evaluateAll(elements =>
      elements.map(element => element.getAttribute("data-element-id")).sort());
    const beforeBand = plans.length, beforeBandWrites = writes.length, bandHead = (await inspect()).revision;
    const band = await lane.evaluate(element => {
      const rect = node => {
        const box = node.getBoundingClientRect();
        return { left: box.left, top: box.top, right: box.right, bottom: box.bottom, width: box.width, height: box.height };
      };
      const viewport = element.closest(".editor-timeline-viewport"), box = rect(element), view = rect(viewport);
      const sticky = rect(element.closest(".editor-track").querySelector(".editor-track-name"));
      const visible = { left: 0, top: 0, right: innerWidth, bottom: innerHeight }, clips = [];
      for (let parent = element.parentElement; parent; parent = parent.parentElement) {
        const style = getComputedStyle(parent), clipX = /auto|scroll|hidden|clip/.test(style.overflowX), clipY = /auto|scroll|hidden|clip/.test(style.overflowY);
        if (!clipX && !clipY) continue;
        const bounds = rect(parent), left = bounds.left + parent.clientLeft, top = bounds.top + parent.clientTop;
        clips.push({ node: parent.className, bounds, overflowX: style.overflowX, overflowY: style.overflowY });
        if (clipX) { visible.left = Math.max(visible.left, left); visible.right = Math.min(visible.right, left + parent.clientWidth); }
        if (clipY) { visible.top = Math.max(visible.top, top); visible.bottom = Math.min(visible.bottom, top + parent.clientHeight); }
      }
      visible.left = Math.max(box.left, visible.left, sticky.right); visible.right = Math.min(box.right, visible.right);
      visible.top = Math.max(box.top, visible.top); visible.bottom = Math.min(box.bottom, visible.bottom);
      const start = { x: visible.left + 8, y: visible.top + 8 };
      const end = { x: Math.min(box.left + 184, visible.right - 8), y: Math.min(start.y + 40, visible.bottom - 8) };
      const actual = document.elementFromPoint(start.x, start.y);
      const targets = [...element.querySelectorAll("[data-timeline-media]")].map(node => ({ id: node.dataset.elementId, box: rect(node) }));
      const crossed = targets.filter(target => target.box.left < end.x && target.box.right > start.x && target.box.top < end.y && target.box.bottom > start.y).map(target => target.id).sort();
      return { start, end, crossed, inside: start.x < end.x && start.y < end.y && actual === element &&
        end.x < visible.right && end.y < visible.bottom, probe: { box, view, sticky, visible, clips, targets,
          scroll: { left: viewport.scrollLeft, top: viewport.scrollTop },
          actual: actual ? { node: actual.tagName, classes: actual.className, box: rect(actual) } : null } };
    });
    expect(band.inside, `Rubberband must begin on a genuinely visible empty lane and end inside its clipped viewport: ${JSON.stringify(band)}`).toBe(true);
    expect(band.crossed, `The visible rubberband must cross both split pieces: ${JSON.stringify(band)}`).toEqual([ids[0], splitId].sort());
    await page.mouse.move(band.start.x, band.start.y); await page.mouse.down();
    await page.mouse.move(band.end.x, band.end.y, { steps: 4 });
    await expect(panel().locator(".editor-selection-box")).toBeVisible();
    await page.keyboard.press("Escape"); await page.mouse.up();
    await expect(panel().locator(".editor-selection-box")).toBeHidden();
    expect(await panel().locator('.editor-timeline-element[data-selected="true"]').evaluateAll(elements =>
      elements.map(element => element.getAttribute("data-element-id")).sort())).toEqual(selectedBefore);
    expect(plans).toHaveLength(beforeBand); expect(writes).toHaveLength(beforeBandWrites);
    expect((await inspect()).revision).toBe(bandHead);
    check("physical empty-lane rubberband crosses both split pieces and Escape restores exact selection without planning or writing", {
      timeline_rubberband: { project_id: projectId, revision: bandHead, crossed_ids: band.crossed } });
  } catch (error) { failure = error; throw error; }
  finally {
    page.off("request", observe);
    const errors = [];
    try { await page.mouse.up(); } catch (error) { errors.push(error); }
    if (copied) try {
      const head = await inspect(); await tool("opencut_project_delete", { idempotency_key: crypto.randomUUID(), projects: [{ project_id: projectId, expected_revision: head.revision }] });
    } catch (error) { errors.push(error); }
    try {
      await page.goto(returnUrl); await expect(page.getByLabel("Scene", { exact: true })).toHaveValue(returnSceneId);
      await expect(page.locator('[aria-label="Timeline editing"]').getByRole("button", { name: "Add track", exact: true })).toBeEnabled();
      expect((await editorSnapshot(tool("opencut_project_inspect", { project_id: fixed.project.id, revision: null }))).revision).toBe(returnHead.revision);
    } catch (error) { errors.push(error); }
    if (errors.length) {
      const detail = errors.map(String).join("\n");
      if (failure instanceof Error) failure.message += `\nGesture fixture cleanup: ${detail}`;
      else if (!failure) throw new Error(detail);
    }
  }
}

// Narrow acceptance entry: one actual uploaded source, 100 metadata placements.
// No decoder, timing, gain or PCM producer is replaced by this instrumentation.
export async function verifyPerformance(browser, endpoint, directory, check) {
  const machine = { platform: os.platform(), release: os.release(), arch: os.arch(),
    cpu: os.cpus()[0]?.model, logical_cpus: os.cpus().length, memory_bytes: os.totalmem(),
    browser: await browser.version() };
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
  page.setDefaultTimeout(30000);
  const errors = [], projects = [];
  page.on("pageerror", error => errors.push(error.message));
  const tool = async (name, input) => {
    const envelope = await page.evaluate(async ({ name, input }) => (await fetch(`/api/tools/${name}`, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input),
    })).json(), { name, input });
    expect(envelope.error, `${name}: ${JSON.stringify(envelope.error)}`).toBeNull();
    return envelope.data;
  };
  let failure;
  try {
    await page.goto(`${endpoint}/editor`);
    const create = page.getByRole("button", { name: "New project", exact: true });
    await expect(create).toBeEnabled(); // Includes the production HttpOnly bootstrap.
    await create.click();
    const dialog = page.getByRole("dialog", { name: "New project", exact: true });
    await dialog.getByLabel("Project name", { exact: true }).fill("Dense source fixture");
    await dialog.getByRole("button", { name: "Create project", exact: true }).click();
    await expect(page).toHaveURL(/\/editor\/.+/);
    const sourceId = decodeURIComponent(new URL(page.url()).pathname.split("/").at(-1));
    projects.push(sourceId);
    const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
    if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
    await expect(page.getByRole("button", { name: "Import media", exact: true })).toBeEnabled();
    const planned = page.waitForResponse(r => r.url().endsWith("/api/tools/opencut_media_import_plan"));
    await page.getByLabel("Import media files", { exact: true }).setInputFiles(path.join(directory, "sample.mp4"));
    const review = page.getByRole("dialog", { name: "Review media changes", exact: true });
    await expect(review).toBeVisible();
    const importEnvelope = await (await planned).json(); expect(importEnvelope.error).toBeNull();
    const applied = page.waitForResponse(r => r.url().endsWith("/api/tools/opencut_edit_apply"));
    await review.getByRole("button", { name: "Apply media changes", exact: true }).click();
    expect((await (await applied).json()).error).toBeNull();
    await expect(review.getByText("Media changes saved.", { exact: true })).toBeVisible();
    await review.getByRole("button", { name: "Done", exact: true }).click();
    const bound = await editorSnapshot(tool("opencut_project_inspect", { project_id: sourceId, revision: null }));
    const sceneId = bound.project.default_sequence_id;
    const sourceScene = bound.project.sequences.find(scene => scene.id === sceneId);
    const asset = bound.project.assets.find(asset => asset.kind === "video");
    expect(asset, "Actual upload must supply an observed video asset").toBeTruthy();
    equalTime(asset.duration, 4);
    expect(asset.metadata.audio.status).toBe("present");
    const audioTrackId = crypto.randomUUID();
    const timing = { kind: "source_range", range: { start: time(0), duration: time(4) },
      rate: { numerator: 1, denominator: 1 }, maintain_pitch: false };
    const templatePlan = await tool("opencut_media_import_plan", {
      project_id: sourceId, expected_revision: bound.revision, sequence_id: sceneId,
      idempotency_key: crypto.randomUUID(), label: "Real source templates", bindings: [], ripple: false,
      new_tracks: [{ id: audioTrackId, name: "Template audio", kind: "audio" }], placements: [
        { element_id: crypto.randomUUID(), name: "Template video", track_id: sourceScene.main_track_id,
          asset_id: asset.id, start: time(0), content: { kind: "video", timing, source_audio_enabled: true } },
        { element_id: crypto.randomUUID(), name: "Template audio", track_id: audioTrackId,
          asset_id: asset.id, start: time(0), content: { kind: "audio", timing } },
      ],
    });
    await tool("opencut_edit_apply", templatePlan.transaction);
    let fixed = await editorSnapshot(tool("opencut_project_inspect", { project_id: sourceId, revision: null }));
    // The real product currently creates one scene. A second empty scene adds
    // metadata only; the default scene's media, tracks and timing stay intact.
    let alternate = fixed.project.sequences.find(scene => scene.id !== sceneId);
    if (!alternate) {
      const main = fixed.project.sequences.find(scene => scene.id === sceneId).tracks.find(track => track.id === sourceScene.main_track_id);
      const trackId = crypto.randomUUID();
      alternate = { id: crypto.randomUUID(), name: "URL refresh notes scene", main_track_id: trackId,
        tracks: [{ ...structuredClone(main), id: trackId, elements: [], channels: [] }],
        captions: [], transitions: [], bookmarks: [] };
      await tool("opencut_edit_apply", { project_id: sourceId, expected_revision: fixed.revision,
        idempotency_key: crypto.randomUUID(), label: "Scene URL refresh metadata fixture",
        operations: [{ type: "sequence_put", sequence: alternate }] });
      fixed = await editorSnapshot(tool("opencut_project_inspect", { project_id: sourceId, revision: null }));
    }
    await page.reload();
    const sceneChoice = page.getByLabel("Scene", { exact: true });
    await expect(sceneChoice).toHaveValue(sceneId);
    await expect(page.locator('[aria-label="Timeline editing"]').getByRole("button", { name: "Add track", exact: true })).toBeEnabled();
    const originalNavigation = await page.evaluate(() => ({ path: location.pathname + location.search + location.hash, state: history.state }));
    const sentinel = crypto.randomUUID();
    const sentinelState = await page.evaluate(sentinel => {
      const url = new URL(location.href); url.searchParams.set("dense_scene_probe", sentinel); url.hash = "dense-scene-refresh-probe";
      const state = { ...(history.state ?? {}), denseSceneRefreshProbe: sentinel };
      history.replaceState(state, "", url.pathname + url.search + url.hash); return state;
    }, sentinel);
    const sceneUrls = [];
    for (const selected of [alternate.id, sceneId]) {
      await sceneChoice.selectOption(selected);
      await expect(sceneChoice).toHaveValue(selected);
      await expect.poll(() => new URL(page.url()).searchParams.get("scene")).toBe(selected);
      const verifyNavigation = async () => {
        const current = new URL(page.url());
        expect(decodeURIComponent(current.pathname.split("/").at(-1))).toBe(sourceId);
        expect(current.searchParams.get("scene")).toBe(selected);
        expect(current.searchParams.get("dense_scene_probe")).toBe(sentinel);
        expect(current.hash).toBe("#dense-scene-refresh-probe");
        expect(await page.evaluate(() => history.state)).toEqual(sentinelState);
      };
      await verifyNavigation();
      await page.reload();
      await expect(sceneChoice).toHaveValue(selected);
      await expect(page.locator('[aria-label="Timeline editing"]').getByRole("button", { name: "Add track", exact: true })).toBeEnabled();
      await verifyNavigation();
      sceneUrls.push({ scene_id: selected, url: page.url() });
    }
    expect((await editorSnapshot(tool("opencut_project_inspect", { project_id: sourceId, revision: null }))).revision).toBe(fixed.revision);
    await page.evaluate(original => history.replaceState(original.state, "", original.path), originalNavigation);
    expect(new URL(page.url()).searchParams.has("dense_scene_probe")).toBe(false);
    check("real Scene URL and refresh: nondefault and default scene persist while query/hash/history.state survive; probe removed before dense timing", {
      v4_scene_navigation: { project_id: sourceId, revision: fixed.revision, scene_switches: sceneUrls,
        query_hash_state_preserved: true, probe_removed: true } });
    const capability = await tool("opencut_audio_preview_capability", {});
    expect(capability.available, capability.unavailable_reason ?? "Real audio runtime required").toBe(true);
    expect(capability.sample_rates).toContain(48000);
    await verifyTimelinePerformance(page, tool, fixed, sceneId, check, { machine, onCopy: id => projects.push(id) });
    expect(errors).toEqual([]);
  } catch (error) { failure = error; throw error; }
  finally {
    const cleanup = [];
    // Navigating closes the product session before removing its fixed project.
    try { await page.goto(`${endpoint}/editor`); } catch (error) { cleanup.push(String(error)); }
    for (const id of projects.reverse()) try {
      const head = await editorSnapshot(tool("opencut_project_inspect", { project_id: id, revision: null }));
      await tool("opencut_project_delete", { idempotency_key: crypto.randomUUID(), projects: [{ project_id: id, expected_revision: head.revision }] });
    } catch (error) { cleanup.push(String(error)); }
    await page.close();
    if (cleanup.length) {
      if (failure instanceof Error) failure.message += `\nPerformance cleanup: ${cleanup.join("\n")}`;
      else throw new Error(cleanup.join("\n"));
    }
  }
}

export async function verifyTimelinePerformance(page, tool, fixed, sceneId, check, options) {
  const budget = Object.freeze({ open_ms: 2000, input_p95_ms: 50, input_max_ms: 100,
    cold_consume_ms: 10000, warm_seek_ms: 1000, stall_ratio: .01, heap_delta_bytes: 64 * 1024 * 1024,
    retained_first_last_growth_bytes: 8 * 1024 * 1024, retained_slope_bytes_per_seek: 256 * 1024 });
  const sourceScene = fixed.project.sequences.find(scene => scene.id === sceneId);
  const templates = sourceScene.tracks.flatMap(track => track.elements);
  const video = templates.find(element => element.content.kind === "video");
  const audio = templates.find(element => element.content.kind === "audio");
  expect(video && audio, "Server-planned real video/audio templates required").toBeTruthy();
  expect(video.content.asset_id).toBe(audio.content.asset_id);
  const asset = fixed.project.assets.find(asset => asset.id === video.content.asset_id);
  equalTime(asset.duration, 4);
  expect(asset.metadata.audio.status).toBe("present");
  expect(sourceScene.captions).toHaveLength(0); expect(sourceScene.transitions).toHaveLength(0);
  const projectId = "dense-acceptance-" + crypto.randomUUID();
  const copy = await tool("opencut_project_copy", { source_project_id: fixed.project.id, source_revision: fixed.revision,
    project_id: projectId, name: "Dense 100 clips / 5 tracks / 300 seconds", idempotency_key: crypto.randomUUID() });
  options.onCopy?.(projectId);
  const dense = structuredClone(sourceScene);
  dense.tracks = Array.from({ length: 5 }, (_, trackIndex) => {
    const kind = trackIndex < 3 ? "video" : "audio";
    const original = sourceScene.tracks.find(track => track.kind === kind);
    return { ...structuredClone(original), id: crypto.randomUUID(), name: `Dense track ${trackIndex + 1}`,
      enabled: true, muted: false, locked: false, channels: [], gain: { unit: "decibels", value: 0 },
      elements: Array.from({ length: 20 }, (_, index) => {
        const element = structuredClone(kind === "video" ? video : audio);
        element.id = crypto.randomUUID(); element.name = `Dense ${trackIndex + 1}:${index + 1}`;
        element.enabled = true; element.channels = [];
        element.timeline_range = { start: time(296 * index, 19), duration: time(4) };
        element.content.audio.gain = { unit: "decibels", value: -18 }; element.content.audio.muted = false;
        element.content.retime = { ...element.content.retime, rate: { numerator: 1, denominator: 1 }, maintain_pitch: false };
        if (kind === "video") element.content.source_audio_enabled = true;
        return element;
      }) };
  });
  dense.main_track_id = dense.tracks[0].id;
  const receipt = await tool("opencut_edit_apply", { project_id: projectId, expected_revision: copy.revision,
    idempotency_key: crypto.randomUUID(), label: "Ordinary dense metadata baseline",
    operations: [{ type: "sequence_put", sequence: dense }] });
  const fixedIdentity = { project_id: projectId, revision: receipt.revision, sequence_id: sceneId, sample_rate: 48000 };
  const evidence = { machine: options.machine, budget, fixture: { ...fixedIdentity, elements: 100, tracks: 5, seconds: 300,
    starts: "296*k/19; k=0..19 on each track", source_seconds: 4, unique_resources: 1,
    source_version: asset.version, rate: "1/1", maintain_pitch: false, animated_gain: false },
    native_preparation: { status: "unverified", reason: "Private native prepared receipts are collected separately by the host acceptance." },
    native_rss: { status: "unverified", reason: "No public process-tree RSS observation." },
    derived_cache: { status: "unverified", reason: "This helper measures the product and consumer, not cache eviction/authorization." },
    audio_errors: [], audio_operations: { create: 0, control: 0, by_kind: {} },
    worklet_requests: [], console_errors: "", audio_create_sessions: [] };
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Performance.enable");
  const heap = async () => (await cdp.send("Performance.getMetrics")).metrics.find(metric => metric.name === "JSHeapUsedSize").value;
  const retainedHeap = async () => { await cdp.send("HeapProfiler.collectGarbage"); return heap(); };
  await page.goto(new URL("/editor", page.url()).href);
  await expect(page.getByRole("button", { name: "New project", exact: true })).toBeEnabled();
  const heapBefore = await retainedHeap(), heaps = [heapBefore];
  let warmRetained;
  const observations = [], pending = new Set(); let observerFailure;
  const redact = text => String(text).replace(/https?:\/\/\S+|(?:\/[^\s/]+){2,}/g, "[redacted locator]");
  const sameFixedIdentity = session => session && Object.entries(fixedIdentity).every(([key, value]) => session.identity?.[key] === value);
  // Explicit public projection: never serialize the session's owner/lease fields.
  const summarizeSession = session => ({ session_id: String(session.session_id).slice(0, 128),
    identity: { project_id: session.identity.project_id, revision: session.identity.revision,
      sequence_id: session.identity.sequence_id, profile: session.identity.profile,
      sample_rate: session.identity.sample_rate, channels: session.identity.channels },
    generation: session.generation, overlay_epoch: session.overlay_epoch, state: session.state,
    failure_code: session.failure_code, actual_start_sample: session.actual_start_sample,
    total_frames: session.total_frames, consumed_frames: session.consumed_frames,
    produced_through_sample: session.produced_through_sample, buffered_frames: session.buffered_frames,
    presentation_time: session.presentation_time, stalled: session.stalled,
    presentation_fresh: session.presentation_fresh });
  const workletRequests = new WeakMap();
  const observeRequest = request => {
    const pathname = new URL(request.url()).pathname;
    if (/\/assets\/worklet-[^/]+\.js$/.test(pathname) && evidence.worklet_requests.length < 32) {
      const value = { pathname, status: null, failure: null };
      evidence.worklet_requests.push(value); workletRequests.set(request, value);
    }
    if (pathname.endsWith("/opencut_audio_preview_create")) evidence.audio_operations.create++;
    if (pathname.endsWith("/opencut_audio_preview_control")) {
      evidence.audio_operations.control++;
      const kind = request.postDataJSON()?.command?.kind;
      if (["play", "pause", "seek", "stop", "heartbeat", "report_consumed", "set_temporary_gain", "clear_temporary_gain"].includes(kind))
        evidence.audio_operations.by_kind[kind] = (evidence.audio_operations.by_kind[kind] ?? 0) + 1;
    }
  };
  const observeFailedRequest = request => {
    const value = workletRequests.get(request);
    if (value) value.failure = redact(request.failure()?.errorText ?? "Request failed").slice(0, 256);
  };
  const observeConsole = message => {
    if (message.type() === "error" && evidence.console_errors.length < 2048)
      evidence.console_errors = (evidence.console_errors + "\n" + redact(message.text())).slice(0, 2048);
  };
  const observe = response => {
    const worklet = workletRequests.get(response.request());
    if (worklet) worklet.status = response.status();
    if (!response.url().endsWith("/api/tools/opencut_audio_preview_control") &&
        !response.url().endsWith("/api/tools/opencut_audio_preview_create")) return;
    const request = response.request().postDataJSON();
    const operation = (async () => {
      const envelope = await response.json();
      const at = await page.evaluate(() => performance.now());
      const session = envelope.data;
      if (envelope.error && (request.project_id === projectId ||
          observations.some(value => value.session.session_id === request.session_id))) {
        if (evidence.audio_errors.length < 32) evidence.audio_errors.push({
          operation: response.url().endsWith("/opencut_audio_preview_create") ? "create" : request.command?.kind ?? "control",
          code: envelope.error.code,
          // Do not persist transport credentials, inputs, locators or host paths.
          message: redact(envelope.error.message ?? "").slice(0, 256),
          session_id: request.session_id ?? null,
        });
      }
      if (!envelope.error && sameFixedIdentity(session)) {
        observations.push({ request, session, at });
        evidence.audio_last_session = summarizeSession(session);
        if (request.project_id === projectId && evidence.audio_create_sessions.length < 4)
          evidence.audio_create_sessions.push(summarizeSession(session));
      }
    })().catch(error => { observerFailure ??= String(error); }).finally(() => pending.delete(operation));
    pending.add(operation);
  };
  page.on("request", observeRequest);
  page.on("requestfailed", observeFailedRequest);
  page.on("console", observeConsole);
  page.on("response", observe);
  try {
    const openStart = performance.now(); // Before navigation, includes network, route and aggregate read.
    await page.goto(new URL(`/editor/${encodeURIComponent(projectId)}?scene=${encodeURIComponent(sceneId)}`, page.url()).href);
    await expect(page.locator(".editor-timeline-element")).toHaveCount(100);
    await expect(page.locator(".editor-track-name")).toHaveCount(5);
    await expect(page.locator('[aria-label="Timeline editing"]').getByRole("button", { name: "Add track", exact: true })).toBeEnabled();
    evidence.open_ms = performance.now() - openStart;
    heaps.push(await heap());
    warmRetained = await retainedHeap(); // Cold-open timing ends before explicit measurement GC.
    await page.evaluate(() => {
      const metrics = window.v4DenseMetrics = { inputs: [], tasks: [], raf_gaps: [], actions: [], lastRaf: null, running: true };
      const observer = new PerformanceObserver(list => {
        for (const entry of list.getEntries()) if (metrics.tasks.length < 10000) metrics.tasks.push({ start: entry.startTime, duration: entry.duration });
      });
      observer.observe({ type: "longtask", buffered: false });
      const input = event => {
        if (!event.isTrusted || !event.target.closest?.(".editor-timeline-panel")) return;
        const at = performance.now();
        requestAnimationFrame(() => requestAnimationFrame(() => {
          if (metrics.inputs.length < 1000) metrics.inputs.push({ type: event.type, at, two_raf_ms: performance.now() - at });
        }));
      };
      const action = event => {
        if (!event.isTrusted || !event.target.closest?.("button")) return;
        const text = event.target.closest("button").textContent;
        if (["Play scene audio", "Go to time", "Pause scene audio"].includes(text)) metrics.actions.push({ text, at: performance.now() });
      };
      for (const type of ["keydown", "pointerdown", "input"]) document.addEventListener(type, input, true);
      document.addEventListener("click", action, true);
      const tick = at => { if (!metrics.running) return; if (metrics.lastRaf !== null && metrics.raf_gaps.length < 20000) metrics.raf_gaps.push(at - metrics.lastRaf); metrics.lastRaf = at; requestAnimationFrame(tick); };
      requestAnimationFrame(tick);
      metrics.stop = () => { metrics.running = false; observer.disconnect();
        for (const type of ["keydown", "pointerdown", "input"]) document.removeEventListener(type, input, true); document.removeEventListener("click", action, true); };
    });
    const zoom = page.getByRole("slider", { name: "Timeline zoom", exact: true });
    await zoom.focus();
    for (let index = 0; index < 20; index++) await zoom.press(index < 10 ? "ArrowRight" : "ArrowLeft");
    // Field wraps a select: its label text also includes the option text.
    await page.getByRole("combobox", { name: /^Audio preview sample rate(?:\s|$)/ }).selectOption("48000");
    const play = page.getByRole("button", { name: "Play scene audio", exact: true });
    await expect(play).toBeEnabled(); await play.click();
    const matches = session => Object.entries(fixedIdentity).every(([key, value]) => session.identity[key] === value);
    const consumed = (sessionId, generation, epoch) => observations.filter(value => value.request.command?.kind === "report_consumed" &&
      value.session.session_id === sessionId && value.session.generation === generation && value.session.overlay_epoch === epoch &&
      matches(value.session) && BigInt(value.session.consumed_frames) > 0n);
    const firstConsumeDeadline = performance.now() + 30000;
    while (!observations.some(value => matches(value.session) && value.request.command?.kind === "report_consumed" && BigInt(value.session.consumed_frames) > 0n)) {
      const failed = evidence.audio_errors.find(value => value.operation === "create");
      if (failed) throw new Error(`Actual audio create failed: ${failed.code}: ${failed.message}`);
      const outputError = (await page.locator(".editor-audio-preview [role=alert]").allTextContents()).join("\n");
      if (outputError) throw new Error(`Actual audio output failed: ${redact(outputError).slice(0, 512)}`);
      if (performance.now() >= firstConsumeDeadline) throw new Error("No successful fixed-revision ReportConsumed within the original 30-second wait.");
      await page.waitForTimeout(100);
    }
    const first = observations.find(value => matches(value.session) && value.request.command?.kind === "report_consumed" && BigInt(value.session.consumed_frames) > 0n);
    expect(first.session.total_frames).toBe("14400000");
    const actionAt = async text => page.evaluate(text => window.v4DenseMetrics.actions.filter(value => value.text === text).at(-1).at, text);
    evidence.cold_consume_ms = first.at - await actionAt("Play scene audio");
    // Steady window: actual service-confirmed reports from this generation only.
    const steadyDeadline = performance.now() + 20000;
    while (consumed(first.session.session_id, first.session.generation, first.session.overlay_epoch).at(-1).at - first.at < 10000) {
      heaps.push(await heap());
      const outputError = (await page.locator(".editor-audio-preview [role=alert]").allTextContents()).join("\n");
      if (outputError) throw new Error(`Actual audio output failed: ${redact(outputError).slice(0, 512)}`);
      if (performance.now() >= steadyDeadline) throw new Error("No ten-second confirmed consumer window within the original 20-second wait.");
      await page.waitForTimeout(250);
    }
    const steady = consumed(first.session.session_id, first.session.generation, first.session.overlay_epoch);
    const windowReports = steady.filter(value => value.at <= first.at + 10000);
    let stalledMs = 0;
    for (let index = 1; index < windowReports.length; index++) if (windowReports[index - 1].request.command.stalled)
      stalledMs += windowReports[index].at - windowReports[index - 1].at;
    evidence.consumer_window = { observed_ms: windowReports.at(-1).at - first.at,
      confirmed_frames: String(BigInt(windowReports.at(-1).session.consumed_frames) - BigInt(first.session.consumed_frames)),
      reports: windowReports.length, stalled_observation_ratio: stalledMs / (windowReports.at(-1).at - first.at),
      method: "Time-weighted Worklet stalled observations in accepted Rust ReportConsumed responses; not an acoustic/device clock measurement." };
    evidence.consumer_window.confirmed_consumer_deficit_ratio = Math.max(0,
      1 - Number(evidence.consumer_window.confirmed_frames) / 48000 /
      (evidence.consumer_window.observed_ms / 1000));
    evidence.consumer_window.deficit_method = "Confirmed Worklet sample advancement against response-arrival elapsed time; includes report delivery jitter and never drives the UI playhead.";
    evidence.seeks = [];
    for (let index = 0; index < 20; index++) {
      const before = observations.length;
      // This is a UI scene-time intention. Rust owns floor/alignment/source clocks.
      const intendedSeconds = (296 * index / 19 + .25).toFixed(6);
      await page.getByLabel("Playhead (seconds)", { exact: true }).fill(intendedSeconds);
      await page.getByRole("button", { name: "Go to time", exact: true }).click();
      await expect.poll(() => observations.slice(before).some(value => value.request.command?.kind === "seek" && matches(value.session)), { timeout: 10000 }).toBe(true);
      const sought = observations.slice(before).find(value => value.request.command?.kind === "seek" && matches(value.session));
      // Independent decimal intention oracle: do not accept a new generation
      // that merely sought the old consumer clock instead of the typed value.
      const intendedMicros = BigInt(intendedSeconds.replace(".", ""));
      const requested = sought.request.command.sequence_time;
      expect(BigInt(requested.ticks) * 1000000n).toBe(intendedMicros * BigInt(requested.timebase));
      const expectedStartSample = intendedMicros * 48000n / 1000000n;
      expect(sought.session.actual_start_sample).toBe(String(expectedStartSample));
      expect(sought.session.session_id).toBe(first.session.session_id);
      expect(sought.session.generation).not.toBe(first.session.generation);
      await expect.poll(() => consumed(sought.session.session_id, sought.session.generation, sought.session.overlay_epoch).length, { timeout: 10000 }).toBeGreaterThan(0);
      const resumed = consumed(sought.session.session_id, sought.session.generation, sought.session.overlay_epoch)[0];
      const latencyMs = resumed.at - await actionAt("Go to time");
      const usedHeap = await heap(); heaps.push(usedHeap);
      const retainedBytes = await retainedHeap(); // After the actual consumption latency; never inside the steady window.
      evidence.seeks.push({ index, session_id: sought.session.session_id, generation: sought.session.generation,
        intended_seconds: intendedSeconds, expected_start_sample: String(expectedStartSample),
        overlay_epoch: sought.session.overlay_epoch, actual_start_sample: sought.session.actual_start_sample,
        presentation_time: resumed.session.presentation_time, consumed_frames: resumed.session.consumed_frames,
        latency_ms: latencyMs, heap_bytes: usedHeap, retained_heap_bytes: retainedBytes });
    }
    for (const observation of observations.filter(value => matches(value.session))) expect(BigInt(observation.session.buffered_frames)).toBeLessThanOrEqual(8192n);
    await page.getByRole("button", { name: "Pause scene audio", exact: true }).click();
    await expect(play).toBeEnabled();
    evidence.pointer_selection = [];
    // Sample early/middle/final clips and different tracks after the retained
    // measurement. Real physical hit tests include both sticky labels and every
    // clipping ancestor; keyboard zoom alone does not establish dense hitability.
    for (const [trackIndex, index] of [[0, 0], [2, 10], [4, 19]]) {
      const id = dense.tracks[trackIndex].elements[index].id;
      const clip = page.locator(`.editor-timeline-element[data-element-id="${id}"]`);
      const button = clip.locator(".editor-element-select");
      await button.scrollIntoViewIfNeeded();
      const point = await button.evaluate(button => {
        const box = button.getBoundingClientRect();
        const visible = { left: Math.max(0, box.left), right: Math.min(innerWidth, box.right),
          top: Math.max(0, box.top), bottom: Math.min(innerHeight, box.bottom) };
        const clips = [];
        for (let parent = button.parentElement; parent; parent = parent.parentElement) {
          const style = getComputedStyle(parent), rect = parent.getBoundingClientRect();
          const left = rect.left + parent.clientLeft, top = rect.top + parent.clientTop;
          if (/(auto|scroll|hidden|clip)/.test(style.overflowX)) {
            visible.left = Math.max(visible.left, left); visible.right = Math.min(visible.right, left + parent.clientWidth);
          }
          if (/(auto|scroll|hidden|clip)/.test(style.overflowY)) {
            visible.top = Math.max(visible.top, top); visible.bottom = Math.min(visible.bottom, top + parent.clientHeight);
          }
          if (/(auto|scroll|hidden|clip)/.test(`${style.overflowX} ${style.overflowY}`))
            clips.push({ class: parent.className, left, top, width: parent.clientWidth, height: parent.clientHeight });
        }
        const label = button.closest(".editor-track")?.querySelector(".editor-track-name").getBoundingClientRect();
        if (label) visible.left = Math.max(visible.left, label.right);
        const x = (visible.left + visible.right) / 2, y = (visible.top + visible.bottom) / 2;
        const hit = document.elementFromPoint(x, y);
        return { x, y, visible, box: { left: box.left, top: box.top, width: box.width, height: box.height },
          clips, hit: !!hit && button.contains(hit), hit_class: hit?.className };
      });
      expect(point.visible.right, `Dense clip has no visible horizontal intersection: ${JSON.stringify(point)}`).toBeGreaterThan(point.visible.left);
      expect(point.visible.bottom, `Dense clip has no visible vertical intersection: ${JSON.stringify(point)}`).toBeGreaterThan(point.visible.top);
      expect(point.hit, `Dense pointer selection is occluded: ${JSON.stringify(point)}`).toBe(true);
      await page.mouse.move(point.x, point.y); await page.mouse.down(); await page.mouse.up();
      await expect(button).toHaveAttribute("aria-pressed", "true");
      await expect(clip).toHaveAttribute("data-selected", "true");
      evidence.pointer_selection.push({ element_id: id, track_index: trackIndex, clip_index: index, point });
    }
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    heaps.push(await heap());
    const metrics = await page.evaluate(() => {
      const value = window.v4DenseMetrics; value.stop();
      return { inputs: value.inputs, tasks: value.tasks, raf_gaps: value.raf_gaps };
    });
    const sorted = metrics.inputs.map(input => input.two_raf_ms).sort((a, b) => a - b);
    expect(sorted.length).toBeGreaterThanOrEqual(20);
    evidence.input = { samples: sorted.length, p95_ms: sorted[Math.ceil(sorted.length * .95) - 1], max_ms: sorted.at(-1), method: "Trusted timeline input capture through two requestAnimationFrame callbacks; includes frame scheduling, not presentation scanout." };
    evidence.long_tasks = metrics.tasks; evidence.raf = { samples: metrics.raf_gaps.length, max_gap_ms: Math.max(...metrics.raf_gaps) };
    const retained = evidence.seeks.map(value => value.retained_heap_bytes);
    const mean = values => values.reduce((sum, value) => sum + value, 0) / values.length;
    const firstMean = mean(retained.slice(0, 5)), lastMean = mean(retained.slice(-5));
    const xMean = (retained.length - 1) / 2, yMean = mean(retained);
    const slope = retained.reduce((sum, value, index) => sum + (index - xMean) * (value - yMean), 0) /
      retained.reduce((sum, _, index) => sum + (index - xMean) ** 2, 0);
    const growth = lastMean - firstMean;
    evidence.heap = { baseline_bytes: heapBefore, warm_retained_baseline_bytes: warmRetained,
      peak_bytes: Math.max(...heaps), delta_bytes: Math.max(...heaps) - heapBefore,
      seek_series_bytes: evidence.seeks.map(value => value.heap_bytes), retained_seek_series_bytes: retained,
      retained_first_five_mean_bytes: firstMean, retained_last_five_mean_bytes: lastMean,
      retained_first_last_growth_bytes: growth, retained_ols_slope_bytes_per_seek: slope,
      source: "CDP Performance.JSHeapUsedSize; raw samples before measurement GC, retained samples after HeapProfiler.collectGarbage",
      leak_status: growth <= budget.retained_first_last_growth_bytes && slope <= budget.retained_slope_bytes_per_seek ? "passed" : "failed",
      method: "Explicit GC at library baseline, after timed dense open, and after each measured successful consumption. No GC during the ten-second steady window. Twenty retained samples: mean(last five)-mean(first five) and ordinary-least-squares slope against seek index 0..19.",
      scope: "Local twenty-seek retained-growth check; 8MiB mean-growth and 256KiB/seek slope tolerate measurement noise, supplementing rather than replacing the original 64MiB raw incremental peak budget. Does not prove unbounded-duration leak freedom." };
    evidence.create_count = observations.filter(value => value.request.project_id === projectId).length;
    expect(evidence.create_count).toBe(1);
    expect(observerFailure).toBeUndefined();
    expect((await editorSnapshot(tool("opencut_project_inspect", { project_id: projectId, revision: null }))).revision).toBe(receipt.revision);
    const failures = [];
    for (const [name, actual, ceiling] of [
      ["open", evidence.open_ms, budget.open_ms], ["input p95", evidence.input.p95_ms, budget.input_p95_ms],
      ["input max", evidence.input.max_ms, budget.input_max_ms], ["cold consumption", evidence.cold_consume_ms, budget.cold_consume_ms],
      ["reported stall", evidence.consumer_window.stalled_observation_ratio, budget.stall_ratio],
      ["confirmed consumer deficit", evidence.consumer_window.confirmed_consumer_deficit_ratio, budget.stall_ratio],
      ["incremental heap", evidence.heap.delta_bytes, budget.heap_delta_bytes],
      ["retained first/last five growth", growth, budget.retained_first_last_growth_bytes],
      ["retained OLS slope", slope, budget.retained_slope_bytes_per_seek],
      ...evidence.seeks.map(value => [`seek ${value.index}`, value.latency_ms, budget.warm_seek_ms]),
    ]) if (actual > ceiling) failures.push({ metric: name, actual, ceiling });
    evidence.ui_status = failures.length ? "failed" : "passed";
    if (failures.length) throw new Error(`Frozen dense-performance thresholds exceeded: ${JSON.stringify({ failures, evidence })}`);
    check("dense ordinary 100 clips/5 tracks/300 seconds: cold route, trusted input→2rAF, CDP heap and 20 fixed-revision real Worklet seeks", { v4_performance: evidence });
    return evidence;
  } catch (error) {
    evidence.audio_ui = await page.evaluate(() => {
      const preview = document.querySelector(".editor-audio-preview");
      return [...(preview?.querySelectorAll('[role="status"], [role="alert"]') ?? [])]
        .map(node => node.textContent ?? "").join("\n").slice(0, 2048);
    }).catch(() => "Audio UI unavailable after failure.");
    const created = evidence.audio_create_sessions.at(-1);
    if (created) try {
      const session = await tool("opencut_audio_preview_inspect", { session_id: created.session_id });
      if (sameFixedIdentity(session) && session.session_id === created.session_id)
        evidence.audio_failure_inspection = { status: "observed", session: summarizeSession(session) };
      else evidence.audio_failure_inspection = { status: "identity_mismatch" };
    } catch (inspectionError) {
      evidence.audio_failure_inspection = { status: "unavailable", error: redact(inspectionError).slice(0, 512) };
    }
    else evidence.audio_failure_inspection = { status: "no_successful_create_observed" };
    if (error instanceof Error && !error.message.includes('"evidence":')) error.message += `\nDense performance observations: ${JSON.stringify(evidence)}`;
    throw error;
  } finally {
    page.off("request", observeRequest); page.off("requestfailed", observeFailedRequest);
    page.off("console", observeConsole); page.off("response", observe); await Promise.allSettled([...pending]);
    await page.evaluate(() => { window.v4DenseMetrics?.stop?.(); delete window.v4DenseMetrics; }).catch(() => {});
    await cdp.detach();
  }
}
