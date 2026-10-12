import { expect } from "@playwright/test";

// Exercise authored gestures through the authenticated product, then reuse the
// caller's existing source-audio/WAV chain as independent actual PCM evidence.
export async function verifyV4AudioGain(page, tool, writes, projectId, sceneId, element, check) {
  const inspect = () => tool("opencut_project_v4_inspect", { project_id: projectId, revision: null });
  await page.getByLabel("V4 scene", { exact: true }).selectOption(sceneId);
  const slider = page.getByRole("slider", { name: `Volume ${element.name}`, exact: true });
  const clip = page.locator(".v4-timeline-element").filter({ has: slider });
  const surface = clip.locator(".v4-gain-surface");
  await expect(slider).toBeVisible();
  await expect(slider).toHaveAttribute("aria-valuenow", "0");
  const fixed = await inspect(), count = writes.length;
  const begin = async () => {
    const box = await slider.boundingBox();
    expect(box.height).toBe(14);
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
  };
  const move = async (db) => {
    const box = await surface.boundingBox();
    const gain = 10 ** (db / 20), y = 1 - Math.sqrt((gain - .001) / 9.999);
    await page.mouse.move(box.x + box.width / 2, box.y + box.height * y);
    await expect(slider).toHaveAttribute("aria-valuenow", String(db));
  };
  // Physical pointer capture, local draft and Escape cancellation cannot publish.
  await begin(); await move(-12);
  expect((await inspect()).revision).toBe(fixed.revision);
  expect(writes).toHaveLength(count);
  await page.keyboard.press("Escape"); await page.mouse.up();
  await expect(slider).toHaveAttribute("aria-valuenow", "0");
  expect((await inspect()).revision).toBe(fixed.revision);
  expect(writes).toHaveLength(count);
  await begin(); await page.mouse.up();
  expect((await inspect()).revision).toBe(fixed.revision);
  expect(writes).toHaveLength(count);

  const save = async (action, command) => {
    await expect(slider).toHaveAttribute("aria-disabled", "false");
    const before = await inspect(), written = writes.length;
    const [response, appliedResponse] = await Promise.all([
      page.waitForResponse(r => r.url().endsWith("/v1/tools/opencut_project_v4_audio_gain_plan")),
      page.waitForResponse(r => r.url().endsWith("/v1/tools/opencut_edit_v4_apply")),
      action(),
    ]).catch(async error => {
      throw new Error(`${error}\nVolume status: ${await page.locator(".v4-gain-status").innerText()}`);
    });
    const plan = await response.json();
    expect(plan.error).toBeNull();
    expect(response.request().postDataJSON()).toMatchObject({ project_id: projectId,
      expected_revision: before.revision, sequence_id: sceneId, element_id: element.id, command });
    const applied = await appliedResponse.json(); expect(applied.error).toBeNull();
    expect(plan.data.transaction.operations).toHaveLength(1);
    expect(writes).toHaveLength(written + 1);
    expect(writes.at(-1)).toEqual(plan.data.transaction);
    await expect.poll(async () => (await inspect()).revision).toBe(applied.data.revision);
    await expect(page.locator(".v4-gain-status")).toContainText("Element audio changes saved.");
    await expect(slider).toHaveAttribute("aria-disabled", "false");
    await expect(slider).toHaveAttribute("aria-valuenow", String(plan.data.decision.new_gain.value));
    await expect(clip.getByRole("button", { name: `${plan.data.decision.element_muted ? "Unmute" : "Mute"} ${element.name}`, exact: true })).toBeVisible();
    return { plan: plan.data, receipt: applied.data };
  };
  await save(() => slider.press("Home"), { kind: "set_decibels", decibels_tenths: -600 });
  await expect(slider).toHaveAttribute("aria-valuenow", "-60");
  await save(() => slider.press("End"), { kind: "set_decibels", decibels_tenths: 200 });
  await expect(slider).toHaveAttribute("aria-valuenow", "20");
  await save(() => slider.press("ArrowLeft"), { kind: "set_decibels", decibels_tenths: 199 });
  await expect(slider).toHaveAttribute("aria-valuenow", "19.9");
  await begin(); await move(-6);
  const saved = await save(() => page.mouse.up(), { kind: "set_decibels", decibels_tenths: -60 });
  const after = await inspect(), expected = structuredClone(fixed.project);
  const target = expected.sequences.find(s => s.id === sceneId).tracks.flatMap(t => t.elements).find(e => e.id === element.id);
  target.content.audio.gain = { unit: "decibels", value: -6 };
  expect(after.project).toEqual(expected);
  await save(() => clip.getByRole("button", { name: `Mute ${element.name}`, exact: true }).click(), { kind: "set_muted", muted: true });
  let actual = await inspect();
  const muted = actual.project.sequences.find(s => s.id === sceneId).tracks.flatMap(t => t.elements).find(e => e.id === element.id);
  expect(muted.content.audio).toMatchObject({ gain: { unit: "decibels", value: -6 }, muted: true });
  await save(() => clip.getByRole("button", { name: `Unmute ${element.name}`, exact: true }).click(), { kind: "set_muted", muted: false });
  expect((await inspect()).project).toEqual(expected);
  // Forward one actual commit to the service, then drop only its response.
  // This exercises receipt recovery without fabricating a server response.
  const applyRoute = "**/v1/tools/opencut_edit_v4_apply";
  let dropped;
  await page.route(applyRoute, async route => {
    const response = await route.fetch();
    dropped = { request: route.request().postDataJSON(), envelope: await response.json() };
    await route.abort("failed");
  }, { times: 1 });
  try {
    await slider.press("ArrowRight");
    await expect(page.locator(".v4-gain-status")).toContainText("Volume save is unconfirmed. Retry the original operation to recover its receipt.");
    expect(dropped.envelope.error).toBeNull();
    expect((await inspect()).revision).toBe(dropped.envelope.data.revision);
    await page.unroute(applyRoute);
    const writesBeforeRecovery = writes.length;
    const [response] = await Promise.all([
      page.waitForResponse(r => r.url().endsWith("/v1/tools/opencut_edit_recovery_v2_inspect") &&
        r.request().postDataJSON().project_id === projectId),
      page.getByRole("button", { name: "Retry original volume operation", exact: true }).click(),
    ]).catch(async error => {
      throw new Error(`${error}\nVolume status: ${await page.locator(".v4-gain-status").innerText()}`);
    });
    const recovered = await response.json();
    expect(recovered.error).toBeNull();
    expect(recovered.data.record.kind).toBe("current_v2");
    expect(recovered.data.record.record.context.intent).toEqual({ kind: "public_apply", request: dropped.request });
    expect(recovered.data.record.record.receipt).toEqual({ kind: "public_apply", transaction: dropped.envelope.data });
    expect((await inspect()).revision).toBe(dropped.envelope.data.revision);
    await expect(slider).toHaveAttribute("aria-valuenow", "-5.9");
    await expect(page.locator(".v4-gain-status")).toContainText("Element audio changes saved.");
    expect(writes).toHaveLength(writesBeforeRecovery);
    await save(() => slider.press("ArrowLeft"), { kind: "set_decibels", decibels_tenths: -60 });
    expect((await inspect()).project).toEqual(expected);
  } finally {
    await page.unroute(applyRoute);
  }
  await page.getByRole("button", { name: "View revision", exact: true }).first().click();
  await expect(slider).toHaveAttribute("aria-disabled", "true");
  await expect(clip.getByRole("button", { name: `Mute ${element.name}`, exact: true })).toBeDisabled();
  await page.getByRole("button", { name: "Return to current", exact: true }).click();
  await page.reload();
  await page.getByLabel("V4 scene", { exact: true }).selectOption(sceneId);
  await expect(slider).toHaveAttribute("aria-valuenow", "-6");
  expect((await inspect()).project).toEqual(expected);
  check("real V4 gain-line pointer draft/Escape/unchanged click write no revision; Home/End/0.1dB keyboard and pointerup each save one exact fixed-head plan; mute retains gain, actual lost response reads the original receipt without another apply or revision; saved revision is read-only and reload preserves all other fields", {
    gain_line: { element_id: element.id, final_decibels: -6, drag_transaction_id: saved.receipt.transaction_id,
      min_decibels: -60, max_decibels: 20, step_decibels: .1, hitbox_pixels: 14,
      video_entry: "explicit product extension", sound_preview_during_drag: false,
      actual_lost_response_recovered: true, recovered_transaction_id: dropped.envelope.data.transaction_id }
  });
}
