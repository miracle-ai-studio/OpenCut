import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { applyMediaReview } from "./verify-v4-editor.mjs";
import { verifyFilePlacementBoundaries } from "./verify-v4-file-import-boundaries.mjs";

const equalTime = (actual, ticks, timebase = 1) =>
  expect(BigInt(actual.ticks) * BigInt(timebase)).toBe(
    BigInt(ticks) * BigInt(actual.timebase),
  );

// Real File bytes, HTTP/native probe, SQLite and unchanged public apply. Drag
// events are browser-dispatched; native clipboard uses Chromium's trusted paste.
// Neither establishes Finder drag delivery, cross-app clipboard or system IME.
export async function verifyV4FileImport(
  browser,
  endpoint,
  directory,
  check,
  { boundaries = false } = {},
) {
  const context = await browser.newContext({
    locale: "en-US",
    viewport: { width: 1440, height: 1100 },
  });
  await context.grantPermissions(["clipboard-read", "clipboard-write"], {
    origin: endpoint,
  });
  const page = await context.newPage();
  page.setDefaultTimeout(30000);
  const errors = [],
    writes = [],
    begins = [],
    plans = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("request", (request) => {
    const name = request.url().split("/").at(-1);
    if (name === "opencut_edit_v4_apply") writes.push(request.postDataJSON());
    if (name === "opencut_upload_stream_begin")
      begins.push(request.postDataJSON());
    if (name === "opencut_media_workspace_plan_v4")
      plans.push(request.postDataJSON());
  });
  const response = (name) =>
    page.waitForResponse((r) => r.url().endsWith(`/v1/tools/${name}`));
  const tool = async (name, input) => {
    const reply = await page.evaluate(
      async ({ name, input }) =>
        (
          await fetch(`/v1/tools/${name}`, {
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
  let projectId,
    sceneId,
    mainId,
    clipboardSaved = false;
  const inspect = () =>
    tool("opencut_project_v4_inspect", {
      project_id: projectId,
      revision: null,
    });
  const scene = (snapshot) =>
    snapshot.project.sequences.find((s) => s.id === sceneId);
  const lane = (id) => page.locator(`[data-file-track="${id}"]`);
  const review = () =>
    page.getByRole("dialog", { name: "Review media changes", exact: true });
  const reload = async () => {
    await page.reload();
    await expect(page.getByRole("main", { name: "V4 editor" })).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Import media", exact: true }),
    ).toBeEnabled();
  };
  const drop = async (trackId, names, at = 0, zoom = 1, scroll = 0) => {
    await expect(
      page.getByRole("button", { name: "Import media", exact: true }),
    ).toBeEnabled();
    const files = await Promise.all(
      names.map(async (name) => ({
        name,
        bytes: [...(await fs.readFile(path.join(directory, name)))],
        type: name.endsWith(".png")
          ? "image/png"
          : name.endsWith(".wav")
            ? "audio/wav"
            : "video/mp4",
      })),
    );
    await page
      .getByLabel("Timeline zoom", { exact: true })
      .evaluate((element, value) => {
        Object.getOwnPropertyDescriptor(
          HTMLInputElement.prototype,
          "value",
        ).set.call(element, String(value));
        element.dispatchEvent(new Event("input", { bubbles: true }));
        element.dispatchEvent(new Event("change", { bubbles: true }));
      }, zoom);
    await lane(trackId).evaluate(
      (element, { files, at, zoom, scroll }) => {
        const viewport = element.closest(".v4-timeline-viewport");
        viewport.scrollLeft = scroll;
        const rect = element.getBoundingClientRect(),
          transfer = new DataTransfer();
        for (const file of files)
          transfer.items.add(
            new File([new Uint8Array(file.bytes)], file.name, {
              type: file.type,
            }),
          );
        element.dispatchEvent(
          new DragEvent("drop", {
            bubbles: true,
            cancelable: true,
            dataTransfer: transfer,
            // DragEvent uses integer CSS pixels. Round explicitly instead of
            // allowing a float just below an integer to truncate one pixel.
            clientX: Math.round(rect.left + at * 80 * zoom),
            clientY: rect.top + 12,
          }),
        );
      },
      { files, at, zoom, scroll },
    );
  };
  const history = async (action) => {
    const area = page.locator(".v4-history");
    await area.getByRole("button", { name: action, exact: true }).click();
    const applying = response(`opencut_edit_v4_${action.toLowerCase()}_apply`);
    await area
      .getByRole("button", {
        name: `Apply ${action.toLowerCase()}`,
        exact: true,
      })
      .click();
    expect((await (await applying).json()).error).toBeNull();
    await expect(
      area.getByRole("button", {
        name: `Apply ${action.toLowerCase()}`,
        exact: true,
      }),
    ).toBeHidden();
  };
  try {
    await page.goto(`${endpoint}/editor`);
    await page
      .getByRole("button", {
        name: "New project with scene notes",
        exact: true,
      })
      .click();
    const create = page.getByRole("dialog", {
      name: "New project with scene notes",
      exact: true,
    });
    await create
      .getByLabel("Project name", { exact: true })
      .fill("File import acceptance");
    await create
      .getByRole("button", { name: "Create project", exact: true })
      .click();
    await expect(page).toHaveURL(/\/editor\/.+/);
    const welcome = page.getByRole("dialog", {
      name: "Welcome to OpenCut",
      exact: true,
    });
    if (await welcome.isVisible())
      await welcome.getByRole("button", { name: "Close", exact: true }).click();
    await expect(page.getByRole("main", { name: "V4 editor" })).toBeVisible();
    projectId = decodeURIComponent(
      new URL(page.url()).pathname.split("/").at(-1),
    );
    const initial = await inspect();
    sceneId = initial.project.default_sequence_id;
    mainId = scene(initial).main_track_id;

    if (boundaries) {
      await verifyFilePlacementBoundaries({
        page,
        projectId,
        sceneId,
        mainId,
        initial,
        tool,
        inspect,
        scene,
        review,
        reload,
        drop,
        response,
        history,
        writes,
        begins,
        plans,
        errors,
        check,
      });
      return;
    }

    let planning = response("opencut_media_workspace_plan_v4");
    const beforeWrites = writes.length;
    await drop(mainId, ["sample.mp4", "sample.png"]);
    await expect(review()).toBeVisible();
    expect(writes).toHaveLength(beforeWrites);
    const first = await applyMediaReview(page, writes, planning);
    expect(plans.at(-1).bindings).toHaveLength(2);
    expect(plans.at(-1).placements).toHaveLength(2);
    expect(plans.at(-1).placements.map((p) => p.track_id)).toEqual([
      mainId,
      mainId,
    ]);
    let saved = await inspect(),
      elements = scene(saved).tracks.find((t) => t.id === mainId).elements;
    expect(elements.map((e) => e.content.kind)).toEqual(["video", "image"]);
    equalTime(elements[0].timeline_range.start, 0);
    equalTime(elements[0].timeline_range.duration, 4);
    equalTime(elements[1].timeline_range.start, 4);
    equalTime(elements[1].timeline_range.duration, 5);
    expect(elements[0].content.source_audio_enabled).toBe(true);
    expect(writes).toHaveLength(beforeWrites + 1);
    await history("Undo");
    expect((await inspect()).project).toEqual(initial.project);
    await history("Redo");
    expect((await inspect()).project).toEqual(saved.project);
    await reload();
    expect((await inspect()).project).toEqual(saved.project);
    check(
      "file drop imports real video/image bytes into one exact reviewed native transaction; independent 0/4s starts, 4/5s durations, source audio, reopen and selective Undo/Redo",
      {
        file_drop: {
          transaction_id: first.receipt.transaction_id,
          batch_size: 2,
          event_source: "browser DataTransfer; not Finder",
        },
      },
    );

    // Undo/Redo preserves content, but appends revisions. Seed against the
    // current fixed head rather than the original import's historical receipt.
    saved = await inspect();
    const audioId = crypto.randomUUID(),
      track = {
        ...scene(saved).tracks[0],
        id: audioId,
        name: "File audio",
        kind: "audio",
        elements: [],
      };
    await tool("opencut_edit_v4_apply", {
      project_id: projectId,
      expected_revision: saved.revision,
      idempotency_key: crypto.randomUUID(),
      label: "File acceptance audio track",
      operations: [{ type: "track_put", sequence_id: sceneId, track }],
    });
    await reload();
    planning = response("opencut_media_workspace_plan_v4");
    await drop(audioId, ["sample.wav", "replacement.wav"], 1.25, 2, 120);
    const audio = await applyMediaReview(page, writes, planning);
    saved = await inspect();
    elements = scene(saved).tracks.find((t) => t.id === audioId).elements;
    expect(elements.map((e) => e.content.kind)).toEqual(["audio", "audio"]);
    equalTime(plans.at(-1).placements[0].start, 5, 4);
    equalTime(elements[0].timeline_range.start, 5, 4);
    equalTime(elements[1].timeline_range.start, 21, 4);
    for (const element of elements)
      equalTime(element.timeline_range.duration, 4);
    check(
      "audio multi-file drop uses the target lane at zoom2 and horizontal scroll; native placement preserves independent 1.25/5.25s starts in one public commit",
      {
        audio_drop: {
          transaction_id: audio.receipt.transaction_id,
          track_id: audioId,
          zoom: 2,
          scroll: 120,
        },
      },
    );

    const png = [...(await fs.readFile(path.join(directory, "sample.png")))];
    await page.evaluate(async (bytes) => {
      window.__fileImportClipboard = await navigator.clipboard.read();
      await navigator.clipboard.write([
        new ClipboardItem({
          "image/png": new Blob([new Uint8Array(bytes)], { type: "image/png" }),
        }),
      ]);
      window.__filePasteEvents = [];
      document.addEventListener("paste", (event) =>
        window.__filePasteEvents.push({
          trusted: event.isTrusted,
          files: event.clipboardData.files.length,
        }),
      );
    }, png);
    clipboardSaved = true;
    await page
      .locator(".v4-track-name")
      .filter({ hasText: "video" })
      .first()
      .click();
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill("12.25");
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
    await page.locator(".v4-timeline-panel").focus();
    planning = response("opencut_media_workspace_plan_v4");
    const modifier = (await page.evaluate(() => /Mac/.test(navigator.platform)))
      ? "Meta"
      : "Control";
    await page.keyboard.press(`${modifier}+V`);
    const pasted = await applyMediaReview(page, writes, planning);
    expect(plans.at(-1).placements[0].track_id).toBe(mainId);
    equalTime(plans.at(-1).placements[0].start, 49, 4);
    expect(await page.evaluate(() => window.__filePasteEvents)).toContainEqual({
      trusted: true,
      files: 1,
    });
    const pastedState = await inspect(),
      pastedElement = scene(pastedState)
        .tracks.find((t) => t.id === mainId)
        .elements.at(-1);
    expect(pastedElement.content.kind).toBe("image");
    equalTime(pastedElement.timeline_range.start, 49, 4);
    equalTime(pastedElement.timeline_range.duration, 5);
    const beforeInput = begins.length;
    await page.getByLabel("Playhead (seconds)", { exact: true }).focus();
    await page.keyboard.press(`${modifier}+V`);
    expect(begins).toHaveLength(beforeInput);
    check(
      "Chromium trusted native PNG paste captures selected track and 12.25s playhead, applies original plan once, and leaves focused input clipboard ownership intact",
      {
        native_file_paste: {
          transaction_id: pasted.receipt.transaction_id,
          trusted: true,
          cross_application_clipboard: "unverified",
          system_ime: "unverified",
        },
      },
    );

    const beforeBad = await inspect(),
      beforeBadWrites = writes.length;
    planning = response("opencut_media_workspace_plan_v4");
    await drop(audioId, ["sample.wav", "sample.png"], 10);
    const refused = await (await planning).json();
    expect(refused.error).toBeTruthy();
    await expect(review()).toBeVisible();
    await expect(
      review().getByRole("button", {
        name: "Apply media changes",
        exact: true,
      }),
    ).toBeHidden();
    expect((await inspect()).project).toEqual(beforeBad.project);
    expect(writes).toHaveLength(beforeBadWrites);
    await review()
      .getByRole("button", { name: "Discard media plan", exact: true })
      .click();
    check(
      "incompatible mixed file batch is rejected by the existing application planner with no partial binding, placement or content revision",
      {
        rejected_batch: {
          error_code: refused.error.code,
          unchanged_revision: beforeBad.revision,
        },
      },
    );

    let unblock, intercepted;
    const blocked = new Promise((resolve) => {
      intercepted = resolve;
    });
    const held = new Promise((resolve) => {
      unblock = resolve;
    });
    const routePattern = "**/v1/stream-uploads/*/chunks/*";
    await page.route(routePattern, async (route) => {
      intercepted();
      await held;
      await route.abort().catch(() => {});
    });
    const beforeCancel = await inspect(),
      beforeCancelPlans = plans.length;
    await drop(mainId, ["sample.mp4"], 20);
    await blocked;
    await page
      .getByRole("button", { name: "Cancel import", exact: true })
      .click();
    unblock();
    await expect(review()).toBeVisible();
    await expect(
      review().getByRole("button", {
        name: "Retry media planning",
        exact: true,
      }),
    ).toBeHidden();
    await expect(
      review().getByRole("button", {
        name: "Apply media changes",
        exact: true,
      }),
    ).toBeHidden();
    expect(plans).toHaveLength(beforeCancelPlans);
    expect((await inspect()).revision).toBe(beforeCancel.revision);
    await review()
      .getByRole("button", { name: "Discard media plan", exact: true })
      .click();
    await page.unroute(routePattern);
    check(
      "cancelled real chunk upload has no partial retryable content intent or late planning; project revision stays fixed",
      { cancelled_upload: { unchanged_revision: beforeCancel.revision } },
    );

    const beforeLock = await inspect(),
      locked = {
        ...scene(beforeLock).tracks.find((t) => t.id === audioId),
        locked: true,
      };
    await tool("opencut_edit_v4_apply", {
      project_id: projectId,
      expected_revision: beforeLock.revision,
      idempotency_key: crypto.randomUUID(),
      label: "File acceptance locked track",
      operations: [{ type: "track_put", sequence_id: sceneId, track: locked }],
    });
    await reload();
    const beforeLockedUpload = begins.length;
    await drop(audioId, ["sample.wav"]);
    await expect(
      page.locator(".v4-timeline-feedback [role=alert]"),
    ).toContainText("unlocked");
    expect(begins).toHaveLength(beforeLockedUpload);
    expect(errors).toEqual([]);
    check(
      "locked target refuses before upload; all file import checks complete with zero page exceptions",
      { page_errors: errors },
    );
  } catch (error) {
    console.error(
      JSON.stringify(
        {
          pageErrors: errors,
          uploads: begins.length,
          plans: plans.length,
          writes: writes.length,
          ui: await page
            .locator("body")
            .innerText()
            .catch(() => "closed"),
        },
        null,
        2,
      ),
    );
    throw error;
  } finally {
    if (clipboardSaved)
      await page
        .evaluate(async () => {
          if (window.__fileImportClipboard.length)
            await navigator.clipboard.write(window.__fileImportClipboard);
          else await navigator.clipboard.writeText("");
        })
        .catch(() => {});
    await context.close();
  }
}
