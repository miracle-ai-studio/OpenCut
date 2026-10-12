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
import { readPreviewPcm, sameAudioIdentity } from "./audio-preview/transport";

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
> & Partial<Pick<PreviewAudioOutput, "finish">>;
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
  reportSequence: bigint;
  timer: ReturnType<typeof setInterval> | null;
  reporting: boolean;
  completedPending: boolean;
  draining: boolean;
  heartbeatAt: number;
  leaseDeadline: number;
};
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));
const counter = (s: unknown): s is string =>
  typeof s === "string" &&
  /^(0|[1-9][0-9]{0,19})$/.test(s) &&
  BigInt(s) <= 18446744073709551615n;

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
    const i = s.identity;
    if (
      s.schema_version !== 1 ||
      !s.session_id ||
      !s.owner_id ||
      !s.lease_id ||
      !i ||
      i.project_id !== a.request.project_id ||
      i.revision !== a.request.revision ||
      i.sequence_id !== a.request.sequence_id ||
      i.sample_rate !== a.output.sampleRate ||
      i.channels !== 2 ||
      i.profile !== "classic-realtime-v31" ||
      !counter(s.generation) ||
      !counter(s.overlay_epoch) ||
      !counter(s.actual_start_sample) ||
      !counter(s.total_frames) ||
      !counter(s.consumed_frames) ||
      !counter(s.lease_remaining_ms) ||
      BigInt(s.actual_start_sample) + BigInt(s.consumed_frames) >
        BigInt(s.total_frames) ||
      !s.authoritative_time ||
      !/^-?\d+$/.test(s.authoritative_time.ticks) ||
      !Number.isInteger(s.authoritative_time.timebase) ||
      s.authoritative_time.timebase <= 0 ||
      (previous &&
        (s.session_id !== previous.session_id ||
          !sameAudioIdentity(i, previous.identity) ||
          s.owner_id !== previous.owner_id ||
          s.lease_id !== previous.lease_id))
    )
      throw new Error(
        "Scene audio returned an unexpected fixed session identity.",
      );
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
            if (a.session) a.picture?.consume?.(a.session, progress.consumedFrames);
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
      reportSequence: 0n,
      timer: null,
      reporting: false,
      completedPending: false,
      draining: false,
      heartbeatAt: Date.now(),
      leaseDeadline: 0,
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
  private stream(a: Active) {
    const s = a.session!;
    a.reportSequence = 0n;
    const output = a.output,
      abort = a.abort;
    const current = () =>
      this.same(a) &&
      a.output === output &&
      a.abort === abort &&
      a.session?.generation === s.generation &&
      a.session.overlay_epoch === s.overlay_epoch;
    a.timer = setInterval(() => {
      void this.report(a).catch(() => {});
    }, 50);
    void (async () => {
      try {
        const response = await a.api.sdk.openV4AudioPreviewStream(
          {
            session_id: s.session_id,
            generation: s.generation,
            overlay_epoch: s.overlay_epoch,
            owner_id: s.owner_id,
            lease_id: s.lease_id,
          },
          { signal: abort.signal },
        );
        if (!current()) {
          await response.body?.cancel().catch(() => {});
          return;
        }
        const blocks = readPreviewPcm(response, s, abort.signal);
        try {
          for (;;) {
            await output.waitForCredit(abort.signal);
            const { value: pcm, done } = await blocks.next();
            if (done) { if (current()) output.finish?.(); break; }
            if (!current()) return;
            await output.push(pcm, abort.signal);
          }
        } finally {
          await blocks.return(undefined);
        }
      } catch (e) {
        // Pause/seek closes the old producer at its generation fence. Its
        // expected EOF must not race the final pause receipt into failure.
        if (current() && !a.draining && !abort.signal.aborted) this.fail(a, e);
      }
    })();
  }
  private completedBlock(a: Active, output: Output) {
    if (!this.same(a) || a.output !== output || a.draining || !a.session)
      return;
    if (a.reporting) a.completedPending = true;
    else void this.report(a).catch(() => {});
  }
  private async report(a: Active, stoppedFrames?: string) {
    if (
      !this.same(a) ||
      !a.session ||
      a.reporting ||
      (a.draining && stoppedFrames === undefined)
    )
      return;
    a.reporting = true;
    a.completedPending = false;
    const previous = a.session;
    const output = a.output;
    const command = this.command(a, {
      kind: "report_consumed",
      sequence: (++a.reportSequence).toString(),
      consumed_frames: stoppedFrames ?? a.progress.consumedFrames,
      playing: stoppedFrames === undefined,
      stalled: stoppedFrames === undefined && a.progress.stalled,
    });
    try {
      const signal = AbortSignal.any([
        a.controls.signal,
        AbortSignal.timeout(3000),
      ]);
      const session = await a.api.v4.audioPreviewControl(command, signal);
      this.checkSession(a, session, previous);
      if (
        !this.same(a) ||
        session.generation !== previous.generation ||
        session.overlay_epoch !== previous.overlay_epoch
      )
        return;
      a.session = session;
      this.present(session.authoritative_time);
      if (session.state === "failed" || session.state === "stopped")
        throw new Error("Scene audio is no longer available.");
      const phase =
        session.state === "ended"
          ? "ended"
          : a.progress.stalled
            ? "stalled"
            : "playing";
      if (!a.draining) this.publish({ session, phase });
      if (phase === "ended") this.invalidate(false);
      // Release production credit before renewing authorization. Both controls
      // remain serialized; a completion during either await is flushed below.
      if (this.same(a) && !a.draining && Date.now() - a.heartbeatAt >= 500) {
        const heartbeat = await a.api.v4.audioPreviewControl(
          this.command(a, { kind: "heartbeat" }),
          signal,
        );
        this.checkSession(a, heartbeat, session);
        if (!this.same(a)) return;
        a.heartbeatAt = Date.now();
      }
      return session;
    } catch (e) {
      if (this.same(a)) this.fail(a, e);
      throw e;
    } finally {
      a.reporting = false;
      if (
        a.completedPending &&
        this.same(a) &&
        !a.draining &&
        a.output === output &&
        a.session?.generation === previous.generation &&
        a.session.overlay_epoch === previous.overlay_epoch
      ) {
        void this.report(a).catch(() => {});
      }
    }
  }
  private stopRemote(a: Active) {
    if (a.session)
      void a.api.v4
        .audioPreviewControl(this.command(a, { kind: "stop" }))
        .catch(() => {});
  }
  private localStop(a: Active) {
    a.picture?.suspend();
    a.completedPending = false;
    a.output.silence();
    if (a.timer) clearInterval(a.timer);
    a.timer = null;
  }
  invalidate(clearPending = true) {
    this.epoch++;
    const a = this.active;
    this.active = null;
    this.desired = null;
    if (clearPending) this.pending = null;
    if (a) {
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
    if (!a || !this.same(a) || !a.session) {
      this.invalidate();
      this.publish({ phase: "paused" });
      return;
    }
    a.draining = true;
    this.localStop(a);
    try {
      const frames = await a.output.stop();
      while (a.reporting && this.same(a))
        await new Promise<void>((resolve) => setTimeout(resolve, 5));
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
      !a.draining &&
      ["playing", "stalled", "loading"].includes(
        this.host.state()?.audioPreview.phase ?? "",
      )
    ) {
      a.draining = true;
      this.localStop(a);
      this.publish({ phase: "loading" });
      void this.seekPlaying(a, time);
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
  private async seekPlaying(a: Active, time: Time) {
    try {
      const frames = await a.output.stop();
      while (a.reporting && this.same(a))
        await new Promise<void>((resolve) => setTimeout(resolve, 5));
      if (!this.same(a)) return;
      await this.report(a, frames);
      if (!this.same(a)) return;
      const previous = a.session!;
      const result = await a.api.v4.audioPreviewControl(
        this.command(a, { kind: "seek", sequence_time: time }),
        a.controls.signal,
      );
      this.checkSession(a, result, previous);
      if (!this.same(a)) return;
      await this.replaceOutput(a, result);
    } catch (e) {
      if (this.same(a)) this.fail(a, e);
    }
  }
  private async replaceOutput(a: Active, session: V4AudioPreviewSession) {
    a.abort.abort();
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
    if (!this.same(a)) return;
    if (a.picture)
      await a.picture.prepare(
        session,
        AbortSignal.any([a.controls.signal, AbortSignal.timeout(10000)]),
      );
    if (!this.same(a)) return;
    a.draining = false;
    this.present(session.authoritative_time);
    this.publish({ session, phase: "loading" });
    this.stream(a);
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
      while (a.reporting && this.same(a))
        await new Promise<void>((resolve) => setTimeout(resolve, 5));
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
