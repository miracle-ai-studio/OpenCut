import type {
  OpenCutClient,
  ToolInput,
  ToolOutput,
} from "../../../lib/editor-sdk";
import type { V4Operation, V4Snapshot } from "./types";

/** Versioned calls share the existing workspace SDK and authenticated session. */
export class V4Api {
  constructor(readonly sdk: OpenCutClient) {}
  sourceArt(
    request: ToolInput<"opencut_source_art_v1_get">,
    signal?: AbortSignal,
  ) {
    return this.sdk.call("opencut_source_art_v1_get", request, { signal });
  }
  waveformDisplay(
    request: ToolInput<"opencut_source_art_waveform_display_v1_get">,
    signal?: AbortSignal,
  ) {
    return this.sdk.call(
      "opencut_source_art_waveform_display_v1_get",
      request,
      { signal },
    );
  }
  cancelSourceArt(request: ToolInput<"opencut_source_art_v1_cancel">) {
    return this.sdk.call("opencut_source_art_v1_cancel", request);
  }
  visualPreviewFrame(
    request: ToolInput<"opencut_visual_preview_v4_frame">,
    signal?: AbortSignal,
  ) {
    return this.sdk.call("opencut_visual_preview_v4_frame", request, {
      signal,
    });
  }
  audioPreviewCapability() {
    return this.sdk.call("opencut_audio_preview_v4_capability", {});
  }
  audioPreviewCreate(
    request: ToolInput<"opencut_audio_preview_v4_create">,
    signal?: AbortSignal,
  ) {
    return this.sdk.call("opencut_audio_preview_v4_create", request, {
      signal,
    });
  }
  audioPreviewControl(
    request: ToolInput<"opencut_audio_preview_v4_control">,
    signal?: AbortSignal,
  ) {
    return this.sdk.call("opencut_audio_preview_v4_control", request, {
      signal,
    });
  }
  audioGainPlan(request: ToolInput<"opencut_project_v4_audio_gain_plan">) {
    return this.sdk.call("opencut_project_v4_audio_gain_plan", request);
  }
  timelinePlan(
    request: ToolInput<"opencut_edit_timeline_plan_v4">,
    signal?: AbortSignal,
  ) {
    return this.sdk.call("opencut_edit_timeline_plan_v4", request, { signal });
  }
  timelineCopy(
    request: ToolInput<"opencut_timeline_v4_copy">,
    signal?: AbortSignal,
  ) {
    return this.sdk.call("opencut_timeline_v4_copy", request, { signal });
  }
  timelinePastePlan(
    request: ToolInput<"opencut_timeline_v4_paste_plan">,
    signal?: AbortSignal,
  ) {
    return this.sdk.call("opencut_timeline_v4_paste_plan", request, { signal });
  }
  recoveryStage(request: ToolInput<"opencut_edit_recovery_v1_stage">) {
    return this.sdk.call("opencut_edit_recovery_v1_stage", request);
  }
  async recoveryRead(projectId: string, recoveryId: string | null = null) {
    const inspected = await this.sdk.call("opencut_edit_recovery_v1_inspect", {
      project_id: projectId,
      recovery_id: recoveryId,
    });
    if (inspected.schema_version !== 1 || !Object.hasOwn(inspected, "record"))
      throw new Error("Invalid recovery inspection result.");
    return inspected.record ?? null;
  }
  recoveryAck(projectId: string, recoveryId: string) {
    return this.sdk.call("opencut_edit_recovery_v1_ack", {
      project_id: projectId,
      recovery_id: recoveryId,
    });
  }
  recoveryV2Stage(request: ToolInput<"opencut_edit_recovery_v2_stage">) {
    return this.sdk.call("opencut_edit_recovery_v2_stage", request);
  }
  async recoveryV2Inspect(projectId: string, recoveryId: string | null = null) {
    const inspected = await this.sdk.call("opencut_edit_recovery_v2_inspect", {
      project_id: projectId,
      recovery_id: recoveryId,
    });
    if (inspected.schema_version !== 2 || !Object.hasOwn(inspected, "record"))
      throw new Error("Invalid recovery inspection result.");
    return inspected;
  }
  recoveryV2Ack(projectId: string, recoveryId: string) {
    return this.sdk.call("opencut_edit_recovery_v2_ack", {
      project_id: projectId,
      recovery_id: recoveryId,
    });
  }
  frameCapability() {
    return this.sdk.call("opencut_project_v4_frame_capability", {});
  }
  framePng(request: ToolInput<"opencut_job_v4_frame_png">) {
    return this.sdk.call("opencut_job_v4_frame_png", request);
  }
  audioRangeCapability() {
    return this.sdk.call("opencut_project_v4_audio_range_capability", {});
  }
  audioRangeWav(request: ToolInput<"opencut_job_v4_audio_range_wav">) {
    return this.sdk.call("opencut_job_v4_audio_range_wav", request);
  }
  audioCapability() {
    return this.sdk.call("opencut_project_v4_audio_capability", {});
  }
  audioWav(request: ToolInput<"opencut_job_v4_audio_wav">) {
    return this.sdk.call("opencut_job_v4_audio_wav", request);
  }
  mediaPlan(request: ToolInput<"opencut_media_workspace_plan_v4">) {
    return this.sdk.call("opencut_media_workspace_plan_v4", request);
  }
  resource(
    projectId: string,
    revision: string,
    resourceId: string,
    signal?: AbortSignal,
  ) {
    return this.sdk.call(
      "opencut_project_v4_resource_inspect",
      {
        project_id: projectId,
        revision,
        resource_id: resourceId,
        kind: "asset",
      },
      { signal },
    );
  }
  async availableMedia(projectId: string, signal?: AbortSignal) {
    const items: ToolOutput<"opencut_project_media_inspect">[] = [];
    let cursor: string | null = null;
    do {
      const page: ToolOutput<"opencut_project_media_query"> =
        await this.sdk.call(
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
  inspect(projectId: string, revision: string | null = null) {
    return this.sdk.call("opencut_project_v4_inspect", {
      project_id: projectId,
      revision,
    });
  }
  async projects(signal?: AbortSignal) {
    const items: ToolOutput<"opencut_project_v4_list">["items"] = [];
    let cursor: string | null = null;
    do {
      const page: ToolOutput<"opencut_project_v4_list"> = await this.sdk.call(
        "opencut_project_v4_list",
        { cursor, limit: 100 },
        { signal },
      );
      items.push(...page.items.filter((item) => item.schema_version === 4));
      cursor = page.next_cursor ?? null;
    } while (cursor);
    return items;
  }
  create(name: string, width: number, height: number, fps: number) {
    return this.sdk.call("opencut_project_v4_create", {
      project: {
        id: crypto.randomUUID(),
        name,
        sequence_id: crypto.randomUUID(),
        width,
        height,
        fps: { numerator: fps, denominator: 1 },
      },
      idempotency_key: crypto.randomUUID(),
    });
  }
  async history(projectId: string, revision: string) {
    const [history, navigation] = await Promise.all([
      this.sdk.call("opencut_project_history", {
        project_id: projectId,
        limit: 50,
      }),
      this.sdk.call("opencut_project_v4_history_navigation", {
        project_id: projectId,
        revision,
      }),
    ]);
    return { history, navigation };
  }
  request(
    snapshot: V4Snapshot,
    label: string,
    operations: V4Operation[],
  ): ToolInput<"opencut_edit_v4_apply"> {
    return {
      project_id: snapshot.project.id,
      expected_revision: snapshot.revision,
      idempotency_key: crypto.randomUUID(),
      label,
      operations,
    };
  }
}
