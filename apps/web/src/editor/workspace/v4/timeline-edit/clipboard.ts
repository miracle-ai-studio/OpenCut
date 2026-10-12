import { OpenCutApiError } from "../../../../lib/editor-sdk";
import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { EditorApi } from "../../api";
import { compareTime } from "../../../model/time";
import type { Time } from "../../../model/types";
import type { V4EditReceipt, V4SessionState } from "../types";
import type { TimelineTarget } from "./types";

export type ClipboardBuffer = ToolOutput<"opencut_timeline_v4_copy">;
export type ClipboardCopyRequest = ToolInput<"opencut_timeline_v4_copy">;
export type ClipboardPasteRequest = ToolInput<"opencut_timeline_v4_paste_plan">;
export type ClipboardPastePlan = ToolOutput<"opencut_timeline_v4_paste_plan">;
export type ClipboardAttempt = {
  request: ClipboardPasteRequest; plan: ClipboardPastePlan | null; receipt: V4EditReceipt | null;
  phase: "planning" | "planned" | "applying" | "unconfirmed" | "conflict" | "failed" | "saved";
  error: string | null;
};
export type ClipboardState = { buffer: ClipboardBuffer | null; phase: "idle" | "copying"; error: string | null; attempt: ClipboardAttempt | null };
export const emptyClipboardState = (): ClipboardState => ({ buffer: null, phase: "idle", error: null, attempt: null });
export type ClipboardHost = {
  api(): EditorApi | null; state(): V4SessionState | null; clipboard(): ClipboardState;
  generation(): number; viewGeneration(): number; publish(state: ClipboardState): void;
  acquire(projectId: string): () => void; selectMany(targets: TimelineTarget[], primaryId?: string): void;
  publicApply?: import("../session").V4SessionHost["publicApply"];
};
type Fence = { api: EditorApi; generation: number; view: number; epoch: number; projectId: string; revision: string; sequenceId: string; historical: boolean };
type Cache = { buffer: ClipboardBuffer | null; attempts: Map<string, ClipboardAttempt> };
const PREFIX = "OpenCut timeline references v4\n";
const message = (error: unknown) => error instanceof Error ? error.message : String(error);
function freeze<T>(value: T): T {
  if (value !== null && typeof value === "object" && !Object.isFrozen(value)) {
    Object.values(value).forEach(freeze); Object.freeze(value);
  }
  return value;
}

/** Only bounded typed references enter the system clipboard. They convey no
 * asset bytes/URI or authority; the service rechecks the digest and live Read. */
export function clipboardFromText(text: string): ClipboardBuffer | null {
  if (!text.startsWith(PREFIX)) return null;
  if (new TextEncoder().encode(text).length > 65536) throw new Error("The timeline clipboard is too large.");
  const candidate: unknown = JSON.parse(text.slice(PREFIX.length));
  const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
  const string = (value: unknown, max = 128): value is string => typeof value === "string" && value.trim().length > 0 &&
    new TextEncoder().encode(value).length <= max && !/[\u0000-\u001f\u007f]/.test(value);
  const keys = ["schema_version", "source_project_id", "source_revision", "source_sequence_id", "items", "selection_digest"];
  if (!object(candidate) || Object.keys(candidate).some(key => !keys.includes(key)) || candidate.schema_version !== 1 ||
      !string(candidate.source_project_id) || !string(candidate.source_revision, 1024) || !string(candidate.source_sequence_id) ||
      !string(candidate.selection_digest, 1024) || !Array.isArray(candidate.items) || candidate.items.length < 1 || candidate.items.length > 60 ||
      candidate.items.some(item => !object(item) || Object.keys(item).some(key => !["source_track_id", "source_track_kind", "source_element_id"].includes(key)) ||
        !string(item.source_track_id) || !string(item.source_element_id) || !["video", "audio", "text", "graphic", "effect", "caption"].includes(String(item.source_track_kind))))
    throw new Error("The timeline clipboard contains invalid references.");
  return freeze(candidate as unknown as ClipboardBuffer);
}

export class V4TimelineClipboard {
  private epoch = 0;
  private reading: AbortController | null = null;
  private readonly authenticated = new Map<string, Cache>();
  private readonly fallback = new WeakMap<EditorApi, Cache>();
  constructor(private readonly host: ClipboardHost, private readonly refresh: () => Promise<void>) {}
  identity() { return { api: this.host.api(), generation: this.host.generation(), viewGeneration: this.host.viewGeneration() }; }
  private cache(api: EditorApi): Cache {
    const identity = api.connectionIdentity;
    let cache = identity ? this.authenticated.get(identity) : this.fallback.get(api);
    if (!cache) {
      cache = { buffer: null, attempts: new Map() };
      if (identity) this.authenticated.set(identity, cache); else this.fallback.set(api, cache);
    }
    return cache;
  }
  resume(api: EditorApi, projectId: string): ClipboardState {
    const cache = this.cache(api); let attempt = cache.attempts.get(projectId) ?? null;
    if (attempt?.phase === "applying") { attempt = { ...attempt, phase: "unconfirmed" }; cache.attempts.set(projectId, attempt); }
    return { buffer: cache.buffer, phase: "idle", error: null, attempt };
  }
  recovered(api: EditorApi, projectId: string, key: string) {
    if (this.cache(api).attempts.get(projectId)?.request.idempotency_key === key) this.cache(api).attempts.delete(projectId);
    if (api === this.host.api() && this.host.state()?.head.project.id === projectId && this.host.clipboard().attempt?.request.idempotency_key === key)
      this.host.publish({ ...this.host.clipboard(), attempt: null });
  }
  private capture(): Fence {
    const api = this.host.api(), state = this.host.state();
    if (!api || !state) throw new Error("Open a project before using the timeline clipboard.");
    const snapshot = state.historical ?? state.head;
    return { api, generation: this.host.generation(), view: this.host.viewGeneration(), epoch: this.epoch,
      projectId: snapshot.project.id, revision: snapshot.revision, sequenceId: state.sequenceId, historical: !!state.historical };
  }
  private sameLocation(fence: Fence): boolean {
    const state = this.host.state(), snapshot = state?.historical ?? state?.head;
    return this.host.api() === fence.api && this.host.generation() === fence.generation && this.host.viewGeneration() === fence.view &&
      snapshot?.project.id === fence.projectId && state?.sequenceId === fence.sequenceId && !!state?.historical === fence.historical;
  }
  private same(fence: Fence, checkRevision = true): boolean {
    const state = this.host.state(), snapshot = state?.historical ?? state?.head;
    return this.sameLocation(fence) && this.epoch === fence.epoch && (!checkRevision || snapshot?.revision === fence.revision);
  }
  private check(fence: Fence, checkRevision = true) {
    if (!this.same(fence, checkRevision)) throw new Error("The clipboard target changed. Return to the original scene to continue.");
  }
  private publishAttempt(fence: Fence, attempt: ClipboardAttempt) {
    const cache = this.cache(fence.api), current = this.host.clipboard().attempt;
    const previous = cache.attempts.get(fence.projectId);
    if (previous?.request.idempotency_key !== attempt.request.idempotency_key && current?.request.idempotency_key !== attempt.request.idempotency_key) return;
    cache.attempts.set(fence.projectId, attempt);
    const wrote = ["applying", "unconfirmed", "conflict", "saved"].includes(attempt.phase);
    if ((wrote ? this.sameLocation(fence) : this.same(fence, false)) && current?.request.idempotency_key === attempt.request.idempotency_key)
      this.host.publish({ ...this.host.clipboard(), attempt });
  }
  private targets(): ClipboardCopyRequest["targets"] {
    const state = this.host.state()!;
    if (state.timelineEdit.selection.length) return structuredClone(state.timelineEdit.selection);
    const scene = (state.historical ?? state.head).project.sequences.find(sequence => sequence.id === state.sequenceId);
    const track = scene?.tracks.find(value => value.elements.some(element => element.id === state.selectedId));
    if (!track || !state.selectedId) throw new Error("Select timeline elements to copy.");
    return [{ track_id: track.id, element_id: state.selectedId }];
  }
  async copy(): Promise<ClipboardBuffer> {
    const fence = this.capture();
    if (this.reading || this.host.clipboard().attempt) throw new Error("Resolve the current clipboard operation first.");
    const request: ClipboardCopyRequest = freeze({ project_id: fence.projectId, revision: fence.revision,
      sequence_id: fence.sequenceId, targets: this.targets() });
    const abort = new AbortController(); this.reading = abort;
    this.host.publish({ ...this.host.clipboard(), phase: "copying", error: null });
    let release: (() => void) | undefined;
    try {
      release = this.host.acquire(fence.projectId); this.check(fence);
      const buffer = await fence.api.v4.timelineCopy(request, abort.signal); this.check(fence);
      if (buffer.schema_version !== 1 || buffer.source_project_id !== request.project_id || buffer.source_revision !== request.revision ||
          buffer.source_sequence_id !== request.sequence_id || !buffer.selection_digest || buffer.items.length !== request.targets.length ||
          buffer.items.some((item, index) => item.source_track_id !== request.targets[index].track_id || item.source_element_id !== request.targets[index].element_id))
        throw new Error("The copied references differ from the fixed selection.");
      this.cache(fence.api).buffer = freeze(buffer);
      this.host.publish({ ...this.host.clipboard(), buffer, phase: "idle", error: null }); return buffer;
    } catch (error) {
      if (this.same(fence)) this.host.publish({ ...this.host.clipboard(), phase: "idle", error: message(error) });
      throw error;
    } finally { if (this.reading === abort) this.reading = null; release?.(); }
  }
  text(): string | null { const api = this.host.api(), buffer = api && this.cache(api).buffer; return buffer ? PREFIX + JSON.stringify(buffer) : null; }
  selectedText(): string | null {
    const fence = this.capture(), buffer = this.cache(fence.api).buffer;
    if (!buffer || buffer.source_project_id !== fence.projectId || buffer.source_revision !== fence.revision || buffer.source_sequence_id !== fence.sequenceId) return null;
    try {
      const targets = this.targets();
      return targets.length === buffer.items.length && targets.every((target, index) =>
        target.track_id === buffer.items[index].source_track_id && target.element_id === buffer.items[index].source_element_id) ? this.text() : null;
    } catch { return null; }
  }
  importText(text: string): boolean {
    const buffer = clipboardFromText(text); if (!buffer) return false;
    const fence = this.capture(); this.cache(fence.api).buffer = buffer;
    this.host.publish({ ...this.host.clipboard(), buffer, error: null }); return true;
  }
  async planPaste(at?: Time, label = "Paste timeline elements"): Promise<ClipboardPastePlan> {
    const fence = this.capture(), state = this.host.state()!, buffer = this.cache(fence.api).buffer;
    if (fence.historical) throw new Error("Return to the current revision before pasting.");
    if (this.reading || this.host.clipboard().attempt || state.timelineEdit.attempt || state.timelineEdit.recoveryError)
      throw new Error("Resolve the current operation before pasting.");
    if (!buffer) throw new Error("Copy timeline elements before pasting.");
    if (buffer.source_project_id !== fence.projectId) throw new Error("Timeline elements can only be pasted within their original project.");
    const groups = new Map<string, ClipboardPasteRequest["placements"][number]>();
    for (const item of buffer.items) {
      let group = groups.get(item.source_track_id);
      if (!group) {
        group = { source_track_id: item.source_track_id, target: { kind: "classic_above_source", new_track_id: crypto.randomUUID(), name: "Pasted track" }, elements: [] };
        groups.set(item.source_track_id, group);
      }
      group.elements.push({ source_element_id: item.source_element_id, new_element_id: crypto.randomUUID() });
    }
    const attempt: ClipboardAttempt = { request: freeze({ project_id: fence.projectId, expected_revision: fence.revision,
      sequence_id: fence.sequenceId, idempotency_key: crypto.randomUUID(), label, clipboard: buffer,
      at: structuredClone(at ?? state.playhead), placements: [...groups.values()] }), phase: "planning", plan: null, receipt: null, error: null };
    this.cache(fence.api).attempts.set(fence.projectId, attempt); this.host.publish({ ...this.host.clipboard(), attempt });
    return this.planAttempt(fence, attempt);
  }
  async duplicate(): Promise<ClipboardPastePlan> {
    const fence = this.capture(), state = this.host.state()!;
    if (fence.historical) throw new Error("Return to the current revision before duplicating.");
    const targets = this.targets(), scene = state.head.project.sequences.find(sequence => sequence.id === fence.sequenceId)!;
    const starts = targets.map(target => scene.tracks.find(track => track.id === target.track_id)!.elements.find(element => element.id === target.element_id)!.timeline_range.start);
    const at = starts.reduce((first, next) => compareTime(first, next) <= 0 ? first : next);
    await this.copy(); this.check(fence);
    return this.planPaste(at, "Duplicate timeline elements");
  }
  async retryPlan() {
    const attempt = this.host.clipboard().attempt;
    if (!attempt || attempt.phase !== "failed" || attempt.plan) throw new Error("No clipboard request is ready to plan.");
    const fence = this.capture();
    if (fence.historical || fence.projectId !== attempt.request.project_id || fence.sequenceId !== attempt.request.sequence_id || fence.revision !== attempt.request.expected_revision)
      throw new Error("The original paste target changed. Discard it before making a new plan.");
    return this.planAttempt(fence, attempt);
  }
  private async planAttempt(fence: Fence, original: ClipboardAttempt): Promise<ClipboardPastePlan> {
    const abort = new AbortController(); this.reading = abort;
    let attempt = { ...original, phase: "planning" as const, error: null }, release: (() => void) | undefined;
    this.publishAttempt(fence, attempt);
    try {
      release = this.host.acquire(fence.projectId); this.check(fence);
      const plan = await fence.api.v4.timelinePastePlan(attempt.request, abort.signal); this.check(fence);
      const tx = plan.transaction;
      if (tx.project_id !== attempt.request.project_id || tx.expected_revision !== attempt.request.expected_revision ||
          tx.idempotency_key !== attempt.request.idempotency_key || tx.label !== attempt.request.label)
        throw new Error("The clipboard plan differs from its captured request.");
      this.publishAttempt(fence, { ...attempt, plan: freeze(plan), phase: "planned" }); return plan;
    } catch (error) {
      if (this.same(fence)) this.publishAttempt(fence, { ...attempt, phase: "failed", error: message(error) });
      throw error;
    } finally { if (this.reading === abort) this.reading = null; release?.(); }
  }
  async apply() {
    const fence = this.capture(); let attempt = this.host.clipboard().attempt;
    if (!attempt?.plan || !["planned", "failed", "unconfirmed"].includes(attempt.phase)) throw new Error("Review the original paste before saving.");
    const wasUnknown = attempt.phase === "unconfirmed";
    if (fence.historical || fence.projectId !== attempt.request.project_id || fence.sequenceId !== attempt.request.sequence_id)
      throw new Error("Return to the original current scene to recover this paste.");
    let dispatched = false, release: (() => void) | undefined;
    try {
      release = this.host.acquire(fence.projectId);
      if (wasUnknown) await fence.api.v4.inspect(fence.projectId); // A head is never a receipt.
      this.check(fence, !wasUnknown);
      if (!wasUnknown && fence.revision !== attempt.request.expected_revision) throw new Error("The paste target changed. Discard it before making a new plan.");
      attempt = { ...attempt, phase: "applying", error: null }; this.publishAttempt(fence, attempt); dispatched = true;
      const receipt = this.host.publicApply ? await this.host.publicApply(fence.api, attempt.plan!.transaction, attempt.request.sequence_id)
        : await fence.api.sdk.call("opencut_edit_v4_apply", attempt.plan!.transaction);
      if (receipt.base_revision !== attempt.request.expected_revision || !receipt.transaction_id || !receipt.revision)
        throw new Error("The paste returned an unexpected receipt.");
      attempt = { ...attempt, phase: "saved", receipt }; this.publishAttempt(fence, attempt);
      if (this.sameLocation(fence)) {
        try {
          await this.refresh();
          if (this.sameLocation(fence) && this.host.state()?.head.revision === receipt.revision &&
              this.host.clipboard().attempt?.request.idempotency_key === attempt.request.idempotency_key) {
            const selection = attempt.plan!.decision.selection;
            this.host.selectMany(selection, selection.at(-1)?.element_id);
          }
        } catch (error) { this.publishAttempt(fence, { ...attempt, error: message(error) }); }
      }
      return receipt;
    } catch (error) {
      const conflict = dispatched && error instanceof OpenCutApiError && error.code === "REVISION_CONFLICT";
      this.publishAttempt(fence, { ...attempt, phase: conflict ? "conflict" : wasUnknown || (dispatched && !(error instanceof OpenCutApiError)) ? "unconfirmed" : "failed", error: message(error) });
      throw error;
    } finally { release?.(); }
  }
  async refreshSaved() {
    const fence = this.capture(), attempt = this.host.clipboard().attempt;
    if (attempt?.phase !== "saved") throw new Error("No saved paste needs a refresh.");
    await this.refresh(); this.publishAttempt(fence, { ...attempt, error: null });
  }
  cancel() {
    const fence = this.capture(), attempt = this.host.clipboard().attempt;
    if (attempt && ["applying", "unconfirmed"].includes(attempt.phase)) throw new Error("Recover the original paste before discarding it.");
    this.epoch++; this.reading?.abort(); this.reading = null; this.cache(fence.api).attempts.delete(fence.projectId);
    this.host.publish({ ...this.host.clipboard(), phase: "idle", attempt: null, error: null });
  }
  invalidate() {
    this.epoch++; this.reading?.abort(); this.reading = null;
    const api = this.host.api(), state = this.host.state(); if (!api || !state) return;
    const current = this.host.clipboard(), attempt = current.attempt;
    const retained = attempt && ["applying", "unconfirmed", "conflict", "saved"].includes(attempt.phase) ? attempt : null;
    if (!retained) this.cache(api).attempts.delete(state.head.project.id);
    this.host.publish({ ...current, phase: "idle", attempt: retained, error: null });
  }
}
