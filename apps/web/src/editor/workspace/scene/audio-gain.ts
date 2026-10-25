import { OpenCutApiError } from "../../../lib/editor-sdk";
import { assertGainPlan } from "./audio-gain-plan";
import type { EditorApi } from "../api";
import type { SessionHost } from "./session";
import type {
  GainAttempt,
  GainDraft,
  GainRequest,
  SessionState,
} from "./types";

/** Affordances only. The service rechecks presence, locking, gain and keys. */
export function audioGainAvailability(
  state: SessionState,
  elementId: string,
) {
  const project = (state.historical ?? state.head).project;
  const sequence = project.sequences.find((s) => s.id === state.sequenceId);
  const track = sequence?.tracks.find((t) =>
    t.elements.some((e) => e.id === elementId),
  );
  const element = track?.elements.find((e) => e.id === elementId);
  if (!track || !element || !["audio", "video"].includes(element.content.kind))
    return null;
  const content = element.content;
  if (content.kind !== "audio" && content.kind !== "video") return null;
  const presence = project.assets.find((a) => a.id === content.asset_id)
    ?.metadata.audio.status;
  const reason = state.historical
    ? "Return to the current revision before editing."
    : track.locked
      ? "The audio element track is locked."
      : content.kind === "video" && presence !== "present"
        ? "This video needs known source audio before editing volume."
        : null;
  const animated = element.channels.some(
    (c) =>
      c.kind === "scalar" &&
      c.binding.kind === "property" &&
      ["gain_decibels", "gain_linear_legacy"].includes(c.binding.parameter) &&
      c.keys.length > 0,
  );
  return {
    track,
    element,
    audio: content.audio,
    animated,
    reason,
    lineReason:
      reason ??
      (animated
        ? "Volume automation is preserved; the static volume line is hidden."
        : content.audio.gain.unit !== "decibels"
          ? "Legacy linear gain cannot be edited with the volume line."
          : null),
    sourceDisabled: content.kind === "video" && !content.source_audio_enabled,
  };
}
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** No document mirror or independent SDK/write fence. Unknown commits retain
 * the original public transaction across routes on this authenticated API. */
export class AudioGain {
  private readonly outstanding = new WeakMap<
    EditorApi,
    Map<string, GainAttempt>
  >();
  private epoch = 0;
  invalidate() {
    this.epoch++;
    this.cancel();
  }
  private gesture: {
    api: EditorApi;
    generation: number;
    draft: GainDraft;
  } | null = null;
  constructor(
    private readonly host: SessionHost,
    private readonly refresh: () => Promise<void>,
    private readonly previewDraft: (
      draft: GainDraft | null,
      preserve?: boolean,
    ) => void = () => {},
  ) {}
  private active() {
    const api = this.host.api(),
      state = this.host.state();
    if (!api || !state) throw new Error("Open an editor project first.");
    return { api, state };
  }
  resume(api: EditorApi, projectId: string) {
    return this.outstanding.get(api)?.get(projectId) ?? null;
  }
  recovered(api: EditorApi, projectId: string, key: string) {
    if (this.outstanding.get(api)?.get(projectId)?.request.idempotency_key === key) this.outstanding.get(api)?.delete(projectId);
    const state = this.host.state();
    if (api === this.host.api() && state?.head.project.id === projectId && state.audioGain?.request.idempotency_key === key)
      this.host.publish({ ...state, audioGain: null });
  }
  private publish(api: EditorApi, attempt: GainAttempt) {
    let entries = this.outstanding.get(api);
    if (!entries) {
      entries = new Map();
      this.outstanding.set(api, entries);
    }
    entries.set(attempt.request.project_id, attempt);
    const state = this.host.state();
    if (
      api === this.host.api() &&
      state?.head.project.id === attempt.request.project_id &&
      state.audioGain?.request.idempotency_key ===
        attempt.request.idempotency_key
    )
      this.host.publish({ ...state, audioGain: attempt });
  }
  private ready(state: SessionState) {
    if (state.propertyDrafts?.length)
      throw new Error("Confirm or cancel the property draft before starting another edit.");
    if (state.audioGain && state.audioGain.phase !== "saved")
      throw new Error("Resolve the previous volume operation first.");
    if (state.audioGain?.receipt?.base_revision === state.head.revision)
      throw new Error("Refresh the saved audio changes before editing again.");
  }
  begin(elementId: string) {
    const { api, state } = this.active();
    this.ready(state);
    const available = audioGainAvailability(state, elementId);
    if (!available || available.lineReason)
      throw new Error(available?.lineReason ?? "Select an audio element.");
    const draft: GainDraft = {
      id: crypto.randomUUID(),
      projectId: state.head.project.id,
      revision: state.head.revision,
      sequenceId: state.sequenceId,
      trackId: available.track.id,
      elementId,
      originalDb: available.audio.gain.value,
      tenths: Math.round(available.audio.gain.value * 10),
      changed: false,
    };
    this.gesture = { api, generation: this.host.generation(), draft };
    this.host.publish({ ...state, selectedId: elementId, gainDraft: draft });
    return draft.id;
  }
  private currentDraft(id: string) {
    const g = this.gesture,
      s = this.host.state();
    if (!g || g.draft.id !== id) return null;
    if (
      g.api !== this.host.api() ||
      g.generation !== this.host.generation() ||
      !s ||
      s.historical ||
      s.head.project.id !== g.draft.projectId ||
      s.head.revision !== g.draft.revision ||
      s.sequenceId !== g.draft.sequenceId ||
      s.selectedId !== g.draft.elementId
    ) {
      this.cancel(id);
      return null;
    }
    return g;
  }
  update(id: string, tenths: number) {
    if (!Number.isInteger(tenths) || tenths < -600 || tenths > 200)
      throw new Error("Invalid volume line value.");
    const g = this.currentDraft(id);
    if (!g) return;
    g.draft = {
      ...g.draft,
      tenths,
      changed: Math.abs(tenths / 10 - g.draft.originalDb) > 0.0001,
    };
    this.host.publish({ ...this.host.state()!, gainDraft: g.draft });
    this.previewDraft(g.draft.changed ? g.draft : null);
  }
  cancel(id?: string, preservePreview = false) {
    if (id && this.gesture?.draft.id !== id) return;
    this.gesture = null;
    const state = this.host.state();
    if (state?.gainDraft) this.host.publish({ ...state, gainDraft: null });
    this.previewDraft(null, preservePreview);
  }
  async commit(id: string) {
    const g = this.currentDraft(id);
    if (!g) return;
    if (this.host.state()?.externalRevision)
      throw new Error("The project changed in another client. Cancel the volume draft and refresh before saving.");
    const draft = g.draft;
    this.cancel(id, draft.changed);
    if (!draft.changed) return;
    return this.start(
      draft.elementId,
      { kind: "set_decibels", decibels_tenths: draft.tenths },
      draft.id,
    );
  }
  async mute(elementId: string, muted: boolean) {
    this.ready(this.active().state);
    this.cancel();
    return this.start(elementId, { kind: "set_muted", muted });
  }
  private async start(
    elementId: string,
    command: GainRequest["command"],
    key: string = crypto.randomUUID(),
  ) {
    const { state } = this.active();
    this.ready(state);
    const available = audioGainAvailability(state, elementId);
    const reason =
      command.kind === "set_decibels"
        ? available?.lineReason
        : available?.reason;
    if (!available || reason)
      throw new Error(reason ?? "Select an audio element.");
    if (command.kind === "set_muted" && command.muted === available.audio.muted)
      return;
    const request: GainRequest = {
      project_id: state.head.project.id,
      expected_revision: state.head.revision,
      sequence_id: state.sequenceId,
      track_id: available.track.id,
      element_id: elementId,
      idempotency_key: key,
      label:
        command.kind === "set_muted"
          ? command.muted
            ? "Mute element"
            : "Unmute element"
          : "Change element volume",
      command,
    };
    this.host.publish({
      ...state,
      selectedId: elementId,
      audioGain: {
        request,
        plan: null,
        receipt: null,
        phase: "planning",
        error: null,
      },
    });
    return this.execute();
  }
  dismiss() {
    const { api, state } = this.active();
    if (state.audioGain?.receipt?.base_revision === state.head.revision)
      throw new Error("Refresh the saved audio changes before editing again.");
    if (
      state.audioGain &&
      ["unconfirmed", "planning", "applying"].includes(state.audioGain.phase)
    )
      throw new Error(
        "Retry the original volume operation to check its outcome.",
      );
    this.outstanding.get(api)?.delete(state.head.project.id);
    this.host.publish({ ...state, audioGain: null });
  }
  async retry() {
    return this.execute();
  }
  async refreshSaved() {
    await this.refresh();
  }
  private async execute() {
    const { api, state } = this.active();
    let attempt = state.audioGain;
    if (!attempt) return;
    const epoch = this.epoch,
      generation = this.host.generation(),
      wasUnknown = attempt.phase === "unconfirmed";
    const same = () => {
      const s = this.host.state();
      return (
        epoch === this.epoch &&
        api === this.host.api() &&
        generation === this.host.generation() &&
        s?.head.project.id === attempt!.request.project_id &&
        !s.historical &&
        s.sequenceId === attempt!.request.sequence_id &&
        s.selectedId === attempt!.request.element_id
      );
    };
    if (!same())
      throw new Error(
        "Return to the original element before retrying its volume operation.",
      );
    let release: (() => void) | undefined,
      dispatched = false;
    try {
      release = this.host.acquire(attempt.request.project_id);
      if (wasUnknown) await this.refresh(); // Read for display only; never infer success.
      if (
        !same() ||
        (!wasUnknown &&
          this.host.state()?.head.revision !==
            attempt.request.expected_revision)
      )
        throw new Error("Project view changed before saving.");
      if (!attempt.plan) {
        const available = audioGainAvailability(
          this.host.state()!,
          attempt.request.element_id,
        );
        if (!available) throw new Error("Select an audio element.");
        attempt = { ...attempt, phase: "planning", error: null };
        this.publish(api, attempt);
        const plan = await api.scene.audioGainPlan(attempt.request);
        assertGainPlan(
          attempt.request,
          plan,
          available.element,
          available.track.muted,
          available.animated,
        );
        attempt = { ...attempt, plan };
      }
      if (
        !same() ||
        (!wasUnknown &&
          this.host.state()?.head.revision !==
            attempt.request.expected_revision)
      )
        throw new Error("Project view changed before saving.");
      if (attempt.plan!.transaction) {
        attempt = { ...attempt, phase: "applying", error: null };
        this.publish(api, attempt);
        dispatched = true;
        const receipt = this.host.publicApply ? await this.host.publicApply(api, attempt.plan!.transaction!, attempt.request.sequence_id)
          : await api.sdk.call("opencut_edit_apply", attempt.plan!.transaction!);
        if (
          receipt.base_revision !== attempt.request.expected_revision ||
          !receipt.revision ||
          !receipt.transaction_id
        )
          throw new Error("Volume save returned an unexpected receipt.");
        attempt = { ...attempt, receipt };
      }
      attempt = { ...attempt, phase: "saved", error: null };
      this.publish(api, attempt);
      if (same()) await this.refresh();
      return attempt.receipt;
    } catch (error) {
      if (
        wasUnknown &&
        dispatched &&
        error instanceof OpenCutApiError &&
        error.code === "REVISION_CONFLICT"
      ) {
        attempt = {
          ...attempt,
          phase: "retryable",
          error:
            "The original volume operation was not saved because the project changed. Dismiss it before choosing a new operation on the current revision.",
        };
        this.publish(api, attempt);
        this.previewDraft(null);
        if (same()) {
          try {
            await this.refresh();
          } catch {
            /* Keep the original rejected transaction. */
          }
        }
        throw new Error(attempt.error!);
      }
      // Once received, a commit receipt remains authoritative even if refreshing fails.
      attempt = {
        ...attempt,
        phase: attempt.receipt
          ? "saved"
          : wasUnknown || (dispatched && !(error instanceof OpenCutApiError))
            ? "unconfirmed"
            : "retryable",
        error: message(error),
      };
      this.publish(api, attempt);
      if (!attempt.receipt && attempt.phase !== "unconfirmed")
        this.previewDraft(null);
      if (attempt.phase === "unconfirmed" && same()) {
        try {
          await this.refresh();
        } catch {
          /* Original retry remains available. */
        }
      }
      throw error;
    } finally {
      release?.();
    }
  }
}
