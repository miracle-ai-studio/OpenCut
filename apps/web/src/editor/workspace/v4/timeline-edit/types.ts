import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { EditorApi } from "../../api";
import type { V4EditReceipt, V4SessionState } from "../types";

// These aliases resolve from the one generated tool declaration. No public
// timeline DTO is copied into the client while host registration is in flight.
export type TimelineRequest = ToolInput<"opencut_edit_timeline_plan_v4">;
export type TimelinePlan = ToolOutput<"opencut_edit_timeline_plan_v4">;
export type TimelineCommand = TimelineRequest["command"];
export type TimelineTarget = TimelinePlan["decision"]["selection"][number];
export type TimelineScope = { projectId: string; revision: string; sequenceId: string };
export type TimelinePlannerTool = "opencut_edit_timeline_plan_v4" | "opencut_edit_element_timeline_plan_v4";
export type TimelineAttempt = {
  tool?: TimelinePlannerTool;
  request: TimelineRequest;
  plan: TimelinePlan | null;
  receipt: V4EditReceipt | null;
  phase: "planning" | "planned" | "applying" | "unconfirmed" | "conflict" | "failed" | "saved";
  error: string | null;
  recoveryId?: string;
};
export type TimelineEditState = {
  selection: TimelineTarget[];
  attempt: TimelineAttempt | null;
  recoveryError?: string | null;
};
export const emptyTimelineEdit = (): TimelineEditState => ({ selection: [], attempt: null });

/** The enclosing V4Session owns state and the shared write fence. The delegate
 * retains pending command identities only, never a project or independent SDK. */
export type TimelineEditHost = {
  api(): EditorApi | null;
  state(): (V4SessionState & { timelineEdit: TimelineEditState }) | null;
  generation(): number;
  viewGeneration(): number;
  publish(state: TimelineEditState, primaryId?: string | null): void;
  acquire(projectId: string): () => void;
};
export type TimelinePlanner = (api: EditorApi, request: TimelineRequest, signal?: AbortSignal, tool?: TimelinePlannerTool) => Promise<TimelinePlan>;
