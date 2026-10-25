import { OpenCutApiError, jobResult } from "../../../lib/editor-sdk";
import type {
  OpenCutClient,
  ToolInput,
  ToolOutput,
} from "../../../lib/editor-sdk";
import { decodeClassic } from "./decode";
import type { ClassicSnapshot } from "./decode";
import { fileDigest, legacyId } from "./identity";
import type { RecoveryMedia } from "./bundle";
import { isRecord } from "./legacy/utils";
type StageSnapshot = Extract<ToolOutput<"opencut_project_inspect">,{kind:"editor"}>["snapshot"];
import { queryProjectMedia } from "../scene/api";
export type RecoveryProgress = {
  phase: "checking" | "uploading" | "preparing" | "review" | "publishing";
  name?: string;
  index?: number;
  count?: number;
  percent?: number;
};
export type PreparedRecovery = {
  snapshot: ClassicSnapshot;
  decoded: ReturnType<typeof decodeClassic>;
  media: RecoveryMedia[];
  targetId: string;
  stageId: string;
  name: string;
};
type RecoverySource=ToolInput<"opencut_classic_analyze">;
export async function prepareRecovery(
  snapshot: ClassicSnapshot,
  media: RecoveryMedia[],
  signal: AbortSignal,
  progress: (p: RecoveryProgress) => void,
): Promise<PreparedRecovery> {
  const decoded = decodeClassic(snapshot);
  if (
    media.length !== snapshot.media.length ||
    new Set(media.map((m) => m.id)).size !== media.length
  )
    throw new Error("Recovery needs every original library file");
  const sorted = [...media].sort((a, b) =>
    a.id < b.id ? -1 : a.id > b.id ? 1 : 0,
  );
  for (const [index, item] of sorted.entries()) {
    progress({
      phase: "checking",
      name: item.file.name,
      index,
      count: media.length,
    });
    if (
      !snapshot.media.some((m) => m.id === item.id) ||
      (await fileDigest(item.file, signal)) !== item.sha256
    )
      throw new Error("Classic media identity changed");
  }
  const metadata = isRecord(snapshot.raw.metadata)
    ? snapshot.raw.metadata
    : snapshot.raw;
  return {
    snapshot,
    decoded,
    media: sorted,
    targetId: legacyId("recovered", [
      decoded.sourceDigest,
      sorted.map((m) => [m.id, m.sha256]),
    ]),
    stageId: legacyId("preparing", [
      decoded.sourceDigest,
      sorted.map((m) => [m.id, m.sha256]),
    ]),
    name: typeof metadata.name === "string" ? metadata.name : "Classic project",
  };
}
const assetId = (prepared: PreparedRecovery, id: string) =>
  legacyId("media", [prepared.targetId, id]);
async function stageSnapshot(sdk:OpenCutClient,projectId:string,revision:string|null=null,signal?:AbortSignal){
  return sdk.inspectEditorProject({project_id:projectId,revision},{signal});
}
async function guardStage(sdk:OpenCutClient,stage:StageSnapshot,initial:StageSnapshot,prepared:PreparedRecovery,signal?:AbortSignal){
  if(JSON.stringify({...stage.project,assets:[]})!==JSON.stringify({...initial.project,assets:[]}))
    throw new Error("The recovery preparation project was edited. Its changes are preserved; recovery cannot overwrite it.");
  if(!stage.project.assets.length)return;
  const available=await queryProjectMedia(sdk,prepared.stageId,signal);
  const operations:ToolInput<"opencut_edit_plan">["operations"]=[];
  for(const asset of stage.project.assets){
    const item=prepared.media.find(m=>assetId(prepared,m.id)===asset.id);
    const original=available.find(m=>m.asset.version?.content_hash===asset.version.content_hash
      && m.asset.version.id===asset.version.id && m.asset.version.size===asset.version.size
      && m.asset.name===item?.file.name);
    if(!item || item.file.name!==asset.name || item.sha256!==asset.version.content_hash || !original)
      throw new Error("The recovery preparation library changed. Its content is preserved.");
    operations.push({type:"asset_bind",id:asset.id,name:asset.name,
      source:{kind:"project_media",media_id:original.media_id,version:asset.version}});
  }
  // Reuse Rust's original authorized resource conversion,including packet FPS
  // and provenance. The browser never constructs or guesses typed metadata.
  for(let offset=0;offset<operations.length;offset+=128){
    const expected=await sdk.call("opencut_edit_plan",{
      project_id:prepared.stageId,expected_revision:stage.revision,
      idempotency_key:legacyId("check-preparation",[stage.revision,offset]),
      label:"Check prepared library",operations:operations.slice(offset,offset+128),
    },{signal});
    if(JSON.stringify(expected)!==JSON.stringify(stage.project))
      throw new Error("The recovery preparation library changed. Its content is preserved.");
  }
}
async function stageInitial(sdk:OpenCutClient,prepared:PreparedRecovery,signal?:AbortSignal){
  const receipt=await sdk.callWithAdmissionRetry("opencut_project_create",{
    project:{id:prepared.stageId,name:`Preparing Classic: ${prepared.name}`,sequence_id:"main",width:320,height:180,
      fps:{numerator:24,denominator:1}},idempotency_key:`prepare-${prepared.targetId}`,
  },{signal});
  return stageSnapshot(sdk,prepared.stageId,receipt.revision,signal);
}
// Durable native sessions are the journal. Pausing cancels observation/transfers,
// not accepted imports or the old source; the same key resolves confirmed offset.
async function upload(
  sdk: OpenCutClient,
  prepared: PreparedRecovery,
  item: RecoveryMedia,
  signal: AbortSignal,
  progress: (p: RecoveryProgress) => void,
) {
  const capabilities = await sdk.hostCapabilities({ signal });
  if (
    !capabilities.uploads ||
    BigInt(item.file.size) > BigInt(capabilities.uploads.max_source_bytes)
  )
    throw new Error("Classic media size is outside exact source addressability");
  const chunkSize = Math.min(
    Number(capabilities.uploads.max_chunk_bytes),
    4 * 1024 * 1024,
  );
  if (!Number.isSafeInteger(chunkSize) || chunkSize < 1)
    throw new Error("Invalid upload chunk limit");
  for (let generation = 0; generation < 256; generation++) {
    let session = await sdk.callWithAdmissionRetry(
      "opencut_upload_begin",
      {
        project_id: prepared.stageId,
        idempotency_key: `${assetId(prepared, item.id)}-${generation}`,
        name: item.file.name,
        size_bytes: String(item.file.size),
      },
      { signal },
    );
    // Read authoritative state even if a begin response was a stored projection.
    session = await sdk.call(
      "opencut_upload_inspect",
      { upload_id: session.upload_id },
      { signal },
    );
    if (
      session.project_id !== prepared.stageId ||
      session.name !== item.file.name ||
      session.size_bytes !== String(item.file.size) ||
      (session.content_hash && session.content_hash !== item.sha256)
    )
      throw new Error("Native recovery upload identity changed");
    if (session.state === "aborted") continue;
    if (session.job_id) {
      const job = await sdk.call(
        "opencut_job_view",
        { job_id: session.job_id },
        { signal },
      );
      if (job.state === "failed" || job.state === "cancelled") continue;
    } else {
      let offset = Number(session.next_offset);
      if (
        !Number.isSafeInteger(offset) ||
        offset < 0 ||
        offset > item.file.size
      )
        throw new Error("Invalid confirmed upload offset");
      while (offset < item.file.size) {
        signal.throwIfAborted();
        const bytes = new Uint8Array(
          await item.file.slice(offset, offset + chunkSize).arrayBuffer(),
        );
        signal.throwIfAborted();
        const reply = await sdk.uploadChunk(
          session.upload_id,
          String(offset),
          bytes,
          { signal },
        );
        if (reply.next_offset !== String(offset + bytes.length))
          throw new Error(
            "Upload acknowledgement is uncertain; retry to inspect its confirmed offset",
          );
        offset += bytes.length;
        progress({
          phase: "uploading",
          name: item.file.name,
          percent: Math.round((offset / item.file.size) * 100),
        });
      }
      session = await sdk.callWithAdmissionRetry(
        "opencut_upload_complete",
        { upload_id: session.upload_id, content_hash: item.sha256 },
        { signal },
      );
    }
    if (!session.job_id) throw new Error("Recovery import has no accepted job");
    progress({ phase: "preparing", name: item.file.name });
    const job = await sdk.waitForJob(session.job_id, {
      signal,
      timeoutMs: 300000,
      pollIntervalMs: 300,
    });
    const result = jobResult(job, "media.import");
    const media = await sdk.call(
      "opencut_project_media_inspect",
      { project_id: prepared.stageId, media_id: result.media_id },
      { signal },
    );
    if (
      media.asset.version?.content_hash !== item.sha256 ||
      media.asset.version.size !== item.file.size
    )
      throw new Error("Published recovery media does not match original bytes");
    // Media is independently published before preparation binding. Release only
    // our terminal upload; deletion retries any still-pending staging cleanup.
    try {
      await sdk.call("opencut_upload_release", {
        upload_id: session.upload_id,
      });
    } catch {
      /* native discoverable retention remains */
    }
    return {id:assetId(prepared,item.id),name:item.file.name,
      media_id:media.media_id,version:media.asset.version};
  }
  throw new Error(
    "Too many failed recovery attempts; original source and native upload records remain available",
  );
}
export async function findRecovered(
  sdk: OpenCutClient,
  prepared: PreparedRecovery,
  signal: AbortSignal,
): Promise<string | null> {
  let events: ToolOutput<"opencut_event_changes">;
  try {
    events = await sdk.call(
      "opencut_event_changes",
      {
        scope: { kind: "projects", project_id: prepared.targetId },
        cursor: null,
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
  const change = events.events.find(
    (change) =>
      change.event.type === "project" &&
      change.event.data.event_type === "project.classic_recovered",
  );
  if (!change || change.event.type !== "project") return null;
  const event = change.event.data;
  const inspected = await sdk.call(
    "opencut_project_inspect",
    { project_id: prepared.targetId, revision: event.revision },
    { signal },
  );
  if(inspected.kind!=="historical")
    throw new Error("The historical recovery receipt refers to different project content.");
  const initial=inspected.snapshot;
  const mappings = prepared.media.map((item) => {
    const asset = initial.project.assets.find(
      (asset) =>
        asset.name === item.file.name &&
        asset.version?.content_hash === item.sha256,
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
    "opencut_classic_analyze",
    {
      content: prepared.decoded.content,
      target_project_id: prepared.targetId,
      media_mappings: mappings,
    },
    { signal },
  );
  if (
    !isRecord(event.payload) ||
    event.payload.source_content_id !== report.source_content_id ||
    initial.project.assets.length!==prepared.media.length
  )
    throw new Error(
      "The existing recovery receipt differs from this source; no project will be overwritten",
    );
  // This only discovers and opens the immutable historical publication. The
  // current parser's editable projection is not the original stored format.
  // Its durable event/source identity and every original media hash must match;
  // no publication, overwrite or claim of current-format parity happens here.
  return prepared.targetId;
}
export async function stageRecoveryMedia(
  sdk: OpenCutClient,
  prepared: PreparedRecovery,
  signal: AbortSignal,
  progress: (p: RecoveryProgress) => void,
): Promise<{ source: RecoverySource; revision: string }> {
  const initial = await stageInitial(sdk, prepared, signal);
  let stage = await stageSnapshot(sdk,prepared.stageId,null,signal);
  await guardStage(sdk, stage, initial, prepared, signal);
  for (const [index, item] of prepared.media.entries()) {
    signal.throwIfAborted();
    progress({
      phase: "uploading",
      name: item.file.name,
      index,
      count: prepared.media.length,
      percent: 0,
    });
    if (stage.project.assets.some((a) => a.id === assetId(prepared, item.id)))
      continue;
    const asset = await upload(sdk, prepared, item, signal, (p) =>
      progress({ ...p, index, count: prepared.media.length }),
    );
    stage = await stageSnapshot(sdk,prepared.stageId,null,signal);
    await guardStage(sdk, stage, initial, prepared, signal);
    if (!stage.project.assets.some((a) => a.id === asset.id)) {
      signal.throwIfAborted();
      await sdk.callWithAdmissionRetry(
        "opencut_edit_apply",
        {
          project_id: prepared.stageId,
          expected_revision: stage.revision,
          idempotency_key: `bind-${asset.id}`,
          label: "Recover Classic library file",
          operations: [{type:"asset_bind",id:asset.id,name:asset.name,
            source:{kind:"project_media",media_id:asset.media_id,version:asset.version}}],
        },
        { signal, beforeAttempt: () => signal.throwIfAborted() },
      );
      stage = await stageSnapshot(sdk,prepared.stageId,null,signal);
      await guardStage(sdk, stage, initial, prepared, signal);
    }
  }
  signal.throwIfAborted();
  progress({ phase: "review" });
  const source: RecoverySource = {
    content: prepared.decoded.content,
    target_project_id: prepared.targetId,
    font_mappings: [],
    media_mappings: prepared.media.map((item) => ({
      classic_media_id: item.id,
      source_project_id: prepared.stageId,
      source_revision: stage.revision,
      asset_id: assetId(prepared, item.id),
    })),
  };
  return { source, revision: stage.revision };
}
export async function cleanupRecovery(
  sdk: OpenCutClient,
  prepared: PreparedRecovery,
): Promise<string | null> {
  try {
    let stage: StageSnapshot;
    try {
      stage = await stageSnapshot(sdk,prepared.stageId);
    } catch (error) {
      if (
        error instanceof OpenCutApiError &&
        ["NOT_FOUND", "PROJECT_NOT_FOUND"].includes(error.code)
      )
        return null;
      throw error;
    }
    const initial = await stageInitial(sdk, prepared);
    await guardStage(sdk, stage, initial, prepared);
    const deleted = await sdk.call("opencut_project_delete", {
      projects: [
        { project_id: prepared.stageId, expected_revision: stage.revision },
      ],
      idempotency_key: `cleanup-${prepared.targetId}`,
    });
    return deleted.cleanup_pending.length
      ? "Recovery completed. Temporary preparation cleanup remains pending."
      : null;
  } catch (error) {
    return String(error);
  }
}
