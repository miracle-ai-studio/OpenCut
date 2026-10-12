import { OpenCutApiError } from "../../../lib/editor-sdk";
import type { EditorApi } from "../api";
import type { V4SessionHost } from "./session";
import type {
  V4AudioWavAttempt,
  V4AudioRangeWavRequest,
  V4SessionState,
} from "./types";

const message = (error: unknown) =>
  error instanceof Error ? error.message : String(error);
export function audioWavAvailability(
  state: V4SessionState,
  scope: "whole" | "range" = "whole",
) {
  const capability =
    scope === "range"
      ? state.audioRangeWavCapability
      : state.audioWavCapability;
  const error =
    scope === "range"
      ? state.audioRangeWavCapabilityError
      : state.audioWavCapabilityError;
  const tool =
    scope === "range"
      ? "opencut_job_v4_audio_range_wav"
      : "opencut_job_v4_audio_wav";
  const rates =
    capability?.sample_rates.filter(
      (rate) => rate === 44100 || rate === 48000,
    ) ?? [];
  const view = state.historical ?? state.head;
  const reason = !capability
    ? (error ?? "Audio WAV export availability is being checked.")
    : !capability.available
      ? (capability.unavailable_reason ??
        "Scene audio WAV export is unavailable on this service.")
      : capability.schema_version !== 1 ||
          capability.submit_tool !== tool ||
          !rates.length
        ? "This service does not support scene audio WAV export."
        : view.project.settings.profile !== "classic_global"
          ? "Audio WAV export currently requires Classic scene settings."
          : null;
  return { rates, reason };
}

/** Canonical sample integers only. The service owns scene bounds and quotas. */
export function audioSampleRange(
  start: string,
  count: string,
): V4AudioRangeWavRequest["range"] {
  const canonical = (value: string) =>
    /^(0|[1-9][0-9]{0,19})$/.test(value) &&
    BigInt(value) <= 18446744073709551615n;
  if (!canonical(start) || !canonical(count) || count === "0")
    throw new Error(
      "Enter a canonical starting sample and a positive frame count.",
    );
  return { start_sample: start, frame_count: count };
}

/** Only submission identity is retained across routes on this API instance.
 * The existing task feed owns jobs; a captured revision never follows the head. */
export class V4AudioWav {
  private readonly capabilityEpoch = { whole: 0, range: 0 };
  private readonly attempts = new WeakMap<
    EditorApi,
    Map<string, V4AudioWavAttempt>
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
  private publish(api: EditorApi, attempt: V4AudioWavAttempt) {
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
      state.audioWav?.request.idempotency_key ===
        attempt.request.idempotency_key
    )
      this.host.publish({ ...state, audioWav: attempt });
  }
  async refreshCapability(scope: "whole" | "range" = "whole") {
    const { api, state } = this.active(),
      generation = this.host.generation(),
      epoch = ++this.capabilityEpoch[scope];
    let result;
    try {
      result = {
        value: await (scope === "range"
          ? api.v4.audioRangeCapability()
          : api.v4.audioCapability()),
        error: null,
      };
    } catch (error) {
      result = {
        value: null,
        error:
          scope === "range" &&
          error instanceof OpenCutApiError &&
          error.status === 404
            ? "This service does not support sample range WAV export."
            : message(error),
      };
    }
    const current = this.host.state();
    if (
      generation === this.host.generation() &&
      api === this.host.api() &&
      epoch === this.capabilityEpoch[scope] &&
      current?.head.project.id === state.head.project.id
    )
      this.host.publish({
        ...current,
        ...(scope === "range"
          ? {
              audioRangeWavCapability: result.value,
              audioRangeWavCapabilityError: result.error,
            }
          : {
              audioWavCapability: result.value,
              audioWavCapabilityError: result.error,
            }),
      });
  }
  async start(sampleRate: number, range?: V4AudioRangeWavRequest["range"]) {
    const { api, state } = this.active();
    if (state.audioWav && state.audioWav.phase !== "accepted")
      throw new Error(
        "Resolve the previous audio WAV export before starting another.",
      );
    const availability = audioWavAvailability(state, range ? "range" : "whole");
    if (range) audioSampleRange(range.start_sample, range.frame_count);
    if (availability.reason) throw new Error(availability.reason);
    if (!availability.rates.some((rate) => rate === sampleRate))
      throw new Error("Choose an available audio sample rate.");
    const view = state.historical ?? state.head,
      sequence = view.project.sequences.find((s) => s.id === state.sequenceId);
    if (!sequence) throw new Error("The selected scene is unavailable.");
    const attempt: V4AudioWavAttempt = {
      request: {
        project_id: view.project.id,
        revision: view.revision,
        sequence_id: sequence.id,
        sample_rate: sampleRate,
        idempotency_key: crypto.randomUUID(),
        ...(range ? { range: { ...range } } : {}),
      },
      sceneName: sequence.name,
      phase: "submitting",
      jobId: null,
      error: null,
    };
    this.host.publish({ ...state, audioWav: attempt });
    this.publish(api, attempt);
    return this.execute(false);
  }
  retry() {
    return this.execute(true);
  }
  dismiss() {
    const { api, state } = this.active();
    if (
      state.audioWav &&
      ["submitting", "unconfirmed"].includes(state.audioWav.phase)
    )
      throw new Error(
        "Confirm the original audio WAV submission before starting another.",
      );
    this.attempts.get(api)?.delete(state.head.project.id);
    this.host.publish({ ...state, audioWav: null });
  }
  private async execute(retry: boolean) {
    const { api, state } = this.active();
    let attempt = state.audioWav;
    if (!attempt) throw new Error("Choose an audio WAV export first.");
    if (retry && attempt.phase === "submitting")
      throw new Error("Wait for the original audio submission to finish.");
    const generation = this.host.generation(),
      view = this.host.viewGeneration(),
      wasUnknown = attempt.phase === "unconfirmed";
    // Explicit replay may recover an older scene/revision after navigation.
    // It always uses the displayed original request, never the current head.
    if (
      !retry &&
      (state.historical ?? state.head).revision !== attempt.request.revision
    )
      throw new Error("Project view changed before starting the audio export.");
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
      const ranged = "range" in attempt.request;
      const job = await ("range" in attempt.request
        ? api.v4.audioRangeWav(attempt.request)
        : api.v4.audioWav(attempt.request));
      if (
        !job.job_id ||
        job.kind !==
          (ranged
            ? "render.project_audio_range.v4"
            : "render.project_audio.v4") ||
        job.target?.project_id !== attempt.request.project_id ||
        job.target.revision !== attempt.request.revision ||
        job.target.sequence_id !== attempt.request.sequence_id
      )
        throw new Error("Audio export returned an unexpected task receipt.");
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
