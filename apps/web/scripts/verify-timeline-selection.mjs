import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { execFileSync } from "node:child_process";

// Real UI + shared application transactions. Synthetic fixture, separate project/page.
export async function verifyTimelineSelection(
  browser,
  endpoint,
  directory,
  check,
) {
  const page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1000 },
  });
  page.setDefaultTimeout(60000);
  const errors = [],
    calls = [],
    checks = [],
    uiWrites = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("response", async (response) => {
    const name = new URL(response.url()).pathname.split("/").at(-1);
    if (
      ![
        "opencut_edit_v3_apply",
        "opencut_project_edit_apply",
        "opencut_edit_v3_undo_apply",
        "opencut_edit_v3_redo_apply",
      ].includes(name)
    )
      return;
    try {
      uiWrites.push({
        name,
        input: response.request().postDataJSON(),
        status: response.status(),
        envelope: await response.json(),
      });
    } catch {}
  });
  const tool = async (name, input) => {
    const result = await page.evaluate(
      async ({ name, input }) => {
        const response = await fetch(`/v1/tools/${name}`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(input),
        });
        return { status: response.status, envelope: await response.json() };
      },
      { name, input },
    );
    calls.push({ name, input, ...result });
    return result.envelope;
  };
  const inspect = async () => {
    const reply = await tool("opencut_project_v3_inspect", {
      project_id: projectId,
      revision: null,
    });
    expect(reply.error).toBeNull();
    return reply.data;
  };
  const write = async (
    operations,
    revision,
    label = "S1 acceptance fixture",
  ) => {
    const head = await inspect();
    return tool("opencut_edit_v3_apply", {
      project_id: projectId,
      expected_revision: revision ?? head.revision,
      idempotency_key: crypto.randomUUID(),
      label,
      operations,
    });
  };
  const base = (operation) => ({ type: "base.apply", operation });
  const note = (message) => {
    checks.push(message);
    check(message);
  };
  let projectId;
  try {
    await page.goto(`${endpoint}/editor`);
    await page
      .getByRole("button", { name: "New project", exact: true })
      .click();
    await page.getByLabel("Project name").fill("S1 selection acceptance");
    await page
      .getByRole("button", { name: "Create project", exact: true })
      .click();
    await expect(
      page.getByRole("region", { name: "Timeline", exact: true }),
    ).toBeVisible();
    const guide = page.locator(".editor-guide");
    await expect(guide).toBeVisible();
    await guide.getByRole("button", { name: "Next", exact: true }).click();
    await guide.getByRole("button", { name: "Next", exact: true }).click();
    await guide.getByRole("button", { name: "Finish", exact: true }).click();
    await page
      .getByRole("button", { name: "Dismiss update", exact: true })
      .click();
    projectId = new URL(page.url()).pathname.split("/")[2];
    await page
      .getByLabel("Media files", { exact: true })
      .first()
      .setInputFiles(path.join(directory, "sample.mp4"));
    await page
      .getByRole("button", { name: "Add sample.mp4 to timeline", exact: true })
      .click();
    const clips = page.getByRole("button", {
      name: "Clip sample.mp4",
      exact: true,
    });
    await expect(clips).toHaveCount(1);
    await clips.first().click();
    await page.keyboard.press("ControlOrMeta+d");
    await expect(clips).toHaveCount(2);
    await page.keyboard.press("ControlOrMeta+d");
    await expect(clips).toHaveCount(3);
    await page.getByRole("button", { name: "Track", exact: true }).click();
    await expect
      .poll(
        async () => (await inspect()).project.base.sequences[0].tracks.length,
      )
      .toBe(2);
    await page.getByRole("button", { name: "Track", exact: true }).click();
    await expect
      .poll(
        async () => (await inspect()).project.base.sequences[0].tracks.length,
      )
      .toBe(3);
    let before = await inspect();
    const sequence = before.project.base.sequences[0],
      [source, destination, locked] = sequence.tracks;
    const first = source.clips[0],
      second = source.clips[1];
    const firstElement = page.locator(`[data-clip-id="${first.id}"]`),
      secondElement = page.locator(`[data-clip-id="${second.id}"]`);
    await firstElement.click();
    await secondElement.click({ modifiers: ["ControlOrMeta"] });
    await expect(page.locator("[data-clip-id][aria-pressed=true]")).toHaveCount(
      2,
    );
    await clips.nth(2).click({ modifiers: ["Shift"] });
    await expect(page.locator("[data-clip-id][aria-pressed=true]")).toHaveCount(
      2,
    );
    await page.keyboard.press("ControlOrMeta+a");
    await expect(page.locator("[data-clip-id][aria-pressed=true]")).toHaveCount(
      3,
    );
    await page.keyboard.press("Escape");
    await expect(page.locator("[data-clip-id][aria-pressed=true]")).toHaveCount(
      0,
    );
    const a = await firstElement.boundingBox(),
      b = await secondElement.boundingBox();
    await page.mouse.move(a.x + 1, a.y - 3);
    await page.mouse.down();
    await page.mouse.move(b.x + b.width - 1, b.y + b.height + 3, { steps: 6 });
    await page.mouse.up();
    await expect(page.locator("[data-clip-id][aria-pressed=true]")).toHaveCount(
      2,
    );
    expect((await inspect()).project).toEqual(before.project);
    note(
      "S1: modifier/range/select-all/marquee selection shares one set and does not edit project content",
    );
    const lane = await page
      .locator(`[data-track-id="${destination.id}"]`)
      .boundingBox();
    await page.mouse.move(a.x + 20, a.y + 20);
    await page.mouse.down();
    await page.mouse.move(a.x + 60, lane.y + 20, { steps: 6 });
    await page.mouse.up();
    await expect
      .poll(
        async () =>
          (await inspect()).project.base.sequences[0].tracks[1].clips.length,
      )
      .toBe(2);
    const moved = await inspect();
    const movedClips = moved.project.base.sequences[0].tracks[1].clips;
    const seconds = (t) => Number(t.ticks) / t.timebase;
    expect(seconds(movedClips[0].timeline_range.start)).toBe(0.5);
    expect(
      seconds(movedClips[1].timeline_range.start) -
        seconds(movedClips[0].timeline_range.start),
    ).toBe(4);
    expect(
      moved.project.base.sequences[0].tracks[0].clips.map((c) => c.id),
    ).toEqual([source.clips[2].id]);
    const history = await tool("opencut_project_history_navigation", {
      project_id: projectId,
      revision: moved.revision,
    });
    expect(history.error).toBeNull();
    expect(history.data.undo.label).toBe("Move clips");
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(before.project);
    await page.getByRole("button", { name: "Redo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(moved.project);
    note(
      "S1: two-clip cross-track drag is one shared transaction with exact half-second displacement and full document undo/redo",
    );
    await page
      .getByRole("button", { name: `Lock ${locked.name}`, exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: `Unlock ${locked.name}`, exact: true }),
    ).toBeVisible();
    const lockedHead = await inspect(),
      lockedLane = await page
        .locator(`[data-track-id="${locked.id}"]`)
        .boundingBox();
    await firstElement.click();
    await secondElement.click({ modifiers: ["ControlOrMeta"] });
    const now = await firstElement.boundingBox();
    await page.mouse.move(now.x + 20, now.y + 20);
    await page.mouse.down();
    await page.mouse.move(now.x + 100, lockedLane.y + 20, { steps: 6 });
    await page.mouse.up();
    await expect(page.getByRole("alert")).toContainText("Unlock");
    expect((await inspect()).revision).toBe(lockedHead.revision);
    await page.mouse.move(now.x + 20, now.y + 20);
    await page.mouse.down();
    await page.mouse.move(now.x + 80, now.y + 20, { steps: 4 });
    await page.keyboard.press("Escape");
    await page.mouse.up();
    expect((await inspect()).revision).toBe(lockedHead.revision);
    note(
      "S1: locked destination rejects the entire group; Escape cancels a live drag without creating a revision",
    );
    const operations = movedClips.map((clip) =>
      base({
        type: "clip.move",
        sequence_id: sequence.id,
        track_id: destination.id,
        clip_id: clip.id,
        destination_track_id: locked.id,
        start: clip.timeline_range.start,
      }),
    );
    operations[0].operation.destination_track_id = source.id;
    const denied = await write(operations);
    expect(denied.error.code).toBe("CONSTRAINT_VIOLATION");
    expect((await inspect()).project).toEqual(lockedHead.project);
    const stale = await write(operations, before.revision);
    expect(stale.error.code).toBe("REVISION_CONFLICT");
    expect((await inspect()).project).toEqual(lockedHead.project);
    note(
      "S1: real shared endpoint rejects locked and stale-revision group requests without partially committing",
    );
    // FPS form identity after an external writer, without a page reload.
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    const update = await write([
      base({
        type: "sequence.update_settings",
        sequence_id: sequence.id,
        fps: { numerator: 30, denominator: 1 },
      }),
    ]);
    expect(update.error).toBeNull();
    await expect(
      page
        .getByRole("region", { name: "Settings", exact: true })
        .getByRole("combobox", { name: "Frame rate", exact: true }),
    ).toHaveValue("30/1");
    await page
      .getByRole("region", { name: "Settings", exact: true })
      .getByRole("button", { name: "Apply settings", exact: true })
      .click();
    await expect
      .poll(async () => (await inspect()).project.base.sequences[0].fps)
      .toEqual({ numerator: 30, denominator: 1 });
    note(
      "AUDIT-02: real Settings form follows an external FPS change and applies 30 FPS",
    );
    // Copy snapshot survives effect modification and source deletion in the service.
    const key = crypto.randomUUID(),
      keyEnd = crypto.randomUUID(),
      effectId = crypto.randomUUID(),
      maskId = crypto.randomUUID();
    const fixture = await write([
      base({
        type: "keyframe.channel_set",
        sequence_id: sequence.id,
        track_id: destination.id,
        clip_id: first.id,
        channel: {
          parameter: "opacity",
          keyframes: [
            {
              id: key,
              time: { ticks: "0", timebase: 1 },
              value: 1,
              interpolation: "linear",
            },
            {
              id: keyEnd,
              time: first.timeline_range.duration,
              value: 0.5,
              interpolation: "linear",
            },
          ],
        },
      }),
      {
        type: "effect.put",
        sequence_id: sequence.id,
        effect: {
          id: effectId,
          target_id: first.id,
          effect: { kind: "gaussian_blur", radius_px: 12 },
        },
      },
      {
        type: "mask.put",
        sequence_id: sequence.id,
        mask: {
          id: maskId,
          target_id: first.id,
          shape: { kind: "rectangle", x: 0.1, y: 0.1, width: 0.8, height: 0.8 },
          inverted: false,
          feather: 0,
        },
      },
      {
        type: "keyframe.curve_set",
        sequence_id: sequence.id,
        curve: {
          id: crypto.randomUUID(),
          schema_version: 1,
          sequence_id: sequence.id,
          clip_id: first.id,
          parameter: "opacity",
          keyframe_id: key,
          curve: { x1: 0.1, y1: 0.2, x2: 0.8, y2: 0.9 },
        },
      },
    ]);
    expect(fixture.error).toBeNull();
    await page.reload();
    await expect(firstElement).toBeVisible();
    await firstElement.click();
    await page.keyboard.press("ControlOrMeta+c");
    const edited = await write(
      [
        {
          type: "effect.put",
          sequence_id: sequence.id,
          effect: {
            id: effectId,
            target_id: first.id,
            effect: { kind: "gaussian_blur", radius_px: 40 },
          },
        },
      ],
      undefined,
      "Clipboard source changed",
    );
    expect(edited.error).toBeNull();
    // Wait for the real periodic UI head refresh via its history entry.
    await page.getByRole("button", { name: "History", exact: true }).click();
    await expect(
      page.locator(".history-row").first().locator("strong"),
    ).toHaveText("Clipboard source changed");
    await page
      .locator(".ruler-content")
      .click({ position: { x: 14 * 80, y: 12 } });
    await page.keyboard.press("ControlOrMeta+v");
    await expect
      .poll(
        async () =>
          (await inspect()).project.base.sequences[0].tracks[1].clips.length,
      )
      .toBe(3);
    const pasted = await inspect();
    const added = pasted.project.base.sequences[0].tracks[1].clips.find(
      (clip) => !movedClips.some((old) => old.id === clip.id),
    );
    expect(
      pasted.project.visual[0].effects.find(
        (effect) => effect.target_id === added.id,
      ).effect.radius_px,
    ).toBe(12);
    expect(
      pasted.project.visual[0].masks.find((mask) => mask.target_id === added.id)
        .id,
    ).not.toBe(maskId);
    expect(
      pasted.project.keyframe_curves.find((curve) => curve.clip_id === added.id)
        .keyframe_id,
    ).toBe(added.channels[0].keyframes[0].id);
    expect(added.channels[0].keyframes[0].id).not.toBe(key);
    await page.locator(".timeline-scroll").evaluate((element) => {
      element.scrollLeft = 0;
    });
    await firstElement.click();
    await page.keyboard.press("Delete");
    await expect(
      page.locator(".history-row").first().locator("strong"),
    ).toHaveText("Delete clips");
    await expect(firstElement).toHaveCount(0);
    const removed = await inspect();
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(pasted.project);
    await page.getByRole("button", { name: "Redo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(removed.project);
    await expect(
      page.getByRole("button", { name: "Track", exact: true }),
    ).toBeEnabled();

    await page.locator(".timeline-scroll").evaluate((element) => {
      element.scrollLeft = 600;
    });
    await page
      .locator(".ruler-content")
      .click({ position: { x: 20 * 80, y: 12 } });
    await page.keyboard.press("ControlOrMeta+v");
    await expect
      .poll(
        async () =>
          (await inspect()).project.visual[0].effects.filter(
            (effect) =>
              effect.effect.kind === "gaussian_blur" &&
              effect.effect.radius_px === 12,
          ).length,
      )
      .toBe(2);
    note(
      "AUDIT-01: real clipboard preserves radius 12, mask and remapped curve/keyframe identities after source effect changes and atomic source deletion with full document undo/redo",
    );
    const previousMaskFrame = await page
      .getByAltText("Rendered project frame", { exact: true })
      .evaluateAll((images) => images[0]?.getAttribute("src") ?? null);
    await page
      .getByRole("button", { name: "Render frame", exact: true })
      .click();
    const image = page.getByAltText("Rendered project frame", { exact: true });
    await expect(image).toBeVisible({ timeout: 60000 });
    await expect(image).not.toHaveAttribute("src", previousMaskFrame ?? "", {
      timeout: 60000,
    });
    const png = path.join(directory, "s1-copy.png");
    const encoded = await image.evaluate(async (element) => {
      const response = await fetch(element.src);
      if (!response.ok) throw new Error("Frame bytes unavailable");
      const blob = await response.blob();
      return new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onerror = reject;
        reader.onload = () => resolve(String(reader.result).split(",")[1]);
        reader.readAsDataURL(blob);
      });
    });
    await fs.writeFile(png, Buffer.from(encoded, "base64"));
    const crop = (filter) =>
      execFileSync("ffmpeg", [
        "-v",
        "error",
        "-i",
        png,
        "-vf",
        filter + ",format=rgb24",
        "-frames:v",
        "1",
        "-f",
        "rawvideo",
        "pipe:1",
      ]);
    expect([...crop("crop=1:1:0:0")]).toEqual([0, 0, 0]);
    expect(Math.max(...crop("crop=1:1:480:270"))).toBeGreaterThan(20);
    note(
      "S1: copied mask compiles to a real native frame with independent expected black corner and colored interior pixels",
    );
    // F12: service history restores content; transaction-scoped client hints
    // restore only valid view IDs, including across a real browser reload.
    await page.locator(".timeline-scroll").evaluate((element) => {
      element.scrollLeft = 0;
    });
    const chosen = page.locator("[data-clip-id]");
    const chosenIds = await chosen.evaluateAll((items) =>
      items.slice(0, 2).map((item) => item.dataset.clipId),
    );
    await chosen.nth(0).click();
    await chosen.nth(1).click({ modifiers: ["ControlOrMeta"] });
    const beforeDelete = await inspect();
    await page.keyboard.press("Delete");
    await expect(page.locator("[data-clip-id][aria-pressed=true]")).toHaveCount(
      0,
    );
    const afterDelete = await inspect();
    expect(afterDelete.revision).not.toBe(beforeDelete.revision);
    await page.reload();
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(beforeDelete.project);
    await expect
      .poll(async () =>
        page
          .locator("[data-clip-id][aria-pressed=true]")
          .evaluateAll((items) =>
            items.map((item) => item.dataset.clipId).sort(),
          ),
      )
      .toEqual(chosenIds.sort());
    const undoHead = await inspect();
    await page.waitForTimeout(300);
    expect((await inspect()).revision).toBe(undoHead.revision); // view restoration writes no content
    await page.getByRole("button", { name: "Redo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(afterDelete.project);
    await expect(page.locator("[data-clip-id][aria-pressed=true]")).toHaveCount(
      0,
    );
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await expect(page.locator("[data-clip-id][aria-pressed=true]")).toHaveCount(
      2,
    );
    await page.getByRole("button", { name: "Track", exact: true }).click();
    await expect(
      page.getByRole("button", { name: "Redo", exact: true }),
    ).toBeDisabled();
    await expect(page.locator("[data-clip-id][aria-pressed=true]")).toHaveCount(
      2,
    );
    note(
      "F12: deleting two selected clips, reload, full-document undo/redo and new-edit redo truncation restores only valid selections without an extra content revision",
    );

    await page
      .getByRole("button", { name: "Manage scenes", exact: true })
      .click();
    let scenes = page.getByRole("dialog", { name: "Scenes", exact: true });
    await scenes
      .getByRole("button", { name: "New scene", exact: true })
      .click();
    await scenes.getByLabel("Scene name").fill("F12 restored scene");
    await scenes
      .getByRole("button", { name: "Create scene", exact: true })
      .click();
    await expect(
      page.getByLabel("Active sequence", { exact: true }),
    ).not.toHaveValue(sequence.id);
    const restoredSceneId = await page
      .getByLabel("Active sequence", { exact: true })
      .inputValue();
    await scenes.getByRole("button", { name: "Close", exact: true }).click();
    await page
      .getByRole("button", { name: "Add sample.mp4 to timeline", exact: true })
      .click();
    await expect(page.locator("[data-clip-id]")).toHaveCount(1);
    await page.locator("[data-clip-id]").click();
    const sceneClipId = await page
      .locator("[data-clip-id]")
      .getAttribute("data-clip-id");
    const beforeSceneDelete = await inspect();
    await page
      .getByRole("button", { name: "Manage scenes", exact: true })
      .click();
    scenes = page.getByRole("dialog", { name: "Scenes", exact: true });
    await scenes
      .getByLabel("Select scene F12 restored scene", { exact: true })
      .check();
    await scenes
      .getByRole("button", { name: "Delete selected (1)", exact: true })
      .click();
    await scenes
      .getByRole("button", { name: "Delete scenes", exact: true })
      .click();
    await expect(
      page.getByLabel("Active sequence", { exact: true }),
    ).toHaveValue(sequence.id);
    const afterSceneDelete = await inspect();
    await page.reload();
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(beforeSceneDelete.project);
    await expect(
      page.getByLabel("Active sequence", { exact: true }),
    ).toHaveValue(restoredSceneId);
    await expect(
      page.locator(`[data-clip-id="${sceneClipId}"]`),
    ).toHaveAttribute("aria-pressed", "true");
    await page.getByRole("button", { name: "Redo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(afterSceneDelete.project);
    await expect(
      page.getByLabel("Active sequence", { exact: true }),
    ).toHaveValue(sequence.id);
    await expect(page.locator("[data-clip-id][aria-pressed=true]")).toHaveCount(
      0,
    );
    note(
      "F12: delete active scene, fallback, reload, undo restores original scene and selection; redo restores fallback with full-document equality",
    );
    // Real file bytes through a browser drop into the existing chosen track.
    const mediaBytes = (
      await fs.readFile(path.join(directory, "sample.mp4"))
    ).toString("base64");
    const dropFiles = async (
      trackId,
      at,
      names,
      bytes = mediaBytes,
      type = "video/mp4",
    ) => {
      const lane = page.locator(`[data-track-id="${trackId}"]`);
      await lane.evaluate(
        (element, { mediaBytes, at, names, type }) => {
          const transfer = new DataTransfer(),
            bytes = Uint8Array.from(atob(mediaBytes), (c) => c.charCodeAt(0));
          for (const name of names)
            transfer.items.add(new File([bytes], name, { type }));
          const rect = element.getBoundingClientRect();
          element.dispatchEvent(
            new DragEvent("drop", {
              bubbles: true,
              cancelable: true,
              dataTransfer: transfer,
              clientX: rect.left + at * 80,
              clientY: rect.top + 20,
            }),
          );
        },
        { mediaBytes: bytes, at, names, type },
      );
    };
    await page.locator(".timeline-scroll").evaluate((element) => {
      element.scrollLeft = 0;
    });
    const beforeDrop = await inspect();
    const writeCount = uiWrites.filter(
      (item) => item.name === "opencut_edit_v3_apply",
    ).length;
    await dropFiles(source.id, 0, ["drop-a.mp4", "drop-b.mp4"]);
    await expect
      .poll(
        async () =>
          (await inspect()).project.base.sequences[0].tracks[0].clips.length,
        { timeout: 60000 },
      )
      .toBe(beforeDrop.project.base.sequences[0].tracks[0].clips.length + 2);
    await expect(
      page.getByRole("button", { name: "Track", exact: true }),
    ).toBeEnabled();
    const dropped = await inspect();
    expect(
      uiWrites.filter((item) => item.name === "opencut_edit_v3_apply"),
    ).toHaveLength(writeCount + 1);
    const inserted = dropped.project.base.sequences[0].tracks[0].clips.filter(
      (clip) =>
        !beforeDrop.project.base.sequences[0].tracks[0].clips.some(
          (old) => old.id === clip.id,
        ),
    );
    expect(
      inserted.map(
        (clip) =>
          Number(clip.timeline_range.start.ticks) /
          clip.timeline_range.start.timebase,
      ),
    ).toEqual([0, 4]);
    expect(
      inserted.map(
        (clip) =>
          dropped.project.base.assets.find(
            (asset) => asset.id === clip.asset_id,
          ).name,
      ),
    ).toEqual(["drop-a.mp4", "drop-b.mp4"]);
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(beforeDrop.project);
    await page.getByRole("button", { name: "Redo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(dropped.project);
    await expect(
      page.getByRole("button", { name: "Track", exact: true }),
    ).toBeEnabled();
    await dropFiles(locked.id, 0, ["locked.mp4"]);
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "existing unlocked target track" }),
    ).toBeVisible();
    expect((await inspect()).project).toEqual(dropped.project);
    note(
      "F16: two real video files drop at 0/4 seconds on the exact chosen track in one content transaction; complete document undo/redo, locked-target rejection without content writes",
    );
    await page
      .getByRole("button", { name: "Dismiss notification", exact: true })
      .click();
    await page
      .getByRole("button", { name: "Add audio track", exact: true })
      .click();
    await expect
      .poll(
        async () =>
          (await inspect()).project.base.sequences[0].tracks.filter(
            (track) => track.kind === "audio",
          ).length,
      )
      .toBe(1);
    await expect(
      page.getByRole("button", { name: "Track", exact: true }),
    ).toBeEnabled();
    const audioTrack = (await inspect()).project.base.sequences[0].tracks.find(
      (track) => track.kind === "audio",
    );
    const wav = (
      await fs.readFile(path.join(directory, "sample.wav"))
    ).toString("base64");
    const beforeAudio = await inspect();
    await dropFiles(audioTrack.id, 1, ["drop.wav"], wav, "audio/wav");
    await expect
      .poll(
        async () =>
          (await inspect()).project.base.sequences[0].tracks.find(
            (track) => track.id === audioTrack.id,
          ).clips.length,
        { timeout: 60000 },
      )
      .toBe(1);
    await expect(
      page.getByRole("button", { name: "Track", exact: true }),
    ).toBeEnabled();
    const afterAudio = await inspect();
    const audioClip = afterAudio.project.base.sequences[0].tracks.find(
      (track) => track.id === audioTrack.id,
    ).clips[0];
    expect(
      Number(audioClip.timeline_range.start.ticks) /
        audioClip.timeline_range.start.timebase,
    ).toBe(1);
    expect(
      afterAudio.project.base.assets.find(
        (asset) => asset.id === audioClip.asset_id,
      ).kind,
    ).toBe("audio");
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(beforeAudio.project);
    await page.getByRole("button", { name: "Redo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(afterAudio.project);
    await expect(
      page.getByRole("button", { name: "Track", exact: true }),
    ).toBeEnabled();
    const redPng = path.join(directory, "drop-red.png");
    execFileSync("ffmpeg", [
      "-v",
      "error",
      "-f",
      "lavfi",
      "-i",
      "color=red:size=640x360",
      "-frames:v",
      "1",
      redPng,
    ]);
    const pngBytes = (await fs.readFile(redPng)).toString("base64");
    await dropFiles(source.id, 14, ["drop.png"], pngBytes, "image/png");
    await expect
      .poll(
        async () =>
          (await inspect()).project.base.assets.filter(
            (asset) => asset.name === "drop.png",
          ).length,
        { timeout: 60000 },
      )
      .toBe(1);
    await expect(
      page.getByRole("button", { name: "Track", exact: true }),
    ).toBeEnabled();
    const afterImage = await inspect(),
      imageAsset = afterImage.project.base.assets.find(
        (asset) => asset.name === "drop.png",
      );
    const imageClip = afterImage.project.base.sequences[0].tracks[0].clips.find(
      (clip) => clip.asset_id === imageAsset.id,
    );
    expect(imageAsset.kind).toBe("image");
    expect(
      Number(imageClip.timeline_range.start.ticks) /
        imageClip.timeline_range.start.timebase,
    ).toBe(14);
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(afterAudio.project);
    await page.getByRole("button", { name: "Redo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(afterImage.project);
    await expect(
      page.getByRole("button", { name: "Track", exact: true }),
    ).toBeEnabled();
    await dropFiles(source.id, 24, ["wrong-track.wav"], wav, "audio/wav");
    await expect(
      page.getByRole("alert").filter({ hasText: "compatible track" }),
    ).toBeVisible();
    expect((await inspect()).project).toEqual(afterImage.project);
    note(
      "F16: real audio and PNG bytes preserve chosen target/time with complete document undo/redo; audio on a video track is explicitly rejected without any content commit",
    );
    await page
      .getByRole("button", { name: "Dismiss notification", exact: true })
      .click();
    await page.locator(".timeline-scroll").evaluate((element) => {
      element.scrollLeft = 700;
    });
    await page
      .locator(".ruler-content")
      .click({ position: { x: 18.5 * 80, y: 12 } });
    const previousRedFrame = await page
      .getByAltText("Rendered project frame", { exact: true })
      .evaluateAll((images) => images[0]?.getAttribute("src") ?? null);
    await page
      .getByRole("button", { name: "Render frame", exact: true })
      .click();
    await expect(
      page.getByAltText("Rendered project frame", { exact: true }),
    ).toBeVisible({ timeout: 60000 });
    await expect(
      page.getByAltText("Rendered project frame", { exact: true }),
    ).not.toHaveAttribute("src", previousRedFrame ?? "", { timeout: 60000 });
    const redFrame = await page
      .getByAltText("Rendered project frame", { exact: true })
      .evaluate(async (element) => {
        const blob = await (await fetch(element.src)).blob();
        return new Promise((resolve, reject) => {
          const reader = new FileReader();
          reader.onerror = reject;
          reader.onload = () => resolve(String(reader.result).split(",")[1]);
          reader.readAsDataURL(blob);
        });
      });
    const redFramePath = path.join(directory, "s1-image-on-video.png");
    await fs.writeFile(redFramePath, Buffer.from(redFrame, "base64"));
    for (const [x, y] of [
      [0, 0],
      [960, 540],
      [1918, 1078],
    ]) {
      const pixel = execFileSync("ffmpeg", [
        "-v",
        "error",
        "-i",
        redFramePath,
        "-vf",
        `crop=1:1:${x}:${y},format=rgb24`,
        "-frames:v",
        "1",
        "-f",
        "rawvideo",
        "pipe:1",
      ]);
      expect(pixel[0]).toBeGreaterThan(240);
      expect(pixel[1]).toBeLessThan(10);
      expect(pixel[2]).toBeLessThan(10);
    }
    note(
      "REPAIR-04: PNG on a video track reaches image IR, visual binding and native backend; independent red pixels at three positions prove the actual composited frame",
    );
    // F43 uses the same existing V3 transaction, with no implicit native cascade.
    const trackFixture = await inspect(),
      videoAsset = trackFixture.project.base.assets.find(
        (asset) => asset.kind === "video",
      ),
      audioAsset = trackFixture.project.base.assets.find(
        (asset) => asset.kind === "audio",
      ),
      time = (ticks) => ({ ticks: String(ticks), timebase: 1 }),
      ids = {
        video: crypto.randomUUID(),
        audio: crypto.randomUUID(),
        graphic: crypto.randomUUID(),
        empty: crypto.randomUUID(),
      },
      leftId = crypto.randomUUID(),
      rightId = crypto.randomUUID(),
      shapeId = crypto.randomUUID(),
      key0 = crypto.randomUUID(),
      key1 = crypto.randomUUID();
    const fixtureTracks = Object.entries(ids).map(([kind, id]) => ({
      id,
      name: `F43 ${kind}`,
      kind: kind === "empty" ? "video" : kind,
      enabled: true,
      muted: false,
      locked: false,
      params: {},
      clips: [],
    }));
    const fixtureClips = [leftId, rightId].map((id, index) => ({
      id,
      asset_id: videoAsset.id,
      timeline_range: { start: time(index), duration: time(1) },
      source_range: { start: time(index + 1), duration: time(1) },
      params: {},
      channels: index
        ? []
        : [
            {
              parameter: "opacity",
              keyframes: [
                { id: key0, time: time(0), value: 1, interpolation: "linear" },
                {
                  id: key1,
                  time: time(1),
                  value: 0.5,
                  interpolation: "linear",
                },
              ],
            },
          ],
    }));
    const newTracks = await write(
      [
        ...fixtureTracks.map((track) =>
          base({ type: "track.create", sequence_id: sequence.id, track }),
        ),
        ...fixtureClips.map((clip) =>
          base({
            type: "clip.insert",
            sequence_id: sequence.id,
            track_id: ids.video,
            clip,
          }),
        ),
        base({
          type: "clip.insert",
          sequence_id: sequence.id,
          track_id: ids.audio,
          clip: {
            id: crypto.randomUUID(),
            asset_id: audioAsset.id,
            timeline_range: { start: time(0), duration: time(2) },
            source_range: { start: time(1), duration: time(2) },
            params: {},
            channels: [],
          },
        }),
        {
          type: "shape.put",
          sequence_id: sequence.id,
          shape: {
            id: shapeId,
            track_id: ids.graphic,
            timeline_range: { start: time(0), duration: time(2) },
            primitive: { kind: "rectangle", width_px: 200, height_px: 100 },
            fill: { red: 0, green: 0, blue: 255, alpha: 255 },
            position_x: 0,
            position_y: 0,
            rotation_degrees: 0,
            opacity: 1,
          },
        },
        ...[leftId, shapeId].flatMap((target_id) => [
          {
            type: "effect.put",
            sequence_id: sequence.id,
            effect: {
              id: crypto.randomUUID(),
              target_id,
              effect: { kind: "gaussian_blur", radius_px: 2 },
            },
          },
          {
            type: "mask.put",
            sequence_id: sequence.id,
            mask: {
              id: crypto.randomUUID(),
              target_id,
              shape: {
                kind: "rectangle",
                x: 0.1,
                y: 0.1,
                width: 0.8,
                height: 0.8,
              },
              inverted: false,
              feather: 0,
            },
          },
        ]),
        {
          type: "keyframe.curve_set",
          sequence_id: sequence.id,
          curve: {
            id: crypto.randomUUID(),
            schema_version: 1,
            sequence_id: sequence.id,
            clip_id: leftId,
            parameter: "opacity",
            keyframe_id: key0,
            curve: { x1: 0.2, y1: 0.1, x2: 0.8, y2: 0.9 },
          },
        },
        {
          type: "transition.put",
          sequence_id: sequence.id,
          transition: {
            id: crypto.randomUUID(),
            outgoing_clip_id: leftId,
            incoming_clip_id: rightId,
            duration: { ticks: "1", timebase: 2 },
            transition: { kind: "crossfade" },
          },
        },
      ],
      undefined,
      "F43 track fixtures",
    );
    expect(newTracks.error).toBeNull();
    await page.reload();
    await expect(
      page.getByRole("button", { name: "Delete track F43 video", exact: true }),
    ).toBeEnabled();
    await page
      .getByRole("button", { name: "Delete track F43 video", exact: true })
      .click();
    let deletion = page.getByRole("dialog", {
      name: "Delete track",
      exact: true,
    });
    await expect(deletion).toContainText(
      "Delete F43 video and its 2 elements?",
    );
    await deletion.getByRole("button", { name: "Cancel", exact: true }).click();
    const reviewed = await inspect();
    expect(reviewed.revision).toBe(newTracks.data.revision);
    await page
      .getByRole("button", { name: "Delete track F43 video", exact: true })
      .click();
    expect(
      (
        await write(
          [
            base({
              type: "track.update",
              sequence_id: sequence.id,
              track_id: ids.empty,
              name: "F43 empty renamed",
              enabled: null,
              muted: null,
              locked: null,
            }),
          ],
          undefined,
          "F43 external change",
        )
      ).error,
    ).toBeNull();
    await expect(deletion.getByRole("status")).toContainText(
      "The project changed.",
    );
    await expect(
      deletion.getByRole("button", { name: "Delete track", exact: true }),
    ).toBeDisabled();
    await deletion.getByRole("button", { name: "Close", exact: true }).click();
    note(
      "F43: track review cancellation performs no edit; a real remote head update disables the captured deletion intent",
    );
    // Timeline clips are virtualized by the visible time interval. The previous
    // pixel test left the viewport at 18.5s; bring the 0s fixtures into view.
    await page.locator(".timeline-scroll").evaluate((element) => {
      element.scrollLeft = 0;
    });
    for (const kind of ["video", "audio", "graphic", "empty"]) {
      const before = await inspect(),
        expected = structuredClone(before.project),
        target = expected.base.sequences[0].tracks.find(
          (track) => track.id === ids[kind],
        ),
        removedClips = new Set(target.clips.map((clip) => clip.id)),
        removedTargets = new Set(removedClips);
      expected.base.sequences[0].tracks =
        expected.base.sequences[0].tracks.filter(
          (track) => track.id !== target.id,
        );
      for (const graph of expected.visual.filter(
        (graph) => graph.sequence_id === sequence.id,
      )) {
        for (const shape of graph.shapes.filter(
          (shape) => shape.track_id === target.id,
        ))
          removedTargets.add(shape.id);
        graph.shapes = graph.shapes.filter(
          (shape) => !removedTargets.has(shape.id),
        );
        graph.effects = graph.effects.filter(
          (node) => !removedTargets.has(node.target_id),
        );
        graph.masks = graph.masks.filter(
          (node) => !removedTargets.has(node.target_id),
        );
        graph.transitions = graph.transitions.filter(
          (node) =>
            !removedClips.has(node.outgoing_clip_id) &&
            !removedClips.has(node.incoming_clip_id),
        );
      }
      expected.keyframe_curves = expected.keyframe_curves.filter(
        (curve) => !removedClips.has(curve.clip_id),
      );
      if (kind === "video")
        await page.locator(`[data-clip-id="${leftId}"]`).click();
      const startWrites = uiWrites.filter(
        (item) => item.name === "opencut_edit_v3_apply",
      ).length;
      await page
        .getByRole("button", {
          name: `Delete track ${target.name}`,
          exact: true,
        })
        .click();
      deletion = page.getByRole("dialog", {
        name: "Delete track",
        exact: true,
      });
      await deletion
        .getByRole("button", { name: "Delete track", exact: true })
        .click();
      await expect(deletion).not.toBeVisible();
      await expect
        .poll(async () => (await inspect()).project)
        .toEqual(expected);
      const after = await inspect();
      expect(after.revision).not.toBe(before.revision);
      expect(
        uiWrites.filter((item) => item.name === "opencut_edit_v3_apply"),
      ).toHaveLength(startWrites + 1);
      if (kind === "video") await page.reload();
      await page.getByRole("button", { name: "Undo", exact: true }).click();
      await expect
        .poll(async () => (await inspect()).project)
        .toEqual(before.project);
      if (kind === "video")
        await expect(
          page.locator(`[data-clip-id="${leftId}"]`),
        ).toHaveAttribute("aria-pressed", "true");
      await page.getByRole("button", { name: "Redo", exact: true }).click();
      await expect
        .poll(async () => (await inspect()).project)
        .toEqual(expected);
      await page.getByRole("button", { name: "Undo", exact: true }).click();
      await expect
        .poll(async () => (await inspect()).project)
        .toEqual(before.project);
      note(
        `F43: ${kind} track deletion is one native content commit, preserves library/unrelated content and restores the complete document on undo/redo`,
      );
    }
    const beforeLockedDeletion = await inspect();
    await expect(
      page.getByRole("button", {
        name: `Delete track ${locked.name}`,
        exact: true,
      }),
    ).toBeDisabled();
    const lockedBatch = await write([
      base({
        type: "track.delete",
        sequence_id: sequence.id,
        track_id: ids.empty,
      }),
      base({
        type: "track.delete",
        sequence_id: sequence.id,
        track_id: locked.id,
      }),
    ]);
    expect(lockedBatch.error.code).toBe("CONSTRAINT_VIOLATION");
    expect((await inspect()).revision).toBe(beforeLockedDeletion.revision);
    expect((await inspect()).project).toEqual(beforeLockedDeletion.project);
    note(
      "F43: locked-track UI protection and direct public batch rejection leave even earlier valid deletes uncommitted",
    );
    // F41 uses the explicit selected 0..1s fixture, followed by its 1..2s neighbor.
    const exactSeconds = (value) => Number(value.ticks) / value.timebase;
    const seek = async (seconds) => {
      await page.locator(".timeline-scroll").evaluate((element) => {
        element.scrollLeft = 0;
      });
      await page
        .locator(".ruler-content")
        .click({ position: { x: seconds * 80, y: 12 } });
    };
    const splitBefore = await inspect();
    await page.locator(`[data-clip-id="${leftId}"]`).click();
    await seek(0.5);
    await page
      .getByRole("button", { name: "Split and remove left", exact: true })
      .click();
    await expect
      .poll(async () =>
        (await inspect()).project.base.sequences[0].tracks
          .find((t) => t.id === ids.video)
          .clips.some((c) => c.id === leftId),
      )
      .toBe(false);
    let splitAfter = await inspect();
    let target = splitAfter.project.base.sequences[0].tracks.find(
      (t) => t.id === ids.video,
    );
    let right = target.clips.find((c) => c.id !== rightId);
    expect(exactSeconds(right.timeline_range.start)).toBe(0.5);
    expect(exactSeconds(right.source_range.start)).toBe(1.5);
    expect(
      exactSeconds(
        target.clips.find((c) => c.id === rightId).timeline_range.start,
      ),
    ).toBe(1);
    expect(
      splitAfter.project.visual[0].effects.some(
        (e) => e.target_id === right.id,
      ),
    ).toBe(true);
    expect(
      splitAfter.project.visual[0].masks.some((e) => e.target_id === right.id),
    ).toBe(true);
    expect(
      splitAfter.project.keyframe_curves.some((e) => e.clip_id === right.id),
    ).toBe(true);
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(splitBefore.project);
    await page.getByRole("button", { name: "Redo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(splitAfter.project);
    await expect(page.locator(`[data-clip-id="${right.id}"]`)).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(splitBefore.project);
    note(
      "F41: remove-left button without ripple retains source 1.5s at timeline 0.5s, visual/animation bindings, unaffected neighbor and exact document/selection undo-redo",
    );
    await page
      .getByRole("button", { name: "Ripple editing", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Ripple editing", exact: true }),
    ).toHaveAttribute("aria-pressed", "true");
    await page.reload();
    await expect(
      page.getByRole("button", { name: "Ripple editing", exact: true }),
    ).toHaveAttribute("aria-pressed", "true");
    await page.locator(`[data-clip-id="${leftId}"]`).click();
    await seek(0.5);
    await page.keyboard.press("q");
    await expect
      .poll(async () =>
        (await inspect()).project.base.sequences[0].tracks
          .find((t) => t.id === ids.video)
          .clips.some((c) => c.id === leftId),
      )
      .toBe(false);
    splitAfter = await inspect();
    target = splitAfter.project.base.sequences[0].tracks.find(
      (t) => t.id === ids.video,
    );
    right = target.clips.find((c) => c.id !== rightId);
    expect(exactSeconds(right.timeline_range.start)).toBe(0);
    expect(exactSeconds(right.source_range.start)).toBe(1.5);
    expect(
      exactSeconds(
        target.clips.find((c) => c.id === rightId).timeline_range.start,
      ),
    ).toBe(0.5);
    expect(
      splitAfter.project.base.sequences[0].tracks.find(
        (t) => t.id === ids.audio,
      ),
    ).toEqual(
      splitBefore.project.base.sequences[0].tracks.find(
        (t) => t.id === ids.audio,
      ),
    );
    await expect(page.locator(".timecode")).toHaveText("00:00.00");
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(splitBefore.project);
    note(
      "F41: persistent ripple mode and Q shortcut close only the selected video row's leading gap, preserve the audio row and restore exact content on undo",
    );
    await page.locator(`[data-clip-id="${leftId}"]`).click();
    await seek(0.5);
    await page.keyboard.press("w");
    await expect
      .poll(async () =>
        exactSeconds(
          (await inspect()).project.base.sequences[0].tracks
            .find((t) => t.id === ids.video)
            .clips.find((c) => c.id === leftId).timeline_range.duration,
        ),
      )
      .toBe(0.5);
    splitAfter = await inspect();
    target = splitAfter.project.base.sequences[0].tracks.find(
      (t) => t.id === ids.video,
    );
    expect(
      exactSeconds(
        target.clips.find((c) => c.id === rightId).timeline_range.start,
      ),
    ).toBe(0.5);
    expect(
      exactSeconds(
        target.clips.find((c) => c.id === leftId).source_range.duration,
      ),
    ).toBe(0.5);
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await expect
      .poll(async () => (await inspect()).project)
      .toEqual(splitBefore.project);
    await page.locator(`[data-clip-id="${leftId}"]`).click();
    await seek(0);
    const boundary = (await inspect()).revision;
    await page.keyboard.press("q");
    await expect(
      page.getByRole("button", { name: "Track", exact: true }),
    ).toBeEnabled();
    expect((await inspect()).revision).toBe(boundary);
    await expect(page.getByRole("alert")).toHaveCount(0);
    await page
      .getByRole("button", { name: "Ripple editing", exact: true })
      .click();
    note(
      "F41: W retains the left source half and moves the next clip by the exact removed duration; split at the start is a no-op without an error or content revision",
    );
    expect(errors).toEqual([]);
    await page.screenshot({
      path: path.join(directory, "s1-selection.png"),
      fullPage: true,
    });
    await fs.writeFile(
      path.join(directory, "s1-selection-report.json"),
      JSON.stringify(
        {
          status: "passed",
          browser: await browser.version(),
          project_id: projectId,
          checks,
          errors,
          calls,
          uiWrites,
        },
        null,
        2,
      ),
    );
  } catch (error) {
    await page.screenshot({
      path: path.join(directory, "s1-selection-failure.png"),
      fullPage: true,
    });
    await fs.writeFile(
      path.join(directory, "s1-selection-report.json"),
      JSON.stringify(
        {
          status: "failed",
          error: String(error),
          checks,
          errors,
          calls,
          uiWrites,
        },
        null,
        2,
      ),
    );
    throw error;
  } finally {
    await page.close();
  }
}
