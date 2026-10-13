import { OpenCutApiError } from "../../../lib/editor-sdk";
import type {
  OpenCutClient,
  ToolInput,
  ToolOutput,
} from "../../../lib/editor-sdk";
import type { ClassicSnapshot } from "./decode";
import type { RecoveryMedia } from "./bundle";
import { legacyId } from "./identity";
import { isRecord } from "./legacy/utils";
import { prepareRecovery, stageRecoveryMedia } from "./recovery";
import type { PreparedRecovery, RecoveryProgress } from "./recovery";

export type FontMappingV4 =
  ToolInput<"opencut_classic_analyze_v4">["font_mappings"][number];
export type RecoveryReviewV4 = {
  prepared: PreparedRecovery;
  source: ToolInput<"opencut_classic_analyze_v4">;
  analysis: ToolOutput<"opencut_classic_analyze_v4">;
  revision: string;
};
export async function prepareRecoveryV4(
  snapshot: ClassicSnapshot,
  media: RecoveryMedia[],
  signal: AbortSignal,
  progress: (p: RecoveryProgress) => void,
) {
  const prepared = await prepareRecovery(snapshot, media, signal, progress);
  const identity = [
    prepared.decoded.sourceDigest,
    prepared.media.map((m) => [m.id, m.sha256]),
  ];
  return {
    ...prepared,
    targetId: legacyId("recovered-v4", identity),
    stageId: legacyId("preparing-v4", identity),
  };
}
export async function reviewRecoveryV4(
  sdk: OpenCutClient,
  prepared: PreparedRecovery,
  signal: AbortSignal,
  progress: (p: RecoveryProgress) => void,
): Promise<RecoveryReviewV4> {
  const { source: mediaSource, revision } = await stageRecoveryMedia(
    sdk,
    prepared,
    signal,
    progress,
  );
  const source: RecoveryReviewV4["source"] = {
    ...mediaSource,
    font_mappings: [],
  };
  const analysis = await sdk.call("opencut_classic_analyze_v4", source, {
    signal,
  });
  return { prepared, source, revision, analysis };
}
/** Reanalysis changes only authorized selectors, never the captured source/media revision. */
export async function mapRecoveryFontsV4(
  sdk: OpenCutClient,
  review: RecoveryReviewV4,
  fontMappings: FontMappingV4[],
  signal: AbortSignal,
): Promise<RecoveryReviewV4> {
  const source = {
    ...review.source,
    font_mappings: structuredClone(fontMappings),
  };
  const analysis = await sdk.call("opencut_classic_analyze_v4", source, {
    signal,
  });
  return { ...review, source, analysis };
}
export function recoveryImportRequestV4(
  review: RecoveryReviewV4,
): ToolInput<"opencut_classic_import_v4"> {
  return {
    source: structuredClone(review.source),
    expected_source_content_id: review.analysis.source_content_id,
    accepted_loss_codes: [
      ...new Set(review.analysis.losses.map((loss) => loss.code)),
    ].sort(),
    // A changed font selector is a different request; never reuse the original key.
    idempotency_key: legacyId("recover-v4", [
      review.source,
      review.analysis.source_content_id,
    ]),
  };
}
export async function publishRecoveryV4(
  sdk: OpenCutClient,
  review: RecoveryReviewV4,
  signal: AbortSignal,
) {
  signal.throwIfAborted();
  if (review.analysis.blockers.length || !review.analysis.proposed_project)
    throw new Error(
      "Classic source has unsupported content; original project is preserved",
    );
  // Admission/retry observes this exact request. No current-head inference or promotion.
  const result = await sdk.callWithAdmissionRetry(
    "opencut_classic_import_v4",
    recoveryImportRequestV4(review),
    { beforeAttempt: () => signal.throwIfAborted() },
  );
  return result.target_project_id;
}
export async function recoveryFontProjectsV4(
  sdk: OpenCutClient,
  signal: AbortSignal,
) {
  const items: ToolOutput<"opencut_project_v4_list">["items"] = [];
  let cursor: string | null = null;
  do {
    const page: ToolOutput<"opencut_project_v4_list"> = await sdk.call(
      "opencut_project_v4_list",
      { cursor, limit: 100 },
      { signal },
    );
    items.push(...page.items.filter((item) => item.schema_version === 4));
    cursor = page.next_cursor ?? null;
  } while (cursor);
  return items;
}
export async function recoveryFontSnapshotV4(
  sdk: OpenCutClient,
  projectId: string,
  signal: AbortSignal,
) {
  return sdk.call(
    "opencut_project_v4_inspect",
    { project_id: projectId, revision: null },
    { signal },
  );
}
/** The immutable import event identifies a prior recovery; later edits are preserved. */
export async function findRecoveredV4(
  sdk: OpenCutClient,
  prepared: PreparedRecovery,
  signal: AbortSignal,
): Promise<string | null> {
  let change: ToolOutput<"opencut_event_changes">["events"][number] | undefined;
  let cursor: string | null = null;
  const seen = new Set<string>();
  do {
    signal.throwIfAborted();
    let page: ToolOutput<"opencut_event_changes">;
    try {
      page = await sdk.call(
        "opencut_event_changes",
        {
          scope: { kind: "projects", project_id: prepared.targetId },
          cursor,
          limit: 100,
        },
        { signal },
      );
    } catch (error) {
      if (
        error instanceof OpenCutApiError &&
        ["NOT_FOUND", "PROJECT_NOT_FOUND"].includes(error.code)
      )
        return null;
      throw error;
    }
    change = page.events.find(
      (row) =>
        row.event.type === "project" &&
        row.event.data.event_type === "project.classic_imported_v4",
    );
    if (change || !page.events.length) break;
    cursor = page.next_cursor ?? null;
    if (cursor && seen.has(cursor))
      throw new Error("Recovery event cursor did not advance");
    if (cursor) seen.add(cursor);
  } while (cursor);
  if (!change || change.event.type !== "project") return null;
  const event = change.event.data;
  const initial = await sdk.call(
    "opencut_project_v4_inspect",
    { project_id: prepared.targetId, revision: event.revision },
    { signal },
  );
  const mediaMappings = prepared.media.map((item) => {
    const asset = initial.project.assets.find(
      (asset) =>
        asset.name === item.file.name &&
        asset.version.content_hash === item.sha256,
    );
    if (!asset)
      throw new Error(
        "Recovered project source receipt does not match this Classic library",
      );
    return {
      classic_media_id: item.id,
      source_project_id: prepared.targetId,
      source_revision: initial.revision,
      asset_id: asset.id,
    };
  });
  const report = await sdk.call(
    "opencut_classic_analyze_v4",
    {
      content: prepared.decoded.content,
      target_project_id: prepared.targetId,
      media_mappings: mediaMappings,
      font_mappings: [],
    },
    { signal },
  );
  if (
    !isRecord(event.payload) ||
    event.payload.source_content_id !== report.source_content_id
  )
    throw new Error(
      "The existing recovery receipt differs from this source; no project will be overwritten",
    );
  return prepared.targetId;
}
