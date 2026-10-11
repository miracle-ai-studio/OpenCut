import { OpenCutApiError } from "../../lib/editor-sdk";
import { EditorApi } from "./api";
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
import { zero } from "../model/time";
import { restoredPlayhead } from "../model/view";
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
  historical: Snapshot | null;
  sequenceId: string | null;
  selection: Selection;
  playhead: Time;
  seekId: number;
  history: History | null;
  jobs: Job[];
  busy: boolean;
  message: string | null;
  error: string | null;
};
const initial = (): WorkspaceState => ({
  connection: "disconnected",
  api: null,
  projects: [],
  snapshot: null,
  historical: null,
  sequenceId: null,
  selection: null,
  playhead: zero,
  seekId: 0,
  history: null,
  jobs: [],
  busy: false,
  message: null,
  error: null,
});
export class WorkspaceController {
  private state = initial();
  private listeners = new Set<() => void>();
  private generation = 0;
  private viewGeneration = 0;
  private writes = new Set<string>();
  private copies = new Set<EditorApi>();
  private syncing = false;
  private disposed = false;
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
        playhead: zero,
        seekId: this.state.seekId + 1,
      };
    }
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
    const generation = ++this.generation;
    this.viewGeneration++;
    this.set({
      snapshot: null,
      historical: null,
      history: null,
      selection: null,
      jobs: [],
      busy: false,
      playhead: zero,
      error: null,
    });
    try {
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
  select(selection: Selection) {
    this.set({ selection });
  }
  seek(playhead: Time) {
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
      (p) => p.id === this.state.snapshot?.project.id,
    )
      ? ++this.generation
      : this.generation;
    this.copies.add(api);
    try {
      const receipt = await api.sdk.call("opencut_project_delete", {
        idempotency_key: idempotencyKey,
        projects: projects.map((p) => ({
          project_id: p.id,
          expected_revision: p.revision,
        })),
      });
      if (generation === this.generation) {
        if (projects.some((p) => p.id === this.state.snapshot?.project.id)) {
          this.viewGeneration++;
          this.set({
            snapshot: null,
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
      this.copies.delete(api);
    }
  }
  async mutate(
    label: string,
    operations: Operation[],
    visual: VisualOperation[] = [],
  ) {
    const { api, snapshot, historical } = this.state;
    if (!api || !snapshot) throw new Error("Open a project first.");
    if (historical)
      throw new Error("Return to the current revision before editing.");
    return this.write(async () =>
      api.apply(snapshot, label, operations, visual),
    );
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
  async write<T>(action: () => Promise<T>): Promise<T> {
    const { api, snapshot } = this.state;
    if (!api || !snapshot) throw new Error("Open a project first.");
    const key = `${api.sdk.baseUrl}/${snapshot.project.id}`;
    if (this.writes.has(key))
      throw new Error("Wait for the current edit to finish.");
    const generation = this.generation,
      viewGeneration = this.viewGeneration;
    this.writes.add(key);
    this.set({ busy: true, error: null });
    try {
      const result = await action();
      if (generation === this.generation) {
        const [head, history] = await Promise.all([
          api.inspect(projectRef(snapshot)),
          api.history(snapshot.project.id),
        ]);
        if (generation === this.generation)
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
          });
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
    });
  }
  async redo() {
    const { api, snapshot, historical } = this.state,
      target = this.navigation?.redo;
    if (!api || !snapshot || historical || !target || !this.canRedo) return;
    await this.write(() =>
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
