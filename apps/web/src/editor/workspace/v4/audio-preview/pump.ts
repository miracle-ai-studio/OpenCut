import type { V4AudioPreviewControl, V4AudioPreviewSession } from "../types";
import type { AudioOutputProgress } from "./output";
import { readPreviewPcm } from "./transport";

export type PumpTransport = {
  open(session: V4AudioPreviewSession, signal: AbortSignal): Promise<Response>;
  control(
    input: V4AudioPreviewControl,
    signal: AbortSignal,
  ): Promise<V4AudioPreviewSession>;
};
export type PumpOutput = {
  waitForCredit(signal: AbortSignal): Promise<void>;
  push(pcm: Float32Array, signal: AbortSignal): Promise<void>;
  finish?(): void;
};
export interface PreviewPump {
  start(session: V4AudioPreviewSession): void;
  observe(progress: AudioOutputProgress): void;
  completed(): void;
  freeze(): void;
  reportStopped(frames: string): Promise<V4AudioPreviewSession | undefined>;
  abort(): void;
}
import { checkPreviewSession, counter } from "./session-identity";
const CONTROL_TIMEOUT_MS = 3000;
// One more serialized report/renewal attempt plus scheduling margin. Subtract
// the whole report RTT: the server's lease snapshot can predate its response.
const RENEWAL_RESERVE_MS = CONTROL_TIMEOUT_MS + 2000;
/** Shared stream/credit delegate; no project time, source clock or DSP work. */
export class PreviewConsumptionPump implements PreviewPump {
  private session: V4AudioPreviewSession | null = null;
  private progress: AudioOutputProgress = {
    consumedFrames: "0",
    stalled: true,
    contextTime: 0,
  };
  private sequence = 0n;
  private timer: ReturnType<typeof setInterval> | null = null;
  private reporting: Promise<V4AudioPreviewSession | undefined> | null = null;
  private pendingCompletion = false;
  private reportingActive = false;
  private draining = false;
  private heartbeatAt = performance.now();
  private readonly streamAbort = new AbortController();
  private readonly controlsAbort = new AbortController();
  constructor(
    private readonly transport: PumpTransport,
    private readonly output: PumpOutput,
    private readonly live: () => boolean,
    private readonly updated: (
      s: V4AudioPreviewSession,
      p: AudioOutputProgress,
    ) => void,
    private readonly failure: (error: unknown) => void,
  ) {}
  private current() {
    return this.live() && !this.controlsAbort.signal.aborted;
  }
  observe(progress: AudioOutputProgress) {
    if (!counter(progress.consumedFrames))
      throw new Error("Invalid device consumed cursor.");
    if (!this.draining && this.current()) this.progress = progress;
  }
  completed() {
    if (this.draining || !this.current()) return;
    if (this.reportingActive) this.pendingCompletion = true;
    else void this.report().catch(() => {});
  }
  start(session: V4AudioPreviewSession) {
    if (this.session) throw new Error("Audio data pump already owns a stream.");
    checkPreviewSession(
      session,
      session.identity,
      session.identity.sample_rate,
    );
    if (!this.current()) throw new Error("Audio data pump has stopped.");
    this.session = session;
    this.timer = setInterval(() => {
      void this.report().catch(() => {});
    }, 50);
    void this.read(session);
  }
  private async read(session: V4AudioPreviewSession) {
    const signal = this.streamAbort.signal;
    try {
      const response = await this.transport.open(session, signal);
      if (!this.current() || signal.aborted) {
        await response.body?.cancel();
        return;
      }
      const blocks = readPreviewPcm(response, session, signal);
      try {
        for (;;) {
          await this.output.waitForCredit(signal);
          const { value, done } = await blocks.next();
          if (done) {
            if (this.current()) this.output.finish?.();
            break;
          }
          if (!this.current()) return;
          await this.output.push(value, signal);
        }
      } finally {
        await blocks.return(undefined);
      }
    } catch (error) {
      if (this.current() && !this.draining && !signal.aborted)
        this.failure(error);
    }
  }
  freeze() {
    this.draining = true;
    this.pendingCompletion = false;
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }
  async reportStopped(frames: string) {
    if (!counter(frames)) throw new Error("Invalid final device cursor.");
    this.freeze();
    while (this.reporting) await this.reporting;
    return this.report(frames);
  }
  private check(s: V4AudioPreviewSession, previous: V4AudioPreviewSession) {
    checkPreviewSession(
      s,
      previous.identity,
      previous.identity.sample_rate,
      previous,
    );
  }
  private report(stopped?: string): Promise<V4AudioPreviewSession | undefined> {
    if (
      !this.current() ||
      !this.session ||
      this.reportingActive ||
      (this.draining && stopped === undefined)
    )
      return Promise.resolve(undefined);
    this.pendingCompletion = false;
    const previous = this.session;
    const command = (
      command: V4AudioPreviewControl["command"],
    ): V4AudioPreviewControl => ({
      session_id: previous.session_id,
      generation: previous.generation,
      owner_id: previous.owner_id,
      lease_id: previous.lease_id,
      control_id: crypto.randomUUID(),
      command,
    });
    this.reportingActive = true;
    const task = (async () => {
      try {
        const began = performance.now();
        const signal = AbortSignal.any([
          this.controlsAbort.signal,
          AbortSignal.timeout(CONTROL_TIMEOUT_MS),
        ]);
        const session = await this.transport.control(
          command({
            kind: "report_consumed",
            sequence: (++this.sequence).toString(),
            consumed_frames: stopped ?? this.progress.consumedFrames,
            playing: stopped === undefined,
            stalled: stopped === undefined && this.progress.stalled,
          }),
          signal,
        );
        this.check(session, previous);
        if (
          !this.current() ||
          session.generation !== previous.generation ||
          session.overlay_epoch !== previous.overlay_epoch
        )
          return;
        this.session = session;
        if (session.state === "failed" || session.state === "stopped")
          throw new Error("Scene audio is no longer available.");
        this.updated(session, this.progress);
        const now = performance.now();
        const leaseUrgent =
          BigInt(session.lease_remaining_ms) <=
          BigInt(Math.ceil(Math.max(0, now - began)) + RENEWAL_RESERVE_MS);
        if (
          this.current() &&
          !this.draining &&
          (leaseUrgent || now - this.heartbeatAt >= 500) &&
          (!this.pendingCompletion || leaseUrgent)
        ) {
          const heartbeat = await this.transport.control(
            command({ kind: "heartbeat" }),
            signal,
          );
          this.check(heartbeat, session);
          if (
            !this.current() ||
            this.draining ||
            heartbeat.generation !== session.generation ||
            heartbeat.overlay_epoch !== session.overlay_epoch
          )
            return;
          this.heartbeatAt = performance.now();
        }
        return session;
      } catch (error) {
        if (this.current()) this.failure(error);
        throw error;
      } finally {
        this.reportingActive = false;
        this.reporting = null;
        if (this.pendingCompletion && this.current() && !this.draining)
          void this.report().catch(() => {});
      }
    })();
    if (this.reportingActive) this.reporting = task;
    return task;
  }
  abort() {
    this.freeze();
    this.streamAbort.abort();
    this.controlsAbort.abort();
  }
}
