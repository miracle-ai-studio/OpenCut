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
    resolve: (time: number) => void;
  }[] = [];
  const close = vi.fn();
  const released: ReturnType<typeof vi.fn>[] = [];
  const renderer = {
    close,
    render: vi.fn(
      (_api, session, signal) =>
        new Promise((resolve) =>
          calls.push({
            session,
            signal,
            resolve: (time) => {
              const release = vi.fn(); released.push(release);
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
              });
            },
          }),
        ),
    ),
  };
  const failure = vi.fn();
  const host = { api: () => f.api, state: () => state } as V4SessionHost;
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
  return { visual, calls, close, failure, s, observe, released };
}

it("releases presented and late bitmap ownership at replacement/stop without publishing canceled pixels", async () => {
  const f = await fixture(true), port = f.visual.start();
  const prepare = port.prepare(f.s, new AbortController().signal);
  await flush(); f.calls[0].resolve(0); await prepare;
  f.observe(port, { ...f.s, authoritative_time: { ticks: "50", timebase: 1000 } });
  await flush();
  port.suspend();
  expect(f.released[0]).toHaveBeenCalledTimes(1);
  f.calls[1].resolve(.05); await flush();
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
  const f = await fixture(), port = f.visual.start();
  try {
    const first = port.prepare(f.s, new AbortController().signal);
    await flush(); f.calls[0].resolve(0); await first;
    f.observe(port, { ...f.s, authoritative_time: { ticks: "1", timebase: 1000 } });
    await flush();
    port.consume?.(f.s, "7424"); // 154.666ms, server still confirms zero.
    expect(f.visual.snapshot().pixels).toBeNull();
    f.calls[1].resolve(0);
    await flush();
    expect(f.visual.snapshot().pixels).toBeNull();
    expect(f.failure).not.toHaveBeenCalled();
  } finally { port.close(); }
});
