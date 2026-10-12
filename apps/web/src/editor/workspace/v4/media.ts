import { OpenCutApiError } from "../../../lib/editor-sdk";
import type { EditorApi } from "../api";
import { uploadMedia } from "../upload";
import type { V4SessionHost } from "./session";
import type {
  V4Asset,
  V4AvailableMedia,
  V4MediaAttempt,
  V4MediaRequest,
} from "./types";

const message = (error: unknown) =>
  error instanceof Error ? error.message : String(error);
/** Retains commands only, never another project document or SDK. Plans are
 * server-authored and public apply receives their exact transaction bytes. */
export class V4Media {
  private readonly attempts = new WeakMap<
    EditorApi,
    Map<string, V4MediaAttempt>
  >();
  private readonly uploads = new WeakMap<
    EditorApi,
    Map<string, AbortController>
  >();
  private availableRequest = 0;
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
  resume(api: EditorApi, projectId: string) {
    return this.attempts.get(api)?.get(projectId) ?? null;
  }
  recovered(api: EditorApi, projectId: string, key: string) {
    if (this.attempts.get(api)?.get(projectId)?.request.idempotency_key === key) this.attempts.get(api)?.delete(projectId);
    const state = this.host.state();
    if (api === this.host.api() && state?.head.project.id === projectId && state.media.attempt?.request.idempotency_key === key)
      this.host.publish({ ...state, media: { ...state.media, attempt: null } });
  }
  private publish(api: EditorApi, attempt: V4MediaAttempt) {
    let projects = this.attempts.get(api);
    if (!projects) {
      projects = new Map();
      this.attempts.set(api, projects);
    }
    projects.set(attempt.request.project_id, attempt);
    const state = this.host.state();
    if (
      api === this.host.api() &&
      state?.head.project.id === attempt.request.project_id &&
      state.media.attempt?.request.idempotency_key ===
        attempt.request.idempotency_key
    )
      this.host.publish({ ...state, media: { ...state.media, attempt } });
  }
  private begin(label: string): { api: EditorApi; attempt: V4MediaAttempt } {
    const { api, state } = this.active();
    if (state.historical)
      throw new Error("Return to the current revision before editing.");
    if (state.media.attempt)
      throw new Error(
        "Resolve the current media changes before starting another operation.",
      );
    const attempt: V4MediaAttempt = {
      request: {
        project_id: state.head.project.id,
        expected_revision: state.head.revision,
        sequence_id: state.sequenceId,
        idempotency_key: crypto.randomUUID(),
        label,
        bindings: [],
        placements: [],
        new_tracks: [],
        ripple: false,
      },
      plan: null,
      receipt: null,
      phase: "planning",
      progress: null,
      error: null,
    };
    this.host.publish({ ...state, media: { ...state.media, attempt } });
    this.publish(api, attempt);
    return { api, attempt };
  }
  private checkView(
    attempt: V4MediaAttempt,
    api: EditorApi,
    generation: number,
    view: number,
    revision = true,
  ) {
    const current = this.host.state();
    if (
      api !== this.host.api() ||
      generation !== this.host.generation() ||
      view !== this.host.viewGeneration() ||
      current?.head.project.id !== attempt.request.project_id ||
      current.historical ||
      current.sequenceId !== attempt.request.sequence_id
    )
      throw new Error(
        "The media target changed. Return to the original scene to continue.",
      );
    if (revision && current.head.revision !== attempt.request.expected_revision)
      throw new Error(
        "Project has changed; discard this plan and review new media changes.",
      );
  }
  async refreshAvailable() {
    const { api, state } = this.active(),
      generation = this.host.generation(),
      request = ++this.availableRequest;
    this.host.publish({
      ...state,
      media: { ...state.media, loading: true, error: null },
    });
    try {
      const available = await api.v4.availableMedia(state.head.project.id);
      const current = this.host.state();
      if (
        request === this.availableRequest &&
        generation === this.host.generation() &&
        api === this.host.api() &&
        current?.head.project.id === state.head.project.id
      )
        this.host.publish({
          ...current,
          media: { ...current.media, available, loading: false, error: null },
        });
    } catch (error) {
      const current = this.host.state();
      if (
        request === this.availableRequest &&
        generation === this.host.generation() &&
        api === this.host.api() &&
        current?.head.project.id === state.head.project.id
      )
        this.host.publish({
          ...current,
          media: { ...current.media, loading: false, error: message(error) },
        });
    }
  }
  async importFiles(files: File[], replacing?: Pick<V4Asset, "id" | "kind">) {
    if (!files.length || files.length > 60 || (replacing && files.length !== 1))
      throw new Error(
        "Choose between 1 and 60 files; replacements require one file.",
      );
    let { api, attempt } = this.begin(
      replacing ? "Replace media" : "Import media",
    );
    const generation = this.host.generation(),
      view = this.host.viewGeneration(),
      abort = new AbortController();
    let requests = this.uploads.get(api);
    if (!requests) {
      requests = new Map();
      this.uploads.set(api, requests);
    }
    requests.set(attempt.request.project_id, abort);
    attempt = { ...attempt, phase: "uploading" };
    this.publish(api, attempt);
    try {
      for (const file of files) {
        this.checkView(attempt, api, generation, view);
        const imported = await uploadMedia(
          api.sdk,
          attempt.request.project_id,
          file,
          (progress) => {
            attempt = { ...attempt, progress };
            this.publish(api, attempt);
          },
          abort.signal,
        );
        // A finished upload belongs to the captured project. It remains in
        // available media if navigation or a concurrent revision changed.
        this.checkView(attempt, api, generation, view);
        if (
          !imported.media_id ||
          imported.media.project_id !== attempt.request.project_id ||
          !imported.version
        )
          throw new Error("Imported media has no verified project identity.");
        if (replacing && imported.asset.kind !== replacing.kind)
          throw new Error(
            "Replacement media must have the same type as the original.",
          );
        attempt = {
          ...attempt,
          request: {
            ...attempt.request,
            bindings: [
              ...attempt.request.bindings,
              {
                media_id: imported.media_id,
                version: imported.version,
                asset_id: replacing?.id ?? crypto.randomUUID(),
                name: imported.asset.name,
                mode: replacing ? "replace" : "bind",
              },
            ],
          },
          progress: imported.warning,
        };
        this.publish(api, attempt);
      }
      await this.planAttempt(api, attempt, generation, view);
    } catch (error) {
      this.publish(api, { ...attempt, phase: "failed", error: message(error) });
      if (api !== this.host.api() || generation !== this.host.generation())
        throw new DOMException("Project changed", "AbortError");
      throw error;
    } finally {
      requests.delete(attempt.request.project_id);
      if (api === this.host.api() && generation === this.host.generation()) {
        await this.refreshAvailable();
        await this.host.refreshTasks().catch(() => {});
      }
    }
  }
  cancelUpload() {
    const { api, state } = this.active();
    this.uploads.get(api)?.get(state.head.project.id)?.abort();
  }
  async recover(media: V4AvailableMedia) {
    const { state } = this.active();
    if (media.project_id !== state.head.project.id || !media.asset.version)
      throw new Error("Available media has no verified project identity.");
    const { api, attempt } = this.begin("Recover imported media");
    attempt.request.bindings.push({
      media_id: media.media_id,
      version: media.asset.version,
      asset_id: crypto.randomUUID(),
      name: media.asset.name,
      mode: "bind",
    });
    return this.planAttempt(
      api,
      attempt,
      this.host.generation(),
      this.host.viewGeneration(),
    );
  }
  async place(
    placement: Omit<V4MediaRequest["placements"][number], "element_id">,
    newTrack?: Omit<V4MediaRequest["new_tracks"][number], "id">,
    scope?: { projectId: string; revision: string; sequenceId: string },
  ) {
    const { state } = this.active();
    if (
      scope &&
      (scope.projectId !== state.head.project.id ||
        scope.revision !== state.head.revision ||
        scope.sequenceId !== state.sequenceId ||
        state.historical)
    )
      throw new Error(
        "The media target changed. Return to the original scene to continue.",
      );
    if (!state.head.project.assets.some((a) => a.id === placement.asset_id))
      throw new Error("The selected project media is unavailable.");
    const { api, attempt } = this.begin("Place media");
    const trackId = newTrack ? crypto.randomUUID() : placement.track_id;
    attempt.request.placements.push({
      ...placement,
      track_id: trackId,
      element_id: crypto.randomUUID(),
    });
    if (newTrack) attempt.request.new_tracks.push({ ...newTrack, id: trackId });
    return this.planAttempt(
      api,
      attempt,
      this.host.generation(),
      this.host.viewGeneration(),
    );
  }
  async retryPlan() {
    const { api, state } = this.active(),
      attempt = state.media.attempt;
    if (!attempt || attempt.plan || !["failed"].includes(attempt.phase))
      throw new Error("No media request is ready to plan.");
    return this.planAttempt(
      api,
      attempt,
      this.host.generation(),
      this.host.viewGeneration(),
    );
  }
  private async planAttempt(
    api: EditorApi,
    original: V4MediaAttempt,
    generation: number,
    view: number,
  ) {
    let attempt = {
      ...original,
      phase: "planning" as V4MediaAttempt["phase"],
      error: null,
    };
    this.publish(api, attempt);
    let release: (() => void) | undefined;
    try {
      release = this.host.acquire(attempt.request.project_id);
      this.checkView(attempt, api, generation, view);
      const plan = await api.v4.mediaPlan(attempt.request);
      this.checkView(attempt, api, generation, view);
      if (
        plan.transaction.project_id !== attempt.request.project_id ||
        plan.transaction.expected_revision !==
          attempt.request.expected_revision ||
        plan.transaction.idempotency_key !== attempt.request.idempotency_key ||
        plan.transaction.label !== attempt.request.label
      )
        throw new Error("Media plan does not match the captured request.");
      attempt = { ...attempt, plan, phase: "planned" };
      this.publish(api, attempt);
      return plan;
    } catch (error) {
      this.publish(api, { ...attempt, phase: "failed", error: message(error) });
      throw error;
    } finally {
      release?.();
    }
  }
  async apply() {
    const { api, state } = this.active();
    let attempt = state.media.attempt;
    if (
      !attempt?.plan ||
      !["planned", "failed", "unconfirmed"].includes(attempt.phase)
    )
      throw new Error("Review a media plan before saving.");
    const generation = this.host.generation(),
      view = this.host.viewGeneration(),
      wasUnknown = attempt.phase === "unconfirmed";
    let dispatched = false;
    const release = this.host.acquire(attempt.request.project_id);
    try {
      if (wasUnknown) await api.v4.inspect(attempt.request.project_id); // Observation is not a receipt.
      this.checkView(attempt, api, generation, view, !wasUnknown);
      attempt = { ...attempt, phase: "applying", error: null };
      this.publish(api, attempt);
      dispatched = true;
      const receipt = this.host.publicApply ? await this.host.publicApply(api, attempt.plan!.transaction, attempt.request.sequence_id)
        : await api.sdk.call("opencut_edit_v4_apply", attempt.plan!.transaction);
      if (
        receipt.base_revision !== attempt.request.expected_revision ||
        !receipt.revision ||
        !receipt.transaction_id
      )
        throw new Error("Media save returned an unexpected receipt.");
      attempt = { ...attempt, receipt, phase: "saved", error: null };
      this.publish(api, attempt);
      if (api === this.host.api() && generation === this.host.generation()) {
        try {
          await this.refresh();
        } catch (error) {
          this.publish(api, { ...attempt, error: message(error) });
        }
      }
      return receipt;
    } catch (error) {
      if (
        wasUnknown &&
        dispatched &&
        error instanceof OpenCutApiError &&
        error.code === "REVISION_CONFLICT"
      ) {
        this.attempts.get(api)?.delete(attempt.request.project_id);
        if (api === this.host.api() && generation === this.host.generation()) {
          const current = this.host.state();
          if (
            current?.media.attempt?.request.idempotency_key ===
            attempt.request.idempotency_key
          )
            this.host.publish({
              ...current,
              media: { ...current.media, attempt: null },
            });
          await this.refresh();
        }
        throw new Error(
          "The original media changes were not saved because the project changed. Review a new plan.",
        );
      }
      this.publish(api, {
        ...attempt,
        phase:
          wasUnknown || (dispatched && !(error instanceof OpenCutApiError))
            ? "unconfirmed"
            : "failed",
        error: message(error),
      });
      if (api !== this.host.api() || generation !== this.host.generation())
        throw new DOMException("Project changed", "AbortError");
      throw error;
    } finally {
      release();
    }
  }
  async refreshSaved() {
    const { api, state } = this.active(),
      attempt = state.media.attempt,
      generation = this.host.generation();
    await this.refresh();
    if (
      attempt?.phase === "saved" &&
      generation === this.host.generation() &&
      api === this.host.api()
    )
      this.publish(api, { ...attempt, error: null });
  }
  dismiss() {
    const { api, state } = this.active(),
      attempt = state.media.attempt;
    if (
      attempt &&
      ["uploading", "planning", "applying", "unconfirmed"].includes(
        attempt.phase,
      )
    )
      throw new Error(
        "Resolve the original media changes before starting another operation.",
      );
    this.attempts.get(api)?.delete(state.head.project.id);
    this.host.publish({ ...state, media: { ...state.media, attempt: null } });
  }
}
