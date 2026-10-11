import { TaskFeed } from "./task-feed";
import { OpenCutClient, OpenCutApiError } from "../../lib/editor-sdk";
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
  readonly sdk: OpenCutClient;
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
    return items;
  }

  async inspect(
    project: ProjectRef,
    revision?: string,
    signal?: AbortSignal,
  ): Promise<Snapshot> {
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
    idempotencyKey = crypto.randomUUID(),
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
