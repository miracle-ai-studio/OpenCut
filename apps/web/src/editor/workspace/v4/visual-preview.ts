import { OpenCutApiError } from "../../../lib/editor-sdk";
import type { V4SessionHost } from "./session";
import type { V4AudioPreviewSession, V4AudioPreviewState } from "./types";
import type { PicturePresentation } from "./audio-preview";
import { VisualPreviewRenderer } from "./visual-preview-renderer";
export type VisualPreviewView = {
  phase: "idle" | "loading" | "ready" | "failed";
  pixels: ImageData | null;
  time: number | null;
  generation: string | null;
  error: string | null;
  runtime: string | null;
};
const empty = (): VisualPreviewView => ({
  phase: "idle",
  pixels: null,
  time: null,
  generation: null,
  error: null,
  runtime: null,
});
const token = (s: V4AudioPreviewSession) =>
  JSON.stringify([
    s.identity,
    s.session_id,
    s.generation,
    s.overlay_epoch,
    s.owner_id,
    s.lease_id,
  ]);
/** Latest confirmed consumer time, one serial renderer and one pending intent.
 * No wall-clock playhead, durable frame jobs or unbounded presentation queue. */
export class V4VisualPreview {
  private view: VisualPreviewView = empty();
  private listeners = new Set<() => void>();
  private epoch = 0;
  private renderer: VisualPreviewRenderer | null = null;
  private abort = new AbortController();
  private desired: V4AudioPreviewSession | null = null;
  private presentedTime: V4AudioPreviewSession["authoritative_time"] | null =
    null;
  private currentToken: string | null = null;
  private consumedTime: V4AudioPreviewSession["authoritative_time"] | null = null;
  private running: Promise<void> | null = null;
  private authorityTimer: ReturnType<typeof setInterval> | null = null;
  private watchdog: ReturnType<typeof setTimeout> | null = null;
  constructor(
    private readonly host: V4SessionHost,
    private readonly failure: (e: Error) => void,
    private readonly factory: () => VisualPreviewRenderer = () =>
      new VisualPreviewRenderer(),
  ) {}
  snapshot = () => this.view;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(patch: Partial<VisualPreviewView>) {
    this.view = { ...this.view, ...patch };
    for (const listener of this.listeners) listener();
  }
  private clear() {
    if (this.authorityTimer) clearInterval(this.authorityTimer);
    this.authorityTimer = null;
    this.presentedTime = null;
    this.abort.abort();
    this.abort = new AbortController();
    this.desired = null;
    this.currentToken = null;
    this.consumedTime = null;
    if (this.watchdog) clearTimeout(this.watchdog);
    this.watchdog = null;
    this.publish({ pixels: null, time: null, generation: null });
  }
  private stop() {
    this.clear();
    this.renderer?.close();
    this.renderer = null;
    this.publish(empty());
  }
  start(): PicturePresentation {
    const paused = this.host.state()?.audioPreview;
    const reuse =
      paused?.phase === "paused" &&
      paused.session &&
      this.currentToken === token(paused.session);
    this.epoch++;
    const epoch = this.epoch;
    if (reuse) {
      this.clear();
      this.publish(empty());
    } else this.stop();
    const valid = () => epoch === this.epoch;
    return {
      prepare: async (session, signal) => {
        if (!valid()) throw new DOMException("Preview replaced", "AbortError");
        this.clear();
        this.currentToken = token(session);
        this.publish({ phase: "loading", error: null });
        // Drain old generation before replacing its GPU resources.
        await this.running?.catch(() => {});
        signal.throwIfAborted();
        if (!valid()) throw new DOMException("Preview replaced", "AbortError");
        await this.render(
          session,
          AbortSignal.any([signal, this.abort.signal]),
          epoch,
        );
      },
      suspend: () => {
        if (valid()) this.clear();
      },
      consume: (session, frames) => {
        if (!valid() || this.currentToken !== token(session)) return;
        // Worklet observation only fences presentation. It never compiles IR
        // or substitutes for Rust's confirmed consumer time.
        this.consumedTime = {
          ticks: (BigInt(session.actual_start_sample) + BigInt(frames)).toString(),
          timebase: session.identity.sample_rate,
        };
        if (this.presentedTime) {
          const c = this.consumedTime, t = this.presentedTime;
          const delta = BigInt(c.ticks) * BigInt(t.timebase) - BigInt(t.ticks) * BigInt(c.timebase);
          if (delta < 0n || delta * 1000n > 150n * BigInt(c.timebase) * BigInt(t.timebase))
            this.publish({ pixels: null, phase: "loading" });
        }
      },
      observe: (state) => {
        if (valid()) this.observe(state, epoch);
      },
      close: () => {
        if (valid()) this.stop();
      },
    };
  }
  private observe(state: V4AudioPreviewState, epoch: number) {
    const session = state.session;
    if (state.phase === "failed" || state.phase === "unconfirmed") {
      this.stop();
      return;
    }
    if (!session) {
      if (state.phase !== "loading") this.stop();
      return;
    }
    // Loading stops old presentation synchronously (seek/gain replacement).
    if (state.phase === "loading" && this.currentToken !== token(session)) {
      this.clear();
      return;
    }
    if (this.currentToken !== token(session)) {
      this.clear();
      this.currentToken = token(session);
    }
    if (!["playing", "stalled", "paused", "ended"].includes(state.phase))
      return;
    if (state.phase === "paused" && !this.authorityTimer) {
      const api = this.host.api(),
        identity = token(session);
      let checking = false;
      this.authorityTimer = setInterval(() => {
        if (checking || !api) return;
        checking = true;
        void api.v4
          .visualPreviewFrame(
            {
              session: {
                session_id: session.session_id,
                generation: session.generation,
                overlay_epoch: session.overlay_epoch,
                owner_id: session.owner_id,
                lease_id: session.lease_id,
              },
              time: null,
              decoded: null,
            },
            this.abort.signal,
          )
          .catch((error) => {
            if (epoch === this.epoch && this.currentToken === identity) {
              this.epoch++;
              this.stop();
              this.publish({
                phase: "failed",
                error: error instanceof Error ? error.message : String(error),
              });
            }
          })
          .finally(() => {
            checking = false;
          });
      }, 1000);
    }
    if (state.phase !== "paused" && this.authorityTimer) {
      clearInterval(this.authorityTimer);
      this.authorityTimer = null;
    }
    this.desired = session;
    if (this.presentedTime) {
      const c = session.authoritative_time,
        t = this.presentedTime;
      const delta =
        BigInt(c.ticks) * BigInt(t.timebase) -
        BigInt(t.ticks) * BigInt(c.timebase);
      if (
        delta < 0n ||
        delta * 1000n > 150n * BigInt(c.timebase) * BigInt(t.timebase)
      )
        this.publish({ pixels: null, phase: "loading" });
    }
    if (this.watchdog) clearTimeout(this.watchdog);
    // A stalled/paused consumer does not advance; playing without a fresh
    // observation loses its picture instead of displaying a misleading clock.
    if (state.phase === "playing")
      this.watchdog = setTimeout(
        () => this.publish({ pixels: null, phase: "loading" }),
        150,
      );
    if (!this.running) {
      this.running = this.drain(epoch).finally(() => {
        this.running = null;
        const state = this.host.state()?.audioPreview;
        if (this.desired && state && epoch === this.epoch)
          this.observe(state, epoch);
      });
    }
  }
  private async drain(epoch: number) {
    while (this.desired && epoch === this.epoch) {
      const session = this.desired;
      this.desired = null;
      const signal = this.abort.signal;
      try {
        await this.render(session, signal, epoch);
      } catch (error) {
        if (epoch !== this.epoch || signal.aborted) return;
        if (
          error instanceof OpenCutApiError &&
          ["PREVIEW_FRAME_STALE", "PREVIEW_GENERATION_CONFLICT"].includes(
            error.code,
          )
        )
          continue;
        const e = error instanceof Error ? error : new Error(String(error));
        this.epoch++;
        this.clear();
        this.renderer?.close();
        this.renderer = null;
        this.publish({ phase: "failed", error: e.message });
        this.failure(e);
        return;
      }
    }
  }
  private async render(
    session: V4AudioPreviewSession,
    signal: AbortSignal,
    epoch: number,
  ) {
    const api = this.host.api();
    if (!api) throw new Error("Preview service disconnected");
    const identity = token(session);
    this.renderer ??= this.factory();
    const result = await this.renderer.render(
      api,
      {
        session_id: session.session_id,
        generation: session.generation,
        overlay_epoch: session.overlay_epoch,
        owner_id: session.owner_id,
        lease_id: session.lease_id,
      },
      signal,
    );
    signal.throwIfAborted();
    if (
      epoch !== this.epoch ||
      this.currentToken !== identity ||
      api !== this.host.api()
    )
      return;
    const latest = this.host.state()?.audioPreview.session;
    // Compare rational public Times exactly, never divide ticks to evaluate IR.
    if (latest && token(latest) === identity) {
      const c = this.consumedTime ?? latest.authoritative_time,
        t = result.frame.plan.identity.time;
      // Leave one main-thread presentation quantum inside the 150ms gate.
      const maxLag = this.consumedTime ? 125n : 150n;
      const delta =
        BigInt(c.ticks) * BigInt(t.timebase) -
        BigInt(t.ticks) * BigInt(c.timebase);
      if (
        delta < 0n ||
        delta * 1000n > maxLag * BigInt(c.timebase) * BigInt(t.timebase)
      ) {
        this.publish({ pixels: null, phase: "loading" });
        return;
      }
    }
    this.presentedTime = result.frame.plan.identity.time;
    this.publish({
      phase: "ready",
      runtime: JSON.stringify(result.runtime),
      pixels: result.pixels,
      time: result.frame.time_seconds,
      generation: session.generation,
      error: null,
    });
  }
}
