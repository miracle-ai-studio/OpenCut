import type {
  OpenCutClient,
  ToolInput,
  ToolOutput,
  CallOptions,
} from "../../../lib/editor-sdk";
import type { Operation, Snapshot } from "./types";

/** Versioned calls share the existing workspace SDK and authenticated session. */
export class Api {
  constructor(readonly sdk: OpenCutClient) {}
  sourceArt(
    request: ToolInput<"opencut_source_art_get">,
    signal?: AbortSignal,
  ) {
    return this.sdk.call("opencut_source_art_get", request, { signal });
  }
  waveformDisplay(
    request: ToolInput<"opencut_source_art_waveform_display_get">,
    signal?: AbortSignal,
  ) {
    return this.sdk.call(
      "opencut_source_art_waveform_display_get",
      request,
      { signal },
    );
  }
  cancelSourceArt(request: ToolInput<"opencut_source_art_cancel">) {
    return this.sdk.call("opencut_source_art_cancel", request);
  }
  visualPreviewFrame(
    request: ToolInput<"opencut_visual_preview_frame">,
    signal?: AbortSignal,
  ) {
    return this.sdk.call("opencut_visual_preview_frame", request, {
      signal,
    });
  }
  audioPreviewCapability() {
    return this.sdk.call("opencut_audio_preview_capability", {});
  }
  audioPreviewCreate(
    request: ToolInput<"opencut_audio_preview_create">,
    signal?: AbortSignal,
  ) {
    return this.sdk.call("opencut_audio_preview_create", request, {
      signal,
    });
  }
  audioPreviewControl(
    request: ToolInput<"opencut_audio_preview_control">,
    signal?: AbortSignal,
    options: Pick<CallOptions, "keepalive"> = {},
  ) {
    return this.sdk.call("opencut_audio_preview_control", request, {
      signal,
      ...options,
    });
  }
  audioGainPlan(request: ToolInput<"opencut_project_audio_gain_plan">) {
    return this.sdk.call("opencut_project_audio_gain_plan", request);
  }
  timelinePlan(request: ToolInput<"opencut_edit_timeline_plan">, signal?: AbortSignal) {
    return this.sdk.call("opencut_edit_timeline_plan", request, { signal });
  }

  timelineCopy(request: ToolInput<"opencut_timeline_copy">, signal?: AbortSignal) {
    return this.sdk.call("opencut_timeline_copy", request, { signal });
  }
  timelinePastePlan(
    request: ToolInput<"opencut_timeline_paste_plan">,
    signal?: AbortSignal,
    operation: "opencut_timeline_paste_plan" | "opencut_timeline_duplicate_plan" = "opencut_timeline_paste_plan",
  ) {
    return this.sdk.call(operation, request, { signal });
  }
  recoveryStage(request: ToolInput<"opencut_edit_recovery_stage">) {
    return this.sdk.call("opencut_edit_recovery_stage", request);
  }
  async recoveryInspect(projectId: string, recoveryId: string | null = null) {
    const inspected = await this.sdk.call("opencut_edit_recovery_inspect", {
      project_id: projectId, recovery_id: recoveryId,
    });
    if (!Object.hasOwn(inspected, "record")) throw new Error("Invalid recovery inspection result.");
    return inspected;
  }
  recoveryAck(projectId: string, recoveryId: string) {
    return this.sdk.call("opencut_edit_recovery_ack", { project_id: projectId, recovery_id: recoveryId });
  }

  mediaPlan(request: ToolInput<"opencut_media_import_plan">) {
    return this.sdk.call("opencut_media_import_plan", request);
  }
  resource(
    projectId: string,
    revision: string,
    resourceId: string,
    signal?: AbortSignal,
  ) {
    return this.sdk.call(
      "opencut_project_resource_inspect",
      {
        project_id: projectId,
        revision,
        resource_id: resourceId,
        kind: "asset",
      },
      { signal },
    );
  }
  availableMedia(projectId:string,signal?:AbortSignal){
    return queryProjectMedia(this.sdk,projectId,signal);
  }
  inspect(projectId: string, revision: string | null = null) {
    return this.sdk.inspectEditorProject( {
      project_id: projectId,
      revision,
    });
  }
  async headRevision(projectId:string) {
    const page=await this.sdk.call("opencut_project_history",{project_id:projectId,from_revision:null,limit:1});
    return page.head_revision;
  }
  async history(projectId: string, revision: string) {
    const [history, navigation] = await Promise.all([
      this.sdk.call("opencut_project_history", {
        project_id: projectId,
        from_revision: revision,
        limit: 50,
      }),
      this.sdk.call("opencut_project_history_navigation", {
        project_id: projectId,
        revision,
      }),
    ]);
    if (history.project_id !== projectId || navigation.project_id !== projectId || navigation.revision !== revision)
      throw Error("History response changed the requested project or revision.");
    const revisionActors = await this.revisionActors(projectId, history.revisions.map(item => item.revision)).catch(() => undefined);
    return { history, navigation, revisionActors };

  }
  async revisionActors(projectId: string, revisions: string[]) {
    if (!revisions.length) return {} as ToolOutput<"opencut_project_revision_actors">["actors"];
    const result = await this.sdk.call("opencut_project_revision_actors", { project_id: projectId, revisions });
    if (result.project_id !== projectId || revisions.some(revision => !Object.hasOwn(result.actors, revision)))
      throw Error("History attribution changed the requested project or revisions.");
    return Object.fromEntries(revisions.map(revision => [revision, result.actors[revision]]));
  }
  request(
    snapshot: Snapshot,
    label: string,
    operations: Operation[],
    idempotencyKey: string = crypto.randomUUID(),
  ): ToolInput<"opencut_edit_apply"> {
    return {
      project_id: snapshot.project.id,
      expected_revision: snapshot.revision,
      idempotency_key: idempotencyKey,
      label,
      operations,
    };
  }
}

export async function queryProjectMedia(sdk:OpenCutClient,projectId:string,signal?:AbortSignal){
    const items: ToolOutput<"opencut_project_media_inspect">[] = [];
    let cursor: string | null = null;
    do {
      const page: ToolOutput<"opencut_project_media_query"> =
        await sdk.call(
          "opencut_project_media_query",
          {
            project_id: projectId,
            limit: 100,
            cursor,
          },
          { signal },
        );
      items.push(...page.media);
      cursor = page.next_cursor ?? null;
    } while (cursor);
    return items;
}
