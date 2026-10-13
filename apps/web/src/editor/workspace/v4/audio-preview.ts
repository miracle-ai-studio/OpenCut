import { OpenCutApiError } from "../../../lib/editor-sdk";
import type { EditorApi } from "../api";
import type { Time } from "../../model/types";
import type { V4SessionHost } from "./session";
import type {
  V4AudioPreviewControl,
  V4AudioPreviewCreate,
  V4AudioPreviewSession,
  V4AudioPreviewState,
  V4GainDraft,
} from "./types";
import { PreviewAudioOutput } from "./audio-preview/output";
import type { AudioOutputProgress } from "./audio-preview/output";
import { PreviewConsumptionPump } from "./audio-preview/pump";
import type { PreviewPump } from "./audio-preview/pump";
import { checkPreviewSession } from "./audio-preview/session-identity";

export const emptyAudioPreview = (): V4AudioPreviewState => ({
  phase: "idle",
  capability: null,
  session: null,
  request: null,
  error: null,
});
type Output = Pick<
  PreviewAudioOutput,
  "ready" | "sampleRate" | "push" | "waitForCredit" | "silence" | "stop"
> &
  Partial<Pick<PreviewAudioOutput, "finish" | "createPump">>;
type OutputFactory = (
  token: string,
  rate: number,
  progress: (value: AudioOutputProgress) => void,
  failure: (error: Error) => void,
  completedBlock: () => void,
) => Output;
export type PicturePresentation = {
  prepare(session: V4AudioPreviewSession, signal: AbortSignal): Promise<void>;
  observe(state: V4AudioPreviewState): void;
  consume?(session: V4AudioPreviewSession, frames: string): void;
  suspend(): void;
  close(): void;
};
type Active = {
  picture?: PicturePresentation;
  api: EditorApi;
  epoch: number;
  generation: number;
  request: V4AudioPreviewCreate;
  session: V4AudioPreviewSession | null;
  output: Output;
  abort: AbortController;
  controls: AbortController;
  progress: AudioOutputProgress;
  draining: boolean;
  seekTarget: Time | null;
  seekTask: Promise<void> | null;
  pauseAfterSeek: boolean;
  seekPreparation: AbortController | null;
  pump: PreviewPump | null;
  leaseDeadline: number;
  documentDeparted: boolean;
  stopRequest: V4AudioPreviewControl | null;
};
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** Fixed-revision playback delegate. Project evaluation, overlays and sample→Time
 * belong to the service; this client only reports actual device consumption. */
export class V4AudioPreview {
  private epoch = 0;
  private active: Active | null = null;
  private pending: {
    api: EditorApi;
    request: V4AudioPreviewCreate;
    control: V4AudioPreviewControl | null;
    session: V4AudioPreviewSession | null;
    picture?: PicturePresentation;
  } | null = null;
  private desired: V4GainDraft | null = null;
  private overlaySequence = 0n;
  constructor(
    private readonly host: V4SessionHost,
    private readonly present: (time: Time) => void,
    private readonly outputFactory: OutputFactory = (
      token,
      rate,
      progress,
      failure,
      completedBlock,
    ) => new PreviewAudioOutput(token, rate, progress, failure, completedBlock),
  ) {}
  private publish(patch: Partial<V4AudioPreviewState>) {
    const state = this.host.state();
    if (state) {
      const next = {
        ...state,
        audioPreview: { ...state.audioPreview, ...patch },
      };
      this.host.publish(next);
      this.active?.picture?.observe(next.audioPreview);
    }
  }
  private same(a: Active) {
    const s = this.host.state(),
      snapshot = s && (s.historical ?? s.head);
    return (
      this.active === a &&
      a.epoch === this.epoch &&
      a.api === this.host.api() &&
      a.generation === this.host.generation() &&
      snapshot?.project.id === a.request.project_id &&
      snapshot.revision === a.request.revision &&
      s?.sequenceId === a.request.sequence_id
    );
  }
  async capability() {
    const api = this.host.api(),
      generation = this.host.generation(),
      epoch = this.epoch;
    if (!api) return;
    try {
      const capability = await api.v4.audioPreviewCapability();
      if (
        api !== this.host.api() ||
        generation !== this.host.generation() ||
        epoch !== this.epoch
      )
        return;
      this.publish({ capability, error: null });
    } catch {
      if (
        api === this.host.api() &&
        generation === this.host.generation() &&
        epoch === this.epoch
      )
        this.publish({
          capability: null,
          error: "This service does not support live scene audio.",
        });
    }
  }
  private checkSession(
    a: Active,
    s: V4AudioPreviewSession,
    previous?: V4AudioPreviewSession,
  ) {
    checkPreviewSession(s, a.request, a.output.sampleRate, previous);
    // A local admission hint only; every Play still rechecks server authority.
    a.leaseDeadline =
      performance.now() + Math.min(30000, Number(s.lease_remaining_ms));
  }
  /** Called by the actual Play event, before the first asynchronous operation. */
  play(sampleRate?: number, picture?: PicturePresentation) {
    const api = this.host.api(),
      state = this.host.state();
    if (!api || !state)
      return Promise.reject(new Error("Open a V4 project first."));
    if (this.pending)
      return Promise.reject(
        new Error("Retry the original audio session before starting another."),
      );
    if (
      this.active &&
      this.same(this.active) &&
      ["loading", "playing", "stalled"].includes(state.audioPreview.phase)
    )
      return Promise.resolve();
    const capability = state.audioPreview.capability;
    const rate =
      sampleRate ??
      capability?.sample_rates.find((r) => r === 48000 || r === 44100);
    if (
      !capability?.available ||
      capability.schema_version !== 1 ||
      capability.profile !== "classic-realtime-v31" ||
      capability.max_block_frames !== 4096 ||
      capability.max_buffered_frames !== 8192 ||
      !rate ||
      ![44100, 48000].includes(rate) ||
      !capability.sample_rates.includes(rate)
    )
      return Promise.reject(
        new Error(
          capability?.unavailable_reason ??
            "Live scene audio is unavailable on this service.",
        ),
      );
    const snapshot = state.historical ?? state.head;
    const paused = this.active;
    if (
      paused &&
      this.same(paused) &&
      state.audioPreview.phase === "paused" &&
      paused.session?.state === "paused" &&
      paused.session.identity.sample_rate === rate &&
      !paused.controls.signal.aborted &&
      paused.leaseDeadline > performance.now()
    )
      return this.begin(
        api,
        paused.request,
        null,
        paused.session,
        picture ?? paused.picture,
      );
    const request: V4AudioPreviewCreate = {
      project_id: snapshot.project.id,
      revision: snapshot.revision,
      sequence_id: state.sequenceId,
      start_time: state.playhead,
      sample_rate: rate,
      idempotency_key: crypto.randomUUID(),
    };
    this.invalidate(false);
    return this.begin(api, request, null, null, picture);
  }
  retry() {
    const pending = this.pending,
      s = this.host.state(),
      api = this.host.api();
    if (
      !pending ||
      !s ||
      api !== pending.api ||
      (s.historical ?? s.head).revision !== pending.request.revision ||
      s.head.project.id !== pending.request.project_id ||
      s.sequenceId !== pending.request.sequence_id
    )
      return Promise.reject(
        new Error(
          "Return to the original scene and revision to recover its audio session.",
        ),
      );
    this.invalidate(false);
    return this.begin(
      api,
      pending.request,
      pending.control,
      pending.session,
      pending.picture,
    );
  }
  private begin(
    api: EditorApi,
    request: V4AudioPreviewCreate,
    originalControl: V4AudioPreviewControl | null = null,
    preparedSession: V4AudioPreviewSession | null = null,
    picture?: PicturePresentation,
  ) {
    const epoch = this.epoch;
    let a: Active;
    let output: Output;
    try {
      output = this.outputFactory(
        request.idempotency_key + ":" + epoch,
        request.sample_rate,
        (progress) => {
          if (this.same(a) && a.output === output && !a.draining) {
            a.progress = progress;
            a.pump?.observe(progress);
            if (a.session)
              a.picture?.consume?.(a.session, progress.consumedFrames);
          }
        },
        (error) => {
          if (this.same(a) && a.output === output) this.fail(a, error);
        },
        () => this.completedBlock(a, output),
      );
    } catch (error) {
      this.publish({
        phase: "failed",
        request,
        session: null,
        error: message(error),
      });
      return Promise.reject(error);
    }
    a = {
      picture,
      api,
      epoch,
      generation: this.host.generation(),
      request,
      session: preparedSession,
      output,
      abort: new AbortController(),
      progress: { consumedFrames: "0", stalled: true, contextTime: 0 },
      controls: new AbortController(),
      draining: false,
      seekTarget: null,
      seekTask: null,
      pauseAfterSeek: false,
      seekPreparation: null,
      pump: null,
      leaseDeadline: 0,
      documentDeparted: false,
      stopRequest: null,
    };
    this.active = a;
    this.publish({ phase: "loading", request, session: null, error: null });
    return this.create(a, originalControl, preparedSession);
  }
  private async create(
    a: Active,
    originalControl: V4AudioPreviewControl | null,
    preparedSession: V4AudioPreviewSession | null,
  ) {
    let control = originalControl,
      dispatched = false;
    try {
      await a.output.ready;
      if (!this.same(a)) return;
      await this.attachPump(a);
      if (!this.same(a)) return;
      let session = preparedSession;
      if (!session) {
        dispatched = true;
        session = await a.api.v4.audioPreviewCreate(
          a.request,
          a.controls.signal,
        );
      }
      this.checkSession(a, session);
      a.session = session;
      if (!this.same(a)) {
        this.stopRemote(a);
        return;
      }
      // A replayed session may have advanced: never play stale producer state.
      if (originalControl) {
        dispatched = true;
        const result = await a.api.v4.audioPreviewControl(
          originalControl,
          a.controls.signal,
        );
        this.checkSession(a, result, session);
        a.session = result;
      }
      if (!this.same(a)) return;
      if (a.session.state === "stopped" || a.session.state === "failed")
        throw new Error(
          "The original scene audio session has stopped. Start a new session.",
        );
      if (!originalControl) {
        control = this.command(a, { kind: "play" });
        dispatched = true;
        a.session = await a.api.v4.audioPreviewControl(
          control,
          a.controls.signal,
        );
      }
      this.checkSession(a, a.session, session);
      if (!this.same(a)) {
        this.stopRemote(a);
        return;
      }
      // Picture preparation happens with an already-resumed AudioContext,
      // before any PCM stream starts. Local execution failure is known.
      dispatched = false;
      if (a.picture)
        await a.picture.prepare(
          a.session,
          AbortSignal.any([a.controls.signal, AbortSignal.timeout(10000)]),
        );
      if (!this.same(a)) return;
      this.pending = null;
      this.present(a.session.authoritative_time);
      this.publish({ session: a.session, phase: "loading", error: null });
      this.stream(a);
      if (this.desired) this.draft(this.desired);
    } catch (e) {
      if (!this.same(a)) return;
      if (!dispatched) this.stopRemote(a);
      const unknown =
        !!originalControl || (dispatched && !(e instanceof OpenCutApiError));
      this.pending = unknown
        ? {
            api: a.api,
            request: a.request,
            control,
            session: preparedSession,
            picture: a.picture,
          }
        : null;
      this.fail(a, e, unknown);
      throw e;
    }
  }
  private command(
    a: Active,
    command: V4AudioPreviewControl["command"],
  ): V4AudioPreviewControl {
    const s = a.session!;
    return {
      session_id: s.session_id,
      generation: s.generation,
      owner_id: s.owner_id,
      lease_id: s.lease_id,
      control_id: crypto.randomUUID(),
      command,
    };
  }
  private async attachPump(a: Active) {
    const output = a.output;
    const updated = (
      session: V4AudioPreviewSession,
      progress: AudioOutputProgress,
    ) => {
      if (!this.same(a) || a.output !== output) return;
      this.checkSession(a, session, a.session ?? undefined);
      a.session = session;
      this.present(session.authoritative_time);
      if (a.draining) return;
      const phase =
        session.state === "ended"
          ? "ended"
          : progress.stalled
            ? "stalled"
            : "playing";
      this.publish({ session, phase });
      if (phase === "ended") this.invalidate(false);
    };
    const failure = (error: unknown) => {
      if (this.same(a) && a.output === output) this.fail(a, error);
    };
    const pump = output.createPump
      ? await output.createPump(a.api.audioPreviewConnection, updated, failure)
      : new PreviewConsumptionPump(
          {
            open: (session, signal) =>
              a.api.sdk.openV4AudioPreviewStream(session, { signal }),
            control: (input, signal) =>
              a.api.v4.audioPreviewControl(input, signal),
          },
          output,
          () => this.same(a) && a.output === output,
          updated,
          failure,
        );
    if (!this.same(a) || a.output !== output) {
      pump.abort();
      return;
    }
    a.pump = pump;
    const abort = () => {
      pump.abort();
      a.controls.signal.removeEventListener("abort", abort);
    };
    a.abort.signal.addEventListener("abort", abort, { once: true });
    a.controls.signal.addEventListener("abort", abort, { once: true });
  }
  private stream(a: Active) {
    a.pump!.start(a.session!);
  }
  private completedBlock(a: Active, output: Output) {
    if (this.same(a) && a.output === output && !a.draining) a.pump?.completed();
  }
  private async report(a: Active, frames: string) {
    const previous = a.session!;
    const session = await a.pump!.reportStopped(frames);
    if (!this.same(a)) return;
    if (!session)
      throw new Error("Scene audio final consumption was not confirmed.");
    this.checkSession(a, session, previous);
    if (
      session.generation !== previous.generation ||
      session.overlay_epoch !== previous.overlay_epoch
    )
      return;
    a.session = session;
    this.present(session.authoritative_time);
    return session;
  }
  private stopRemote(a: Active) {
    const s = a.session,
      old = a.stopRequest;
    if (
      s &&
      !(
        old &&
        old.session_id === s.session_id &&
        old.generation === s.generation &&
        old.owner_id === s.owner_id &&
        old.lease_id === s.lease_id
      )
    ) {
      const request = this.command(a, { kind: "stop" });
      a.stopRequest = request;
      void a.api.v4
        .audioPreviewControl(
          request,
          undefined,
          a.documentDeparted ? { keepalive: true } : {},
        )
        .catch(() => {});
    }
  }
  private localStop(a: Active) {
    a.picture?.suspend();
    a.output.silence();
    a.pump?.freeze();
  }
  /** Discard this document's sound, including BFCache departure. No fabricated
   * final cursor: Stop uses only the last validated owner/lease/generation. */
  retireDocument() {
    this.invalidate(false, true);
    // BFCache can restore this same object: unknown admission/control must
    // retain its original request rather than create another session.
    this.publish(
      this.pending
        ? {
            phase: "unconfirmed",
            session: this.pending.session,
            request: this.pending.request,
          }
        : { phase: "paused", session: null, request: null, error: null },
    );
  }
  invalidate(clearPending = true, documentDeparted = false) {
    this.epoch++;
    const a = this.active;
    this.active = null;
    this.desired = null;
    if (clearPending) this.pending = null;
    if (a) {
      a.documentDeparted = documentDeparted;
      a.picture?.close();
      this.localStop(a);
      a.abort.abort();
      a.controls.abort();
      void a.output.stop().catch(() => {});
      this.stopRemote(a);
    }
  }
  private fail(a: Active, error: unknown, unknown = false) {
    this.localStop(a);
    a.abort.abort();
    a.controls.abort();
    if (!unknown) this.stopRemote(a);
    void a.output.stop().catch(() => {});
    if (this.active === a) {
      a.picture?.close();
      this.active = null;
      this.epoch++;
    }
    this.publish({
      phase: unknown ? "unconfirmed" : "failed",
      session: a.session,
      error: message(error),
    });
  }
  async pause() {
    const a = this.active;
    if (a?.seekTask && this.same(a)) {
      a.pauseAfterSeek = true;
      this.localStop(a);
      a.seekPreparation?.abort();
      await a.seekTask;
      return;
    }
    if (!a || !this.same(a) || !a.session) {
      this.invalidate();
      this.publish({ phase: "paused" });
      return;
    }
    a.draining = true;
    this.localStop(a);
    try {
      const frames = await a.output.stop();
      if (!this.same(a)) return;
      await this.report(a, frames);
      if (!this.same(a)) return;
      const previous = a.session!,
        request = this.command(a, { kind: "pause" });
      const result = await a.api.v4.audioPreviewControl(
        request,
        a.controls.signal,
      );
      this.checkSession(a, result, previous);
      if (!this.same(a)) return;
      a.session = result;
      this.present(result.authoritative_time);
      this.publish({ session: result, phase: "paused" });
    } catch (e) {
      if (this.same(a)) this.fail(a, e);
      throw e;
    } finally {
      a.abort.abort();
    }
  }
  seek(time?: Time) {
    const a = this.active;
    if (
      time &&
      a?.session &&
      this.same(a) &&
      (a.seekTask ||
        (!a.draining &&
          ["playing", "stalled", "loading"].includes(
            this.host.state()?.audioPreview.phase ?? "",
          )))
    ) {
      a.seekTarget = time;
      a.draining = true;
      this.localStop(a);
      a.seekPreparation?.abort();
      this.publish({ phase: "loading" });
      if (!a.seekTask) {
        const task = this.seekPlaying(a);
        a.seekTask = task;
        void task.finally(() => {
          if (a.seekTask === task) a.seekTask = null;
        });
      }
      return true;
    }
    this.invalidate();
    this.publish({ phase: "paused", session: null, request: null });
    return false;
  }
  revisionChanged() {
    // A new head is not evidence that an unknown gain transaction succeeded.
    // Keep the captured base/overlay receipt identity, but stop its old sound.
    this.invalidate(false);
    this.publish({ phase: "paused" });
  }
  private async seekPlaying(a: Active) {
    try {
      const frames = await a.output.stop();
      if (!this.same(a)) return;
      await this.report(a, frames);
      if (!this.same(a)) return;
      // One latest target, one control in flight. Adopt every accepted identity
      // before the next command, but never prepare/publish a superseded target.
      while (this.same(a) && (a.seekTarget || a.pauseAfterSeek)) {
        const time = a.seekTarget;
        a.seekTarget = null;
        const previous = a.session!;
        const result = await a.api.v4.audioPreviewControl(
          this.command(
            a,
            time ? { kind: "seek", sequence_time: time } : { kind: "pause" },
          ),
          a.controls.signal,
        );
        this.checkSession(a, result, previous);
        a.session = result;
        if (!this.same(a)) {
          this.stopRemote(a);
          return;
        }
        if (a.seekTarget || (a.pauseAfterSeek && result.state !== "paused"))
          continue;
        if (result.state === "paused") {
          a.pauseAfterSeek = false;
          a.abort.abort();
          this.present(result.authoritative_time);
          this.publish({ session: result, phase: "paused" });
          return;
        }
        const started = await this.replaceOutput(a, result, true);
        if (!this.same(a)) return;
        if (!a.seekTarget && !a.pauseAfterSeek) return;
        this.localStop(a);
        a.draining = true;
        const finalFrames = await a.output.stop();
        if (!this.same(a)) return;
        if (started) await this.report(a, finalFrames);
        else if (finalFrames !== "0")
          throw new Error(
            "Superseded scene preparation consumed unexpected sound.",
          );
        a.abort.abort();
      }
    } catch (e) {
      if (this.same(a)) this.fail(a, e);
    }
  }
  private async replaceOutput(
    a: Active,
    session: V4AudioPreviewSession,
    seeking = false,
  ) {
    const superseded = () => seeking && !!(a.seekTarget || a.pauseAfterSeek);
    a.abort.abort();
    a.pump = null;
    a.session = session;
    a.abort = new AbortController();
    a.progress = { consumedFrames: "0", stalled: true, contextTime: 0 };
    const output = this.outputFactory(
      session.session_id +
        ":" +
        session.generation +
        ":" +
        session.overlay_epoch,
      session.identity.sample_rate,
      (p) => {
        if (this.same(a) && a.output === output && !a.draining) {
          a.progress = p;
          a.pump?.observe(p);
          if (a.session) a.picture?.consume?.(a.session, p.consumedFrames);
        }
      },
      (e) => {
        if (this.same(a) && a.output === output) this.fail(a, e);
      },
      () => this.completedBlock(a, output),
    );
    a.output = output;
    await output.ready;
    if (!this.same(a) || superseded()) return false;
    await this.attachPump(a);
    if (!this.same(a) || superseded()) return false;
    if (a.picture) {
      const preparation = new AbortController();
      if (seeking) a.seekPreparation = preparation;
      try {
        await a.picture.prepare(
          session,
          AbortSignal.any([
            a.controls.signal,
            preparation.signal,
            AbortSignal.timeout(10000),
          ]),
        );
      } catch (error) {
        if (superseded() && preparation.signal.aborted) return false;
        throw error;
      } finally {
        if (a.seekPreparation === preparation) a.seekPreparation = null;
      }
    }
    if (!this.same(a) || superseded()) return false;
    a.draining = false;
    this.present(session.authoritative_time);
    this.publish({ session, phase: "loading" });
    this.stream(a);
    return true;
  }
  draft(draft: V4GainDraft | null, preserve = false) {
    if (preserve) return;
    if (!draft && !this.desired && !this.active?.session?.temporary_gain)
      return;
    this.desired = draft;
    const a = this.active;
    if (
      draft &&
      a &&
      (draft.projectId !== a.request.project_id ||
        draft.revision !== a.request.revision ||
        draft.sequenceId !== a.request.sequence_id)
    ) {
      this.desired = null;
      return;
    }
    if (!a || !this.same(a) || !a.session || a.draining) return;
    this.localStop(a);
    void this.overlay(a);
  }
  private async overlay(a: Active) {
    a.draining = true;
    try {
      const frames = await a.output.stop();
      if (!this.same(a)) return;
      await this.report(a, frames);
      if (!this.same(a)) return;
      // Coalesce pointer movements; only the service replaces the element gain.
      const draft = this.desired,
        previous = a.session!;
      const command: V4AudioPreviewControl["command"] = draft
        ? {
            kind: "set_temporary_gain",
            gain: {
              track_id: draft.trackId,
              element_id: draft.elementId,
              decibels_tenths: draft.tenths,
              gesture_id: draft.id,
              update_sequence: (++this.overlaySequence).toString(),
            },
          }
        : previous.temporary_gain
          ? {
              kind: "clear_temporary_gain",
              gesture_id: previous.temporary_gain.gesture_id,
            }
          : { kind: "seek", sequence_time: previous.authoritative_time };
      const request = this.command(a, command);
      const session = await a.api.v4.audioPreviewControl(
        request,
        a.controls.signal,
      );
      this.checkSession(a, session, previous);
      if (!this.same(a)) return;
      await this.replaceOutput(a, session);
      if (this.desired !== draft)
        queueMicrotask(() => this.draft(this.desired));
    } catch (e) {
      if (this.same(a)) this.fail(a, e);
    }
  }
}
