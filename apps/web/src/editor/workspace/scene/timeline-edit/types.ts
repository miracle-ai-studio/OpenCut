import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { EditorApi } from "../../api";
import type { EditReceipt, SessionState } from "../types";

// These aliases resolve from the one generated tool declaration. No public
// timeline DTO is copied into the client while host registration is in flight.
export type TimelineRequest = ToolInput<"opencut_edit_timeline_plan">;
export type TimelinePlan = ToolOutput<"opencut_edit_timeline_plan">;
export type TimelineCommand = TimelineRequest["command"];
export type TimelineTarget = TimelinePlan["decision"]["selection"][number];
export type TimelineScope = {
  projectId: string;
  revision: string;
  sequenceId: string;
};
export type TimelineAttempt = {
  automatic?: boolean;
  request: TimelineRequest;
  plan: TimelinePlan | null;
  receipt: EditReceipt | null;
  phase:
    | "planning"
    | "planned"
    | "applying"
    | "unconfirmed"
    | "conflict"
    | "failed"
    | "saved";
  error: string | null;
};
export type TimelineEditState = {
  selection: TimelineTarget[];
  attempt: TimelineAttempt | null;
  recoveryError?: string | null;
};
export const emptyTimelineEdit = (): TimelineEditState => ({
  selection: [],
  attempt: null,
});

/** The enclosing V4Session owns state and the shared write fence. The delegate
 * retains pending command identities only, never a project or independent SDK. */
export type TimelineEditHost = {
  report?(error: unknown): void;
  api(): EditorApi | null;
  state(): (SessionState & { timelineEdit: TimelineEditState }) | null;
  generation(): number;
  viewGeneration(): number;
  publish(state: TimelineEditState, primaryId?: string | null): void;
  acquire(projectId: string): () => void;
};
export type TimelinePlanner = (
  api: EditorApi,
  request: TimelineRequest,
  signal?: AbortSignal,
) => Promise<TimelinePlan>;
