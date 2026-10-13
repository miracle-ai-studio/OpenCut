import { OpenCutApiError } from "../../../lib/editor-sdk";
import type { EditorApi } from "../api";
import type { V4SessionHost } from "./session";
import type { V4VideoMp4Attempt, V4SessionState } from "./types";

const message = (error: unknown) =>
  error instanceof Error ? error.message : String(error);
export function videoMp4Availability(state: V4SessionState) {
  const capability = state.videoMp4Capability;
  const captions = !!(state.historical ?? state.head).project.sequences.find(
    (s) => s.id === state.sequenceId,
  )?.captions.length;
  const view = state.historical ?? state.head;
  const reason = !capability
    ? (state.videoMp4CapabilityError ??
      "Video MP4 export availability is being checked.")
    : !capability.available
      ? (capability.unavailable_reason ??
        "Scene video MP4 export is unavailable on this service.")
      : capability.schema_version !== (captions ? 2 : 1) ||
          capability.submit_tool !==
            (captions
              ? "opencut_job_v4_caption_video_mp4"
              : "opencut_job_v4_video_mp4")
        ? "This service does not support scene video MP4 export."
        : view.project.settings.profile !== "classic_global"
          ? "Video MP4 export currently requires Classic scene settings."
          : null;
  return { reason };
}

/** Only submission identity is retained across routes on this API instance.
 * The existing task feed owns jobs; a captured revision never follows the head. */
export class V4VideoMp4 {
  private capabilityEpoch = 0;
  private readonly attempts = new WeakMap<
    EditorApi,
    Map<string, V4VideoMp4Attempt>
  >();
  constructor(private readonly host: V4SessionHost) {}
  private active() {
    const api = this.host.api(),
      state = this.host.state();
    if (!api || !state) throw new Error("Open a V4 project first.");
    return { api, state };
  }
  resume(api: EditorApi, projectId: string) {
    return this.attempts.get(api)?.get(projectId) ?? null;
  }
  private publish(api: EditorApi, attempt: V4VideoMp4Attempt) {
    let projects = this.attempts.get(api);
    if (!projects) {
      projects = new Map();
      this.attempts.set(api, projects);
    }
    if (attempt.phase === "accepted")
      projects.delete(attempt.request.project_id);
    else projects.set(attempt.request.project_id, attempt);
    const state = this.host.state();
    if (
      api === this.host.api() &&
      state?.head.project.id === attempt.request.project_id &&
      state.videoMp4?.request.idempotency_key ===
        attempt.request.idempotency_key
    )
      this.host.publish({ ...state, videoMp4: attempt });
  }
  async refreshCapability() {
    const { api, state } = this.active(),
      generation = this.host.generation(),
      epoch = ++this.capabilityEpoch;
    let value = null,
      error = null;
    try {
      value = (state.historical ?? state.head).project.sequences.find(
        (s) => s.id === state.sequenceId,
      )?.captions.length
        ? await api.sdk.call(
            "opencut_project_v4_caption_video_mp4_capability",
            {},
          )
        : await api.v4.videoCapability();
    } catch (e) {
      error = message(e);
    }
    const current = this.host.state();
    if (
      generation === this.host.generation() &&
      api === this.host.api() &&
      epoch === this.capabilityEpoch &&
      current?.head.project.id === state.head.project.id
    )
      this.host.publish({
        ...current,
        videoMp4Capability: value,
        videoMp4CapabilityError: error,
      });
  }
  async start() {
    const { api, state } = this.active();
    if (state.videoMp4 && state.videoMp4.phase !== "accepted")
      throw new Error(
        "Resolve the previous video MP4 export before starting another.",
      );
    const availability = videoMp4Availability(state);
    if (availability.reason) throw new Error(availability.reason);
    const view = state.historical ?? state.head,
      sequence = view.project.sequences.find((s) => s.id === state.sequenceId);
    if (!sequence) throw new Error("The selected scene is unavailable.");
    const attempt: V4VideoMp4Attempt = {
      tool: sequence.captions.length
        ? "opencut_job_v4_caption_video_mp4"
        : "opencut_job_v4_video_mp4",
      request: {
        project_id: view.project.id,
        revision: view.revision,
        sequence_id: sequence.id,
        idempotency_key: crypto.randomUUID(),
      },
      sceneName: sequence.name,
      phase: "submitting",
      jobId: null,
      error: null,
    };
    this.host.publish({ ...state, videoMp4: attempt });
    this.publish(api, attempt);
    return this.execute(false);
  }
  retry() {
    return this.execute(true);
  }
  dismiss() {
    const { api, state } = this.active();
    if (
      state.videoMp4 &&
      ["submitting", "unconfirmed"].includes(state.videoMp4.phase)
    )
      throw new Error(
        "Confirm the original video MP4 submission before starting another.",
      );
    this.attempts.get(api)?.delete(state.head.project.id);
    this.host.publish({ ...state, videoMp4: null });
  }
  private async execute(retry: boolean) {
    const { api, state } = this.active();
    let attempt = state.videoMp4;
    if (!attempt) throw new Error("Choose an video MP4 export first.");
    if (retry && attempt.phase === "submitting")
      throw new Error("Wait for the original video submission to finish.");
    const generation = this.host.generation(),
      view = this.host.viewGeneration(),
      wasUnknown = attempt.phase === "unconfirmed";
    // Explicit replay may recover an older scene/revision after navigation.
    // It always uses the displayed original request, never the current head.
    if (
      !retry &&
      (state.historical ?? state.head).revision !== attempt.request.revision
    )
      throw new Error("Project view changed before starting the video export.");
    let release: () => void;
    try {
      release = this.host.acquire(attempt.request.project_id);
    } catch (error) {
      this.publish(api, {
        ...attempt,
        phase: wasUnknown ? "unconfirmed" : "retryable",
        error: message(error),
      });
      throw error;
    }
    let dispatched = false;
    try {
      if (
        generation !== this.host.generation() ||
        view !== this.host.viewGeneration()
      )
        throw new DOMException("Project view changed.", "AbortError");
      attempt = { ...attempt, phase: "submitting", error: null };
      this.publish(api, attempt);
      dispatched = true;
      const tool = attempt.tool ?? "opencut_job_v4_video_mp4";
      const job =
        tool === "opencut_job_v4_caption_video_mp4"
          ? await api.sdk.call(tool, attempt.request)
          : await api.v4.videoMp4(attempt.request);
      if (
        !job.job_id ||
        job.kind !==
          (tool === "opencut_job_v4_caption_video_mp4"
            ? "render.project_caption_video.v4"
            : "render.project_video.v4") ||
        job.target?.project_id !== attempt.request.project_id ||
        job.target.revision !== attempt.request.revision ||
        job.target.sequence_id !== attempt.request.sequence_id
      )
        throw new Error("Video export returned an unexpected task receipt.");
      attempt = {
        ...attempt,
        phase: "accepted",
        jobId: job.job_id,
        error: null,
      };
      this.publish(api, attempt);
      if (generation === this.host.generation() && api === this.host.api()) {
        try {
          await this.host.refreshTasks();
        } catch (error) {
          this.publish(api, { ...attempt, error: message(error) });
        }
      }
      return job.job_id;
    } catch (error) {
      const uncertain =
        wasUnknown || (dispatched && !(error instanceof OpenCutApiError));
      attempt = {
        ...attempt,
        phase: uncertain ? "unconfirmed" : "retryable",
        error: message(error),
      };
      this.publish(api, attempt);
      if (generation !== this.host.generation() || api !== this.host.api())
        throw new DOMException("Project view changed.", "AbortError");
      throw error;
    } finally {
      release();
    }
  }
}
