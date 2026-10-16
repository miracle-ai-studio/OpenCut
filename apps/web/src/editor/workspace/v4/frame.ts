import { OpenCutApiError } from "../../../lib/editor-sdk";
import type { PublicJob } from "../../../lib/editor-sdk";
import type { EditorApi } from "../api";
import type { V4SessionHost } from "./session";
import type { V4FrameAttempt, V4FrameRequest, V4SessionState } from "./types";
import { compareTime } from "../../model/time";
import { viewedVisualProfile } from "./visual-profile";

import { sameTarget, verifiedResult, readPng } from "./frame/png";
const message = (error: unknown) =>
  error instanceof Error ? error.message : String(error);

export function textFrameAvailable(state: V4SessionState) {
  const capability = state.textFrameCapability;
  return (
    capability?.available === true &&
    capability.schema_version === 2 &&
    capability.submit_tool === "opencut_job_v4_text_frame_png"
  );
}

export function captionFrameAvailable(state: V4SessionState) {
  const capability = state.captionFrameCapability;
  return (
    capability?.available === true &&
    capability.schema_version === 3 &&
    capability.submit_tool === "opencut_job_v4_caption_frame_png"
  );
}
export function hasGraphics(state: V4SessionState) {
  return (
    (state.historical ?? state.head).project.sequences
      .find((s) => s.id === state.sequenceId)
      ?.tracks.some(
        (track) =>
          track.enabled &&
          track.elements.some(
            (element) => element.enabled && element.content.kind === "graphic",
          ),
      ) ?? false
  );
}
export function graphicFrameAvailable(state: V4SessionState) {
  const capability = state.graphicFrameCapability;
  return (
    capability?.available === true &&
    capability.schema_version === 1 &&
    capability.submit_tool === "opencut_job_v4_graphic_frame_png"
  );
}
export function sceneFrameAvailable(state: V4SessionState) {
  const c = state.sceneFrameCapability; return !!c && c.available && c.schema_version === 1 && c.submit_tool === "opencut_job_v4_scene_frame_png";
}
export function maskFrameAvailable(state: V4SessionState) {
  const capability = state.maskFrameCapability;
  return (
    capability?.available === true &&
    capability.schema_version === 1 &&
    capability.submit_tool === "opencut_job_v4_mask_frame_png"
  );
}
export function blendFrameAvailable(state: V4SessionState) {
  const capability = state.blendFrameCapability;
  return (
    capability?.available === true &&
    capability.schema_version === 1 &&
    capability.submit_tool === "opencut_job_v4_blend_frame_png"
  );
}
const hasCaptions = (state: V4SessionState) =>
  ((state.historical ?? state.head).project.sequences.find(
    (s) => s.id === state.sequenceId,
  )?.captions.length ?? 0) > 0;
export function frameAvailability(state: V4SessionState) {
  const view = state.historical ?? state.head;
  if (view.project.settings.profile !== "classic_global")
    return "Scene frames currently require Classic scene settings.";
  if (viewedVisualProfile(state) === "scene") return sceneFrameAvailable(state) ? null : (state.sceneFrameCapability?.unavailable_reason ?? "This service does not support Classic scene effects and mask stacks.");
  if (viewedVisualProfile(state) === "mask")
    return maskFrameAvailable(state)
      ? null
      : (state.maskFrameCapability?.unavailable_reason ??
          "This service does not support scene frames with rectangular or elliptical masks.");
  if (viewedVisualProfile(state) === "blend")
    return blendFrameAvailable(state)
      ? null
      : (state.blendFrameCapability?.unavailable_reason ??
          "This service does not support scene frames with blend modes.");
  if (hasGraphics(state))
    return graphicFrameAvailable(state)
      ? null
      : (state.graphicFrameCapability?.unavailable_reason ??
          "This service does not support scene frames containing graphics.");
  if (hasCaptions(state))
    return captionFrameAvailable(state)
      ? null
      : (state.captionFrameCapability?.unavailable_reason ??
          "This service does not support pinned caption frames.");
  if (textFrameAvailable(state)) return null;
  const sequence = view.project.sequences.find(
    (row) => row.id === state.sequenceId,
  );
  if (
    sequence?.tracks.some(
      (track) =>
        track.enabled &&
        track.elements.some(
          (element) => element.enabled && element.content.kind === "text",
        ),
    )
  )
    return (
      state.textFrameCapability?.unavailable_reason ??
      "This service does not support scene frames containing text."
    );
  const capability = state.frameCapability;
  return !capability
    ? (state.frameCapabilityError ??
        "Scene frame availability is being checked.")
    : !capability.available
      ? (capability.unavailable_reason ??
        "Scene frames are unavailable on this service.")
      : capability.schema_version !== 1 ||
          capability.submit_tool !== "opencut_job_v4_frame_png"
        ? "This service does not support scene frames."
        : (state.historical ?? state.head).project.settings.profile !==
            "classic_global"
          ? "Scene frames currently require Classic scene settings."
          : null;
}

/** Commands and display loading only. WorkspaceState.jobs/TaskFeed remain the
 * sole task authority; seek never computes renderer time or resource clocks. */
export class V4Frame {
  private attempts = new WeakMap<EditorApi, Map<string, V4FrameAttempt>>();
  private epoch = 0;
  private download: AbortController | null = null;
  private capabilityEpoch = 0;
  private owned: { api: EditorApi; attempt: V4FrameAttempt } | null = null;
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
  private publish(api: EditorApi, attempt: V4FrameAttempt) {
    let projects = this.attempts.get(api);
    if (!projects) {
      projects = new Map();
      this.attempts.set(api, projects);
    }
    if (["submitting", "unconfirmed", "retryable"].includes(attempt.phase))
      projects.set(attempt.request.project_id, attempt);
    else projects.delete(attempt.request.project_id);
    const state = this.host.state();
    if (
      api === this.host.api() &&
      state?.head.project.id === attempt.request.project_id &&
      state.frame?.request.idempotency_key === attempt.request.idempotency_key
    ) {
      if (attempt.jobId) this.owned = { api, attempt };
      this.host.publish({ ...state, frame: attempt });
    }
  }
  private matches(request: V4FrameRequest) {
    const state = this.host.state(),
      view = state && (state.historical ?? state.head);
    return (
      !!state &&
      view?.project.id === request.project_id &&
      view.revision === request.revision &&
      state.sequenceId === request.sequence_id &&
      compareTime(state.playhead, request.time) === 0
    );
  }
  async refreshCapability() {
    const { api, state } = this.active(),
      generation = this.host.generation(),
      epoch = ++this.capabilityEpoch;
    try {
      const [
        capability,
        textFrameCapability,
        captionFrameCapability,
        graphicFrameCapability,
        blendFrameCapability,
        maskFrameCapability,
        sceneFrameCapability,
      ] = await Promise.all([
        api.v4.frameCapability().catch(() => null),
        api.sdk
          .call("opencut_project_v4_text_frame_capability", {})
          .catch(() => null),
        api.sdk
          .call("opencut_project_v4_caption_frame_capability", {})
          .catch(() => null),
        api.sdk
          .call("opencut_project_v4_graphic_frame_capability", {})
          .catch(() => null),
        api.sdk
          .call("opencut_project_v4_blend_frame_capability", {})
          .catch(() => null),
        api.sdk
          .call("opencut_project_v4_mask_frame_capability", {})
          .catch(() => null),
        api.sdk.call("opencut_project_v4_scene_frame_capability", {}).catch(() => null),
      ]);
      const current = this.host.state();
      if (
        epoch === this.capabilityEpoch &&
        generation === this.host.generation() &&
        api === this.host.api() &&
        current?.head.project.id === state.head.project.id
      )
        this.host.publish({
          ...current,
          frameCapability: capability,
          textFrameCapability,
          captionFrameCapability,
          graphicFrameCapability,
          blendFrameCapability,
          maskFrameCapability,
        sceneFrameCapability,
          frameCapabilityError: capability
            ? null
            : "This service does not support scene frames.",
        });
    } catch (error) {
      const current = this.host.state();
      if (
        epoch === this.capabilityEpoch &&
        generation === this.host.generation() &&
        api === this.host.api() &&
        current?.head.project.id === state.head.project.id
      )
        this.host.publish({
          ...current,
          frameCapability: null,
          frameCapabilityError: "This service does not support scene frames.",
        });
    }
  }
  /** Called on view/time change or canvas unmount. Cancel only this canvas's
   * known unfinished frame, retaining unknown acceptance for explicit replay. */
  invalidate() {
    this.epoch++;
    this.download?.abort();
    this.download = null;
    const owned = this.owned;
    this.owned = null;
    if (owned?.attempt.jobId && owned.attempt.phase === "accepted")
      void owned.api.sdk
        .cancelPublicJob(owned.attempt.jobId)
        .then(() => this.host.refreshTasks())
        .catch(() => {});
    const state = this.host.state(),
      attempt = state?.frame;
    if (
      state &&
      attempt &&
      !["submitting", "unconfirmed", "retryable"].includes(attempt.phase)
    )
      this.host.publish({ ...state, frame: null });
  }
  async start() {
    const { api, state } = this.active();
    if (
      state.frame &&
      ["submitting", "unconfirmed"].includes(state.frame.phase)
    )
      throw new Error("Confirm the original scene frame request first.");
    const reason = frameAvailability(state);
    if (reason) throw new Error(reason);
    this.invalidate();
    const view = state.historical ?? state.head;
    const attempt: V4FrameAttempt = {
      protocol:
        viewedVisualProfile(state) === "scene" ? "scene-v1" : viewedVisualProfile(state) === "mask"
          ? "mask-v1"
          : viewedVisualProfile(state) === "blend"
            ? "blend-v1"
            : hasGraphics(state)
              ? "graphic-v1"
              : hasCaptions(state) && captionFrameAvailable(state)
                ? 3
                : textFrameAvailable(state)
                  ? 2
                  : 1,
      request: {
        project_id: view.project.id,
        revision: view.revision,
        sequence_id: state.sequenceId,
        time: structuredClone(state.playhead),
        idempotency_key: crypto.randomUUID(),
      },
      phase: "submitting",
      jobId: null,
      error: null,
    };
    this.host.publish({ ...this.host.state()!, frame: attempt });
    this.publish(api, attempt);
    return this.execute();
  }
  retry() {
    if (this.host.state()?.frame?.phase === "submitting")
      throw new Error("Wait for the original scene frame request.");
    return this.execute();
  }
  private async execute() {
    const { api, state } = this.active();
    let attempt = state.frame;
    if (
      !attempt ||
      !["submitting", "unconfirmed", "retryable"].includes(attempt.phase)
    )
      throw new Error("Choose a scene frame request first.");
    const wasUnknown = attempt.phase === "unconfirmed",
      epoch = this.epoch,
      generation = this.host.generation();
    attempt = { ...attempt, phase: "submitting", error: null };
    this.publish(api, attempt);
    try {
      const job = await (attempt.protocol === "scene-v1" ? api.sdk.call("opencut_job_v4_scene_frame_png", attempt.request) : attempt.protocol === "mask-v1"
        ? api.sdk.call("opencut_job_v4_mask_frame_png", attempt.request)
        : attempt.protocol === "blend-v1"
          ? api.sdk.call("opencut_job_v4_blend_frame_png", attempt.request)
          : attempt.protocol === "graphic-v1"
            ? api.sdk.call("opencut_job_v4_graphic_frame_png", attempt.request)
            : attempt.protocol === 3
              ? api.sdk.call(
                  "opencut_job_v4_caption_frame_png",
                  attempt.request,
                )
              : attempt.protocol === 2
                ? api.v4.textFramePng(attempt.request)
                : api.v4.framePng(attempt.request));
      if (!job.job_id || !sameTarget(job, attempt.request, attempt.protocol))
        throw new Error("Scene frame returned an unexpected task identity.");
      attempt = {
        ...attempt,
        phase: "accepted",
        jobId: job.job_id,
        error: null,
      };
      this.publish(api, attempt);
      if (
        epoch !== this.epoch ||
        generation !== this.host.generation() ||
        api !== this.host.api() ||
        !this.matches(attempt.request) ||
        this.host.state()?.frame?.request.idempotency_key !==
          attempt.request.idempotency_key
      ) {
        if (["queued", "running"].includes(job.state))
          await api.sdk.cancelPublicJob(job.job_id);
        this.publish(api, {
          ...attempt,
          phase: "failed",
          error:
            "This scene frame belongs to a previous view. Render the current frame again.",
        });
      }
      if (generation === this.host.generation() && api === this.host.api())
        await this.host.refreshTasks();
      return job.job_id;
    } catch (error) {
      // An accepted receipt remains known even if cancellation/feed refresh failed.
      this.publish(api, {
        ...attempt,
        phase: attempt.jobId
          ? "failed"
          : wasUnknown || !(error instanceof OpenCutApiError)
            ? "unconfirmed"
            : "retryable",
        error: message(error),
      });
      throw error;
    }
  }
  async cancel() {
    const { api, state } = this.active(),
      attempt = state.frame;
    if (!attempt?.jobId)
      throw new Error(
        "Confirm the original scene frame request before cancelling its task.",
      );
    this.download?.abort();
    this.download = null;
    this.epoch++;
    await api.sdk.cancelPublicJob(attempt.jobId);
    this.publish(api, {
      ...attempt,
      phase: "failed",
      error:
        "Scene frame cancellation requested. Check Tasks for its final state.",
    });
    await this.host.refreshTasks();
  }
  rejectImage() {
    const { api, state } = this.active();
    if (state.frame)
      this.publish(api, {
        ...state.frame,
        phase: "failed",
        error: "The scene frame image could not be displayed.",
      });
  }
  rejectTask(job: PublicJob) {
    const { api, state } = this.active(),
      attempt = state.frame;
    if (!attempt?.jobId || attempt.jobId !== job.job_id) return;
    this.download?.abort();
    this.download = null;
    this.epoch++;
    this.publish(api, {
      ...attempt,
      phase: "failed",
      error:
        job.result_unavailable?.message ??
        job.failure?.message ??
        "Scene frame task did not complete.",
    });
  }
  retryDownload(jobs: readonly PublicJob[]) {
    const { api, state } = this.active(),
      attempt = state.frame;
    if (
      !attempt?.jobId ||
      attempt.phase !== "failed" ||
      !this.matches(attempt.request) ||
      !jobs.some(
        (job) =>
          job.job_id === attempt.jobId &&
          job.state === "succeeded" &&
          !job.result_unavailable,
      )
    )
      throw new Error("The saved frame is unavailable in this view.");
    this.publish(api, { ...attempt, phase: "accepted", error: null });
    return this.observe(jobs);
  }
  async observe(jobs: readonly PublicJob[]) {
    const { api, state } = this.active(),
      attempt = state.frame;
    if (
      !attempt?.jobId ||
      attempt.phase !== "accepted" ||
      !this.matches(attempt.request)
    )
      return null;
    const job = jobs.find((row) => row.job_id === attempt.jobId);
    if (!job || ["queued", "running"].includes(job.state)) return null;
    if (job.state !== "succeeded" || job.result_unavailable) {
      this.publish(api, {
        ...attempt,
        phase: "failed",
        error:
          job.result_unavailable?.message ??
          job.failure?.message ??
          "Scene frame task did not complete.",
      });
      return null;
    }
    const epoch = this.epoch,
      generation = this.host.generation(),
      request = new AbortController();
    this.download = request;
    this.publish(api, { ...attempt, phase: "downloading", error: null });
    try {
      const settings = (state.historical ?? state.head).project.settings;
      if (settings.profile !== "classic_global")
        throw new Error(
          "Scene frames currently require Classic scene settings.",
        );
      const { result, artifact } = verifiedResult(
        job,
        attempt.request,
        settings.settings.canvas_size.width,
        settings.settings.canvas_size.height,
        attempt.protocol,
      );
      const blob = await readPng(api, artifact, result, request.signal);
      if (
        request.signal.aborted ||
        epoch !== this.epoch ||
        generation !== this.host.generation() ||
        api !== this.host.api() ||
        !this.matches(attempt.request) ||
        this.host.state()?.frame?.request.idempotency_key !==
          attempt.request.idempotency_key
      )
        return null;
      this.publish(api, { ...attempt, phase: "ready", error: null });
      return { blob, result };
    } catch (error) {
      if (
        !request.signal.aborted &&
        epoch === this.epoch &&
        generation === this.host.generation() &&
        api === this.host.api()
      )
        this.publish(api, {
          ...attempt,
          phase: "failed",
          error: message(error),
        });
      return null;
    } finally {
      if (this.download === request) this.download = null;
    }
  }
}
