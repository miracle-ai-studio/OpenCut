import type { ToolInput } from "../../../lib/editor-sdk";
import { OpenCutApiError } from "../../../lib/editor-sdk";
import type { V4SessionHost } from "./session";
import type { V4AudioPreviewSession, V4AudioPreviewState } from "./types";
import type { PicturePresentation } from "./audio-preview";
import {
  VisualPreviewRenderer,
  type VisualPreviewProfile,
} from "./visual-preview-renderer";
import { sceneVisualProfile } from "./visual-profile";
type Renderer = Pick<VisualPreviewRenderer, "render" | "close">;
export type TransformDraftInput = {
  gestureIntent?: ToolInput<"opencut_visual_preview_v4_transform_draft_v2_frame">["gesture_intent"];
  draftId: string;
  transaction: ToolInput<"opencut_edit_v4_apply">;
  sequenceId: string;
  time: V4AudioPreviewSession["authoritative_time"];
};
type PendingTransformDraft = {
  input: TransformDraftInput;
  api: NonNullable<ReturnType<V4SessionHost["api"]>>;
  session: V4AudioPreviewSession;
  check(): void;
  signal: AbortSignal;
  resolve(): void;
  reject(error: unknown): void;
};

export type VisualPreviewView = {
  phase: "idle" | "loading" | "ready" | "failed";
  pixels: ImageData | ImageBitmap | null;
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
  private renderer: Renderer | null = null;
  private profile: VisualPreviewProfile = "text";
  private abort = new AbortController();
  private desired: V4AudioPreviewSession | null = null;
  private presentedTime: V4AudioPreviewSession["authoritative_time"] | null =
    null;
  private currentToken: string | null = null;
  private consumedTime: V4AudioPreviewSession["authoritative_time"] | null =
    null;
  private running: Promise<void> | null = null;
  private transformDraftId: string | null = null;
  private transformPending: PendingTransformDraft | null = null;
  private transformDraining = false;
  private transformAbort: AbortController | null = null;
  private transformSerial = 0;
  private transformSignalCleanup: (() => void) | null = null;
  private authorityTimer: ReturnType<typeof setInterval> | null = null;
  private watchdog: ReturnType<typeof setTimeout> | null = null;
  constructor(
    private readonly host: V4SessionHost,
    private readonly failure: (e: Error) => void,
    private readonly factory: () => Renderer = () =>
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
    if (
      "pixels" in patch &&
      this.view.pixels &&
      patch.pixels !== this.view.pixels &&
      "close" in this.view.pixels
    )
      this.view.pixels.close();
    this.view = { ...this.view, ...patch };
    for (const listener of this.listeners) listener();
  }
  private clear() {
    this.dropTransformDraft();
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
  private dropTransformDraft() {
    this.transformSerial++;
    this.transformSignalCleanup?.();
    this.transformSignalCleanup = null;
    this.transformDraftId = null;
    this.transformAbort?.abort();
    this.transformAbort = null;
    this.transformPending?.reject(
      new DOMException("Transform draft replaced", "AbortError"),
    );
    this.transformPending = null;
  }
  /** One running read-only draft and one latest request share the original GPU
   * queue. Every picture is compiled by Rust from a transform-only transaction. */
  previewTransformDraft(
    input: TransformDraftInput,
    signal: AbortSignal,
  ): Promise<void> {
    const api = this.host.api(),
      state = this.host.state(),
      session = state?.audioPreview.session;
    if (
      !api ||
      !state ||
      state.historical ||
      state.audioPreview.phase !== "paused" ||
      !session ||
      this.currentToken !== token(session) ||
      this.view.phase !== "ready" ||
      state.head.project.id !== input.transaction.project_id ||
      state.head.revision !== input.transaction.expected_revision ||
      state.sequenceId !== input.sequenceId ||
      JSON.stringify(state.playhead) !== JSON.stringify(input.time)
    )
      return Promise.reject(
        new Error(
          "Pause the live preview on the original scene before transforming content.",
        ),
      );
    const captured = structuredClone(input),
      epoch = this.epoch,
      identity = api.connectionIdentity,
      expectedToken = token(session),
      serial = ++this.transformSerial;
    const check = () => {
      signal.throwIfAborted();
      const current = this.host.state(),
        active = current?.audioPreview.session;
      if (
        serial !== this.transformSerial ||
        epoch !== this.epoch ||
        this.transformDraftId !== captured.draftId ||
        api !== this.host.api() ||
        identity !== api.connectionIdentity ||
        current?.historical ||
        current?.audioPreview.phase !== "paused" ||
        !active ||
        token(active) !== expectedToken ||
        current.head.project.id !== captured.transaction.project_id ||
        current.head.revision !== captured.transaction.expected_revision ||
        current.sequenceId !== captured.sequenceId ||
        JSON.stringify(current.playhead) !== JSON.stringify(captured.time)
      )
        throw new DOMException("Transform draft target changed", "AbortError");
    };
    this.transformDraftId = captured.draftId;
    this.desired = null;
    this.transformAbort?.abort();
    this.transformPending?.reject(
      new DOMException("Transform draft replaced", "AbortError"),
    );
    const result = new Promise<void>((resolve, reject) => {
      this.transformPending = {
        api,
        session,
        input: captured,
        check,
        signal,
        resolve,
        reject,
      };
    });
    this.transformSignalCleanup?.();
    const onAbort = () => {
      if (serial === this.transformSerial)
        this.clearTransformDraft(captured.draftId);
    };
    signal.addEventListener("abort", onAbort, { once: true });
    this.transformSignalCleanup = () =>
      signal.removeEventListener("abort", onAbort);
    if (signal.aborted) onAbort();
    if (!this.transformDraining) {
      this.transformDraining = true;
      void this.serialize(async () => {
        try {
          while (this.transformPending) {
            const pending = this.transformPending;
            this.transformPending = null;
            const abort = new AbortController();
            this.transformAbort = abort;
            let picture: Awaited<ReturnType<Renderer["render"]>> | null = null;
            try {
              pending.check();
              const renderer = this.renderer;
              if (!renderer) throw new Error("Live preview is unavailable.");
              picture = await renderer.render(
                pending.api,
                {
                  session_id: pending.session.session_id,
                  generation: pending.session.generation,
                  overlay_epoch: pending.session.overlay_epoch,
                  owner_id: pending.session.owner_id,
                  lease_id: pending.session.lease_id,
                },
                // Let the current HTTP phase finish. Pointer cancellation
                // prevents the next phase/publication; it does not pretend to
                // have stopped the host's blocking compile.
                this.abort.signal,
                "scene",
                undefined,
                {
                  draftId: pending.input.draftId,
                  transaction: pending.input.transaction,
                  gestureIntent: pending.input.gestureIntent,
                  time: pending.input.time,
                  intentSignal: AbortSignal.any([pending.signal, abort.signal]),
                },
              );
              pending.check();
              if (
                BigInt(picture.frame.plan.identity.time.ticks) *
                  BigInt(pending.input.time.timebase) !==
                BigInt(pending.input.time.ticks) *
                  BigInt(picture.frame.plan.identity.time.timebase)
              )
                throw new Error(
                  "Transform draft returned another paused time.",
                );
              this.publish({
                pixels: picture.pixels,
                phase: "ready",
                time: picture.frame.time_seconds,
                generation: pending.session.generation,
                runtime: JSON.stringify(picture.runtime),
                error: null,
              });
              picture = null;
              pending.resolve();
            } catch (error) {
              pending.reject(error);
            } finally {
              if (picture && "close" in picture.pixels) picture.pixels.close();
              if (this.transformAbort === abort) this.transformAbort = null;
            }
          }
        } finally {
          this.transformDraining = false;
        }
      });
    }
    return result;
  }
  clearTransformDraft(draftId: string): void {
    if (this.transformDraftId !== draftId) return;
    this.dropTransformDraft();
    this.presentedTime = null;
    const state = this.host.state()?.audioPreview;
    if (state?.phase === "paused" && state.session) {
      this.desired = state.session;
      if (!this.running) void this.serialize(() => this.drain(this.epoch));
    }
  }
  /** An explicit paused-frame query copies one actual post-mask alpha texel
   * per layer. It never infers opacity from the opaque finished composition. */
  async hitTest(point: { x: number; y: number }) {
    const api = this.host.api(),
      state = this.host.state(),
      session = state?.audioPreview.session,
      epoch = this.epoch;
    if (
      this.transformDraftId ||
      !api ||
      !state ||
      state.audioPreview.phase !== "paused" ||
      !session ||
      this.currentToken !== token(session) ||
      this.view.phase !== "ready"
    )
      throw new Error(
        "Pause the live preview before selecting picture content.",
      );
    const expectedToken = token(session),
      snapshot = state.historical ?? state.head;
    const check = () => {
      const current = this.host.state(),
        active = current?.audioPreview.session;
      if (
        this.transformDraftId !== null ||
        epoch !== this.epoch ||
        api !== this.host.api() ||
        current?.audioPreview.phase !== "paused" ||
        !active ||
        token(active) !== expectedToken ||
        current.head.project.id !== state.head.project.id ||
        (current.historical ?? current.head).revision !== snapshot.revision ||
        current.sequenceId !== state.sequenceId ||
        JSON.stringify(current.playhead) !== JSON.stringify(state.playhead)
      )
        throw new DOMException("Canvas hit query replaced", "AbortError");
    };
    return this.serialize(async () => {
      check();
      this.abort.signal.throwIfAborted();
      const renderer = this.renderer;
      if (!renderer) throw new Error("Live preview is unavailable.");
      const result = await renderer.render(
        api,
        {
          session_id: session.session_id,
          generation: session.generation,
          overlay_epoch: session.overlay_epoch,
          owner_id: session.owner_id,
          lease_id: session.lease_id,
        },
        this.abort.signal,
        this.profile,
        point,
      );
      try {
        check();
        const observation = result.hitTest;
        if (
          !observation ||
          observation.point.x !== point.x ||
          observation.point.y !== point.y
        )
          throw new Error(
            "The renderer omitted the requested content hit observation.",
          );
        return {
          projectId: snapshot.project.id,
          revision: snapshot.revision,
          sequenceId: state.sequenceId,
          time: structuredClone(result.frame.plan.identity.time),
          hits: observation.hits,
        };
      } finally {
        if ("close" in result.pixels) result.pixels.close();
      }
    });
  }
  /** Reserve the renderer synchronously. Waiting before reserving permits two
   * paused queries (or a query and a replacement) to enter the same GPU realm. */
  private serialize<T>(work: () => Promise<T>): Promise<T> {
    const previous = this.running;
    const task = (async () => {
      await previous;
      return work();
    })();
    const reservation = task.then(
      () => {},
      () => {},
    );
    this.running = reservation;
    void reservation.then(() => {
      if (this.running !== reservation) return;
      this.running = null;
      const state = this.host.state()?.audioPreview;
      if (this.desired && state) this.observe(state, this.epoch);
    });
    return task;
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
        const state = this.host.state(),
          snapshot = state && (state.historical ?? state.head);
        if (
          !snapshot ||
          snapshot.project.id !== session.identity.project_id ||
          snapshot.revision !== session.identity.revision
        )
          throw new Error(
            "Preview session differs from the fixed project view",
          );
        const sequence = snapshot.project.sequences.find(
          (s) => s.id === session.identity.sequence_id,
        );
        if (!sequence)
          throw new Error(
            "Preview scene is unavailable in the fixed project view",
          );
        this.profile = sceneVisualProfile(
          sequence,
          "visible",
          snapshot.project.settings,
          snapshot.project.fonts,
        );
        this.publish({ phase: "loading", error: null });
        // Drain old generation before replacing its GPU resources.
        const renderSignal = AbortSignal.any([signal, this.abort.signal]);
        await this.serialize(async () => {
          renderSignal.throwIfAborted();
          if (!valid())
            throw new DOMException("Preview replaced", "AbortError");
          await this.render(session, renderSignal, epoch);
        });
      },
      suspend: () => {
        if (valid()) this.clear();
      },
      consume: (session, frames) => {
        if (!valid() || this.currentToken !== token(session)) return;
        // Worklet observation only fences presentation. It never compiles IR
        // or substitutes for Rust's confirmed consumer time.
        this.consumedTime = {
          ticks: (
            BigInt(session.actual_start_sample) + BigInt(frames)
          ).toString(),
          timebase: session.identity.sample_rate,
        };
        if (this.presentedTime) {
          const c = this.consumedTime,
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
    if (this.transformDraftId && state.phase !== "paused") {
      this.dropTransformDraft();
      this.presentedTime = null;
      this.publish({ pixels: null, phase: "loading" });
    }
    if (state.phase === "paused" && !this.authorityTimer) {
      const api = this.host.api(),
        identity = token(session);
      let checking = false;
      this.authorityTimer = setInterval(() => {
        if (checking || !api) return;
        checking = true;
        const input = {
          session: {
            session_id: session.session_id,
            generation: session.generation,
            overlay_epoch: session.overlay_epoch,
            owner_id: session.owner_id,
            lease_id: session.lease_id,
          },
          time: null,
          decoded: null,
          rasters: null,
        };
        const authority =
          this.profile === "scene"
            ? api.sdk.call(
                "opencut_visual_preview_v4_scene_frame",
                { ...input, captions: null, graphics: null },
                { signal: this.abort.signal },
              )
            : this.profile === "mask"
              ? api.sdk.call(
                  "opencut_visual_preview_v4_mask_frame",
                  { ...input, captions: null, graphics: null },
                  { signal: this.abort.signal },
                )
              : this.profile === "blend"
                ? api.sdk.call(
                    "opencut_visual_preview_v4_blend_frame",
                    { ...input, captions: null, graphics: null },
                    { signal: this.abort.signal },
                  )
                : this.profile === "graphic"
                  ? api.sdk.call(
                      "opencut_visual_preview_v4_graphic_frame",
                      { ...input, captions: null, graphics: null },
                      { signal: this.abort.signal },
                    )
                  : this.profile === "caption"
                    ? api.sdk.call(
                        "opencut_visual_preview_v4_caption_frame",
                        { ...input, captions: null },
                        { signal: this.abort.signal },
                      )
                    : api.v4.visualPreviewTextFrame(input, this.abort.signal);
        void authority
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
    if (this.transformDraftId && state.phase === "paused") return;
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
      void this.serialize(() => this.drain(epoch));
    }
  }
  private async drain(epoch: number) {
    while (this.desired && epoch === this.epoch && !this.transformDraftId) {
      const session = this.desired;
      this.desired = null;
      const shown = this.presentedTime,
        requested = session.authoritative_time;
      // A consumer report may renew the lease without advancing the cursor.
      // Re-executing that same confirmed frame wastes decode/GPU and FIFO time.
      if (
        shown &&
        BigInt(requested.ticks) * BigInt(shown.timebase) <=
          BigInt(shown.ticks) * BigInt(requested.timebase)
      )
        continue;
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
      this.profile,
    );
    const discard = () => {
      if ("close" in result.pixels) result.pixels.close();
    };
    if (signal.aborted) {
      discard();
      signal.throwIfAborted();
    }
    if (
      epoch !== this.epoch ||
      this.currentToken !== identity ||
      api !== this.host.api()
    ) {
      discard();
      return;
    }
    if (this.transformDraftId) {
      discard();
      return;
    }
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
        discard();
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
