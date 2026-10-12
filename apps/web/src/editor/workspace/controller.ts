import { OpenCutApiError } from "../../lib/editor-sdk";
import { EditorApi } from "./api";
import { V4Session } from "./v4/session";
import { clearSourceArt } from "./v4/source-art/service";
import type { V4SessionState } from "./v4/types";
import { uploadMedia } from "./upload";
import { placeAssets } from "../model/edit-requests";
import { activeSequence, projectRef } from "../model/types";
import type {
  History,
  Job,
  Operation,
  ProjectRef,
  ProjectEdit,
  Selection,
  Snapshot,
  Time,
  VisualOperation,
} from "../model/types";
import { zero, compareTime, addTime } from "../model/time";
import { restoredPlayhead } from "../model/view";
import { HistoryViewStore, type HistoryView } from "../shared/view-preferences";
import {
  sameSelection,
  validSelections,
  type ElementSelection,
} from "../model/selection";
import {
  readTimelinePreferences,
  saveTimelinePreferences,
} from "../shared/view-preferences";

type Connection = "disconnected" | "connecting" | "connected";
export type WorkspaceState = {
  connection: Connection;
  api: EditorApi | null;
  projects: ProjectRef[];
  snapshot: Snapshot | null;
  v4: V4SessionState | null;
  historical: Snapshot | null;
  sequenceId: string | null;
  selection: Selection;
  selections: ElementSelection[];
  playhead: Time;
  seekId: number;
  history: History | null;
  jobs: Job[];
  busy: boolean;
  ripple: boolean;
  message: string | null;
  error: string | null;
};
const initial = (): WorkspaceState => ({
  connection: "disconnected",
  api: null,
  projects: [],
  snapshot: null,
  v4: null,
  historical: null,
  sequenceId: null,
  selection: null,
  selections: [],
  playhead: zero,
  seekId: 0,
  history: null,
  jobs: [],
  busy: false,
  ripple: false,
  message: null,
  error: null,
});
export class WorkspaceController {
  private state = initial();
  private listeners = new Set<() => void>();
  private generation = 0;
  private viewGeneration = 0;
  private interactionGeneration = 0;
  private historyViews = new HistoryViewStore();
  private writes = new Set<string>();
  private copies = new Set<EditorApi>();
  private imports = new Set<string>();
  private syncing = false;
  private disposed = false;
  readonly v4 = new V4Session({
    api: () => this.state.api,
    state: () => this.state.v4,
    generation: () => this.generation,
    viewGeneration: () => this.viewGeneration,
    changeView: () => ++this.viewGeneration,
    publish: (v4) => this.set({ v4 }),
    refreshTasks: () => this.sync(),
    acquire: (projectId) => {
      const api = this.state.api;
      if (!api) throw new Error("OpenCut is not ready.");
      const key = `${api.sdk.baseUrl}/${projectId}`;
      if (this.writes.has(key))
        throw new Error("Wait for the current edit to finish.");
      const generation = this.generation;
      this.writes.add(key);
      this.set({ busy: true, error: null });
      return () => {
        this.writes.delete(key);
        if (generation === this.generation) this.set({ busy: false });
      };
    },
  });
  constructor(
    private readonly createApi = (endpoint: string, token: string) =>
      new EditorApi(endpoint, token),
  ) {}
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private set(patch: Partial<WorkspaceState>) {
    if (this.disposed) return;
    // One controller owns one versioned project session; legacy consumers never
    // receive a projected V4 aggregate or independently writable mirror.
    if (patch.snapshot) patch.v4 = null;
    if (patch.v4) {
      patch.snapshot = null;
      patch.historical = null;
      patch.history = null;
      patch.selection = null;
      patch.selections = [];
    }
    if ("selection" in patch && !("selections" in patch))
      patch.selections = patch.selection ? [patch.selection] : [];
    const previousProject = this.state.snapshot?.project.id;
    const previousSequence = this.state.sequenceId;
    this.state = { ...this.state, ...patch };
    const view = this.state.historical ?? this.state.snapshot;
    if (
      view &&
      !view.project.sequences.some((s) => s.id === this.state.sequenceId)
    ) {
      this.state = {
        ...this.state,
        sequenceId: view.project.default_sequence_id,
        selection: null,
        selections: [],
        playhead: zero,
        seekId: this.state.seekId + 1,
      };
    }
    const selections = validSelections(
      view,
      this.state.sequenceId,
      this.state.selections,
    );
    if (
      view &&
      this.state.sequenceId &&
      (previousProject !== view.project.id ||
        previousSequence !== this.state.sequenceId)
    )
      this.state.ripple = readTimelinePreferences(
        view.project.id,
        this.state.sequenceId,
      ).ripple;
    this.state = {
      ...this.state,
      selections,
      selection: selections.at(-1) ?? null,
    };
    if (
      this.state.api &&
      this.state.snapshot &&
      !this.state.historical &&
      this.state.sequenceId &&
      (previousProject !== this.state.snapshot.project.id ||
        previousSequence !== this.state.sequenceId)
    )
      this.state.api.coverViews.save(
        this.state.snapshot.project.id,
        this.state.sequenceId,
      );
    this.listeners.forEach((listener) => listener());
  }
  notify(message: string) {
    this.set({ message, error: null });
  }
  report(error: unknown) {
    if (error instanceof DOMException && error.name === "AbortError") return;
    this.set({
      error: error instanceof Error ? error.message : String(error),
      message: null,
    });
  }
  clearMessage() {
    this.set({ error: null, message: null });
  }
  disconnect() {
    this.v4.clipboard.invalidate();
    this.v4.timelineEdit.invalidate();
    this.v4.audioPreview.invalidate();
    this.state.api?.covers.dispose();
    if (this.state.api) clearSourceArt(this.state.api);
    this.generation++;
    this.viewGeneration++;
    this.state = initial();
    this.listeners.forEach((listener) => listener());
  }
  dispose() {
    this.disconnect();
    this.disposed = true;
    this.listeners.clear();
  }
  async start() {
    try {
      const response = await fetch("/app/session", {
        method: "POST",
        headers: { "X-OpenCut-App": "web" },
        credentials: "same-origin",
      });
      if (!response.ok)
        throw new Error("OpenCut could not start. Please try again.");
      await this.connect(location.origin, "");
    } catch (error) {
      this.report(error);
    }
  }
  async connect(endpoint: string, token: string) {
    this.disconnect();
    const generation = this.generation;
    this.set({ connection: "connecting" });
    try {
      const api = this.createApi(endpoint, token);
      await api.sdk.hostCapabilities();
      await api.authenticateConnection?.();
      const projects = await api.projects();
      if (generation !== this.generation) return;
      this.set({ connection: "connected", api, projects });
    } catch (error) {
      if (generation === this.generation) {
        this.set({ connection: "disconnected" });
        this.report(error);
      }
    }
  }
  async refreshProjects() {
    const { api } = this.state;
    const generation = this.generation;
    if (!api) return;
    const projects = await api.projects();
    if (generation === this.generation) this.set({ projects });
  }
  async open(project: ProjectRef, sequenceId?: string | null) {
    const { api } = this.state;
    if (!api) return;
    clearSourceArt(api);
    this.v4.clipboard.invalidate();
    this.v4.timelineEdit.invalidate();
    const generation = ++this.generation;
    this.viewGeneration++;
    this.set({
      snapshot: null,
      v4: null,
      historical: null,
      history: null,
      selection: null,
      jobs: [],
      busy: false,
      playhead: zero,
      error: null,
    });
    try {
      if (project.format === "v4") {
        await this.v4.open(project, sequenceId);
        return;
      }
      const [snapshot, history] = await Promise.all([
        api.inspect(project),
        api.history(project.id),
      ]);
      if (generation !== this.generation) return;
      const scene = snapshot.project.sequences.some((s) => s.id === sequenceId)
        ? sequenceId!
        : snapshot.project.default_sequence_id;
      this.set({
        snapshot,
        history,
        sequenceId: scene,
        playhead: restoredPlayhead(
          snapshot,
          scene,
          readTimelinePreferences(snapshot.project.id, scene).playhead,
        ),
        seekId: this.state.seekId + 1,
      });
      await this.sync();
    } catch (error) {
      if (generation === this.generation) this.report(error);
    }
  }
  async create(name: string, width: number, height: number, fps: number) {
    const { api } = this.state;
    const generation = this.generation;
    if (!api) return;
    const snapshot = await api.create(name, width, height, fps);
    if (generation !== this.generation) return;
    await this.refreshProjects();
    if (generation === this.generation) await this.open(projectRef(snapshot));
  }
  async createV4(name: string, width: number, height: number, fps: number) {
    const { api } = this.state,
      generation = this.generation;
    if (!api) throw new Error("OpenCut is not ready.");
    const snapshot = await api.v4.create(name, width, height, fps);
    if (generation !== this.generation) return;
    await this.refreshProjects();
    if (generation === this.generation)
      await this.open({
        id: snapshot.project.id,
        name: snapshot.project.name,
        revision: snapshot.revision,
        format: "v4",
      });
  }
  select(selection: Selection, mode: "replace" | "toggle" = "replace") {
    this.interactionGeneration++;
    if (!selection || mode === "replace") this.set({ selection });
    else {
      const previous = this.state.selections.filter(
        (item) => item.kind === selection.kind,
      );
      this.selectMany(
        previous.some((item) => sameSelection(item, selection))
          ? previous.filter((item) => !sameSelection(item, selection))
          : [...previous, selection],
      );
    }
  }
  selectMany(selections: ElementSelection[]) {
    this.interactionGeneration++;
    this.set({ selections, selection: selections.at(-1) ?? null });
  }
  seek(playhead: Time) {
    this.interactionGeneration++;
    this.set({ playhead, seekId: this.state.seekId + 1 });
    this.savePlayhead(playhead);
  }
  present(playhead: Time) {
    this.set({ playhead });
    this.savePlayhead(playhead);
  }
  private savePlayhead(playhead: Time) {
    const { snapshot, historical, sequenceId } = this.state;
    if (snapshot && sequenceId && !historical)
      saveTimelinePreferences(snapshot.project.id, sequenceId, { playhead });
  }
  chooseSequence(sequenceId: string) {
    this.viewGeneration++;
    const view = this.state.historical ?? this.state.snapshot;
    const playhead = view
      ? restoredPlayhead(
          view,
          sequenceId,
          readTimelinePreferences(view.project.id, sequenceId).playhead,
        )
      : zero;
    this.set({
      sequenceId,
      selection: null,
      playhead,
      seekId: this.state.seekId + 1,
    });
  }
  returnToHead() {
    this.viewGeneration++;
    this.set({ historical: null, selection: null });
  }
  async inspectRevision(revision: string) {
    const { api, snapshot } = this.state;
    if (!api || !snapshot) return;
    const generation = this.generation,
      view = ++this.viewGeneration;
    const historical = await api.inspect(projectRef(snapshot), revision);
    if (generation === this.generation && view === this.viewGeneration)
      this.set({ historical, selection: null });
  }
  async olderHistory() {
    const { api, snapshot, history } = this.state;
    if (!api || !snapshot || !history?.next_cursor) return;
    const generation = this.generation,
      page = await api.history(snapshot.project.id, history.next_cursor);
    if (
      generation !== this.generation ||
      this.state.snapshot?.revision !== snapshot.revision ||
      this.state.history?.next_cursor !== history.next_cursor
    )
      return;
    const known = new Set(history.revisions.map((item) => item.revision));
    this.set({
      history: {
        ...history,
        revisions: [
          ...history.revisions,
          ...page.revisions.filter((item) => !known.has(item.revision)),
        ],
        next_cursor: page.next_cursor,
      },
    });
  }
  async sync() {
    if (this.state.v4) {
      const { api, v4 } = this.state;
      if (!api || this.syncing || this.copies.size) return;
      const generation = this.generation,
        projectId = v4.head.project.id;
      this.syncing = true;
      try {
        // Task observations continue during an edit; head refresh still respects
        // the shared write fence. Only WorkspaceState owns task rows.
        const [head, jobs] = await Promise.allSettled([
          this.writes.size ? Promise.resolve() : this.v4.refresh(),
          api.tasks(projectId),
        ]);
        if (
          generation !== this.generation ||
          api !== this.state.api ||
          this.state.v4?.head.project.id !== projectId
        )
          return;
        if (jobs.status === "fulfilled") this.set({ jobs: jobs.value.jobs });
        if (head.status === "rejected") throw head.reason;
        if (jobs.status === "rejected") throw jobs.reason;
      } catch (error) {
        if (generation === this.generation) this.report(error);
      } finally {
        this.syncing = false;
      }
      return;
    }
    const { api, snapshot } = this.state;
    if (!api || !snapshot || this.syncing || this.copies.has(api)) return;
    const generation = this.generation;
    this.syncing = true;
    try {
      const [head, jobs] = await Promise.all([
        api.inspect(projectRef(snapshot)),
        api.tasks(snapshot.project.id),
      ]);
      const history =
        head.revision === snapshot.revision &&
        this.state.history?.navigation?.revision === head.revision
          ? this.state.history
          : await api.history(snapshot.project.id);
      if (generation !== this.generation) return;
      if (
        this.writes.size ||
        this.state.snapshot?.revision !== snapshot.revision
      )
        return;
      this.set({ snapshot: head, history, jobs: jobs.jobs });
    } catch (error) {
      if (generation === this.generation) this.report(error);
    } finally {
      this.syncing = false;
    }
  }
  async editProject(label: string, operations: ProjectEdit[]) {
    const { api, snapshot, historical } = this.state;
    if (!api || !snapshot) throw new Error("Open a project first.");
    if (historical)
      throw new Error("Return to the current revision before editing.");
    if (snapshot.format !== "visual")
      throw new Error(
        "Scene management requires a project created with this editor.",
      );
    const result = await this.write(() =>
      api.projectEdit(snapshot, label, operations),
    );
    await this.refreshProjects();
    return result;
  }
  async duplicateProjects(projects: ProjectRef[], names: string[]) {
    if (projects.some((project) => project.format === "v4"))
      throw new Error("V4 copies must use the versioned copy operation.");
    const { api } = this.state;
    if (!api || this.copies.has(api))
      throw new Error("Wait for the current edit to finish.");
    if (
      !projects.length ||
      projects.length > 64 ||
      projects.length !== names.length
    )
      throw new Error("Select between 1 and 64 projects.");
    const generation = this.generation;
    this.copies.add(api);
    const coverScenes = new Map(
      projects.map((project) => [project.id, api.coverViews.read(project.id)]),
    );
    try {
      const receipt = await api.sdk.call("opencut_project_duplicate", {
        idempotency_key: crypto.randomUUID(),
        projects: projects.map((project, index) => ({
          source_project_id: project.id,
          expected_revision: project.revision,
          target_project_id: crypto.randomUUID(),
          name: names[index],
        })),
      });
      if (generation === this.generation) {
        for (const copy of receipt.projects) {
          const scene = coverScenes.get(copy.source_project_id);
          if (scene) api.coverViews.save(copy.project_id, scene);
        }
        await this.refreshProjects();
        this.notify("Projects duplicated");
      }
      return receipt;
    } finally {
      this.copies.delete(api);
    }
  }
  async renameProject(project: ProjectRef, name: string) {
    const { api, snapshot } = this.state;
    if (!api) throw new Error("OpenCut is not ready.");
    if (snapshot?.project.id === project.id && !this.state.historical)
      return this.editProject("Rename project", [
        { type: "rename_project", name },
      ]);
    const key = `${api.sdk.baseUrl}/${project.id}`;
    if (this.writes.has(key))
      throw new Error("Wait for the current edit to finish.");
    this.writes.add(key);
    try {
      const current = await api.inspect(project);
      await api.projectEdit(current, "Rename project", [
        { type: "rename_project", name },
      ]);
      await this.refreshProjects();
    } finally {
      this.writes.delete(key);
    }
  }
  async deleteProjects(
    projects: ProjectRef[],
    idempotencyKey: string = crypto.randomUUID(),
  ) {
    const { api } = this.state;
    if (!api || this.copies.has(api))
      throw new Error("Wait for the current edit to finish.");
    const generation = projects.some(
      (p) =>
        p.id ===
        (this.state.snapshot?.project.id ?? this.state.v4?.head.project.id),
    )
      ? ++this.generation
      : this.generation;
    this.copies.add(api);
    const projectIds = projects.map((project) => project.id);
    const resumeCovers = api.covers.hold(projectIds);
    try {
      await api.covers.quiesce(projectIds);
      const input = {
        idempotency_key: idempotencyKey,
        projects: projects.map((p) => ({
          project_id: p.id,
          expected_revision: p.revision,
        })),
      };
      const deadline = Date.now() + 10000;
      const remove = async (): ReturnType<
        typeof api.sdk.call<"opencut_project_delete">
      > => {
        for (;;) {
          try {
            return await api.sdk.call("opencut_project_delete", input);
          } catch (error) {
            // PROJECT_BUSY is an explicit no-commit rejection. A prior frame may
            // still be ending after its UI has left; never cancel arbitrary jobs.
            if (
              !(error instanceof OpenCutApiError) ||
              error.code !== "PROJECT_BUSY" ||
              Date.now() >= deadline
            )
              throw error;
            await new Promise((resolve) => setTimeout(resolve, 200));
          }
        }
      };
      const receipt = await remove();
      if (generation === this.generation) {
        if (
          projects.some(
            (p) =>
              p.id ===
              (this.state.snapshot?.project.id ??
                this.state.v4?.head.project.id),
          )
        ) {
          this.v4.clipboard.invalidate();
          this.v4.timelineEdit.invalidate();
          this.viewGeneration++;
          this.set({
            snapshot: null,
            v4: null,
            historical: null,
            history: null,
            sequenceId: null,
            selection: null,
            playhead: zero,
          });
        }
        await this.refreshProjects();
        this.notify("Projects deleted");
      }
      return receipt;
    } finally {
      resumeCovers();
      this.copies.delete(api);
    }
  }
  async mutate(
    label: string,
    operations: Operation[],
    visual: VisualOperation[] = [],
    expectedRevision?: string,
    mediaImport?: Parameters<EditorApi["apply"]>[6],
  ) {
    const { api, snapshot, historical } = this.state;
    if (!api || !snapshot) throw new Error("Open a project first.");
    if (historical)
      throw new Error("Return to the current revision before editing.");
    if (expectedRevision && expectedRevision !== snapshot.revision)
      throw new Error(
        "Project has changed; refresh the snapshot and try again.",
      );
    return this.write(async () =>
      api.apply(
        snapshot,
        label,
        operations,
        visual,
        undefined,
        this.state.ripple && this.state.sequenceId
          ? this.state.sequenceId
          : undefined,
        mediaImport,
      ),
    );
  }
  toggleRipple() {
    const { snapshot, sequenceId, ripple } = this.state;
    if (!snapshot || !sequenceId) return;
    saveTimelinePreferences(snapshot.project.id, sequenceId, {
      ripple: !ripple,
    });
    this.set({ ripple: !ripple });
  }
  async split(retain: "both" | "left" | "right") {
    const {
      api,
      snapshot,
      historical,
      sequenceId,
      selections,
      playhead,
      ripple,
    } = this.state;
    const sequence = activeSequence(snapshot, sequenceId);
    if (!api || !snapshot || !sequence || historical) return;
    const chosen = selections.length
      ? selections
      : [
          ...sequence.tracks.flatMap((track) =>
            track.clips
              .filter(
                (clip) =>
                  compareTime(clip.timeline_range.start, playhead) < 0 &&
                  compareTime(
                    addTime(
                      clip.timeline_range.start,
                      clip.timeline_range.duration,
                    ),
                    playhead,
                  ) > 0,
              )
              .map((clip) => ({
                kind: "clip" as const,
                id: clip.id,
                trackId: track.id,
              })),
          ),
          ...(sequence.caption_cues ?? [])
            .filter(
              (cue) =>
                compareTime(cue.timeline_range.start, playhead) < 0 &&
                compareTime(
                  addTime(
                    cue.timeline_range.start,
                    cue.timeline_range.duration,
                  ),
                  playhead,
                ) > 0,
            )
            .map((cue) => ({ kind: "caption" as const, id: cue.id })),
          ...(
            snapshot.visual.find((graph) => graph.sequence_id === sequence.id)
              ?.shapes ?? []
          )
            .filter(
              (shape) =>
                compareTime(shape.timeline_range.start, playhead) < 0 &&
                compareTime(
                  addTime(
                    shape.timeline_range.start,
                    shape.timeline_range.duration,
                  ),
                  playhead,
                ) > 0,
            )
            .map((shape) => ({ kind: "shape" as const, id: shape.id })),
        ];
    if (!chosen.length) return;
    const generation = this.generation,
      viewGeneration = this.viewGeneration;
    let selection: import("../../lib/editor-sdk").ToolOutput<"opencut_edit_classic_plan_v1">["selection"];
    try {
      await this.write(
        async () => {
          const result = await api.sdk.call("opencut_edit_classic_plan_v1", {
            project_id: snapshot.project.id,
            expected_revision: snapshot.revision,
            sequence_id: sequence.id,
            idempotency_key: crypto.randomUUID(),
            label:
              retain === "both"
                ? "Split elements"
                : retain === "right"
                  ? "Split and remove left"
                  : "Split and remove right",
            ripple,
            command: {
              kind: "split",
              at: playhead,
              retain,
              clip_ids: chosen
                .filter((s) => s.kind === "clip")
                .map((s) => s.id),
              caption_ids: chosen
                .filter((s) => s.kind === "caption")
                .map((s) => s.id),
              shape_ids: chosen
                .filter((s) => s.kind === "shape")
                .map((s) => s.id),
            },
          });
          selection = result.selection;
          if (
            this.generation !== generation ||
            this.viewGeneration !== viewGeneration ||
            this.state.historical
          )
            throw new DOMException("The edit view changed", "AbortError");
          return api.sdk.call("opencut_edit_v3_apply", result.plan.transaction);
        },
        undefined,
        (head) =>
          selection
            ? {
                sequenceId: sequence.id,
                selections: [
                  ...head.project.sequences
                    .find((s) => s.id === sequence.id)!
                    .tracks.flatMap((track) =>
                      track.clips
                        .filter((clip) => selection!.clip_ids.includes(clip.id))
                        .map((clip) => ({
                          kind: "clip" as const,
                          id: clip.id,
                          trackId: track.id,
                        })),
                    ),
                  ...selection.caption_ids.map((id) => ({
                    kind: "caption" as const,
                    id,
                  })),
                  ...selection.shape_ids.map((id) => ({
                    kind: "shape" as const,
                    id,
                  })),
                ],
                playhead: selection.playhead ?? playhead,
              }
            : null,
      );
    } catch (error) {
      if (error instanceof OpenCutApiError && error.code === "NO_EFFECT") {
        this.clearMessage();
        return;
      }
      throw error;
    }
  }
  async removeMedia(assetIds: string[]) {
    const { api, snapshot, historical } = this.state;
    if (!api || !snapshot || historical)
      throw new Error("Return to the current revision before editing.");
    return this.write(() =>
      api.sdk.call("opencut_project_media_remove", {
        project_id: snapshot.project.id,
        expected_revision: snapshot.revision,
        asset_ids: assetIds,
        idempotency_key: crypto.randomUUID(),
      }),
    );
  }
  async importMedia(
    files: File[],
    onProgress: (value: string) => void,
    signal: AbortSignal,
    target?: { sequenceId: string; trackId?: string; at?: Time },
    replacing?: { id: string; kind: string },
  ) {
    const { api, snapshot, historical } = this.state;
    if (!api || !snapshot || historical)
      throw new Error("Return to the current revision before importing.");
    if (!files.length || files.length > 60 || (replacing && files.length !== 1))
      throw new Error(
        "Choose between 1 and 60 files; replacements require one file.",
      );
    const key = `${api.sdk.baseUrl}/${snapshot.project.id}`;
    if (this.imports.has(key))
      throw new Error("Wait for the current import to finish.");
    const generation = this.generation,
      viewGeneration = this.viewGeneration,
      ripple = this.state.ripple;
    const sequence = activeSequence(
      snapshot,
      target?.sequenceId ?? this.state.sequenceId,
    );
    if (!sequence) throw new Error("The import scene no longer exists.");
    if (target?.trackId) {
      const track = sequence?.tracks.find(
        (track) => track.id === target.trackId,
      );
      if (!track || track.locked)
        throw new Error("Choose an existing unlocked target track.");
    }
    this.imports.add(key);
    try {
      const media: import("../../lib/editor-sdk").ToolInput<"opencut_media_import_plan_v1">["media"] =
        [];
      const assets = [],
        warnings: string[] = [];
      for (const file of files) {
        signal.throwIfAborted();
        const imported = await uploadMedia(
          api.sdk,
          snapshot.project.id,
          file,
          onProgress,
          signal,
        );
        signal.throwIfAborted();
        if (
          generation !== this.generation ||
          viewGeneration !== this.viewGeneration
        )
          throw new DOMException("Import target changed", "AbortError");
        if (replacing && imported.asset.kind !== replacing.kind)
          throw new Error(
            "Replacement media must have the same type as the original.",
          );
        media.push({
          media_id: imported.asset.id,
          replace_asset_id: replacing?.id ?? null,
        });
        assets.push(
          replacing ? { ...imported.asset, id: replacing.id } : imported.asset,
        );
        if (imported.warning) warnings.push(imported.warning);
      }
      const operations: Operation[] = assets.map((asset) => ({
        type: replacing ? "asset.replace" : "asset.bind",
        asset,
      }));
      if (target && sequence)
        operations.push(
          ...placeAssets(assets, sequence, target.at, target.trackId),
        );
      signal.throwIfAborted();
      await this.mutate(
        replacing
          ? "Replace media"
          : target
            ? "Import and insert media"
            : "Import media",
        operations,
        [],
        snapshot.revision,
        {
          sequenceId: sequence.id,
          media,
          ripple,
          signal,
          beforeApply: () => {
            signal.throwIfAborted();
            if (
              generation !== this.generation ||
              viewGeneration !== this.viewGeneration
            )
              throw new DOMException("Import target changed", "AbortError");
            if (this.state.snapshot?.revision !== snapshot.revision)
              throw new Error(
                "Project has changed; refresh the snapshot and try again.",
              );
          },
        },
      );
      if (warnings.length) this.notify(warnings.join("\n"));
      return assets;
    } catch (error) {
      if (generation !== this.generation)
        throw new DOMException("Import project changed", "AbortError");
      throw error;
    } finally {
      this.imports.delete(key);
    }
  }
  private captureView(): HistoryView {
    return structuredClone({
      sequenceId: this.state.sequenceId,
      selections: this.state.selections,
      playhead: this.state.playhead,
    });
  }
  private historyHost(api: EditorApi) {
    const url = new URL(api.sdk.baseUrl);
    return url.origin + url.pathname;
  }
  async write<T>(
    action: () => Promise<T>,
    restore?: HistoryView | null,
    viewAfter?: (head: Snapshot) => HistoryView | null,
  ): Promise<T> {
    const { api, snapshot } = this.state;
    if (!api || !snapshot) throw new Error("Open a project first.");
    const key = `${api.sdk.baseUrl}/${snapshot.project.id}`;
    if (this.writes.has(key))
      throw new Error("Wait for the current edit to finish.");
    const generation = this.generation,
      viewGeneration = this.viewGeneration,
      interactionGeneration = this.interactionGeneration,
      beforeView = this.captureView();
    this.writes.add(key);
    this.set({ busy: true, error: null });
    try {
      const result = await action();
      if (generation === this.generation) {
        const [head, history] = await Promise.all([
          api.inspect(projectRef(snapshot)),
          api.history(snapshot.project.id),
        ]);
        if (generation === this.generation) {
          // Bind hints to the acknowledged transaction's exact head. A remote
          // edit racing this refresh must never receive another edit's hints.
          const transaction =
            result && typeof result === "object" && "transaction" in result
              ? (result.transaction as {
                  transaction_id?: unknown;
                  revision?: unknown;
                })
              : result &&
                  typeof result === "object" &&
                  "transaction_id" in result
                ? (result as { transaction_id?: unknown; revision?: unknown })
                : null;
          const exact = transaction && transaction.revision === head.revision;
          const sameView =
            viewGeneration === this.viewGeneration &&
            interactionGeneration === this.interactionGeneration &&
            !this.state.historical;
          const intendedView =
            restore ?? (exact && sameView ? viewAfter?.(head) : null);
          const restored =
            intendedView && exact && sameView
              ? {
                  sequenceId: head.project.sequences.some(
                    (s) => s.id === intendedView.sequenceId,
                  )
                    ? intendedView.sequenceId
                    : head.project.default_sequence_id,
                  selections: intendedView.selections,
                  playhead: restoredPlayhead(
                    head,
                    intendedView.sequenceId ?? head.project.default_sequence_id,
                    intendedView.playhead,
                  ),
                  seekId: this.state.seekId + 1,
                }
              : {};
          this.set({
            snapshot: head,
            history,
            projects: this.state.projects.map((project) =>
              project.id === head.project.id
                ? { ...project, ...projectRef(head) }
                : project,
            ),
            historical:
              viewGeneration === this.viewGeneration
                ? null
                : this.state.historical,
            message: "Saved",
            ...restored,
          });
          if (
            restore === undefined &&
            exact &&
            sameView &&
            typeof transaction.transaction_id === "string"
          )
            this.historyViews.save({
              host: this.historyHost(api),
              projectId: snapshot.project.id,
              transactionId: transaction.transaction_id,
              before: beforeView,
              after: this.captureView(),
            });
          if (intendedView && exact && sameView)
            this.savePlayhead(this.state.playhead);
        }
      }
      return result;
    } catch (error) {
      if (generation === this.generation) {
        this.report(error);
        if (
          error instanceof OpenCutApiError &&
          error.code === "REVISION_CONFLICT"
        ) {
          const head = await api.inspect(projectRef(snapshot));
          if (generation === this.generation) this.set({ snapshot: head });
        }
      }
      throw error;
    } finally {
      this.writes.delete(key);
      if (generation === this.generation) this.set({ busy: false });
    }
  }
  private get navigation() {
    const { snapshot, history, historical } = this.state;
    const navigation = history?.navigation;
    return !historical &&
      navigation?.revision === snapshot?.revision &&
      navigation?.project_id === snapshot?.project.id
      ? navigation
      : null;
  }
  get canUndo() {
    return !!this.navigation?.undo;
  }
  get canRedo() {
    return !!this.navigation?.redo;
  }
  async undo() {
    const { api, snapshot, historical } = this.state;
    const target = this.navigation?.undo?.transaction_id;
    if (!api || !snapshot || !target || historical) return;
    const request = {
      project_id: snapshot.project.id,
      expected_revision: snapshot.revision,
      target_transaction_id: target,
      idempotency_key: crypto.randomUUID(),
    };
    const saved = this.historyViews.read(
      this.historyHost(api),
      snapshot.project.id,
      target,
    );
    await this.write(async () => {
      const plan = await api.sdk.call(
        snapshot.format === "visual"
          ? "opencut_edit_v3_undo_plan"
          : "opencut_edit_undo_plan",
        request,
      );
      if (plan.conflicts.length)
        throw new Error(
          "This change conflicts with later edits and cannot be undone automatically.",
        );
      const result = await api.sdk.call(
        snapshot.format === "visual"
          ? "opencut_edit_v3_undo_apply"
          : "opencut_edit_undo_apply",
        request,
      );
      return result;
    }, saved?.before ?? null);
  }
  async redo() {
    const { api, snapshot, historical } = this.state,
      target = this.navigation?.redo;
    if (!api || !snapshot || historical || !target || !this.canRedo) return;
    const saved = this.historyViews.read(
      this.historyHost(api),
      snapshot.project.id,
      target.transaction_id,
    );
    await this.write(
      () =>
        api.sdk.call(
          snapshot.format === "visual"
            ? "opencut_edit_v3_redo_apply"
            : "opencut_edit_redo_apply",
          {
            project_id: snapshot.project.id,
            expected_revision: snapshot.revision,
            undo_transaction_id: target.undo_transaction_id,
            idempotency_key: crypto.randomUUID(),
          },
        ),
      saved?.after ?? null,
    );
  }
  async track(kind: "video" | "audio" | "graphic") {
    const sequence = activeSequence(this.state.snapshot, this.state.sequenceId);
    if (!sequence) return;
    await this.mutate("Add track", [
      {
        type: "track.create",
        sequence_id: sequence.id,
        track: {
          id: crypto.randomUUID(),
          name: `${kind === "audio" ? "Audio" : kind === "graphic" ? "Graphics" : "Video"} ${sequence.tracks.length + 1}`,
          kind,
          enabled: true,
          muted: false,
          locked: false,
          clips: [],
          params: {},
        },
      },
    ]);
  }
}
