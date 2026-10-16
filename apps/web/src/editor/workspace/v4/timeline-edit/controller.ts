import { OpenCutApiError } from "../../../../lib/editor-sdk";
import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { EditorApi } from "../../api";
import { normalizeTimelineSelection, selectTimelineTarget } from "./selection";
import { RecoveryPointersV1 } from "./recovery";
import type {
  TimelineAttempt,
  TimelineCommand,
  TimelineEditHost,
  TimelinePlanner,
  TimelineScope,
  TimelineTarget,
} from "./types";

export type TrackRemovalRequest = ToolInput<"opencut_track_v1_remove_plan">;
export type TrackRemovalReview = {
  request: TrackRemovalRequest;
  transaction: ToolOutput<"opencut_track_v1_remove_plan">;
  trackName: string;
  elements: number;
  captions: number;
  transitions: number;
};

const message = (error: unknown) =>
  error instanceof Error ? error.message : String(error);
function freeze<T>(value: T): T {
  if (value !== null && typeof value === "object" && !Object.isFrozen(value)) {
    for (const child of Object.values(value)) freeze(child);
    Object.freeze(value);
  }
  return value;
}
function exact(value: unknown): string {
  return JSON.stringify(value, (_key, item) =>
    item && typeof item === "object" && !Array.isArray(item)
      ? Object.fromEntries(
          Object.entries(item).sort(([a], [b]) => a.localeCompare(b)),
        )
      : item,
  );
}

/** Fixed-head command/review/recovery. Domain edits are authored by the planner;
 * confirming or recovering submits its exact immutable public transaction. */
export class V4TimelineEdit {
  private epoch = 0;
  private planning: {
    api: EditorApi;
    projectId: string;
    key: string;
    abort: AbortController;
  } | null = null;
  private readonly outstanding = new WeakMap<
    EditorApi,
    Map<string, TimelineAttempt>
  >();
  private readonly authenticatedOutstanding = new Map<
    string,
    Map<string, TimelineAttempt>
  >();
  private readonly recoveryErrors = new Map<string, string>();
  private readonly recoveryReads = new Map<string, number>();
  private readonly pointers = new RecoveryPointersV1();
  private readonly trackRemovals = new WeakMap<TrackRemovalReview, {
    identity: ReturnType<V4TimelineEdit["identity"]>;
    epoch: number;
    attempted: boolean;
    receipt?: import("../types").V4EditReceipt;
  }>();
  constructor(
    private readonly host: TimelineEditHost,
    private readonly planner: TimelinePlanner,
    private readonly refresh: () => Promise<void>,
    private readonly durable = false,
    private readonly applyPublic?: import("../session").V4SessionHost["publicApply"],
  ) {}
  private active() {
    const api = this.host.api(),
      state = this.host.state();
    if (!api || !state) throw new Error("Open a V4 project first.");
    return { api, state };
  }
  identity() {
    return {
      api: this.host.api(),
      generation: this.host.generation(),
      viewGeneration: this.host.viewGeneration(),
    };
  }
  private checkTrackRemoval(request: TrackRemovalRequest, identity: ReturnType<V4TimelineEdit["identity"]>, epoch: number, fixedHead: boolean) {
    const { api, state } = this.active();
    if (api !== identity.api || identity.generation !== this.host.generation() ||
        identity.viewGeneration !== this.host.viewGeneration() || epoch !== this.epoch || state.historical ||
        request.project_id !== state.head.project.id || request.sequence_id !== state.sequenceId ||
        (fixedHead && request.expected_revision !== state.head.revision))
      throw Error("The track removal target changed. Return to its original project and review again.");
    if (state.timelineEdit.attempt || state.timelineEdit.recoveryError)
      throw Error("Resolve the previous timeline operation before removing a track.");
    if (!this.applyPublic) throw Error("Track removal requires the durable public-edit recovery adapter.");
    return { api, state };
  }
  async prepareTrackRemoval(request: TrackRemovalRequest, identity = this.identity(), signal?: AbortSignal): Promise<TrackRemovalReview> {
    identity = { ...identity };
    const original = freeze(structuredClone(request)), epoch = this.epoch,
      { api, state } = this.checkTrackRemoval(original, identity, epoch, true),
      sequence = state.head.project.sequences.find(s => s.id === request.sequence_id),
      track = sequence?.tracks.find(t => t.id === request.track_id);
    if (!track || track.locked || track.id === sequence!.main_track_id)
      throw Error("Only an unlocked non-main track can be removed.");
    const elements = new Set(track.elements.map(e => e.id)),
      captions = new Set(sequence!.captions.filter(c => c.track_id === track.id).map(c => c.id)),
      transitions = new Set(sequence!.transitions.filter(t => elements.has(t.incoming_element_id) || elements.has(t.outgoing_element_id)).map(t => t.id)),
      trackName = track.name, elementCount = elements.size;
    const release = this.host.acquire(request.project_id);
    try {
      signal?.throwIfAborted();
      const transaction = await api.sdk.call("opencut_track_v1_remove_plan", original, { signal });
      signal?.throwIfAborted();
      this.checkTrackRemoval(original, identity, epoch, true);
      if (transaction.project_id !== original.project_id || transaction.expected_revision !== original.expected_revision ||
          transaction.idempotency_key !== original.idempotency_key || transaction.label !== original.label)
        throw Error("Track removal plan changed the original request identity.");
      const remainingCaptions = new Set(captions), remainingTransitions = new Set(transitions);
      for (const [index, op] of transaction.operations.entries()) {
        if (op.type === "transition_delete" && op.sequence_id === original.sequence_id && index < transitions.size && remainingTransitions.delete(op.transition_id)) continue;
        if (op.type === "caption_delete" && op.sequence_id === original.sequence_id && !remainingTransitions.size && index < transitions.size + captions.size && remainingCaptions.delete(op.caption_id)) continue;
        if (op.type === "track_delete" && op.sequence_id === original.sequence_id && op.track_id === original.track_id &&
            !remainingTransitions.size && !remainingCaptions.size && index === transaction.operations.length - 1) continue;
        throw Error("Track removal plan changed content outside the selected track.");
      }
      if (transaction.operations.length !== transitions.size + captions.size + 1 || transaction.operations.at(-1)?.type !== "track_delete")
        throw Error("Track removal plan is incomplete.");
      const review = freeze({ request: original, transaction: structuredClone(transaction), trackName,
        elements: elementCount, captions: captions.size, transitions: transitions.size });
      this.trackRemovals.set(review, { identity, epoch, attempted: false });
      return review;
    } finally { release(); }
  }
  async applyTrackRemoval(review: TrackRemovalReview) {
    const captured = this.trackRemovals.get(review);
    if (!captured) throw Error("Review the original track removal plan before saving.");
    const { api } = this.checkTrackRemoval(review.request, captured.identity, captured.attempted ? this.epoch : captured.epoch, !captured.attempted),
      release = this.host.acquire(review.request.project_id);
    try {
      if (!captured.receipt) {
        captured.attempted = true;
        const receipt = await this.applyPublic!(api, review.transaction, review.request.sequence_id);
        if (receipt.base_revision !== review.request.expected_revision || !receipt.revision || !receipt.transaction_id)
          throw Error("Track removal returned another edit receipt.");
        captured.receipt = receipt;
      }
      // A disconnected view must not receive a late refresh or selection change.
      this.checkTrackRemoval(review.request, captured.identity, this.epoch, false);
      await this.refresh();
      const state = this.host.state(), sequence = state?.head.project.sequences.find(s => s.id === state.sequenceId);
      // refresh() deliberately invalidates stale timeline reads. Check its new
      // view directly, just as the existing timeline apply path does.
      if (state && sequence && api === this.host.api() && captured.identity.generation === this.host.generation() &&
          captured.identity.viewGeneration === this.host.viewGeneration() && !state.historical &&
          state.head.project.id === review.request.project_id && state.sequenceId === review.request.sequence_id &&
          state.head.revision === captured.receipt.revision) {
        captured.epoch = this.epoch;
        const selection = normalizeTimelineSelection(sequence, state.timelineEdit.selection);
        this.host.publish({ ...state.timelineEdit, selection }, selection.at(-1)?.element_id ?? null);
      }
      return captured.receipt;
    } finally { release(); }
  }
  /** Whole-page recovery discovers the host record even when staging succeeded
   * before the browser could save its non-secret locator. It never applies. */
  async restore(api: EditorApi, projectId: string) {
    const identity = api.connectionIdentity,
      cached = this.resume(api, projectId);
    if (!this.durable || !identity)
      return { attempt: cached, recoveryError: null };
    const scope = `${identity}\u0000${projectId}`;
    const read = (this.recoveryReads.get(scope) ?? 0) + 1;
    this.recoveryReads.set(scope, read);
    const generation = this.host.generation(),
      view = this.host.viewGeneration(),
      epoch = this.epoch;
    const initial = this.projects(api)?.get(projectId);
    const current = () =>
      this.recoveryReads.get(scope) === read &&
      api === this.host.api() &&
      generation === this.host.generation() &&
      view === this.host.viewGeneration() &&
      epoch === this.epoch &&
      this.projects(api)?.get(projectId) === initial;
    const latest = () => ({
      attempt: this.projects(api)?.get(projectId) ?? null,
      recoveryError: this.recoveryErrors.get(scope) ?? null,
    });
    try {
      const hints = this.pointers.list(identity);
      const record = await api.v4.recoveryRead(projectId);
      if (!current()) return latest();
      if (!record) {
        for (const hint of hints.filter((item) => item.projectId === projectId))
          this.pointers.remove(identity, projectId, hint.recoveryId);
        this.recoveryErrors.delete(scope);
        return { attempt: cached, recoveryError: null };
      }
      if (
        record.schema_version !== 1 ||
        record.project_id !== projectId ||
        record.context.kind !== "timeline_v4" ||
        !record.context.plan.transaction ||
        record.context.request.project_id !== projectId ||
        record.context.request.sequence_id !== record.sequence_id
      )
        throw new Error("The service returned an invalid recovery record.");
      const { request, plan } = record.context;
      const tx = plan.transaction!;
      if (
        tx.project_id !== projectId ||
        tx.expected_revision !== request.expected_revision ||
        tx.idempotency_key !== request.idempotency_key ||
        tx.label !== request.label ||
        (record.receipt &&
          record.receipt.base_revision !== request.expected_revision)
      )
        throw new Error(
          "The recovery transaction differs from its fixed target.",
        );
      this.pointers.put(identity, {
        projectId,
        sequenceId: record.sequence_id,
        recoveryId: record.recovery_id,
      });
      // Preserve exact in-memory object identity on ordinary reconnects.
      const prior =
        cached?.request.idempotency_key === request.idempotency_key
          ? cached
          : null;
      if (
        prior?.plan &&
        (exact(prior.request) !== exact(request) ||
          exact(prior.plan) !== exact(plan))
      )
        throw new Error(
          "The durable recovery record differs from the original review.",
        );
      const attempt: TimelineAttempt = {
        tool: prior?.tool ?? "opencut_edit_timeline_plan_v4",
        request: prior?.request ?? freeze(request),
        plan: prior?.plan ?? freeze(plan),
        phase: record.receipt ? "saved" : "unconfirmed",
        receipt: record.receipt ?? prior?.receipt ?? null,
        error: null,
        recoveryId: record.recovery_id,
      };
      this.projects(api, true)!.set(projectId, attempt);
      this.recoveryErrors.delete(scope);
      return { attempt, recoveryError: null };
    } catch (error) {
      if (!current()) return latest();
      const reason = message(error);
      this.recoveryErrors.set(scope, reason);
      return { attempt: cached, recoveryError: reason };
    }
  }
  resetRecoveryPointer() {
    this.pointers.reset();
  }
  private async stage(api: EditorApi, attempt: TimelineAttempt) {
    if (!this.durable || !api.connectionIdentity) return attempt;
    const identity = api.connectionIdentity;
    try {
      const staged = await api.v4.recoveryStage({
        context: {
          kind: "timeline_v4",
          request: attempt.request,
          plan: attempt.plan!,
        },
      });
      const record = await api.v4.recoveryRead(
        attempt.request.project_id,
        staged.recovery_id,
      );
      if (
        !record ||
        record.schema_version !== 1 ||
        record.recovery_id !== staged.recovery_id ||
        record.project_id !== attempt.request.project_id ||
        record.context.kind !== "timeline_v4" ||
        exact(record.context.request) !== exact(attempt.request) ||
        exact(record.context.plan) !== exact(attempt.plan)
      )
        throw new Error(
          "The original timeline transaction could not be durably verified.",
        );
      const stagedAttempt = { ...attempt, recoveryId: record.recovery_id };
      this.publish(api, stagedAttempt);
      this.pointers.put(identity, {
        projectId: record.project_id,
        sequenceId: record.sequence_id,
        recoveryId: record.recovery_id,
      });
      this.recoveryErrors.delete(
        `${identity}\u0000${attempt.request.project_id}`,
      );
      const current = this.host.state();
      if (
        api === this.host.api() &&
        current?.head.project.id === attempt.request.project_id
      )
        this.host.publish({ ...current.timelineEdit, recoveryError: null });
      return stagedAttempt;
    } catch (error) {
      const reason = message(error);
      this.recoveryErrors.set(
        `${identity}\u0000${attempt.request.project_id}`,
        reason,
      );
      const current = this.host.state();
      if (
        api === this.host.api() &&
        current?.head.project.id === attempt.request.project_id
      )
        this.host.publish({ ...current.timelineEdit, recoveryError: reason });
      throw error;
    }
  }
  private async releaseRecovery(api: EditorApi, attempt: TimelineAttempt) {
    if (!attempt.recoveryId || !api.connectionIdentity) return;
    try {
      const proof = await api.v4.recoveryAck(
        attempt.request.project_id,
        attempt.recoveryId,
      );
      if (
        proof.recovery_id !== attempt.recoveryId ||
        (attempt.phase === "saved" &&
          (proof.resolution !== "saved" ||
            proof.receipt?.transaction_id !== attempt.receipt?.transaction_id))
      )
        throw new Error(
          "The recovery acknowledgment does not match the original receipt.",
        );
    } catch (error) {
      // An ack response may itself be lost after the atomic delete. Only a
      // reauthorized absence of this exact record permits locator cleanup;
      // the known edit receipt (rather than absence) remains the saved proof.
      if (
        !(error instanceof OpenCutApiError) ||
        error.code !== "NOT_FOUND" ||
        (await api.v4.recoveryRead(
          attempt.request.project_id,
          attempt.recoveryId,
        ))
      )
        throw error;
    }
    this.pointers.remove(
      api.connectionIdentity,
      attempt.request.project_id,
      attempt.recoveryId,
    );
  }
  async discard() {
    const { api, state } = this.active(),
      attempt = state.timelineEdit.attempt;
    if (attempt && ["applying", "unconfirmed"].includes(attempt.phase))
      throw new Error(
        "Recover the original timeline operation before discarding it.",
      );
    if (attempt?.recoveryId) {
      await this.releaseRecovery(api, attempt);
      this.publish(api, { ...attempt, recoveryId: undefined });
    }
    if (
      api !== this.host.api() ||
      this.host.state()?.head.project.id !== state.head.project.id ||
      this.host.state()?.timelineEdit.attempt?.request.idempotency_key !==
        attempt?.request.idempotency_key
    )
      throw new Error("The timeline target changed while releasing recovery.");
    this.cancel();
  }
  resume(api: EditorApi, projectId: string) {
    const projects = this.projects(api),
      attempt = projects?.get(projectId);
    if (attempt?.phase === "applying") {
      const unconfirmed: TimelineAttempt = { ...attempt, phase: "unconfirmed" };
      projects!.set(projectId, unconfirmed);
      return unconfirmed;
    }
    return attempt ?? null;
  }
  recovered(api: EditorApi, projectId: string, key: string) {
    if (this.projects(api)?.get(projectId)?.request.idempotency_key === key)
      this.projects(api)?.delete(projectId);
    const state = this.host.state();
    if (
      api === this.host.api() &&
      state?.head.project.id === projectId &&
      state.timelineEdit.attempt?.request.idempotency_key === key
    )
      this.host.publish({ ...state.timelineEdit, attempt: null });
  }
  private projects(api: EditorApi, create = false) {
    const identity = api.connectionIdentity;
    const cached = identity
      ? this.authenticatedOutstanding.get(identity)
      : this.outstanding.get(api);
    if (cached || !create) return cached;
    const projects = new Map<string, TimelineAttempt>();
    if (identity) this.authenticatedOutstanding.set(identity, projects);
    else this.outstanding.set(api, projects);
    return projects;
  }
  private publish(api: EditorApi, attempt: TimelineAttempt) {
    let projects = this.projects(api);
    const current = this.host.state(),
      key = attempt.request.idempotency_key;
    const visible =
      api === this.host.api() &&
      current?.head.project.id === attempt.request.project_id &&
      current.timelineEdit.attempt?.request.idempotency_key === key;
    if (
      projects?.get(attempt.request.project_id)?.request.idempotency_key !==
        key &&
      !visible
    )
      return;
    projects ??= this.projects(api, true)!;
    projects.set(attempt.request.project_id, attempt);
    if (visible && current)
      this.host.publish({ ...current.timelineEdit, attempt });
  }
  private check(
    api: EditorApi,
    attempt: TimelineAttempt,
    generation: number,
    view: number,
    epoch: number,
    checkRevision = true,
  ) {
    const state = this.host.state();
    if (
      api !== this.host.api() ||
      generation !== this.host.generation() ||
      view !== this.host.viewGeneration() ||
      epoch !== this.epoch ||
      state?.head.project.id !== attempt.request.project_id ||
      state.historical ||
      state.sequenceId !== attempt.request.sequence_id
    )
      throw new Error(
        "The timeline target changed. Return to the original scene to continue.",
      );
    if (
      checkRevision &&
      state.head.revision !== attempt.request.expected_revision
    )
      throw new Error(
        "The project changed. Discard this plan and review a new timeline operation.",
      );
  }
  select(
    target: TimelineTarget,
    mode: "replace" | "toggle" | "extend" = "replace",
  ) {
    const { state } = this.active();
    if (state.timelineEdit.attempt?.phase === "applying")
      throw new Error("Wait for the timeline operation to finish.");
    const sequence = (state.historical ?? state.head).project.sequences.find(
      (s) => s.id === state.sequenceId,
    );
    if (!sequence) return;
    const selection = selectTimelineTarget(
      sequence,
      state.timelineEdit.selection,
      target,
      mode,
    );
    this.host.publish(
      { ...state.timelineEdit, selection },
      selection.at(-1)?.element_id ?? null,
    );
  }
  clearSelection() {
    const { state } = this.active();
    this.host.publish({ ...state.timelineEdit, selection: [] }, null);
  }
  selectMany(targets: readonly TimelineTarget[], primaryId?: string | null) {
    const { state } = this.active();
    if (state.timelineEdit.attempt?.phase === "applying")
      throw new Error("Wait for the timeline operation to finish.");
    const sequence = (state.historical ?? state.head).project.sequences.find(
      (s) => s.id === state.sequenceId,
    );
    if (!sequence) return;
    const selection = normalizeTimelineSelection(sequence, [...targets]);
    const primary =
      primaryId && selection.some((target) => target.element_id === primaryId)
        ? primaryId
        : (selection.at(-1)?.element_id ?? null);
    this.host.publish({ ...state.timelineEdit, selection }, primary);
  }
  /** Navigation invalidates outstanding reads, but preserves unknown writes for
   * explicit same-request recovery when the original authenticated view returns. */
  invalidate(clearSelection = true) {
    this.epoch++;
    const planning = this.planning;
    planning?.abort.abort();
    if (
      planning &&
      this.projects(planning.api)?.get(planning.projectId)?.request
        .idempotency_key === planning.key
    )
      this.projects(planning.api)?.delete(planning.projectId);
    const api = this.host.api(),
      state = this.host.state();
    if (!api || !state) return;
    const attempt = state.timelineEdit.attempt;
    if (attempt && ["planning", "planned", "failed"].includes(attempt.phase)) {
      this.projects(api)?.delete(state.head.project.id);
      this.host.publish({
        selection: clearSelection ? [] : state.timelineEdit.selection,
        attempt: null,
      });
    } else if (clearSelection)
      this.host.publish({ ...state.timelineEdit, selection: [] });
  }
  cancel() {
    const { api, state } = this.active(),
      attempt = state.timelineEdit.attempt;
    if (attempt && ["applying", "unconfirmed"].includes(attempt.phase))
      throw new Error(
        "Recover the original timeline operation before discarding it.",
      );
    if (attempt?.recoveryId)
      throw new Error(
        "Release the durable recovery record before discarding it.",
      );
    this.epoch++;
    this.planning?.abort.abort();
    this.projects(api)?.delete(state.head.project.id);
    this.host.publish({ ...state.timelineEdit, attempt: null });
  }
  async plan(
    command: TimelineCommand,
    label = "Edit timeline",
    scope?: TimelineScope,
  ) {
    const { api, state } = this.active();
    if (state.historical)
      throw new Error("Return to the current revision before editing.");
    if (
      state.timelineEdit.recoveryError ||
      (api.connectionIdentity &&
        this.recoveryErrors.has(
          `${api.connectionIdentity}\u0000${state.head.project.id}`,
        ))
    )
      throw new Error(
        "Restore the timeline recovery state before starting another operation.",
      );
    if (
      scope &&
      (scope.projectId !== state.head.project.id ||
        scope.revision !== state.head.revision ||
        scope.sequenceId !== state.sequenceId)
    )
      throw new Error(
        "The timeline target changed. Close this form and review a new operation.",
      );
    if (state.timelineEdit.attempt)
      throw new Error(
        "Resolve the previous timeline operation before starting another.",
      );
    const sequence = state.head.project.sequences.find(
      (scene) => scene.id === state.sequenceId,
    )!;
    const targets =
      command.kind === "move_selection"
        ? command.moves.map((move) => move.target)
        : command.kind === "trim"
          ? [command.target]
          : command.kind === "split"
            ? command.targets.map((split) => split.target)
            : command.kind === "delete"
              ? command.targets
              : [];
    const captured = {
      generation: this.host.generation(),
      view: this.host.viewGeneration(),
      epoch: this.epoch,
    };
    const complete = (await api.sdk.hostCapabilities()).tools.includes(
      "opencut_edit_timeline_v5_plan",
    );
    if (
      api !== this.host.api() ||
      captured.generation !== this.host.generation() ||
      captured.view !== this.host.viewGeneration() ||
      captured.epoch !== this.epoch ||
      this.host.state()?.head.revision !== state.head.revision ||
      this.host.state()?.timelineEdit.attempt
    )
      throw new Error(
        "The timeline target changed while reading host capabilities.",
      );
    const includesText =
      command.kind === "add_track"
        ? command.track_kind === "text"
        : ("track_id" in command &&
            sequence.tracks.find((track) => track.id === command.track_id)
              ?.kind === "text") ||
          targets.some((target) =>
            sequence.tracks
              .find((track) => track.id === target.track_id)
              ?.elements.some(
                (element) =>
                  element.id === target.element_id &&
                  element.content.kind === "text",
              ),
          );
    const attempt: TimelineAttempt = {
      tool: complete
        ? "opencut_edit_timeline_v5_plan"
        : includesText
          ? "opencut_edit_element_timeline_plan_v4"
          : "opencut_edit_timeline_plan_v4",
      request: freeze({
        project_id: state.head.project.id,
        expected_revision: state.head.revision,
        sequence_id: state.sequenceId,
        idempotency_key: crypto.randomUUID(),
        label,
        command: structuredClone(command),
      }),
      plan: null,
      receipt: null,
      phase: "planning",
      error: null,
    };
    this.host.publish({ ...state.timelineEdit, attempt });
    this.publish(api, attempt);
    return this.planAttempt(api, attempt);
  }
  async retryPlan() {
    const { api, state } = this.active(),
      attempt = state.timelineEdit.attempt;
    if (!attempt || attempt.phase !== "failed" || attempt.plan)
      throw new Error("No original timeline request is ready to plan.");
    return this.planAttempt(api, attempt);
  }
  private async planAttempt(api: EditorApi, original: TimelineAttempt) {
    const generation = this.host.generation(),
      view = this.host.viewGeneration(),
      epoch = this.epoch;
    let attempt: TimelineAttempt = {
      ...original,
      phase: "planning",
      error: null,
    };
    const planning = {
      api,
      projectId: attempt.request.project_id,
      key: attempt.request.idempotency_key,
      abort: new AbortController(),
    };
    this.planning = planning;
    this.publish(api, attempt);
    let release: (() => void) | undefined;
    try {
      release = this.host.acquire(attempt.request.project_id);
      this.check(api, attempt, generation, view, epoch);
      const plan = await this.planner(
        api,
        attempt.request,
        planning.abort.signal,
        attempt.tool ?? "opencut_edit_timeline_plan_v4",
      );
      this.check(api, attempt, generation, view, epoch);
      const tx = plan.transaction;
      if (
        tx &&
        (tx.project_id !== attempt.request.project_id ||
          tx.expected_revision !== attempt.request.expected_revision ||
          tx.idempotency_key !== attempt.request.idempotency_key ||
          tx.label !== attempt.request.label)
      )
        throw new Error("Timeline plan does not match the captured request.");
      attempt = { ...attempt, plan: freeze(plan), phase: "planned" };
      this.publish(api, attempt);
      return plan;
    } catch (error) {
      // A cancelled read cannot restore its discarded plan/command cache.
      if (epoch === this.epoch)
        this.publish(api, {
          ...attempt,
          phase: "failed",
          error: message(error),
        });
      throw error;
    } finally {
      if (this.planning === planning) this.planning = null;
      release?.();
    }
  }
  async apply() {
    const { api, state } = this.active();
    let attempt = state.timelineEdit.attempt;
    if (
      !attempt?.plan ||
      !["planned", "failed", "unconfirmed"].includes(attempt.phase)
    )
      throw new Error("Review the original timeline plan before saving.");
    const generation = this.host.generation(),
      view = this.host.viewGeneration(),
      epoch = this.epoch;
    const wasUnknown = attempt.phase === "unconfirmed";
    let dispatched = false,
      release: (() => void) | undefined;
    try {
      release = this.host.acquire(attempt.request.project_id);
      if (wasUnknown) await api.v4.inspect(attempt.request.project_id); // Observation is not a receipt.
      this.check(api, attempt, generation, view, epoch, !wasUnknown);
      const tx = attempt.plan.transaction;
      if (!tx) {
        this.cancel();
        return null;
      }
      // Only legacy V1 records use their original staged context/ack. New
      // production timeline writes share the versioned V2 coordinator.
      const legacy = !!attempt.recoveryId || !this.applyPublic;
      if (legacy && attempt.tool === "opencut_edit_timeline_v5_plan")
        throw new Error(
          "Complete timeline edits require the shared durable public-apply recovery adapter.",
        );
      if (legacy) attempt = await this.stage(api, attempt);
      this.check(api, attempt, generation, view, epoch, !wasUnknown);
      attempt = { ...attempt, phase: "applying", error: null };
      this.publish(api, attempt);
      dispatched = true;
      const receipt =
        !legacy && this.applyPublic
          ? await this.applyPublic(api, tx, attempt.request.sequence_id)
          : await api.sdk.call("opencut_edit_v4_apply", tx);
      if (
        receipt.base_revision !== attempt.request.expected_revision ||
        !receipt.revision ||
        !receipt.transaction_id
      )
        throw new Error("Timeline save returned an unexpected receipt.");
      attempt = { ...attempt, phase: "saved", receipt, error: null };
      this.publish(api, attempt);
      if (
        api === this.host.api() &&
        generation === this.host.generation() &&
        view === this.host.viewGeneration()
      ) {
        try {
          await this.releaseRecovery(api, attempt);
          attempt = { ...attempt, recoveryId: undefined };
          this.publish(api, attempt);
        } catch (error) {
          // The edit receipt stays saved even when cleanup fails. An old API
          // must retain the host record while a reconnect shows unconfirmed.
          attempt = { ...attempt, error: message(error) };
          this.publish(api, attempt);
        }
      }
      if (api === this.host.api() && generation === this.host.generation()) {
        try {
          await this.refresh();
          const current = this.host.state();
          if (
            current &&
            !current.historical &&
            current.head.project.id === attempt.request.project_id &&
            current.sequenceId === attempt.request.sequence_id &&
            current.head.revision === receipt.revision &&
            view === this.host.viewGeneration() &&
            this.projects(api)?.get(attempt.request.project_id)?.request
              .idempotency_key === attempt.request.idempotency_key
          ) {
            const sequence = current.head.project.sequences.find(
              (s) => s.id === current.sequenceId,
            );
            const selection = sequence
              ? normalizeTimelineSelection(
                  sequence,
                  attempt.plan!.decision.selection,
                )
              : [];
            this.host.publish(
              { ...current.timelineEdit, selection },
              selection.at(-1)?.element_id ?? null,
            );
          }
        } catch (error) {
          this.publish(api, { ...attempt, error: message(error) });
        }
      }
      return receipt;
    } catch (error) {
      const cached = this.projects(api)?.get(attempt.request.project_id);
      if (
        cached?.request.idempotency_key === attempt.request.idempotency_key &&
        cached.recoveryId
      )
        attempt = { ...attempt, recoveryId: cached.recoveryId };
      const conflict =
        dispatched &&
        error instanceof OpenCutApiError &&
        error.code === "REVISION_CONFLICT";
      this.publish(api, {
        ...attempt,
        phase: conflict
          ? "conflict"
          : wasUnknown ||
              !!attempt.recoveryId ||
              (dispatched && !(error instanceof OpenCutApiError))
            ? "unconfirmed"
            : "failed",
        error: conflict
          ? "The original timeline operation was not saved because the project changed. Discard it before reviewing a new plan."
          : message(error),
      });
      throw error;
    } finally {
      release?.();
    }
  }
  async refreshSaved() {
    const { api, state } = this.active(),
      attempt = state.timelineEdit.attempt;
    if (attempt?.phase !== "saved")
      throw new Error("No saved timeline receipt needs a refresh.");
    await this.refresh();
    this.publish(api, { ...attempt, error: null });
  }
}
