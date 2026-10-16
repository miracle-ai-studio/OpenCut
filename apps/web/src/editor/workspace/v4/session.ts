import { V4CaptionRecovery, type CaptionContext } from "./captions/recovery";
import { V4VisualPreview } from "./visual-preview";
import { V4AudioGain } from "./audio-gain";
import { V4AudioPreview, emptyAudioPreview } from "./audio-preview";
import { ExportVideoV2 } from "./export_video_v2";
import { V4VideoMp4 } from "./video-mp4";
import { V4AudioWav } from "./audio-wav";
import { V4SourceAudio } from "./source-audio";
import { V4Media } from "./media";
import { V4Frame } from "./frame";
import { V4TimelineEdit } from "./timeline-edit/controller";
import { V4TimelineClipboard } from "./timeline-edit/clipboard";
import { emptyTimelineEdit } from "./timeline-edit/types";
import { normalizeTimelineSelection } from "./timeline-edit/selection";
import { V4FontRecovery, fontRecoveryBlocked, emptyFontRecovery } from "./text/font-recovery";
import { V4EditRecovery, emptyEditRecovery } from "./edit-recovery";
import type { ToolInput } from "../../../lib/editor-sdk";
import type { EditorApi } from "../api";
import type { ProjectRef, Time } from "../../model/types";
import { zero } from "../../model/time";
import type { V4BookmarkOperation, V4SessionState, V4Operation, V4EditReceipt } from "./types";

export type V4SessionHost = {
  api(): EditorApi | null;
  state(): V4SessionState | null;
  generation(): number;
  viewGeneration(): number;
  changeView(): number;
  publish(state: V4SessionState): void;
  acquire(projectId: string): () => void;
  refreshTasks(): Promise<void>;
  publicApply?(api: EditorApi, request: ToolInput<"opencut_edit_v4_apply">, sequenceId: string | null): Promise<V4EditReceipt>;
};
/** A delegate with no project state or SDK of its own. The workspace controller
 * owns the mutually exclusive legacy/V4 session, routing and write fence. */
export class V4Session {
  readonly recovery: V4EditRecovery;
  readonly fontRecovery: V4FontRecovery;
  readonly captionRecovery: V4CaptionRecovery;
  readonly timelineEdit: V4TimelineEdit;
  readonly clipboard: V4TimelineClipboard;
  readonly audioGain: V4AudioGain;
  readonly audioPreview: V4AudioPreview;
  readonly visualPreview: V4VisualPreview;
  readonly sourceAudio: V4SourceAudio;
  readonly audioWav: V4AudioWav;
  readonly videoMp4: V4VideoMp4;
  readonly exporter: ExportVideoV2;
  readonly media: V4Media;
  readonly frame: V4Frame;
  constructor(private readonly host: V4SessionHost) {
    this.fontRecovery = new V4FontRecovery(host, () => this.refresh());
    this.captionRecovery = new V4CaptionRecovery(host, () => this.refresh());
    const edits = { ...host, acquire: (projectId: string) => this.acquireEdit(projectId) };
    this.recovery = new V4EditRecovery(edits, () => this.refresh());
    const writes: V4SessionHost = { ...edits, publicApply: (api, request, sequenceId) => this.recovery.publicApply(api, request, sequenceId) };
    this.visualPreview = new V4VisualPreview(host, () => { void this.audioPreview.pause().catch(() => {}); });
    this.audioPreview = new V4AudioPreview(host, (time) =>
      this.presentAudio(time),
    );
    this.audioGain = new V4AudioGain(
      writes,
      () => this.refresh(),
      (draft, preserve) => this.audioPreview.draft(draft, preserve),
    );
    this.sourceAudio = new V4SourceAudio(writes, () => this.refresh());
    this.audioWav = new V4AudioWav(host);
    this.videoMp4 = new V4VideoMp4(host);
    this.exporter = new ExportVideoV2(host);
    this.media = new V4Media(writes, () => this.refresh());
    this.frame = new V4Frame(host);
    this.timelineEdit = new V4TimelineEdit({
      ...edits,
      publish: (timelineEdit, primaryId) => {
        const state = host.state();
        if (!state) return;
        if (primaryId !== undefined && primaryId !== state.selectedId) this.audioGain.cancel();
        const current = host.state();
        if (current) host.publish({ ...current, timelineEdit,
          ...(primaryId === undefined ? {} : { selectedId: primaryId }) });
      },
    }, (api, request, signal, tool) => api.v4.timelinePlan(request, signal, tool), () => this.refresh(), true,
      (api, request, sequenceId) => this.recovery.publicApply(api, request, sequenceId));
    this.clipboard = new V4TimelineClipboard({
      ...edits,
      publicApply: writes.publicApply,
      clipboard: () => host.state()!.clipboard,
      publish: clipboard => {
        const state = host.state();
        if (state) host.publish({ ...state, clipboard });
      },
      selectMany: (targets, primaryId) => {
        this.timelineEdit.selectMany(targets, primaryId);
        const state = host.state();
        if (primaryId && !state?.timelineEdit.selection.some(target => target.element_id === primaryId)) this.select(primaryId);
      },
    }, () => this.refresh());
  }
  async generateCatalogSound(request:ToolInput<"opencut_catalog_resources_v1_generate">,signal?:AbortSignal) {
    const {api,state}=this.active();
    if(state.historical||state.head.project.id!==request.project_id||state.gainDraft||state.recovery?.record||state.recovery?.error||["reading","staging","applying"].includes(state.recovery?.phase??""))throw new Error("Resolve the current edit in the original project before generating a sound.");
    const release=this.acquireEdit(request.project_id);
    try{return await api.sdk.callWithAdmissionRetry("opencut_catalog_resources_v1_generate",request,{signal});}finally{release();}
  }
  async importCatalogResource(request:ToolInput<"opencut_catalog_resources_v1_import">,signal?:AbortSignal) {
    const {api,state}=this.active();
    if(state.historical||state.head.project.id!==request.project_id||state.gainDraft||state.recovery?.record||state.recovery?.error||["reading","staging","applying"].includes(state.recovery?.phase??""))throw new Error("Resolve the current edit in the original project before acquiring a resource.");
    const release=this.acquireEdit(request.project_id);
    try {return await api.sdk.callWithAdmissionRetry("opencut_catalog_resources_v1_import",request,{signal});} finally {release();}
  }
  async importBrandResource(request:ToolInput<"opencut_brand_resources_v1_import">,signal?:AbortSignal) {
    const {api,state}=this.active();
    if(state.historical||state.head.project.id!==request.project_id||state.gainDraft||state.recovery?.record||state.recovery?.error||["reading","staging","applying"].includes(state.recovery?.phase??""))throw new Error("Resolve the current edit in the original project before acquiring a brand.");
    const release=this.acquireEdit(request.project_id);
    try{return await api.sdk.callWithAdmissionRetry("opencut_brand_resources_v1_import",request,{signal});}finally{release();}
  }
  private acquireEdit(projectId: string) {
    if (this.captionRecovery.blocked()) throw new Error("Resolve the original caption edit before starting another operation.");
    if (fontRecoveryBlocked(this.host.state()?.fontRecovery))
      throw new Error("Resolve the original font import before starting another operation.");
    return this.host.acquire(projectId);
  }
  private active() {
    const api = this.host.api(),
      state = this.host.state();
    if (!api || !state) throw new Error("Open a V4 project first.");
    return { api, state };
  }
  async open(project: ProjectRef, sequenceId?: string | null) {
    this.recovery.invalidate();
    this.fontRecovery.invalidate();
    this.captionRecovery.invalidate();
    this.clipboard.invalidate();
    this.timelineEdit.invalidate();
    this.audioPreview.invalidate();
    this.audioGain.invalidate();
    const api = this.host.api();
    if (!api) return;
    const generation = this.host.generation(), view = this.host.viewGeneration();
    const [head, capability, frameCapability, textFrameCapability, rangeCapability, captionFrameCapability, graphicFrameCapability, blendFrameCapability, maskFrameCapability, sceneFrameCapability] =
      await Promise.all([
        api.v4.inspect(project.id),
        api.v4.audioCapability().then(
          (value) => ({ value, error: null }),
          () => ({
            value: null,
            error: "This service does not support scene audio WAV export.",
          }),
        ),
        api.v4.frameCapability().then(
          (value) => ({ value, error: null }),
          () => ({
            value: null,
            error: "This service does not support scene frames.",
          }),
        ),
        api.sdk.call("opencut_project_v4_text_frame_capability", {}).catch(() => null),
        api.v4.audioRangeCapability().then(
          (value) => ({ value, error: null }),
          () => ({
            value: null,
            error: "This service does not support sample range WAV export.",
          }),
        ),
        api.sdk.call("opencut_project_v4_caption_frame_capability", {}).catch(() => null),
        api.sdk.call("opencut_project_v4_graphic_frame_capability", {}).catch(() => null),
        api.sdk.call("opencut_project_v4_blend_frame_capability", {}).catch(() => null),
        api.sdk.call("opencut_project_v4_mask_frame_capability", {}).catch(() => null),
        api.sdk.call("opencut_project_v4_scene_frame_capability", {}).catch(() => null),
      ]);
    const { history, navigation } = await api.v4.history(
      project.id,
      head.revision,
    );
    await this.captionRecovery.restore(api, head.project.id);
    const captionBlocked = this.captionRecovery.blocked();
    const fontRecovery = captionBlocked ? emptyFontRecovery() : await this.fontRecovery.restore(api, head.project.id);
    const restoredRecovery = captionBlocked || fontRecoveryBlocked(fontRecovery) ? { state: emptyEditRecovery(), legacy: false }
      : await this.recovery.restore(api, head.project.id);
    const restoredTimeline = restoredRecovery.legacy ? await this.timelineEdit.restore(api, head.project.id)
      : { attempt: this.timelineEdit.resume(api, head.project.id), recoveryError: null };
    if (api !== this.host.api() || generation !== this.host.generation() || view !== this.host.viewGeneration()) return;
    this.host.publish({
      head,
      recovery: restoredRecovery.state,
      fontRecovery,
      textFrameCapability,
      captionFrameCapability,
      graphicFrameCapability,
      blendFrameCapability,
      maskFrameCapability,
      sceneFrameCapability,
      timelineEdit: { ...emptyTimelineEdit(), ...restoredTimeline },
      clipboard: this.clipboard.resume(api, head.project.id),
      audioPreview: emptyAudioPreview(),
      historical: null,
      history,
      navigation,
      sequenceId: head.project.sequences.some((s) => s.id === sequenceId)
        ? sequenceId!
        : head.project.default_sequence_id,
      playhead: zero,
      selectedId: null,
      historyPlan: null,
      gainDraft: null,
      audioGain: this.audioGain.resume(api, head.project.id),
      sourceAudio: this.sourceAudio.resume(api, head.project.id, head.revision),
      videoMp4Capability: null,
      videoMp4CapabilityError: null,
      videoMp4: this.videoMp4.resume(api, head.project.id),
      audioWavCapability: capability.value,
      audioWavCapabilityError: capability.error,
      audioRangeWavCapability: rangeCapability.value,
      audioRangeWavCapabilityError: rangeCapability.error,
      audioWav: this.audioWav.resume(api, head.project.id),
      media: {
        attempt: this.media.resume(api, head.project.id),
        available: [],
        loading: false,
        error: null,
      },
      frameCapability: frameCapability.value,
      frameCapabilityError: frameCapability.error,
      frame: this.frame.resume(api, head.project.id),
    });
  }
  async recoverTimeline(resetPointer = false) {
    const { api, state } = this.active(), generation = this.host.generation(), view = this.host.viewGeneration();
    if (resetPointer) this.timelineEdit.resetRecoveryPointer();
    const restored = await this.timelineEdit.restore(api, state.head.project.id);
    const current = this.host.state();
    if (api === this.host.api() && generation === this.host.generation() && view === this.host.viewGeneration() &&
        current?.head.project.id === state.head.project.id)
      this.host.publish({ ...current, timelineEdit: { ...current.timelineEdit, ...restored } });
  }
  async recoverEdit(resetPointer = false) {
    const { api, state } = this.active(), generation = this.host.generation(), view = this.host.viewGeneration();
    if (resetPointer) { this.recovery.resetPointer(); this.fontRecovery.resetPointer(); }
    await this.captionRecovery.restore(api, state.head.project.id);
    if (this.captionRecovery.blocked()) return;
    const fontRecovery = await this.fontRecovery.restore(api, state.head.project.id);
    if (fontRecoveryBlocked(fontRecovery)) return;
    const restored = await this.recovery.restore(api, state.head.project.id);
    if (api === this.host.api() && generation === this.host.generation() && view === this.host.viewGeneration()) {
      const current = this.host.state();
      if (current) this.host.publish({ ...current, recovery: restored.state });
      if (restored.legacy) await this.recoverTimeline();
    }
  }
  async retryOriginalEdit() {
    const { api, state } = this.active(), record = state.recovery?.record, generation = this.host.generation(), view = this.host.viewGeneration();
    if (!record) throw new Error("No original edit needs recovery.");
    const receipt = await this.recovery.retry();
    if (api === this.host.api() && generation === this.host.generation() && view === this.host.viewGeneration()) {
      const key = record.context.intent.request.idempotency_key;
      if (receipt.kind === "public_apply") {
        this.timelineEdit.recovered(api, record.project_id, key); this.clipboard.recovered(api, record.project_id, key);
        this.audioGain.recovered(api, record.project_id, key); this.media.recovered(api, record.project_id, key); this.sourceAudio.recovered(api, record.project_id, key);
      }
      const current = this.host.state();
      if (current && (receipt.kind === "undo" || receipt.kind === "redo")) this.host.publish({ ...current, historyPlan: null });
    }
    return receipt;
  }
  async refresh() {
    const { api, state } = this.active(),
      generation = this.host.generation();
    const head = await api.v4.inspect(state.head.project.id);
    const { history, navigation } = await api.v4.history(
      head.project.id,
      head.revision,
    );
    if (
      generation !== this.host.generation() ||
      this.host.state()?.head.revision !== state.head.revision
    )
      return;
    const current = this.host.state()!;
    const project = (current.historical ?? head).project;
    const sequenceId = project.sequences.some(
      (s) => s.id === current.sequenceId,
    )
      ? current.sequenceId
      : project.default_sequence_id;
    if (head.revision !== current.head.revision) {
      if (!current.historical) this.timelineEdit.invalidate(false);
      if (!current.historical) this.clipboard.invalidate();
      this.audioGain.cancel();
      if (!current.historical) this.audioPreview.revisionChanged();
    }
    const sequence = project.sequences.find(s => s.id === sequenceId)!;
    const timelineEdit = this.host.state()!.timelineEdit;
    const selection = normalizeTimelineSelection(sequence, timelineEdit.selection);
    const selectedId = current.selectedId && (
      sequence.bookmarks.some(b => b.id === current.selectedId) ||
      sequence.captions.some(c => c.id === current.selectedId) ||
      sequence.tracks.some(t => t.id === current.selectedId || t.elements.some(e => e.id === current.selectedId))
    ) ? current.selectedId : null;
    this.host.publish({
      ...current,
      audioPreview: this.host.state()!.audioPreview,
      timelineEdit: { ...timelineEdit, selection },
      clipboard: this.host.state()!.clipboard,
      selectedId,
      gainDraft:
        head.revision === current.head.revision ? current.gainDraft : null,
      head,
      sequenceId,
      history,
      navigation,
      historyPlan:
        head.revision === state.head.revision ? current.historyPlan : null,
    });
  }
  chooseSequence(sequenceId: string) {
    const { state } = this.active();
    if (
      !(state.historical ?? state.head).project.sequences.some(
        (s) => s.id === sequenceId,
      )
    )
      return;
    this.audioGain.invalidate();
    this.clipboard.invalidate();
    this.timelineEdit.invalidate();
    this.audioPreview.seek();
    this.host.changeView();
    this.host.publish({
      ...state,
      audioPreview: this.host.state()!.audioPreview,
      timelineEdit: this.host.state()!.timelineEdit,
      clipboard: this.host.state()!.clipboard,
      gainDraft: null,
      sequenceId,
      selectedId: null,
      playhead: zero,
      historyPlan: null,
    });
  }
  seek(playhead: Time) {
    if (this.audioPreview.seek(playhead)) return;
    const { state } = this.active();
    this.host.publish({ ...state, playhead });
  }
  /** Only service-confirmed consumer Time can present continuous sound. */
  private presentAudio(playhead: Time) {
    const state = this.host.state();
    if (state) this.host.publish({ ...state, playhead });
  }
  select(selectedId: string | null) {
    const { state } = this.active();
    const sequence = (state.historical ?? state.head).project.sequences.find(s => s.id === state.sequenceId);
    const track = sequence?.tracks.find(t => t.elements.some(e => e.id === selectedId &&
      ["video", "audio", "image", "text"].includes(e.content.kind)));
    if (track && selectedId) {
      this.timelineEdit.select({ track_id: track.id, element_id: selectedId });
      return;
    }
    if (selectedId !== state.selectedId) this.audioGain.cancel();
    const current = this.host.state()!;
    this.host.publish({ ...current, selectedId, timelineEdit: { ...current.timelineEdit, selection: [] } });
  }
  returnToHead() {
    this.clipboard.invalidate();
    this.audioPreview.seek();
    const { state } = this.active();
    this.timelineEdit.invalidate();
    if (state.historical) this.audioGain.invalidate();
    this.host.changeView();
    this.host.publish({
      ...state,
      timelineEdit: this.host.state()!.timelineEdit,
      clipboard: this.host.state()!.clipboard,
      historical: null,
      gainDraft: null,
      sequenceId: state.head.project.sequences.some(
        (s) => s.id === state.sequenceId,
      )
        ? state.sequenceId
        : state.head.project.default_sequence_id,
      selectedId: null,
      historyPlan: null,
    });
  }
  async inspectRevision(revision: string) {
    this.clipboard.invalidate();
    this.timelineEdit.invalidate();
    this.audioPreview.seek();
    this.audioGain.invalidate();
    const { api, state } = this.active(),
      generation = this.host.generation(),
      view = this.host.changeView();
    const historical = await api.v4.inspect(state.head.project.id, revision);
    if (
      generation !== this.host.generation() ||
      view !== this.host.viewGeneration()
    )
      return;
    this.host.publish({
      ...this.host.state()!,
      historical,
      gainDraft: null,
      sequenceId: historical.project.sequences.some(
        (s) => s.id === state.sequenceId,
      )
        ? state.sequenceId
        : historical.project.default_sequence_id,
      selectedId: null,
      historyPlan: null,
    });
  }
  async write(label: string, operations: V4Operation[]) {
    const { api, state } = this.active();
    if (state.historical)
      throw new Error("Return to the current revision before editing.");
    const release = this.acquireEdit(state.head.project.id),
      generation = this.host.generation(),
      view = this.host.viewGeneration();
    const request = api.v4.request(state.head, label, operations);
    try {
      await api.sdk.call("opencut_edit_v4_plan", request);
      if (
        generation !== this.host.generation() ||
        view !== this.host.viewGeneration()
      )
        throw new Error("Project view changed before saving.");
      const result = await this.recovery.publicApply(api, request, state.sequenceId);
      if (generation === this.host.generation()) await this.refresh();
      return result;
    } finally {
      release();
    }
  }
  /** A typed application planner already owns defaults, placement and time.
   * Preserve its original request/key through the shared durable write port. */
  async applyPlanned(request: ToolInput<"opencut_edit_v4_apply">, sequenceId: string) {
    const { api, state } = this.active();
    if (state.historical || state.head.project.id !== request.project_id ||
      state.head.revision !== request.expected_revision || state.sequenceId !== sequenceId)
      throw new Error("The edit target changed.");
    const generation = this.host.generation(), view = this.host.viewGeneration();
    const release = this.acquireEdit(request.project_id);
    try {
      if (api !== this.host.api() || generation !== this.host.generation() || view !== this.host.viewGeneration())
        throw new Error("The edit target changed.");
      const receipt = await this.recovery.publicApply(api, request, sequenceId);
      if (api === this.host.api() && generation === this.host.generation() && this.host.state()?.head.project.id === request.project_id)
        await this.refresh();
      return receipt;
    } finally { release(); }
  }
  async applyCaption(context: CaptionContext) {
    const { api, state } = this.active(), request = context.intent.request;
    if (state.historical || state.head.project.id !== request.project_id || state.head.revision !== request.expected_revision || state.sequenceId !== request.sequence_id)
      throw new Error("The caption target changed.");
    if (fontRecoveryBlocked(state.fontRecovery) || state.recovery?.record || state.recovery?.error)
      throw new Error("Resolve the original edit before changing captions.");
    const generation = this.host.generation(), view = this.host.viewGeneration(), release = this.host.acquire(request.project_id);
    try {
      const receipt = await this.captionRecovery.execute(api, context);
      if (api === this.host.api() && generation === this.host.generation() && view === this.host.viewGeneration() && this.host.state()?.head.project.id === request.project_id) await this.refresh();
      return receipt;
    } finally { release(); }
  }
  async importFont(request: ToolInput<"opencut_font_v1_import"> | ToolInput<"opencut_font_derived_v1_import">, protocol: "static" | "derived" | "static_v3" | "synthesis" = "static") {
    if (this.captionRecovery.blocked()) throw new Error("Resolve the original caption edit before importing fonts.");
    const { api, state } = this.active();
    if (state.historical || state.head.project.id !== request.project_id || state.head.revision !== request.expected_revision)
      throw new Error("The edit target changed.");
    const generation = this.host.generation(), view = this.host.viewGeneration(), release = this.host.acquire(request.project_id);
    try {
      const receipt = await this.fontRecovery.execute(api, { request, sequence_id: state.sequenceId }, protocol);
      if (api === this.host.api() && generation === this.host.generation() && view === this.host.viewGeneration() && this.host.state()?.head.project.id === request.project_id) await this.refresh();
      return receipt;
    } finally { release(); }
  }
  bookmark(operation: V4BookmarkOperation) {
    return this.write("Edit scene bookmark", [{ type: "bookmark", operation }]);
  }
  async planUndo(target_transaction_id: string) {
    this.clipboard.invalidate();
    this.timelineEdit.invalidate();
    const { api, state } = this.active(),
      generation = this.host.generation(),
      view = this.host.changeView();
    if (state.historical)
      throw new Error("Return to the current revision before editing.");
    const input = {
      project_id: state.head.project.id,
      expected_revision: state.head.revision,
      target_transaction_id,
      idempotency_key: crypto.randomUUID(),
    };
    const plan = await api.sdk.call("opencut_edit_v4_undo_plan", input);
    if (
      generation === this.host.generation() &&
      view === this.host.viewGeneration() &&
      this.host.state()?.head.revision === state.head.revision
    )
      this.host.publish({
        ...this.host.state()!,
        historyPlan: { direction: "undo", input, plan },
      });
  }
  async planRedo(undo_transaction_id: string) {
    this.clipboard.invalidate();
    this.timelineEdit.invalidate();
    const { api, state } = this.active(),
      generation = this.host.generation(),
      view = this.host.changeView();
    if (state.historical)
      throw new Error("Return to the current revision before editing.");
    const input = {
      project_id: state.head.project.id,
      expected_revision: state.head.revision,
      undo_transaction_id,
      idempotency_key: crypto.randomUUID(),
    };
    const plan = await api.sdk.call("opencut_edit_v4_redo_plan", input);
    if (
      generation === this.host.generation() &&
      view === this.host.viewGeneration() &&
      this.host.state()?.head.revision === state.head.revision
    )
      this.host.publish({
        ...this.host.state()!,
        historyPlan: { direction: "redo", input, plan },
      });
  }
  async olderHistory() {
    const { api, state } = this.active(),
      generation = this.host.generation();
    if (!state.history.next_cursor) return;
    const page = await api.sdk.call("opencut_project_history", {
      project_id: state.head.project.id,
      from_revision: state.history.next_cursor,
      limit: 50,
    });
    if (
      generation !== this.host.generation() ||
      this.host.state()?.history.next_cursor !== state.history.next_cursor
    )
      return;
    const current = this.host.state()!;
    this.host.publish({
      ...current,
      history: {
        ...current.history,
        next_cursor: page.next_cursor,
        revisions: [
          ...current.history.revisions,
          ...page.revisions.filter(
            (r) =>
              !current.history.revisions.some(
                (old) => old.revision === r.revision,
              ),
          ),
        ],
      },
    });
  }
  async applyHistory() {
    const { api, state } = this.active(),
      proposal = state.historyPlan;
    if (
      !proposal ||
      state.historical ||
      proposal.plan.conflicts.length ||
      proposal.input.expected_revision !== state.head.revision
    )
      throw new Error(
        "Choose a conflict-free history plan for the current revision.",
      );
    const release = this.acquireEdit(state.head.project.id),
      generation = this.host.generation();
    try {
      const wrapped = await this.recovery.execute(api, { sequence_id: state.sequenceId,
        intent: proposal.direction === "undo" ? { kind: "undo", request: proposal.input } : { kind: "redo", request: proposal.input } });
      if (wrapped.kind !== "undo" && wrapped.kind !== "redo") throw new Error("The service returned a different history receipt.");
      const result = wrapped.commit;
      if (generation === this.host.generation()) await this.refresh();
      return result;
    } finally {
      release();
    }
  }
  async restoreRevision(revision: string) {
    const { api, state } = this.active();
    if (!state.history.revisions.some(item => item.revision === revision)) throw new Error("Choose a saved project revision to restore.");
    const release = this.acquireEdit(state.head.project.id), generation = this.host.generation();
    try {
      const receipt = await this.recovery.execute(api, { sequence_id: null, intent: { kind: "restore", request: {
        project_id: state.head.project.id, expected_revision: state.head.revision, target_revision: revision, idempotency_key: crypto.randomUUID(), asset_versions: {} } } });
      if (receipt.kind !== "restore") throw new Error("The service returned a different restore receipt.");
      if (generation === this.host.generation()) { this.returnToHead(); await this.refresh(); }
      return receipt.transaction;
    } finally { release(); }
  }
}
