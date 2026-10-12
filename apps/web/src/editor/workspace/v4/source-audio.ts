import { OpenCutApiError } from "../../../lib/editor-sdk";
import type { EditorApi } from "../api";
import type { V4SessionHost } from "./session";
import type {
  V4SourceAudioAttempt,
  V4SourceAudioRequest,
  V4SessionState,
} from "./types";

/** Client affordances only. The authoritative planner and commit recheck scope,
 * current revision, locking, audio presence and destination fit on the server. */
export function sourceAudioAvailability(state: V4SessionState) {
  const view = state.historical ?? state.head;
  const sequence = view.project.sequences.find(
    (s) => s.id === state.sequenceId,
  );
  const track = sequence?.tracks.find((t) =>
    t.elements.some((e) => e.id === state.selectedId),
  );
  const element = track?.elements.find((e) => e.id === state.selectedId);
  if (!sequence || !track || !element || element.content.kind !== "video")
    return null;
  const assetId = element.content.asset_id;
  const asset = view.project.assets.find((a) => a.id === assetId);
  const reason = state.historical
    ? "Return to the current revision before editing."
    : track.locked
      ? "The source video track is locked."
      : !asset
        ? "Source video media is unavailable."
        : asset.metadata.audio.status === "unknown"
          ? "Audio presence must be checked before separating or restoring source audio."
          : asset.metadata.audio.status === "absent"
            ? "This video has no source audio."
            : null;
  return {
    sequence,
    track,
    element,
    sourceEnabled: element.content.source_audio_enabled,
    reason,
  };
}
const message = (error: unknown) =>
  error instanceof Error ? error.message : String(error);
const unknown = (error: unknown) => !(error instanceof OpenCutApiError);

/** Command-only retention across routes on the same authenticated API instance.
 * No document mirror, local persistence, media decoding or independent write fence. */
export class V4SourceAudio {
  private readonly outstanding = new WeakMap<
    EditorApi,
    Map<string, V4SourceAudioAttempt>
  >();
  constructor(
    private readonly host: V4SessionHost,
    private readonly refresh: () => Promise<void>,
  ) {}
  private active() {
    const api = this.host.api(),
      state = this.host.state();
    if (!api || !state) throw new Error("Open a V4 project first.");
    return { api, state };
  }
  private remember(api: EditorApi, attempt: V4SourceAudioAttempt) {
    let projects = this.outstanding.get(api);
    if (!projects) {
      projects = new Map();
      this.outstanding.set(api, projects);
    }
    if (attempt.phase === "saved" && attempt.headChecked)
      projects.delete(attempt.request.project_id);
    else projects.set(attempt.request.project_id, attempt);
  }
  resume(api: EditorApi, projectId: string, headRevision: string) {
    const cached = this.outstanding.get(api)?.get(projectId);
    if (!cached) return null;
    // Opening reads the head, but never infers a receipt from project contents.
    const attempt = {
      ...cached,
      headChecked:
        !cached.receipt || headRevision !== cached.receipt.base_revision,
    };
    this.remember(api, attempt);
    return attempt;
  }
  recovered(api: EditorApi, projectId: string, key: string) {
    if (this.outstanding.get(api)?.get(projectId)?.request.idempotency_key === key) this.outstanding.get(api)?.delete(projectId);
    const state = this.host.state();
    if (api === this.host.api() && state?.head.project.id === projectId && state.sourceAudio?.request.idempotency_key === key)
      this.host.publish({ ...state, sourceAudio: null });
  }
  private publish(attempt: V4SourceAudioAttempt, api: EditorApi) {
    this.remember(api, attempt);
    const state = this.host.state();
    // A reopened view may receive only its matching command outcome, never an old snapshot.
    if (
      api === this.host.api() &&
      state?.head.project.id === attempt.request.project_id &&
      state.sourceAudio?.request.idempotency_key ===
        attempt.request.idempotency_key
    )
      this.host.publish({ ...state, sourceAudio: attempt });
  }
  private sameView(
    attempt: V4SourceAudioAttempt,
    generation: number,
    view: number,
  ) {
    const state = this.host.state();
    return (
      generation === this.host.generation() &&
      view === this.host.viewGeneration() &&
      state?.head.project.id === attempt.request.project_id &&
      !state.historical &&
      state.sequenceId === attempt.request.sequence_id &&
      state.selectedId === attempt.request.element_id
    );
  }
  async start(command: "separate" | "restore") {
    const { api, state } = this.active();
    if (
      state.sourceAudio &&
      (state.sourceAudio.phase !== "saved" || !state.sourceAudio.headChecked)
    )
      throw new Error("Resolve the previous source audio operation first.");
    const available = sourceAudioAvailability(state);
    if (!available)
      throw new Error("Select a video element to edit its source audio.");
    if (available.reason) throw new Error(available.reason);
    if ((command === "restore") === available.sourceEnabled)
      throw new Error("This source audio state is already active.");
    const request: V4SourceAudioRequest = {
      project_id: state.head.project.id,
      expected_revision: state.head.revision,
      sequence_id: available.sequence.id,
      track_id: available.track.id,
      element_id: available.element.id,
      idempotency_key: crypto.randomUUID(),
      label:
        command === "separate"
          ? "Separate source audio"
          : "Restore source audio",
      command:
        command === "restore"
          ? { kind: "restore" }
          : {
              kind: "separate",
              audio_element_id: crypto.randomUUID(),
              destination: {
                kind: "first_available",
                fallback_track_id: crypto.randomUUID(),
              },
            },
    };
    const attempt: V4SourceAudioAttempt = {
      request,
      plan: null,
      receipt: null,
      phase: "planning",
      error: null,
      headChecked: false,
    };
    this.remember(api, attempt);
    this.host.publish({ ...state, sourceAudio: attempt });
    return this.execute(false);
  }
  async retry() {
    return this.execute(true);
  }
  async refreshSaved() {
    const { api, state } = this.active();
    const attempt = state.sourceAudio;
    if (!attempt?.receipt)
      throw new Error("No saved source audio receipt is available.");
    return this.check(attempt, this.host.generation(), api);
  }
  dismiss() {
    const { api, state } = this.active();
    if (
      state.sourceAudio &&
      ["planning", "applying", "unconfirmed"].includes(state.sourceAudio.phase)
    )
      throw new Error(
        "Check the saved outcome before starting another operation.",
      );
    this.outstanding.get(api)?.delete(state.head.project.id);
    this.host.publish({ ...state, sourceAudio: null });
  }
  private async check(
    attempt: V4SourceAudioAttempt,
    generation: number,
    api: EditorApi,
  ) {
    if (generation !== this.host.generation() || api !== this.host.api()) {
      this.publish(attempt, api);
      return attempt;
    }
    try {
      await this.refresh();
      if (generation !== this.host.generation() || api !== this.host.api()) {
        this.publish(attempt, api);
        return attempt;
      }
      const reflected =
        !attempt.receipt ||
        this.host.state()?.head.revision !== attempt.receipt.base_revision;
      attempt = {
        ...attempt,
        headChecked: reflected,
        error: reflected
          ? attempt.error
          : "Saved changes have not appeared in this view. Refresh the saved changes.",
      };
    } catch (error) {
      attempt = { ...attempt, headChecked: false, error: message(error) };
    }
    this.publish(attempt, api);
    return attempt;
  }
  private async execute(retry: boolean) {
    const { api, state } = this.active();
    let attempt = state.sourceAudio;
    if (!attempt) throw new Error("Choose a source audio operation first.");
    if (state.historical)
      throw new Error("Return to the current revision before editing.");
    if (
      state.sequenceId !== attempt.request.sequence_id ||
      state.selectedId !== attempt.request.element_id
    )
      throw new Error(
        "Return to the original video before retrying its source audio operation.",
      );
    const generation = this.host.generation(),
      view = this.host.viewGeneration();
    let release: () => void;
    try {
      release = this.host.acquire(attempt.request.project_id);
    } catch (error) {
      this.publish(
        {
          ...attempt,
          phase: attempt.phase === "unconfirmed" ? "unconfirmed" : "blocked",
          error: message(error),
        },
        api,
      );
      throw error;
    }
    let dispatched = false;
    const wasUnknown = attempt.phase === "unconfirmed";
    try {
      if (retry) {
        // A read does not prove commit/non-commit. Only replaying the identical
        // public transaction can recover its receipt; never synthesize a new plan.
        attempt = await this.check(attempt, generation, api);
        if (!attempt.headChecked)
          throw new Error(
            attempt.error ?? "Could not check the saved outcome.",
          );
      }
      if (!this.sameView(attempt, generation, view))
        throw new Error("Project view changed before saving.");
      const current = this.host.state()!;
      if (
        !wasUnknown &&
        current.head.revision !== attempt.request.expected_revision
      )
        throw new Error(
          "Project has changed; refresh the snapshot and try again.",
        );
      if (!attempt.plan) {
        attempt = { ...attempt, phase: "planning", error: null };
        this.publish(attempt, api);
        const plan = await api.sdk.call(
          "opencut_project_v4_source_audio_plan",
          attempt.request,
        );
        if (
          plan.transaction.project_id !== attempt.request.project_id ||
          plan.transaction.expected_revision !==
            attempt.request.expected_revision ||
          plan.transaction.idempotency_key !== attempt.request.idempotency_key
        )
          throw new Error(
            "Source audio plan does not match the captured request.",
          );
        attempt = { ...attempt, plan };
      }
      if (
        !this.sameView(attempt, generation, view) ||
        (!wasUnknown &&
          this.host.state()?.head.revision !==
            attempt.request.expected_revision)
      )
        throw new Error("Project view changed before saving.");
      attempt = { ...attempt, phase: "applying", error: null };
      this.publish(attempt, api);
      dispatched = true;
      const receipt = this.host.publicApply ? await this.host.publicApply(api, attempt.plan!.transaction, attempt.request.sequence_id)
        : await api.sdk.call("opencut_edit_v4_apply", attempt.plan!.transaction);
      if (
        receipt.base_revision !== attempt.request.expected_revision ||
        !receipt.revision ||
        !receipt.transaction_id
      )
        throw new Error("Source audio save returned an unexpected receipt.");
      attempt = { ...attempt, phase: "saved", receipt, error: null };
      this.publish(attempt, api);
      await this.check(attempt, generation, api);
      return receipt;
    } catch (error) {
      // Backend replay precedes CAS and is rechecked atomically. Only this
      // identical replay conflict disproves the previous unknown commit.
      if (
        wasUnknown &&
        dispatched &&
        error instanceof OpenCutApiError &&
        error.code === "REVISION_CONFLICT"
      ) {
        this.outstanding.get(api)?.delete(attempt.request.project_id);
        if (generation !== this.host.generation() || api !== this.host.api())
          throw new DOMException("Project view changed.", "AbortError");
        const current = this.host.state();
        if (
          current?.sourceAudio?.request.idempotency_key ===
          attempt.request.idempotency_key
        )
          this.host.publish({ ...current, sourceAudio: null });
        try {
          await this.refresh();
        } catch (refreshError) {
          if (generation !== this.host.generation() || api !== this.host.api())
            throw new DOMException("Project view changed.", "AbortError");
          throw refreshError;
        }
        if (generation !== this.host.generation() || api !== this.host.api())
          throw new DOMException("Project view changed.", "AbortError");
        throw new Error(
          "The original operation was not saved because the project changed. Choose the operation again on the current revision.",
        );
      }
      // A rejected retry cannot disprove an earlier unknown commit, including
      // revocation before receipt replay. Keep the unresolved original identity.
      const uncertain = wasUnknown || (dispatched && unknown(error));
      attempt = {
        ...attempt,
        phase: uncertain
          ? "unconfirmed"
          : dispatched || error instanceof OpenCutApiError
            ? "retryable"
            : "blocked",
        error: message(error),
        headChecked: false,
      };
      this.publish(attempt, api);
      if (uncertain && (!retry || dispatched))
        await this.check(attempt, generation, api);
      if (generation !== this.host.generation() || api !== this.host.api())
        throw new DOMException("Project view changed.", "AbortError");
      throw error;
    } finally {
      release();
    }
  }
}
