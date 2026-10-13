import type { ToolInput, ToolOutput } from "../../../lib/editor-sdk";
import type { JobResultMap } from "../../../lib/editor-contracts.generated";
import type { TimelineEditState } from "./timeline-edit/types";
import type { ClipboardState } from "./timeline-edit/clipboard";
import type { FontRecoveryState } from "./text/font-recovery";
import type { EditRecoveryState } from "./edit-recovery";

export type V4FrameRequest = ToolInput<"opencut_job_v4_frame_png"> | ToolInput<"opencut_job_v4_text_frame_png">;
export type V4AudioPreviewCreate = ToolInput<"opencut_audio_preview_v4_create">;
export type V4AudioPreviewControl =
  ToolInput<"opencut_audio_preview_v4_control">;
export type V4AudioPreviewSession =
  ToolOutput<"opencut_audio_preview_v4_create">;
export type V4AudioPreviewCapability =
  ToolOutput<"opencut_audio_preview_v4_capability">;
export type V4AudioPreviewState = {
  phase:
    | "idle"
    | "loading"
    | "playing"
    | "stalled"
    | "paused"
    | "ended"
    | "unconfirmed"
    | "failed";
  capability: V4AudioPreviewCapability | null;
  session: V4AudioPreviewSession | null;
  request: V4AudioPreviewCreate | null;
  error: string | null;
};
export type V4FrameCapability =
  ToolOutput<"opencut_project_v4_frame_capability">;
export type V4FrameResult = JobResultMap["render.project_frame.v4"] | JobResultMap["render.project_text_frame.v4"] | JobResultMap["render.project_caption_frame.v4"];
export type V4FrameAttempt = {
  protocol: 1 | 2 | 3;
  request: V4FrameRequest;
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

export type V4Snapshot = ToolOutput<"opencut_project_v4_inspect">;
export type V4Document = V4Snapshot["project"];
export type V4Sequence = V4Document["sequences"][number];
export type V4Element = V4Sequence["tracks"][number]["elements"][number];
export type V4Bookmark = V4Sequence["bookmarks"][number];
export type V4Operation =
  ToolInput<"opencut_edit_v4_apply">["operations"][number];
export type V4BookmarkOperation = Extract<
  V4Operation,
  { type: "bookmark" }
>["operation"];
export type V4Summary = ToolOutput<"opencut_project_v4_list">["items"][number];
export type V4Navigation = ToolOutput<"opencut_project_v4_history_navigation">;
export type V4RevisionPage = ToolOutput<"opencut_project_history">;
export type V4HistoryPlan = ToolOutput<"opencut_edit_v4_undo_plan">;
export type V4HistoryRequest =
  | {
      direction: "undo";
      input: ToolInput<"opencut_edit_v4_undo_apply">;
      plan: V4HistoryPlan;
    }
  | {
      direction: "redo";
      input: ToolInput<"opencut_edit_v4_redo_apply">;
      plan: V4HistoryPlan;
    };
export type V4SourceAudioRequest =
  ToolInput<"opencut_project_v4_source_audio_plan">;
export type V4SourceAudioPlan =
  ToolOutput<"opencut_project_v4_source_audio_plan">;
export type V4EditReceipt = ToolOutput<"opencut_edit_v4_apply">;
export type V4MediaRequest = ToolInput<"opencut_media_workspace_plan_v4">;
export type V4MediaPlan = ToolOutput<"opencut_media_workspace_plan_v4">;
export type V4AvailableMedia = ToolOutput<"opencut_project_media_inspect">;
export type V4Asset = V4Document["assets"][number];
export type V4MediaAttempt = {
  request: V4MediaRequest;
  plan: V4MediaPlan | null;
  receipt: V4EditReceipt | null;
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
export type V4MediaState = {
  attempt: V4MediaAttempt | null;
  available: V4AvailableMedia[];
  loading: boolean;
  error: string | null;
};
export type V4SourceAudioAttempt = {
  request: V4SourceAudioRequest;
  plan: V4SourceAudioPlan | null;
  receipt: V4EditReceipt | null;
  phase:
    "planning" | "applying" | "retryable" | "blocked" | "unconfirmed" | "saved";
  error: string | null;
  headChecked: boolean;
};
export type V4AudioWavRequest = ToolInput<"opencut_job_v4_audio_wav">;
export type V4AudioWavCapability =
  ToolOutput<"opencut_project_v4_audio_capability">;
export type V4AudioRangeWavRequest =
  ToolInput<"opencut_job_v4_audio_range_wav">;
export type V4AudioRangeWavCapability =
  ToolOutput<"opencut_project_v4_audio_range_capability">;
export type V4AudioWavAttempt = {
  request: V4AudioWavRequest | V4AudioRangeWavRequest;
  sceneName: string;
  phase: "submitting" | "unconfirmed" | "retryable" | "accepted";
  jobId: string | null;
  error: string | null;
};
export type V4SessionState = {
  recovery?: EditRecoveryState;
  fontRecovery?: FontRecoveryState;
  captionFrameCapability?: ToolOutput<"opencut_project_v4_caption_frame_capability"> | null;
  textFrameCapability?: ToolOutput<"opencut_project_v4_text_frame_capability"> | null;
  timelineEdit: TimelineEditState;
  clipboard: ClipboardState;
  audioPreview: V4AudioPreviewState;
  head: V4Snapshot;
  historical: V4Snapshot | null;
  sequenceId: string;
  playhead: V4Bookmark["time"];
  selectedId: string | null;
  history: V4RevisionPage;
  navigation: V4Navigation;
  historyPlan: V4HistoryRequest | null;
  sourceAudio: V4SourceAudioAttempt | null;
  audioGain: V4GainAttempt | null;
  gainDraft: V4GainDraft | null;
  videoMp4Capability: V4VideoMp4Capability | null;
  videoMp4CapabilityError: string | null;
  videoMp4: V4VideoMp4Attempt | null;
  audioWavCapability: V4AudioWavCapability | null;
  audioWavCapabilityError: string | null;
  audioRangeWavCapability: V4AudioRangeWavCapability | null;
  audioRangeWavCapabilityError: string | null;
  audioWav: V4AudioWavAttempt | null;
  media: V4MediaState;
  frameCapability: V4FrameCapability | null;
  frameCapabilityError: string | null;
  frame: V4FrameAttempt | null;
};

export type V4GainRequest = ToolInput<"opencut_project_v4_audio_gain_plan">;
export type V4GainPlan = ToolOutput<"opencut_project_v4_audio_gain_plan">;
export type V4GainAttempt = {
  request: V4GainRequest;
  plan: V4GainPlan | null;
  receipt: V4EditReceipt | null;
  phase: "planning" | "applying" | "retryable" | "unconfirmed" | "saved";
  error: string | null;
};
/** Presentation draft only; the fixed document remains in the controller. */
export type V4GainDraft = {
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

export type V4VideoMp4Request = ToolInput<"opencut_job_v4_video_mp4">;
export type V4VideoMp4Capability = ToolOutput<"opencut_project_v4_video_capability">;
export type V4VideoMp4Attempt = {
  request: V4VideoMp4Request;
  sceneName: string;
  phase: "submitting" | "unconfirmed" | "retryable" | "accepted";
  jobId: string | null;
  error: string | null;
};
