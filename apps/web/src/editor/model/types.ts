import type { ToolOutput, Job as SdkJob } from "../../lib/editor-sdk";

export type HistoricalSnapshot=Extract<ToolOutput<"opencut_project_inspect">,{kind:"historical"}>["snapshot"];
export type BaseSnapshot=Pick<HistoricalSnapshot,"project"|"revision">;
export type VisualSnapshot=HistoricalSnapshot;
export type Time =
  BaseSnapshot["project"]["sequences"][number]["tracks"][number]["clips"][number]["timeline_range"]["start"];
export type Sequence = BaseSnapshot["project"]["sequences"][number];
export type Track = Sequence["tracks"][number];
export type Clip = Track["clips"][number];
export type Asset = BaseSnapshot["project"]["assets"][number];
export type Caption = NonNullable<Sequence["caption_cues"]>[number];
export type VisualGraph = NonNullable<VisualSnapshot["visual"]>[number];
export type Curve = NonNullable<
  VisualSnapshot["curves"]
>[number];
export type History = ToolOutput<"opencut_project_history"> & {
  navigation: ToolOutput<"opencut_project_history_navigation"> | null;
};
export type Job = SdkJob;
export type LibraryMetadata =
  ToolOutput<"opencut_project_list">["items"][number];
export type ProjectRef = {
  library?: LibraryMetadata;
  id: string;
  name: string;
  revision: string;
  format: "base" | "visual" | "editor" | "readonly";
};
export type Snapshot = {
  project: BaseSnapshot["project"];
  revision: string;
  format: "base" | "visual";
  visual: VisualGraph[];
  curves: Curve[];
  readOnly?: boolean;
};
export type Selection =
  | { kind: "clip"; trackId: string; id: string }
  | { kind: "caption" | "shape"; id: string }
  | null;
export const normalizeBase=(snapshot:BaseSnapshot):Snapshot=>({
  ...snapshot,format:"base",visual:[],curves:[],readOnly:true,
});
export const normalizeHistorical=(snapshot:HistoricalSnapshot):Snapshot=>({
  project:snapshot.project,revision:snapshot.revision,format:snapshot.format,
  visual:snapshot.visual??[],curves:snapshot.curves??[],readOnly:true,
});
export const normalizeVisual=normalizeHistorical;
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
