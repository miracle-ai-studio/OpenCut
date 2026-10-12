import { expect } from "@playwright/test";
import { applyMediaReview } from "./verify-v4-editor.mjs";

const equalTime = (actual, ticks, timebase = 1) =>
  expect(BigInt(actual.ticks) * BigInt(timebase)).toBe(
    BigInt(ticks) * BigInt(actual.timebase),
  );

// Supplementary cases only: reuse the import harness, fixed binary and public
// planner. These do not repeat the prior trusted clipboard/lock/zoom matrix.
export async function verifyFilePlacementBoundaries({
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
}) {
  const audioId = crypto.randomUUID(),
    batchId = crypto.randomUUID();
  const audioTrack = (id, name) => ({
    ...scene(initial).tracks[0],
    id,
    name,
    kind: "audio",
    elements: [],
  });
  expect(initial.project.settings.profile).toBe("classic_global");
  await tool("opencut_edit_v4_apply", {
    project_id: projectId,
    expected_revision: initial.revision,
    idempotency_key: crypto.randomUUID(),
    label: "Precise file import fixture",
    operations: [
      {
        type: "settings_set",
        settings: {
          ...initial.project.settings,
          settings: {
            ...initial.project.settings.settings,
            fps: { numerator: 24, denominator: 1 },
          },
        },
      },
      {
        type: "track_put",
        sequence_id: sceneId,
        track: audioTrack(audioId, "Precise audio"),
      },
      {
        type: "track_put",
        sequence_id: sceneId,
        track: audioTrack(batchId, "Batch audio"),
      },
    ],
  });
  await reload();
  const before = await inspect();
  let planning = response("opencut_media_workspace_plan_v4");
  // DragEvent coordinates are integer CSS pixels. At zoom5, 1.26s is
  // exactly 504px; zoom1 would truncate 100.8px to 100px (=1.25s).
  await drop(audioId, ["fractional.wav"], 1.26, 5);
  const firstPlan = await (await planning).json();
  expect(firstPlan.error).toBeNull();
  const firstDecision = firstPlan.data.decisions.placements[0];
  equalTime(firstDecision.requested_start, 63, 50);
  equalTime(firstDecision.resolved_start, 5, 4);
  equalTime(firstDecision.timeline_duration, 101, 100);
  equalTime(firstDecision.source_range.start, 0);
  equalTime(firstDecision.source_range.duration, 101, 100);
  await expect(
    review().getByRole("region", {
      name: "Placement of fractional.wav",
      exact: true,
    }),
  ).toContainText("Target track: Precise audio");
  await expect(review()).toContainText(
    "Requested position: 00:01.26 (63/50 s)",
  );
  await expect(review()).toContainText(
    "Frame alignment changed the requested position.",
  );
  // Use the already observed response; do not issue a second planning request.
  const first = await applyMediaReview(
    page,
    writes,
    Promise.resolve({ json: async () => firstPlan }),
  );
  const saved = await inspect();
  const element = scene(saved).tracks.find((track) => track.id === audioId)
    .elements[0];
  equalTime(element.timeline_range.start, 5, 4);
  equalTime(element.timeline_range.duration, 101, 100);
  equalTime(element.source_span.source_duration, 101, 100);
  equalTime(element.source_span.trim_start, 0);
  equalTime(element.source_span.trim_end, 0);
  await history("Undo");
  expect((await inspect()).project).toEqual(before.project);
  await history("Redo");
  expect((await inspect()).project).toEqual(saved.project);
  await reload();
  expect((await inspect()).project).toEqual(saved.project);
  check(
    "48480-sample WAV preserves exact 1.01s extent while 1.26s request resolves to 1.25s at 24fps; review, one original commit, Undo/Redo and reopen agree",
    {
      fractional_placement: {
        source_samples: 48480,
        sample_rate: 48000,
        requested_start: { ticks: "63", timebase: 50 },
        resolved_start: { ticks: "5", timebase: 4 },
        duration: { ticks: "101", timebase: 100 },
        transaction_id: first.receipt.transaction_id,
      },
    },
  );

  const discard = async () => {
    await review()
      .getByRole("button", { name: "Discard media plan", exact: true })
      .click();
    await expect(review()).toBeHidden();
  };
  const unchanged = async (snapshot, writeCount) => {
    expect(await inspect()).toEqual(snapshot);
    expect(writes).toHaveLength(writeCount);
    await expect(
      review().getByRole("button", {
        name: "Apply media changes",
        exact: true,
      }),
    ).toBeHidden();
  };
  const beforeBatch = await inspect(),
    batchWrites = writes.length;
  planning = response("opencut_media_workspace_plan_v4");
  await drop(batchId, ["fractional.wav", "fractional-b.wav"]);
  const collision = await (await planning).json();
  expect(collision.error).toBeTruthy();
  expect(collision.error.message).toMatch(/overlap/i);
  equalTime(plans.at(-1).placements[0].start, 0);
  equalTime(plans.at(-1).placements[1].start, 101, 100);
  await expect(review()).toBeVisible();
  await expect(
    review().getByRole("region", {
      name: "Placement of fractional-b.wav",
      exact: true,
    }),
  ).toContainText("Requested position: 00:01.01 (101/100 s)");
  await unchanged(beforeBatch, batchWrites);
  const collisionRequest = structuredClone(plans.at(-1)),
    collisionBegins = begins.length;
  planning = response("opencut_media_workspace_plan_v4");
  await review()
    .getByRole("button", { name: "Retry media planning", exact: true })
    .click();
  expect((await (await planning).json()).error).toBeTruthy();
  expect(plans.at(-1)).toEqual(collisionRequest);
  expect(begins).toHaveLength(collisionBegins);
  await unchanged(beforeBatch, batchWrites);
  await discard();
  check(
    "non-frame batch collision is refused atomically; exact ordered intentions remain visible and retry uses original IDs/key without reupload or a content revision",
    {
      batch_collision: {
        code: collision.error.code,
        unchanged_revision: beforeBatch.revision,
        first_end_seconds: "101/100",
        second_aligned_start_seconds: "1/1",
        retry_reuploaded: false,
      },
    },
  );

  const beforeExisting = await inspect(),
    existingWrites = writes.length;
  planning = response("opencut_media_workspace_plan_v4");
  await drop(audioId, ["fractional-b.wav"], 1.25);
  const existingCollision = await (await planning).json();
  expect(existingCollision.error.message).toMatch(/overlap/i);
  await expect(review()).toBeVisible();
  await unchanged(beforeExisting, existingWrites);
  await discard();
  check(
    "collision with an existing clip preserves the complete head and publishes neither partial binding nor placement",
    {
      existing_collision: {
        code: existingCollision.error.code,
        unchanged_revision: beforeExisting.revision,
      },
    },
  );

  const beginPattern = "**/v1/tools/opencut_upload_stream_begin";
  await page.route(beginPattern, async (route) => {
    if (route.request().postDataJSON().name !== "fractional-b.wav")
      return route.continue();
    await route.fulfill({
      status: 500,
      contentType: "application/json",
      body: JSON.stringify({
        api_version: "1",
        request_id: crypto.randomUUID(),
        warnings: [],
        data: null,
        error: {
          code: "STORAGE_ERROR",
          message: "Injected second upload admission failure",
          details: {},
        },
      }),
    });
  });
  const beforeFailure = await inspect(),
    failureWrites = writes.length,
    failurePlans = plans.length;
  try {
    await drop(audioId, ["sample.wav", "fractional-b.wav"], 12);
    await expect(review()).toBeVisible();
    await expect(review().getByRole("alert")).toContainText(
      "Injected second upload admission failure",
    );
    await expect(
      review().getByRole("button", {
        name: "Retry media planning",
        exact: true,
      }),
    ).toBeHidden();
    expect(plans).toHaveLength(failurePlans);
    await unchanged(beforeFailure, failureWrites);
  } finally {
    await page.unroute(beginPattern);
  }
  await discard();
  const available = await tool("opencut_project_media_query", {
    project_id: projectId,
    cursor: null,
    limit: 60,
  });
  const recoveredMedia = available.media.find(
    (item) => item.asset.name === "sample.wav",
  );
  expect(recoveredMedia.asset.version).toBeTruthy();
  const recoveryBegins = begins.length;
  await page.locator(".v4-available-media > summary").click();
  planning = response("opencut_media_workspace_plan_v4");
  await page
    .getByRole("button", { name: "Recover sample.wav", exact: true })
    .click();
  const recovered = await applyMediaReview(page, writes, planning);
  expect(plans.at(-1).bindings.map((binding) => binding.media_id)).toEqual([
    recoveredMedia.media_id,
  ]);
  expect(plans.at(-1).placements).toEqual([]);
  expect(begins).toHaveLength(recoveryBegins);
  const afterRecovery = await inspect();
  expect(afterRecovery.project.sequences).toEqual(
    beforeFailure.project.sequences,
  );
  expect(afterRecovery.project.assets).toHaveLength(
    beforeFailure.project.assets.length + 1,
  );
  check(
    "second-file upload admission failure leaves no partial retryable batch; first real completed upload is recovered with its original media/version and no reupload",
    {
      partial_upload_recovery: {
        injected_failure: "second admission HTTP response",
        media_id: recoveredMedia.media_id,
        version: recoveredMedia.asset.version,
        transaction_id: recovered.receipt.transaction_id,
        reuploaded: false,
      },
    },
  );

  const planPattern = "**/v1/tools/opencut_media_workspace_plan_v4";
  await page.route(planPattern, (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/json",
      body: JSON.stringify({
        api_version: "1",
        request_id: crypto.randomUUID(),
        warnings: [],
        data: null,
        error: {
          code: "STORAGE_ERROR",
          message: "Injected transient planning failure",
          details: {},
        },
      }),
    }),
  );
  const beforeTransient = await inspect(),
    transientWrites = writes.length;
  try {
    await drop(batchId, ["fractional-b.wav"], 4);
    await expect(review()).toBeVisible();
    await expect(review().getByRole("alert")).toContainText(
      "Injected transient planning failure",
    );
    await unchanged(beforeTransient, transientWrites);
  } finally {
    await page.unroute(planPattern);
  }
  const original = structuredClone(plans.at(-1)),
    retryBegins = begins.length;
  planning = response("opencut_media_workspace_plan_v4");
  await review()
    .getByRole("button", { name: "Retry media planning", exact: true })
    .click();
  const retried = await applyMediaReview(page, writes, planning);
  expect(plans.at(-1)).toEqual(original);
  expect(begins).toHaveLength(retryBegins);
  equalTime(
    scene(await inspect()).tracks.find((track) => track.id === batchId)
      .elements[0].timeline_range.duration,
    101,
    100,
  );
  check(
    "transient plan failure retries the complete original fractional intent/IDs/key against native planning and commits unchanged without another upload",
    {
      transient_plan_retry: {
        injected_failure: "planning HTTP response",
        idempotency_key: original.idempotency_key,
        transaction_id: retried.receipt.transaction_id,
        reuploaded: false,
      },
    },
  );

  planning = response("opencut_media_workspace_plan_v4");
  await drop(mainId, ["sample.png"], 2.01, 5);
  const mainPlan = await (await planning).json();
  expect(mainPlan.error).toBeNull();
  equalTime(mainPlan.data.decisions.placements[0].requested_start, 201, 100);
  equalTime(mainPlan.data.decisions.placements[0].resolved_start, 0);
  expect(mainPlan.data.decisions.placements[0].main_track_start_adjusted).toBe(
    true,
  );
  await expect(review()).toContainText(
    "The main track moved this placement to the scene start.",
  );
  const main = await applyMediaReview(
    page,
    writes,
    Promise.resolve({ json: async () => mainPlan }),
  );
  equalTime(
    scene(await inspect()).tracks.find((track) => track.id === mainId)
      .elements[0].timeline_range.start,
    0,
  );
  expect(errors).toEqual([]);
  check(
    "review explicitly explains empty-main-track adjustment from requested 2.01s to 0s; original public apply agrees and all boundary cases have zero page exceptions",
    {
      main_track_adjustment: {
        requested_start: { ticks: "201", timebase: 100 },
        resolved_start: { ticks: "0", timebase: 1 },
        transaction_id: main.receipt.transaction_id,
      },
      page_errors: errors,
    },
  );
}
