import type { ToolInput, ToolOutput, PublicJob } from "../../lib/editor-sdk";

export type BaseSnapshot = ToolOutput<"opencut_project_inspect">;
export type VisualSnapshot = ToolOutput<"opencut_project_v3_inspect">;
export type Time =
  BaseSnapshot["project"]["sequences"][number]["tracks"][number]["clips"][number]["timeline_range"]["start"];
export type Sequence = BaseSnapshot["project"]["sequences"][number];
export type Track = Sequence["tracks"][number];
export type Clip = Track["clips"][number];
export type Asset = BaseSnapshot["project"]["assets"][number];
export type Caption = NonNullable<Sequence["caption_cues"]>[number];
export type Operation =
  ToolInput<"opencut_edit_apply">["request"]["operations"][number];
export type VisualOperation =
  ToolInput<"opencut_edit_v3_apply">["operations"][number];
export type VisualGraph = VisualSnapshot["project"]["visual"][number];
export type Curve = NonNullable<
  VisualSnapshot["project"]["keyframe_curves"]
>[number];
export type TimelineCommand =
  ToolInput<"opencut_edit_timeline_plan">["command"];
export type ProjectEdit =
  ToolInput<"opencut_project_edit_apply">["operations"][number];
export type History = ToolOutput<"opencut_project_history"> & {
  navigation: ToolOutput<"opencut_project_history_navigation"> | null;
};
export type Job = PublicJob;
export type LibraryMetadata =
  ToolOutput<"opencut_project_library">["items"][number];
export type ProjectRef = {
  library?: LibraryMetadata;
  id: string;
  name: string;
  revision: string;
  format: "base" | "visual";
};
export type Snapshot = {
  project: BaseSnapshot["project"];
  revision: string;
  format: ProjectRef["format"];
  visual: VisualGraph[];
  curves: Curve[];
};
export type Selection =
  | { kind: "clip"; trackId: string; id: string }
  | { kind: "caption" | "shape"; id: string }
  | null;
export const normalizeBase = (snapshot: BaseSnapshot): Snapshot => ({
  ...snapshot,
  format: "base",
  visual: [],
  curves: [],
});
export const normalizeVisual = (snapshot: VisualSnapshot): Snapshot => ({
  project: snapshot.project.base,
  revision: snapshot.revision,
  format: "visual",
  visual: snapshot.project.visual,
  curves: snapshot.project.keyframe_curves ?? [],
});
export const projectRef = (snapshot: Snapshot): ProjectRef => ({
  id: snapshot.project.id,
  name: snapshot.project.name,
  revision: snapshot.revision,
  format: snapshot.format,
});
export const activeSequence = (snapshot: Snapshot | null, id?: string | null) =>
  snapshot?.project.sequences.find(
    (sequence) => sequence.id === (id ?? snapshot.project.default_sequence_id),
  ) ?? null;
export const terminal = (job: Job) =>
  ["succeeded", "failed", "cancelled"].includes(job.state);
