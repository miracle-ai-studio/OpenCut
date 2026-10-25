import { type ToolOutput, OpenCutApiError } from "../../lib/editor-sdk";
import { EditorApi } from "./api";
import { Session } from "./scene/session";
import { clearSourceArt } from "./scene/source-art/service";
import type { SessionState } from "./scene/types";
import { normalizeHistorical, projectRef } from "../model/types";
import type { History, Job, ProjectRef, Selection, Snapshot, Time } from "../model/types";
import { zero } from "../model/time";
import { restoredPlayhead } from "../model/view";
import { sameSelection, validSelections, type ElementSelection } from "../model/selection";
import { readTimelinePreferences, saveTimelinePreferences } from "../shared/view-preferences";

type Connection = "disconnected" | "connecting" | "connected";
export type WorkspaceState = {
  connection: Connection;
  api: EditorApi | null;
  projects: ProjectRef[];
  snapshot: Snapshot | null;
  scene: SessionState | null;
  preserved?: Extract<ToolOutput<"opencut_project_inspect">,{kind:"preserved"}>["view"] | null;
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
  scene: null,
  preserved: null,
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
  private localConnection = true;
  /** Retain the attempted connection mode even when authentication failed. */
  get canStartLocalSession() { return this.localConnection; }
  private listeners = new Set<() => void>();
  private generation = 0;
  private viewGeneration = 0;
  private interactionGeneration = 0;
  private writes = new Set<string>();
  private copies = new Set<EditorApi>();
  private syncing = false;
  private disposed = false;
  readonly scene = new Session({
    api: () => this.state.api,
    state: () => this.state.scene,
    generation: () => this.generation,
    viewGeneration: () => this.viewGeneration,
    changeView: () => ++this.viewGeneration,
    publish: (scene) => this.set({ scene }),
    refreshTasks: () => this.sync(),
    acquire: (projectId, draftOwner) => this.acquire(projectId, draftOwner),
    report: error => this.report(error),
  });
  private acquire(projectId: string, draftOwner?: string) {
      if (this.state.scene?.head.project.id === projectId &&
          this.state.scene.propertyDrafts?.some(owner => owner !== draftOwner))
        throw new Error("Confirm or cancel the property draft before starting another edit.");
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
  }
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
    if (patch.snapshot || patch.scene) patch.preserved = null;
    if (patch.preserved) {
      patch.snapshot = null; patch.scene = null; patch.historical = null; patch.history = null;
      patch.selection = null; patch.selections = [];
    }
    if (patch.snapshot) patch.scene = null;
    if (patch.scene) {
      patch.snapshot = null;
      patch.historical = null;
      patch.history = null;
      patch.selection = null;
      patch.selections = [];
    }
    if ("selection" in patch && !("selections" in patch))
      patch.selections = patch.selection ? [patch.selection] : [];
    const previousSelections = this.state.selections;
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
    const validatedSelections = validSelections(
      view,
      this.state.sequenceId,
      this.state.selections,
    );
    // Scene playback does not change the legacy selection. Retain equal
    // references so editing subscribers can ignore device-counter updates.
    const selections = validatedSelections.length === previousSelections.length &&
      validatedSelections.every((item, index) => sameSelection(item, previousSelections[index]))
      ? previousSelections : validatedSelections;
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
    if (this.state.api && this.canStartLocalSession &&
        /valid bearer token is required|local browser session expired or ended/i.test(error instanceof Error ? error.message : String(error))) {
      this.disconnect();
    }
    this.set({
      error: error instanceof Error ? error.message : String(error),
      message: null,
    });
  }
  clearMessage() {
    this.set({ error: null, message: null });
  }
  disconnect() {
    this.scene.frame.invalidate();
    this.scene.clipboard.invalidate();
    this.scene.timelineEdit.invalidate();
    this.scene.audioPreview.invalidate();
    this.scene.visualPreview.dispose();
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
  private localSessionEpoch = 0;
  private localSessionLogout: Promise<void> | null = null;
  private localSessionRequests: Promise<void> = Promise.resolve();
  private requestLocalSession(method: "POST" | "DELETE") {
    // A late bootstrap response can still set a cookie after its UI is stale.
    // Queue cookie mutations so logout always revokes that last issued grant.
    const operation = this.localSessionRequests.then(() =>
      fetch("/app/session", {
        method,
        headers: { "X-OpenCut-App": "web" },
        credentials: "same-origin",
      }),
    );
    this.localSessionRequests = operation.then(
      () => {},
      () => {},
    );
    return operation;
  }
  /** Explicit logout is separate from disconnect: switching APIs or disposing a
   * view must never revoke another local browser session as a side effect. */
  async logoutLocalSession() {
    if (this.localSessionLogout) return this.localSessionLogout;
    const api = this.state.api;
    if (
      api &&
      (api.audioPreviewConnection.token ||
        new URL(api.sdk.baseUrl).origin !== location.origin)
    )
      throw new Error("Only the local browser session can be ended here.");
    const attempt = ++this.localSessionEpoch;
    this.disconnect();
    const operation = (async () => {
      const response = await this.requestLocalSession("DELETE");
      if (!response.ok)
        throw new Error("The local session could not be ended. Retry ending it.");
      if (attempt === this.localSessionEpoch && !this.state.api)
        this.notify("Local session ended.");
    })();
    this.localSessionLogout = operation;
    try {
      await operation;
    } finally {
      if (this.localSessionLogout === operation) this.localSessionLogout = null;
    }
  }
  async start() {
    if (!this.canStartLocalSession) {
      this.report(new Error("Reconnect to the original service with valid credentials."));
      return false;
    }
    const generation = this.generation;
    try {
      if (this.localSessionLogout) await this.localSessionLogout;
      if (generation !== this.generation || !this.canStartLocalSession) return false;
      const attempt = ++this.localSessionEpoch;
      const response = await this.requestLocalSession("POST");
      if (!response.ok)
        throw new Error("OpenCut could not start. Please try again.");
      if (attempt !== this.localSessionEpoch || generation !== this.generation || !this.canStartLocalSession) return false;
      return await this.connect(location.origin, "");
    } catch (error) {
      if (generation === this.generation) this.report(error);
      return false;
    }
  }
  async connect(endpoint: string, token: string) {
    this.localConnection = false;
    this.disconnect();
    const generation = this.generation;
    this.set({ connection: "connecting" });
    try {
      this.localConnection = !token && typeof location !== "undefined" &&
        new URL(endpoint).origin === location.origin;
      const api = this.createApi(endpoint, token);
      await api.sdk.hostCapabilities();
      await api.authenticateConnection?.();
      const projects = await api.projects();
      if (generation !== this.generation) return false;
      this.set({ connection: "connected", api, projects });
      return true;
    } catch (error) {
      if (generation === this.generation) {
        this.set({ connection: "disconnected" });
        this.report(error);
      }
      return false;
    }
  }
  async refreshProjects() {
    const { api } = this.state;
    const generation = this.generation;
    if (!api) return;
    const projects = await api.projects();
    if (generation === this.generation) this.set({ projects });
  }
  async open(project: Omit<ProjectRef, "format"> & Partial<Pick<ProjectRef, "format">>, sequenceId?: string | null) {
    const { api } = this.state;
    if (!api) return;
    this.scene.frame.invalidate();
    clearSourceArt(api);
    this.scene.clipboard.invalidate();
    this.scene.timelineEdit.invalidate();
    const generation = ++this.generation;
    this.viewGeneration++;
    this.set({
      snapshot: null,
      scene: null,
      preserved: null,
      historical: null,
      history: null,
      selection: null,
      jobs: [],
      busy: false,
      playhead: zero,
      error: null,
    });
    try {
      const inspected=await api.sdk.call("opencut_project_inspect",{project_id:project.id,revision:null});
      if(generation!==this.generation)return;
      if(inspected.kind==="preserved"){
        const preserved=inspected.view;
        if(!preserved.read_only || preserved.project_id!==project.id)throw new Error("The service returned invalid preservation data.");
        this.set({preserved,sequenceId:preserved.known_content.default_sequence_id});return;
      }
      if(inspected.kind==="editor"){
        await this.scene.open({...project,format:"editor"},sequenceId,inspected.snapshot);return;
      }
      const snapshot=normalizeHistorical(inspected.snapshot);
      const history=await api.history(project.id,snapshot.revision);
      if (generation !== this.generation) return;
      const scene = snapshot.project.sequences.some((s) => s.id === sequenceId)
        ? sequenceId!
        : snapshot.project.default_sequence_id;
      this.set({
        snapshot,
        historical:null,
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
    const {api}=this.state,generation=this.generation;
    if (!api) throw new Error("OpenCut is not ready.");
    const receipt=await api.create(name,width,height,fps);
    if(generation!==this.generation)return;
    await this.refreshProjects();
    if(generation!==this.generation)return;
    const project=this.state.projects.find((p)=>p.id===receipt.project_id);
    if(!project)throw new Error("Created project is unavailable in this workspace.");
    await this.open(project);
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
    if (this.state.scene) {
      const { api, scene } = this.state;
      if (!api || this.syncing || this.copies.size) return;
      const generation = this.generation,
        projectId = scene.head.project.id;
      this.syncing = true;
      try {
        // Task observations continue during an edit; head refresh still respects
        // the shared write fence. Only WorkspaceState owns task rows.
        const [head, jobs] = await Promise.allSettled([
          this.writes.size ? Promise.resolve() : this.scene.refresh(true),
          api.tasks(projectId),
        ]);
        if (
          generation !== this.generation ||
          api !== this.state.api ||
          this.state.scene?.head.project.id !== projectId
        )
          return;
        if (jobs.status === "fulfilled") {
          const next=jobs.value.jobs,previous=this.state.jobs;
          // TaskFeed retains row objects until a native change arrives. An
          // unchanged authorized poll must not rebuild the whole workbench.
          if(next.length!==previous.length||next.some((job,index)=>job!==previous[index]))
            this.set({jobs:next});
        }
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
  async duplicateProjects(projects: ProjectRef[], names: string[]) {
    if (projects.some((project) => project.format === "editor" || project.format === "readonly"))
      throw new Error("Copy this project from the editor.");
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
    const { api, scene } = this.state;
    if (!api) throw new Error("OpenCut is not ready.");
    if (project.format !== "editor") throw new Error("Convert this historical project before editing.");
    if (scene?.head.project.id === project.id && scene.historical)
      throw new Error("Return to the current revision before editing.");
    if (this.scene.recovery.blocked()) throw new Error("Resolve the original edit before renaming another project.");
    const generation = this.generation, view = this.viewGeneration, release = this.acquire(project.id);
    try {
      const snapshot = await api.sdk.inspectEditorProject({ project_id: project.id, revision: project.revision });
      const key = await this.metadataKey("rename", [project.id, project.revision, name]);
      const request = api.scene.request(snapshot, "Rename project", [{ type: "rename", name }], key);
      await api.sdk.call("opencut_edit_plan", request);
      if (generation !== this.generation || view !== this.viewGeneration) throw new DOMException("The edit view changed", "AbortError");
      await this.scene.recovery.restore(api, project.id);
      const receipt = await this.scene.recovery.publicApply(api, request, null);
      if (generation === this.generation) {
        await this.refreshProjects();
        if (this.state.scene?.head.project.id === project.id) await this.scene.refresh();
      }
      return receipt;
    } finally { release(); }
  }
  private async metadataKey(action: string, value: unknown) {
    const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(JSON.stringify(value)));
    return action + ":" + Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, "0")).join("");
  }
  async promoteProject() {
    const { api, snapshot, historical } = this.state;
    if (!api || !snapshot) throw new Error("Open a historical project first.");
    if (historical) throw new Error("Return to the current revision before converting.");
    const generation = this.generation, view = this.viewGeneration, release = this.acquire(snapshot.project.id);
    try {
      const asset_versions = Object.fromEntries(snapshot.project.assets.filter(asset => asset.version).sort((a,b) => a.id < b.id ? -1 : a.id > b.id ? 1 : 0).map(asset => [asset.id, asset.version!]));
      const key = await this.metadataKey("promote", [snapshot.project.id, snapshot.revision, asset_versions]);
      const request = {project_id:snapshot.project.id,expected_revision:snapshot.revision,idempotency_key:key,asset_versions};
      const receipt = await api.sdk.callWithAdmissionRetry("opencut_project_promote", request, {beforeAttempt:()=>{
        if (generation !== this.generation || view !== this.viewGeneration) throw new DOMException("The conversion view changed", "AbortError");
      }});
      // Read the actual immutable publication,not only a success-shaped reply.
      const converted = await api.sdk.inspectEditorProject({project_id:snapshot.project.id,revision:receipt.revision});
      if (converted.project.id !== snapshot.project.id || converted.revision !== receipt.revision) throw new Error("Conversion publication identity changed.");
      if (generation === this.generation && view === this.viewGeneration) {
        await this.refreshProjects();
        await this.open({id:converted.project.id,name:converted.project.name,revision:converted.revision,format:"editor"});
      }
      return receipt;
    } finally { release(); }
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
        (this.state.snapshot?.project.id ?? this.state.scene?.head.project.id ?? this.state.preserved?.project_id),
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
                this.state.scene?.head.project.id),
          )
        ) {
          this.scene.clipboard.invalidate();
          this.scene.timelineEdit.invalidate();
          this.viewGeneration++;
          this.set({
            snapshot: null,
            scene: null,
            preserved: null,
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
}
