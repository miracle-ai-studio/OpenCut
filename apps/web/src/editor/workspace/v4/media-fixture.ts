import { vi } from "vitest";
import { setupV4 } from "./fixture";
import type {
  V4AvailableMedia,
  V4MediaPlan,
  V4MediaRequest,
  V4EditReceipt,
} from "./types";

/** Client protocol fixture only. Actual probe/placement and bytes are verified
 * by the root browser candidate, not fabricated by this fixture. */
export async function setupMedia() {
  const fixture = await setupV4(),
    { api, current, snapshots } = fixture;
  const observation: V4AvailableMedia = {
    project_id: current().project.id,
    media_id: "observed-media",
    job_id: "import-job",
    upload_id: "upload",
    asset: {
      id: "different-legacy-asset-id",
      name: "Imported image",
      uri: "opencut-media://observed-media",
      kind: "image",
      version: {
        id: `av_${"b".repeat(64)}`,
        content_hash: "b".repeat(64),
        size: 123,
      },
      metadata: { width: 64, height: 64 },
    },
  };
  const requests: V4MediaRequest[] = [],
    plans: V4MediaPlan[] = [],
    receipts = new Map<string, V4EditReceipt>();
  const base = vi.mocked(api.sdk.call).getMockImplementation()!;
  const planFor = (request: V4MediaRequest): V4MediaPlan => {
    const operations: V4MediaPlan["transaction"]["operations"] =
      request.bindings.map((binding) => ({
        type: "asset_bind",
        id: binding.asset_id,
        name: binding.name,
        source: {
          kind: "project_media",
          media_id: binding.media_id,
          version: binding.version,
        },
      }));
    for (const track of request.new_tracks)
      operations.push({
        type: "track_put",
        sequence_id: request.sequence_id,
        track: {
          ...track,
          enabled: true,
          muted: false,
          locked: false,
          opacity: 1,
          gain: { unit: "decibels", value: 0 },
          elements: [],
          channels: [],
        },
      });
    for (const placement of request.placements)
      operations.push({
        type: "element_put",
        sequence_id: request.sequence_id,
        track_id: placement.track_id,
        element: {
          ...structuredClone(
            current().project.sequences[0].tracks[0].elements[0],
          ),
          id: placement.element_id,
          name: placement.name,
          content: { kind: "image", asset_id: placement.asset_id },
          timeline_range: {
            start: { ticks: "0", timebase: 1 },
            duration: { ticks: "5", timebase: 2 },
          },
        },
      });
    return {
      original_request_digest: "fixture-digest",
      transaction: {
        project_id: request.project_id,
        expected_revision: request.expected_revision,
        idempotency_key: request.idempotency_key,
        label: request.label,
        operations,
      },
      decisions: {
        settings_scope: { profile: "classic_global" },
        previous_fps: { numerator: 60, denominator: 1 },
        raised_fps: null,
        first_visual_fps: { numerator: 24, denominator: 1 },
        canvas_change: null,
        bound_asset_ids: request.bindings
          .filter((b) => b.mode === "bind")
          .map((b) => b.asset_id),
        replaced_asset_ids: request.bindings
          .filter((b) => b.mode === "replace")
          .map((b) => b.asset_id),
        video_asset_ids_without_fps: [],
        placements: request.placements.map((placement) => ({
          element_id: placement.element_id,
          requested_start: placement.start,
          resolved_start: { ticks: "0", timebase: 1 },
          timeline_duration: { ticks: "5", timebase: 2 },
          source_range: null,
          main_track_start_adjusted: true,
        })),
      },
    };
  };
  const commit = (transaction: V4MediaPlan["transaction"]) => {
    const known = receipts.get(transaction.idempotency_key);
    if (known) return known;
    const scene = current().project.sequences[0];
    for (const op of transaction.operations) {
      if (op.type === "asset_bind")
        current().project.assets = [
          ...current().project.assets.filter((a) => a.id !== op.id),
          {
            ...structuredClone(current().project.assets[0]),
            id: op.id,
            name: op.name,
            version: observation.asset.version!,
          },
        ];
      if (op.type === "track_put") scene.tracks.push(structuredClone(op.track));
      if (op.type === "element_put")
        scene.tracks
          .find((t) => t.id === op.track_id)!
          .elements.push(structuredClone(op.element));
    }
    current().revision = `media-${transaction.idempotency_key}`;
    snapshots.set(current().revision, structuredClone(current()));
    const receipt = {
      base_revision: transaction.expected_revision,
      revision: current().revision,
      transaction_id: `tx-${transaction.idempotency_key}`,
      changed_ids: [],
      created_ids: [],
      deleted_ids: [],
    };
    receipts.set(transaction.idempotency_key, receipt);
    return receipt;
  };
  vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
    if (name === "opencut_project_media_query")
      return {
        media: [structuredClone(observation)],
        next_cursor: null,
      } as never;
    if (name === "opencut_media_workspace_plan_v4") {
      const request = structuredClone(input as V4MediaRequest),
        plan = planFor(request);
      requests.push(request);
      plans.push(plan);
      return plan as never;
    }
    if (name === "opencut_edit_v4_apply")
      return commit(input as V4MediaPlan["transaction"]) as never;
    return base(name, input, options);
  });
  return { ...fixture, observation, requests, plans, planFor, commit };
}
