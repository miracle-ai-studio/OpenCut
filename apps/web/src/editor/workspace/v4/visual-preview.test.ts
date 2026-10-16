import { afterEach, expect, it, vi } from "vitest";
vi.mock("./visual-preview-renderer", () => ({
  VisualPreviewRenderer: class {},
}));
import { V4VisualPreview } from "./visual-preview";
import type { VisualPreviewRenderer } from "./visual-preview-renderer";
import type { V4SessionHost } from "./session";
import type { V4AudioPreviewSession, V4AudioPreviewState } from "./types";
import { setupAudioPreview } from "./audio-preview-fixture";
const flush = async () => {
  for (let i = 0; i < 30; i++) await Promise.resolve();
};
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});
async function fixture(bitmaps = false) {
  const f = await setupAudioPreview();
  let state = f.state();
  let activeApi = f.api;
  const s = f.sessionFor({
    project_id: state.head.project.id,
    revision: state.head.revision,
    sequence_id: state.sequenceId,
    sample_rate: 48000,
    start_time: state.playhead,
    idempotency_key: "picture",
  });
  const calls: {
    session: unknown;
    signal: AbortSignal;
    point?: { x: number; y: number };
    resolve: (time: number) => void;
  }[] = [];
  const close = vi.fn();
  const released: ReturnType<typeof vi.fn>[] = [];
  const renderer = {
    close,
    render: vi.fn(
      (
        _api,
        session,
        signal,
        _profile,
        point?: { x: number; y: number },
        _draft?: Parameters<VisualPreviewRenderer["render"]>[5],
      ) =>
        new Promise((resolve) =>
          calls.push({
            session,
            signal,
            point,
            resolve: (time) => {
              const release = vi.fn();
              released.push(release);
              resolve({
                frame: {
                  plan: {
                    identity: {
                      time: {
                        ticks: String(Math.round(time * 48000)),
                        timebase: 48000,
                      },
                    },
                  },
                  time_seconds: time,
                },
                pixels: {
                  width: 1,
                  height: 1,
                  data: new Uint8ClampedArray([255, 0, 0, 255]),
                  ...(bitmaps ? { close: release } : {}),
                },
                runtime: { gpuBackend: "mock" },
                ...(point
                  ? {
                      hitTest: {
                        point,
                        hits: [{ elementId: "opaque", alpha: 255 }],
                      },
                    }
                  : {}),
              });
            },
          }),
        ),
    ),
  };
  const failure = vi.fn();
  const host = { api: () => activeApi, state: () => state } as V4SessionHost;
  const visual = new V4VisualPreview(
    host,
    failure,
    () => renderer as unknown as VisualPreviewRenderer,
  );
  const observe = (
    port: ReturnType<typeof visual.start>,
    session: V4AudioPreviewSession,
    phase: V4AudioPreviewState["phase"] = "playing",
  ) => {
    state = {
      ...state,
      audioPreview: { ...state.audioPreview, session, phase },
    };
    port.observe(state.audioPreview);
  };
  return {
    visual,
    calls,
    close,
    failure,
    s,
    observe,
    released,
    renderer,
    api: f.api,
    replaceApi: (next: typeof f.api) => { activeApi = next; },
    state: () => state,
  };
}

it("releases presented and late bitmap ownership at replacement/stop without publishing canceled pixels", async () => {
  const f = await fixture(true),
    port = f.visual.start();
  const prepare = port.prepare(f.s, new AbortController().signal);
  await flush();
  f.calls[0].resolve(0);
  await prepare;
  f.observe(port, {
    ...f.s,
    authoritative_time: { ticks: "50", timebase: 1000 },
  });
  await flush();
  port.suspend();
  expect(f.released[0]).toHaveBeenCalledTimes(1);
  f.calls[1].resolve(0.05);
  await flush();
  expect(f.released[1]).toHaveBeenCalledTimes(1);
  expect(f.visual.snapshot().pixels).toBeNull();
  port.close();
  expect(f.released[0]).toHaveBeenCalledTimes(1);
});
it("coalesces consumer observations to one in-flight frame and the latest intent; overdue pixels clear", async () => {
  const f = await fixture(),
    port = f.visual.start();
  try {
    const prepare = port.prepare(f.s, new AbortController().signal);
    await flush();
    f.calls[0].resolve(0);
    await prepare;
    const at = (n: number) => ({
      ...f.s,
      authoritative_time: { ticks: String(n), timebase: 1000 },
    });
    f.observe(port, at(50));
    await flush();
    f.observe(port, at(100));
    f.observe(port, at(200));
    f.observe(port, at(300));
    expect(f.calls).toHaveLength(2);
    expect(f.visual.snapshot().pixels).toBeNull();
    f.calls[1].resolve(0.05);
    await flush();
    expect(f.calls).toHaveLength(3);
    expect(f.visual.snapshot().pixels).toBeNull();
    f.calls[2].resolve(0.3);
    await flush();
    expect(f.visual.snapshot().time).toBe(0.3);
  } finally {
    port.close();
  }
});
it("seek cancels and drains an old frame before preparing the new generation; late pixels never publish", async () => {
  const f = await fixture(),
    port = f.visual.start();
  try {
    const first = port.prepare(f.s, new AbortController().signal);
    await flush();
    f.calls[0].resolve(0);
    await first;
    f.observe(port, {
      ...f.s,
      authoritative_time: { ticks: "50", timebase: 1000 },
    });
    await flush();
    port.suspend();
    expect(f.calls[1].signal.aborted).toBe(true);
    expect(f.visual.snapshot().pixels).toBeNull();
    const next = port.prepare(
      {
        ...f.s,
        generation: "2",
        authoritative_time: { ticks: "2", timebase: 1 },
      },
      new AbortController().signal,
    );
    await flush();
    expect(f.calls).toHaveLength(2);
    f.calls[1].resolve(0.05);
    await flush();
    expect(f.calls).toHaveLength(3);
    expect(f.visual.snapshot().pixels).toBeNull();
    f.calls[2].resolve(2);
    await next;
    expect(f.visual.snapshot()).toMatchObject({ generation: "2", time: 2 });
    expect(f.failure).not.toHaveBeenCalled();
  } finally {
    port.close();
  }
});
it("playing without fresh consumer observations removes its picture and close releases resources", async () => {
  const f = await fixture(),
    port = f.visual.start();
  const first = port.prepare(f.s, new AbortController().signal);
  await flush();
  f.calls[0].resolve(0);
  await first;
  vi.useFakeTimers();
  f.observe(port, f.s);
  await flush();
  expect(f.calls).toHaveLength(1);
  expect(f.visual.snapshot().pixels).not.toBeNull();
  vi.advanceTimersByTime(151);
  expect(f.visual.snapshot().pixels).toBeNull();
  port.close();
  expect(f.close).toHaveBeenCalled();
  expect(f.visual.snapshot().phase).toBe("idle");
});

it("fences pixels with actual Worklet consumption while server confirmation is delayed", async () => {
  const f = await fixture(),
    port = f.visual.start();
  try {
    const first = port.prepare(f.s, new AbortController().signal);
    await flush();
    f.calls[0].resolve(0);
    await first;
    f.observe(port, {
      ...f.s,
      authoritative_time: { ticks: "1", timebase: 1000 },
    });
    await flush();
    port.consume?.(f.s, "7424"); // 154.666ms, server still confirms zero.
    expect(f.visual.snapshot().pixels).toBeNull();
    f.calls[1].resolve(0);
    await flush();
    expect(f.visual.snapshot().pixels).toBeNull();
    expect(f.failure).not.toHaveBeenCalled();
  } finally {
    port.close();
  }
});

it("selects the graphic profile from the fixed scene and keeps it for paused authority checks", async () => {
  const f = await fixture();
  const sequence = f
    .state()
    .head.project.sequences.find((s) => s.id === f.s.identity.sequence_id)!;
  sequence.tracks[0].elements.push({
    enabled: true,
    content: { kind: "graphic" },
  } as never);
  sequence.captions.push({} as never);
  const port = f.visual.start();
  try {
    const first = port.prepare(f.s, new AbortController().signal);
    await flush();
    expect(f.renderer.render.mock.calls[0][3]).toBe("graphic");
    f.calls[0].resolve(0);
    await first;
    vi.useFakeTimers();
    f.observe(port, f.s, "paused");
    // Mutable UI selection must not switch the prepared session's protocol.
    sequence.tracks[0].elements.pop();
    sequence.captions.length = 0;
    vi.mocked(f.api.sdk.call).mockResolvedValue({} as never);
    await vi.advanceTimersByTimeAsync(1000);
    expect(f.api.sdk.call).toHaveBeenCalledWith(
      "opencut_visual_preview_v4_graphic_frame",
      expect.objectContaining({
        session: expect.objectContaining({ generation: f.s.generation }),
        graphics: null,
        captions: null,
      }),
      expect.anything(),
    );
  } finally {
    port.close();
  }
});

it("rejects a preview session for another revision before rendering", async () => {
  const f = await fixture(),
    port = f.visual.start();
  try {
    await expect(
      port.prepare(
        { ...f.s, identity: { ...f.s.identity, revision: "other" } },
        new AbortController().signal,
      ),
    ).rejects.toThrow("fixed project view");
    expect(f.renderer.render).not.toHaveBeenCalled();
  } finally {
    port.close();
  }
});

it("selects blend before graphics and captions and retains its protocol for paused authority checks", async () => {
  const f = await fixture();
  const sequence = f
    .state()
    .head.project.sequences.find((s) => s.id === f.s.identity.sequence_id)!;
  const element = sequence.tracks[0].elements[0];
  element.visual!.blend_mode = "multiply";
  sequence.tracks[0].elements.push({
    enabled: true,
    content: { kind: "graphic" },
  } as never);
  sequence.captions.push({} as never);
  const port = f.visual.start();
  try {
    const prepared = port.prepare(f.s, new AbortController().signal);
    await flush();
    expect(f.renderer.render.mock.calls[0][3]).toBe("blend");
    f.calls[0].resolve(0);
    await prepared;
    vi.useFakeTimers();
    f.observe(port, f.s, "paused");
    element.visual!.blend_mode = "normal";
    vi.mocked(f.api.sdk.call).mockResolvedValue({} as never);
    await vi.advanceTimersByTimeAsync(1000);
    expect(f.api.sdk.call).toHaveBeenCalledWith(
      "opencut_visual_preview_v4_blend_frame",
      expect.objectContaining({
        session: expect.objectContaining({ generation: f.s.generation }),
        graphics: null,
        captions: null,
      }),
      expect.anything(),
    );
  } finally {
    port.close();
  }
});

it("reserves simultaneous paused alpha queries before either awaits the renderer", async () => {
  const f = await fixture(true),
    port = f.visual.start();
  try {
    const prepared = port.prepare(f.s, new AbortController().signal);
    await flush();
    f.calls[0].resolve(0);
    await prepared;
    f.observe(port, f.s, "paused");
    await flush();
    const first = f.visual.hitTest({ x: 0, y: 0 }),
      second = f.visual.hitTest({ x: 0, y: 0 });
    await flush();
    expect(f.calls).toHaveLength(2);
    expect(f.calls[1].point).toEqual({ x: 0, y: 0 });
    f.calls[1].resolve(0);
    await first;
    await flush();
    expect(f.calls).toHaveLength(3);
    f.calls[2].resolve(0);
    expect((await second).hits).toEqual([{ elementId: "opaque", alpha: 255 }]);
    expect(f.released[1]).toHaveBeenCalledTimes(1);
    expect(f.released[2]).toHaveBeenCalledTimes(1);
    expect(f.visual.snapshot().phase).toBe("ready");
  } finally {
    port.close();
  }
});
it("a finishing presentation cannot erase queued query ownership or enter it concurrently", async () => {
  const f = await fixture(),
    port = f.visual.start();
  try {
    const prepared = port.prepare(f.s, new AbortController().signal);
    await flush();
    f.calls[0].resolve(0);
    await prepared;
    f.observe(
      port,
      { ...f.s, authoritative_time: { ticks: "50", timebase: 1000 } },
      "paused",
    );
    await flush();
    expect(f.calls).toHaveLength(2);
    const first = f.visual.hitTest({ x: 0, y: 0 }),
      second = f.visual.hitTest({ x: 0, y: 0 });
    f.calls[1].resolve(0.05);
    await flush();
    expect(f.calls).toHaveLength(3);
    f.observe(
      port,
      { ...f.s, authoritative_time: { ticks: "100", timebase: 1000 } },
      "paused",
    );
    await flush();
    expect(f.calls).toHaveLength(3);
    f.calls[2].resolve(0.05);
    await first;
    await flush();
    expect(f.calls).toHaveLength(4);
    f.calls[3].resolve(0.05);
    await second;
    await flush();
    expect(f.calls).toHaveLength(5);
    f.calls[4].resolve(0.1);
    await flush();
    expect(f.visual.snapshot().time).toBe(0.1);
  } finally {
    port.close();
  }
});

function transformInput(
  f: Awaited<ReturnType<typeof fixture>>,
  draftId = "draft",
) {
  const state = f.state();
  return {
    draftId,
    sequenceId: state.sequenceId,
    time: structuredClone(state.playhead),
    transaction: {
      project_id: state.head.project.id,
      expected_revision: state.head.revision,
      idempotency_key: "original-transform",
      label: "Transform selection",
      operations: [],
    },
  };
}
it("coalesces transform drafts into the existing render queue and clears only the owning draft", async () => {
  const f = await fixture(true),
    port = f.visual.start();
  try {
    const prepared = port.prepare(f.s, new AbortController().signal);
    await flush();
    f.calls[0].resolve(0);
    await prepared;
    f.observe(port, f.s, "paused");
    await flush();
    const first = f.visual
      .previewTransformDraft(transformInput(f), new AbortController().signal)
      .catch((error) => error);
    await flush();
    expect(f.calls).toHaveLength(2);
    const skipped = f.visual
      .previewTransformDraft(transformInput(f), new AbortController().signal)
      .catch((error) => error);
    const latest = f.visual.previewTransformDraft(
      transformInput(f),
      new AbortController().signal,
    );
    await flush();
    expect(f.calls).toHaveLength(2);
    f.calls[1].resolve(0);
    await flush();
    expect((await first).name).toBe("AbortError");
    expect((await skipped).name).toBe("AbortError");
    expect(f.released[1]).toHaveBeenCalledOnce();
    expect(f.calls).toHaveLength(3);
    expect(f.renderer.render.mock.calls[2][3]).toBe("scene");
    expect(f.renderer.render.mock.calls[2][5]).toMatchObject({
      draftId: "draft",
      transaction: { idempotency_key: "original-transform" },
    });
    f.calls[2].resolve(0);
    await latest;
    const pixels = f.visual.snapshot().pixels;
    f.visual.clearTransformDraft("unrelated-draft");
    expect(f.visual.snapshot().pixels).toBe(pixels);
    f.visual.clearTransformDraft("draft");
    await flush();
    expect(f.calls).toHaveLength(4);
    f.calls[3].resolve(0);
    await flush();
    expect(f.released[2]).toHaveBeenCalledOnce();
    expect(f.visual.snapshot().phase).toBe("ready");
  } finally {
    port.close();
  }
});
it("drops a late transform picture after cancellation and restores the original paused frame", async () => {
  const f = await fixture(true),
    port = f.visual.start();
  try {
    const prepared = port.prepare(f.s, new AbortController().signal);
    await flush();
    f.calls[0].resolve(0);
    await prepared;
    f.observe(port, f.s, "paused");
    await flush();
    const abort = new AbortController();
    const draft = f.visual
      .previewTransformDraft(transformInput(f), abort.signal)
      .catch((error) => error);
    await flush();
    abort.abort();
    f.calls[1].resolve(0);
    await flush();
    expect((await draft).name).toBe("AbortError");
    expect(f.released[1]).toHaveBeenCalledOnce();
    expect(f.calls).toHaveLength(3);
    f.calls[2].resolve(0);
    await flush();
    expect(f.visual.snapshot().pixels).not.toBeNull();
  } finally {
    port.close();
  }
});

it("retires a transform immediately when the consumer starts playing and discards its late pixels",async()=>{
  const f=await fixture(true),port=f.visual.start();
  try {
    const prepared=port.prepare(f.s,new AbortController().signal);
    await flush();f.calls[0].resolve(0);await prepared;
    f.observe(port,f.s,"paused");await flush();
    const draft=f.visual.previewTransformDraft(transformInput(f),new AbortController().signal).catch(error=>error);
    await flush();f.observe(port,f.s,"playing");
    expect(f.visual.snapshot().pixels).toBeNull();
    f.calls[1].resolve(0);await flush();
    expect((await draft).name).toBe("AbortError");
    expect(f.released[1]).toHaveBeenCalledOnce();
    f.calls[2].resolve(0);await flush();
    expect(f.visual.snapshot().phase).toBe("ready");
  }finally {port.close();}
});

it("rejects a transform result captured before the API connection is replaced", async () => {
  const f = await fixture(true), port = f.visual.start();
  try {
    const prepared = port.prepare(f.s, new AbortController().signal);
    await flush(); f.calls[0].resolve(0); await prepared;
    f.observe(port, f.s, "paused"); await flush();
    const draft = f.visual.previewTransformDraft(transformInput(f), new AbortController().signal).catch(error => error);
    await flush();
    const original = f.visual.snapshot().pixels;
    f.replaceApi(Object.create(f.api));
    f.calls[1].resolve(0); await flush();
    expect((await draft).name).toBe("AbortError");
    expect(f.released[1]).toHaveBeenCalledOnce();
    expect(f.visual.snapshot().pixels).toBe(original);
  } finally { port.close(); }
});

it("does not publish transform pixels after the fixed preview generation changes", async () => {
  const f = await fixture(true), port = f.visual.start();
  try {
    const prepared = port.prepare(f.s, new AbortController().signal);
    await flush(); f.calls[0].resolve(0); await prepared;
    f.observe(port, f.s, "paused"); await flush();
    const draft = f.visual.previewTransformDraft(transformInput(f), new AbortController().signal).catch(error => error);
    await flush();
    const next = { ...f.s, generation: "2" };
    f.observe(port, next, "paused");
    f.calls[1].resolve(0); await flush();
    expect((await draft).name).toBe("AbortError");
    expect(f.released[1]).toHaveBeenCalledOnce();
    expect(f.visual.snapshot().pixels).toBeNull();
  } finally { port.close(); }
});
