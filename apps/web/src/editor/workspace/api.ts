import { TaskFeed } from "./task-feed";
import { V4Api } from "./v4/api";
import { ProjectCovers, thumbnail } from "./project-covers";
import type { CoverRequest } from "./project-covers";
import { CoverSceneViews } from "../shared/view-preferences";
import {
  OpenCutClient,
  OpenCutApiError,
  JobCompletionError,
} from "../../lib/editor-sdk";
import type { ToolInput, ToolOutput } from "../../lib/editor-sdk";
import { normalizeBase, normalizeVisual } from "../model/types";
import type {
  Operation,
  ProjectRef,
  Snapshot,
  VisualOperation,
  TimelineCommand,
  ProjectEdit,
} from "../model/types";

export class EditorApi {
  private authenticatedIdentity: string | null = null;
  /** In-memory recovery scope, established only by the authenticated service.
   * It contains service/principal/credential identity, never the bearer token. */
  get connectionIdentity() { return this.authenticatedIdentity; }
  async authenticateConnection() {
    if (this.authenticatedIdentity !== null) return;
    const access = await this.sdk.call("opencut_access_inspect", {});
    if (this.authenticatedIdentity !== null) return;
    if (!access.principal_id || typeof access.owner !== "boolean" ||
        (access.credential_id != null && typeof access.credential_id !== "string"))
      throw new Error("The service returned an invalid connection identity.");
    this.authenticatedIdentity = JSON.stringify([
      this.sdk.baseUrl, access.principal_id, access.credential_id ?? null, access.owner,
    ]);
  }
  readonly covers = new ProjectCovers((project, signal) =>
    this.projectCover(project, signal),
  );
  readonly sdk: OpenCutClient;
  readonly v4: V4Api;
  readonly coverViews: CoverSceneViews;
  private feed: TaskFeed;
  constructor(endpoint: string, token: string) {
    this.sdk = new OpenCutClient({
      baseUrl: endpoint,
      token,
      fetch: token
        ? undefined
        : async (input, init) => {
            const headers = new Headers(init?.headers);
            headers.delete("Authorization");
            return fetch(input, {
              ...init,
              headers,
              credentials: "same-origin",
            });
          },
    });
    this.coverViews = new CoverSceneViews(this.sdk.baseUrl);
    this.v4 = new V4Api(this.sdk);
    this.feed = new TaskFeed(this.sdk);
  }
  async projects(signal?: AbortSignal): Promise<ProjectRef[]> {
    const items: ProjectRef[] = [];
    let cursor: string | null = null;
    do {
      const page: ToolOutput<"opencut_project_library"> = await this.sdk.call(
        "opencut_project_library",
        { cursor, limit: 100 },
        { signal },
      );
      items.push(
        ...page.items.map((item) => ({
          id: item.project_id,
          name: item.name,
          revision: item.revision,
          format: item.format,
          library: item,
        })),
      );
      cursor = page.next_cursor ?? null;
    } while (cursor);
    const v4 = await this.v4.projects(signal);
    items.push(
      ...v4.map((item) => ({
        id: item.project_id,
        name: item.name,
        revision: item.revision,
        format: "v4" as const,
        v4: item,
      })),
    );
    this.covers.retain(items);
    return items;
  }

  private async projectCover(
    project: CoverRequest,
    signal: AbortSignal,
  ): Promise<Blob | null> {
    if (project.format === "v4") return null;
    const snapshot = await this.inspect(project, project.revision, signal);
    if (
      snapshot.revision !== project.revision ||
      snapshot.project.id !== project.id
    )
      throw new Error("Project cover revision changed");
    const sequenceId = snapshot.project.sequences.some(
      (s) => s.id === project.coverSequenceId,
    )
      ? project.coverSequenceId!
      : snapshot.project.default_sequence_id;
    const sequence = snapshot.project.sequences.find(
      (s) => s.id === sequenceId,
    );
    if (!sequence) throw new Error("Project cover scene unavailable");
    const hasContent =
      sequence.tracks.some((t) => t.clips.length) ||
      sequence.caption_cues?.length ||
      snapshot.visual.some(
        (v) => v.sequence_id === sequenceId && v.shapes.length,
      );
    if (!hasContent) return null;
    const input = {
      project_id: project.id,
      revision: project.revision,
      sequence_id: sequenceId,
      requested_time: { ticks: "0", timebase: 1 },
      alignment: "nearest" as const,
    };
    signal.throwIfAborted();
    // Observe acceptance even if navigation happens while the request is in flight,
    // then cancel the known task instead of hiding an unknown acceptance outcome.
    const accepted =
      snapshot.format === "visual"
        ? await this.sdk.callWithAdmissionRetry(
            "opencut_job_v3_frame",
            { ...input, idempotency_key: crypto.randomUUID() },
            { beforeAttempt: () => signal.throwIfAborted() },
          )
        : await this.sdk.callWithAdmissionRetry(
            "opencut_job_submit",
            {
              kind: "preview.composited_frame",
              input,
              idempotency_key: crypto.randomUUID(),
            },
            { beforeAttempt: () => signal.throwIfAborted() },
          );
    try {
      signal.throwIfAborted();
      const job = await this.sdk.waitForPublicJob(accepted.job_id, {
        signal,
        timeoutMs: 180000,
        pollIntervalMs: 300,
      });
      const artifact = job.artifacts.find((a) => a.mime_type === "image/png");
      if (!artifact) throw new Error("Project cover unavailable");
      const response = await this.sdk.downloadArtifact(artifact, { signal });
      if (!response.body) throw new Error("Project cover unavailable");
      const reader = response.body.getReader(),
        parts: BlobPart[] = [];
      let size = 0;
      try {
        for (;;) {
          const { value, done } = await reader.read();
          if (done) break;
          size += value.byteLength;
          if (size > 32 * 1024 * 1024)
            throw new Error("Project cover exceeds image limit");
          parts.push(new Uint8Array(value));
        }
      } finally {
        await reader.cancel().catch(() => {});
        reader.releaseLock();
      }
      return await thumbnail(new Blob(parts, { type: "image/png" }), signal);
    } finally {
      if (signal.aborted) {
        const cancelled = await this.sdk.cancelPublicJob(accepted.job_id);
        if (cancelled.state === "queued" || cancelled.state === "running") {
          try {
            await this.sdk.waitForPublicJob(accepted.job_id, {
              timeoutMs: 30000,
              pollIntervalMs: 200,
            });
          } catch (error) {
            if (!(error instanceof JobCompletionError)) throw error;
          }
        }
      }
    }
  }

  async inspect(
    project: ProjectRef,
    revision?: string,
    signal?: AbortSignal,
  ): Promise<Snapshot> {
    if (project.format === "v4")
      throw new Error("Use the complete V4 workspace to inspect this project.");
    if (project.format === "visual") {
      try {
        return normalizeVisual(
          await this.sdk.call(
            "opencut_project_v3_inspect",
            { project_id: project.id, revision: revision ?? null },
            { signal },
          ),
        );
      } catch (error) {
        if (
          !revision ||
          !(error instanceof OpenCutApiError) ||
          error.code !== "UNSUPPORTED_PROJECT_VERSION"
        )
          throw error;
      }
    }
    return normalizeBase(
      revision
        ? await this.sdk.call(
            "opencut_revision_inspect",
            { project_id: project.id, revision },
            { signal },
          )
        : await this.sdk.call(
            "opencut_project_inspect",
            { project_id: project.id },
            { signal },
          ),
    );
  }
  async create(name: string, width: number, height: number, fps: number) {
    return normalizeVisual(
      await this.sdk.call("opencut_project_v3_create", {
        project: {
          id: crypto.randomUUID(),
          name,
          sequence_id: crypto.randomUUID(),
          width,
          height,
          fps: { numerator: fps, denominator: 1 },
        },
        idempotency_key: crypto.randomUUID(),
      }),
    );
  }
  async history(projectId: string, fromRevision: string | null = null) {
    const page = await this.sdk.call("opencut_project_history", {
      project_id: projectId,
      from_revision: fromRevision,
      limit: 50,
    });
    const navigation = fromRevision
      ? null
      : await this.sdk.call("opencut_project_history_navigation", {
          project_id: projectId,
          revision: page.head_revision,
        });
    return { ...page, navigation };
  }
  async apply(
    snapshot: Snapshot,
    label: string,
    operations: Operation[],
    visual: VisualOperation[] = [],
    idempotencyKey: string = crypto.randomUUID(),
    rippleSequenceId?: string,
    mediaImport?: {
      sequenceId: string;
      media: ToolInput<"opencut_media_import_plan_v1">["media"];
      beforeApply: () => void;
      ripple: boolean;
      signal: AbortSignal;
    },
  ) {
    if (snapshot.format === "visual") {
      const request: ToolInput<"opencut_edit_v3_apply"> = {
        project_id: snapshot.project.id,
        expected_revision: snapshot.revision,
        idempotency_key: idempotencyKey,
        label,
        operations: [
          ...operations.map((operation) => ({
            type: "base.apply" as const,
            operation,
          })),
          ...visual,
        ],
      };
      if (mediaImport) {
        const result = await this.sdk.callWithAdmissionRetry(
          "opencut_media_import_plan_v1",
          {
            project_id: snapshot.project.id,
            expected_revision: snapshot.revision,
            sequence_id: mediaImport.sequenceId,
            idempotency_key: idempotencyKey,
            label,
            media: mediaImport.media,
            ripple: mediaImport.ripple,
            operations: request.operations.filter(
              (op) =>
                !(
                  op.type === "base.apply" &&
                  ["asset.bind", "asset.replace"].includes(op.operation.type)
                ),
            ),
          },
          { signal: mediaImport.signal },
        );
        return this.sdk.callWithAdmissionRetry(
          "opencut_edit_v3_apply",
          result.transaction,
          {
            signal: mediaImport.signal,
            beforeAttempt: mediaImport.beforeApply,
          },
        );
      }
      if (rippleSequenceId) {
        const result = await this.sdk.call("opencut_edit_classic_plan_v1", {
          project_id: snapshot.project.id,
          expected_revision: snapshot.revision,
          sequence_id: rippleSequenceId,
          idempotency_key: idempotencyKey,
          label,
          ripple: true,
          command: { kind: "apply", operations: request.operations },
        });
        return this.sdk.call("opencut_edit_v3_apply", result.plan.transaction);
      }
      await this.sdk.call("opencut_edit_v3_plan", request);
      return this.sdk.call("opencut_edit_v3_apply", request);
    }
    if (visual.length)
      throw new Error(
        "Effects are available in projects created with this editor.",
      );
    const input = {
      project_id: snapshot.project.id,
      request: {
        expected_revision: snapshot.revision,
        idempotency_key: idempotencyKey,
        label,
        operations,
      },
    };
    await this.sdk.call("opencut_edit_plan", input);
    mediaImport?.beforeApply();
    return this.sdk.call("opencut_edit_apply", input);
  }
  async projectEdit(
    snapshot: Snapshot,
    label: string,
    operations: ProjectEdit[],
  ) {
    const request = {
      project_id: snapshot.project.id,
      expected_revision: snapshot.revision,
      idempotency_key: crypto.randomUUID(),
      label,
      operations,
    };
    return this.sdk.call("opencut_project_edit_apply", request);
  }
  async timeline(
    snapshot: Snapshot,
    sequenceId: string,
    label: string,
    command: TimelineCommand,
  ) {
    const plan = await this.sdk.call("opencut_edit_timeline_plan", {
      project_id: snapshot.project.id,
      expected_revision: snapshot.revision,
      sequence_id: sequenceId,
      idempotency_key: crypto.randomUUID(),
      label,
      command,
    });
    return plan;
  }
  tasks(projectId: string, signal?: AbortSignal) {
    return this.feed.read(projectId, signal);
  }
}
