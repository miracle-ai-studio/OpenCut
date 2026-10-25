import type { ToolInput, ToolOutput } from "../../../lib/editor-sdk";
import type { JobResultMap } from "../../../lib/editor-contracts.generated";
import type { TimelineEditState } from "./timeline-edit/types";
import type { ClipboardState } from "./timeline-edit/clipboard";
import type { EditRecoveryState } from "./edit-recovery";

export type FrameRequest = Extract<ToolInput<"opencut_render_submit">, {kind:"frame"}>["request"];
export type AudioPreviewCreate = ToolInput<"opencut_audio_preview_create">;
export type AudioPreviewControl =
  ToolInput<"opencut_audio_preview_control">;
export type AudioPreviewSession =
  ToolOutput<"opencut_audio_preview_create">;
export type AudioPreviewCapability =
  ToolOutput<"opencut_audio_preview_capability">;
export type AudioPreviewState = {
  phase:
    | "idle"
    | "loading"
    | "playing"
    | "stalled"
    | "paused"
    | "ended"
    | "unconfirmed"
    | "failed";
  /** Preview rendering can fail independently of the authorized audio stream. */
  pictureError?: string | null;
  deviceUnderflowEvents?: number;
  deviceUnderflowFrames?: string;
  capability: AudioPreviewCapability | null;
  session: AudioPreviewSession | null;
  request: AudioPreviewCreate | null;
  error: string | null;
};
export type FrameCapability = ToolOutput<"opencut_render_capability">;
export type FrameResult = JobResultMap["render.frame"];
export type FrameAttempt = {
  request: FrameRequest;
  phase:
    | "submitting"
    | "unconfirmed"
    | "retryable"
    | "accepted"
    | "downloading"
    | "ready"
    | "failed";
  jobId: string | null;
  error: string | null;
};

export type Snapshot = Extract<ToolOutput<"opencut_project_inspect">,{kind:"editor"}>["snapshot"];
export type Document = Snapshot["project"];
export type Sequence = Document["sequences"][number];
export type Element = Sequence["tracks"][number]["elements"][number];
export type Bookmark = Sequence["bookmarks"][number];
export type Operation =
  ToolInput<"opencut_edit_apply">["operations"][number];
export type BookmarkOperation = Extract<
  Operation,
  { type: "bookmark" }
>["operation"];
export type Summary = ToolOutput<"opencut_project_list">["items"][number];
export type Navigation = ToolOutput<"opencut_project_history_navigation">;
export type RevisionPage = ToolOutput<"opencut_project_history">;
export type HistoryPlan = ToolOutput<"opencut_edit_undo_plan">;
export type HistoryRequest =
  | {
      direction: "undo";
      /** Presentation intent only; never part of the native history request. */
      review: boolean;
      input: ToolInput<"opencut_edit_undo_apply">;
      plan: HistoryPlan;
    }
  | {
      direction: "redo";
      review: boolean;
      input: ToolInput<"opencut_edit_redo_apply">;
      plan: HistoryPlan;
    };
export type SourceAudioRequest =
  ToolInput<"opencut_project_source_audio_plan">;
export type SourceAudioPlan =
  ToolOutput<"opencut_project_source_audio_plan">;
export type EditReceipt = ToolOutput<"opencut_edit_apply">;
export type MediaRequest = ToolInput<"opencut_media_import_plan">;
export type MediaPlan = ToolOutput<"opencut_media_import_plan">;
export type AvailableMedia = ToolOutput<"opencut_project_media_inspect">;
export type Asset = Document["assets"][number];
export type MediaAttempt = {
  request: MediaRequest;
  plan: MediaPlan | null;
  receipt: EditReceipt | null;
  phase:
    | "uploading"
    | "planning"
    | "planned"
    | "applying"
    | "unconfirmed"
    | "saved"
    | "failed";
  progress: string | null;
  error: string | null;
};
export type MediaState = {
  attempt: MediaAttempt | null;
  available: AvailableMedia[];
  loading: boolean;
  error: string | null;
};
export type SourceAudioAttempt = {
  request: SourceAudioRequest;
  plan: SourceAudioPlan | null;
  receipt: EditReceipt | null;
  phase:
    "planning" | "applying" | "retryable" | "blocked" | "unconfirmed" | "saved";
  error: string | null;
  headChecked: boolean;
};
export type AudioWavRequest = Extract<ToolInput<"opencut_render_submit">,{kind:"audio"}>["request"];
export type AudioWavCapability = ToolOutput<"opencut_render_capability">;
export type AudioRangeWavRequest = AudioWavRequest & {range:NonNullable<AudioWavRequest["range"]>};
export type AudioRangeWavCapability = AudioWavCapability;
export type AudioWavAttempt = {
  request: AudioWavRequest | AudioRangeWavRequest;
  sceneName: string;
  phase: "submitting" | "unconfirmed" | "retryable" | "accepted";
  jobId: string | null;
  error: string | null;
};
export type SessionState = {
  /** UI-only ownership of unconfirmed property inputs; never project data. */
  propertyDrafts?: readonly string[];
  recovery?: EditRecoveryState;
  timelineEdit: TimelineEditState;
  clipboard: ClipboardState;
  audioPreview: AudioPreviewState;
  head: Snapshot;
  historical: Snapshot | null;
  sequenceId: string;
  playhead: Bookmark["time"];
  selectedId: string | null;
  history: RevisionPage;
  revisionActors?: ToolOutput<"opencut_project_revision_actors">["actors"];
  externalRevision?: string | null;
  navigation: Navigation;
  historyPlan: HistoryRequest | null;
  sourceAudio: SourceAudioAttempt | null;
  audioGain: GainAttempt | null;
  gainDraft: GainDraft | null;
  audioWavCapability: AudioWavCapability | null;
  audioWavCapabilityError: string | null;
  audioRangeWavCapability: AudioRangeWavCapability | null;
  audioRangeWavCapabilityError: string | null;
  audioWav: AudioWavAttempt | null;
  media: MediaState;
  frameCapability: FrameCapability | null;
  frameCapabilityError: string | null;
  frame: FrameAttempt | null;
};

export type GainRequest = ToolInput<"opencut_project_audio_gain_plan">;
export type GainPlan = ToolOutput<"opencut_project_audio_gain_plan">;
export type GainAttempt = {
  request: GainRequest;
  plan: GainPlan | null;
  receipt: EditReceipt | null;
  phase: "planning" | "applying" | "retryable" | "unconfirmed" | "saved";
  error: string | null;
};
/** Presentation draft only; the fixed document remains in the controller. */
export type GainDraft = {
  id: string;
  projectId: string;
  revision: string;
  sequenceId: string;
  trackId: string;
  elementId: string;
  originalDb: number;
  tenths: number;
  changed: boolean;
};

