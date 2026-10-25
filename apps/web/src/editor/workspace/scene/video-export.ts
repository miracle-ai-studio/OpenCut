import { OpenCutApiError, type ToolOutput } from "../../../lib/editor-sdk";
import type { EditorApi } from "../api";
import type { SessionHost } from "./session";
import { ExportVideoRecovery, type ExportVideoAttempt, type ExportVideoOptions, type StoragePort } from "./video-export-recovery";
export type { ExportVideoAttempt, ExportVideoOptions, ExportVideoRequest } from "./video-export-recovery";

type Capability = ToolOutput<"opencut_render_capability">;
type State = { capability: Capability | null; capabilityError: string | null; checking: boolean; attempt: ExportVideoAttempt | null };
const empty = (): State => ({ capability: null, capabilityError: null, checking: false, attempt: null });
const message = (e: unknown) => e instanceof Error ? e.message : String(e);

/** Only capability and the original submission intent live here. The workspace
 * remains the project authority; the existing task feed owns execution/cancel.
 * The recovery module's tab-scoped record retains no paths, media bytes, credentials or bearer tokens. */
export class VideoExport {
  private state: State = empty();
  private epoch = 0;
  private readonly listeners = new Set<() => void>();
  private readonly activeRequests = new Set<string>();
  private readonly recovery: ExportVideoRecovery;
  constructor(private readonly host: SessionHost, storage?: StoragePort) { this.recovery = new ExportVideoRecovery(storage); }
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  getSnapshot = () => this.state;
  private publish(patch: Partial<State>) { this.state = { ...this.state, ...patch }; for (const listener of this.listeners) listener(); }
  private key(api: EditorApi, project: string) {
    if (!api.connectionIdentity) throw Error("Authenticate the service before exporting.");
    return `opencut.export-video:${JSON.stringify([api.connectionIdentity, project])}`;
  }
  private active() {
    const api = this.host.api(), state = this.host.state();
    if (!api || !state) throw Error("Open a project first.");
    return { api, state };
  }
  private load(api: EditorApi, project: string) {
    return this.recovery.read(api.connectionIdentity, project);
  }
  private retain(api: EditorApi, attempt: ExportVideoAttempt, generation = this.host.generation()) {
    this.recovery.write(api.connectionIdentity, attempt);
    if (api === this.host.api() && generation === this.host.generation() && this.host.state()?.head.project.id === attempt.request.project_id)
      this.publish({ attempt: structuredClone(attempt) });
  }
  async refreshCapability() {
    const { api, state } = this.active(), generation = this.host.generation(), epoch = ++this.epoch;
    this.publish({ checking: true, capability: null, capabilityError: null, attempt: null });
    try {
      await api.authenticateConnection();
      const capability = await api.sdk.call("opencut_render_capability", {});
      if (api !== this.host.api() || generation !== this.host.generation() || epoch !== this.epoch || this.host.state()?.head.project.id !== state.head.project.id) return;
      this.publish({ capability, attempt: this.load(api, state.head.project.id) });
    } catch (error) {
      if (api === this.host.api() && generation === this.host.generation() && epoch === this.epoch) this.publish({ capabilityError: message(error) });
    } finally { if (api === this.host.api() && generation === this.host.generation() && epoch === this.epoch) this.publish({ checking: false }); }
  }
  unavailable(options: ExportVideoOptions) {
    const c = this.state.capability;
    return !c ? this.state.capabilityError ?? "Export availability is being checked."
      : !c.video.available ? c.video.unavailable_reason ?? "Video export is unavailable."
      : !c.formats.includes(options.format) || !c.qualities.includes(options.quality) ? "This export format or quality is unavailable."
      : options.include_audio && !c.video_audio ? "The audio renderer or required audio encoders are unavailable. Turn off Include audio to export video only." : null;
  }
  async start(options: ExportVideoOptions) {
    const { api, state } = this.active(), previous = this.load(api, state.head.project.id);
    if (previous) throw Error("Resolve or reset the previous export before starting another.");
    const reason = this.unavailable(options); if (reason) throw Error(reason);
    const view = state.historical ?? state.head, scene = view.project.sequences.find(s => s.id === state.sequenceId);
    if (!scene) throw Error("The selected scene is unavailable.");
    const attempt: ExportVideoAttempt = { request: { project_id: view.project.id, revision: view.revision, sequence_id: scene.id,
      idempotency_key: crypto.randomUUID(), options: structuredClone(options) }, sceneName: scene.name, phase: "submitting", jobId: null, error: null };
    // Persist BEFORE dispatch. Storage failure stops here, before a job can exist.
    this.retain(api, attempt);
    return this.execute(api, attempt, false);
  }
  async retry() {
    const { api, state } = this.active(), attempt = this.load(api, state.head.project.id);
    if (!attempt || attempt.phase === "accepted") throw Error("There is no unresolved export.");
    return this.execute(api, attempt, true);
  }
  dismiss() {
    const { api, state } = this.active(), attempt = this.load(api, state.head.project.id);
    if (attempt && ["submitting", "unconfirmed"].includes(attempt.phase)) throw Error("Confirm the original export before resetting it.");
    this.recovery.remove(api.connectionIdentity, state.head.project.id); this.publish({ attempt: null });
  }
  private async execute(api: EditorApi, original: ExportVideoAttempt, replay: boolean) {
    const request = structuredClone(original.request), key = this.key(api, request.project_id), generation = this.host.generation(), view = this.host.viewGeneration();
    if (this.activeRequests.has(key)) throw Error("Wait for the original export submission to finish.");
    const current = this.host.state();
    if (api !== this.host.api() || current?.head.project.id !== request.project_id || (!replay && (current.historical ?? current.head).revision !== request.revision)) throw Error("Project view changed before export.");
    this.activeRequests.add(key);
    let release: (() => void) | null = null, dispatched = false;
    try {
      release = this.host.acquire(request.project_id);
      if (api !== this.host.api() || generation !== this.host.generation() || view !== this.host.viewGeneration()) throw Error("Project view changed before export.");
      this.retain(api, { ...original, phase: "submitting", error: null }, generation); dispatched = true;
      const job = await api.sdk.call("opencut_render_submit", {kind:"video",request});
      if (!job.job_id || job.kind !== (request.range ? "render.video_range" : "render.video") || job.target?.project_id !== request.project_id || job.target.revision !== request.revision || job.target.sequence_id !== request.sequence_id)
        throw Error("Export returned an unexpected task receipt.");
      this.retain(api, { ...original, phase: "accepted", jobId: job.job_id, error: null }, generation);
      if (api === this.host.api() && generation === this.host.generation()) await this.host.refreshTasks().catch(() => {});
      return job.job_id;
    } catch (error) {
      this.retain(api, { ...original, phase: original.phase === "unconfirmed" || (dispatched && !(error instanceof OpenCutApiError)) ? "unconfirmed" : "retryable", error: message(error).slice(0, 2048) }, generation);
      throw error;
    } finally { release?.(); this.activeRequests.delete(key); }
  }
}
