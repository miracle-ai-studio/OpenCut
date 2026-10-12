import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { PreviewAudioOutput } from "./output";

vi.mock("../audio-preview-mastering", () => ({
  createClassicMasteringChain: () => ({ input: {}, disconnect: vi.fn() }),
}));
vi.mock("./worklet.ts?worker&url", () => ({ default: "/worklet.js" }));
let rate = 48000;
class Device {
  sampleRate = rate;
  state = "running";
  currentTime = 500;
  destination = {};
  onstatechange: (() => void) | null = null;
  audioWorklet = { addModule: vi.fn(async () => {}) };
  resume = vi.fn(async () => {});
  close = vi.fn(async () => {
    this.state = "closed";
  });
}
class Node {
  static latest: Node;
  onprocessorerror: (() => void) | null = null;
  connect = vi.fn();
  disconnect = vi.fn();
  port = {
    onmessage: null as ((event: MessageEvent) => void) | null,
    close: vi.fn(),
    start: vi.fn(),
    postMessage: vi.fn((message: { kind: string; token?: string }) => {
      if (message.kind === "stop")
        queueMicrotask(() =>
          this.emit({
            kind: "stopped",
            token: "generation",
            consumedFrames: "128",
          }),
        );
    }),
  };
  constructor() {
    Node.latest = this;
  }
  emit(data: unknown) {
    this.port.onmessage?.({ data } as MessageEvent);
  }
}
beforeEach(() => {
  rate = 48000;
  vi.stubGlobal("AudioContext", Device);
  vi.stubGlobal("AudioWorkletNode", Node);
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

it("keeps two transfer slots until the actual processor completes a block and ignores stale progress", async () => {
  const progress = vi.fn(),
    completedBlock = vi.fn(),
    output = new PreviewAudioOutput("generation", 48000, progress, vi.fn(), completedBlock);
  const abort = new AbortController();
  try {
    await output.ready;
    await output.push(new Float32Array(8), abort.signal);
    await output.push(new Float32Array(8), abort.signal);
    let released = false;
    const third = output.push(new Float32Array(8), abort.signal).then(() => {
      released = true;
    });
    Node.latest.emit({
      kind: "progress",
      token: "old",
      completedBlocks: 1,
      consumedFrames: "4",
      copied: 4,
    });
    await Promise.resolve();
    expect(released).toBe(false);
    expect(progress).not.toHaveBeenCalled();
    expect(completedBlock).not.toHaveBeenCalled();
    Node.latest.emit({ kind: "progress", token: "generation", completedBlocks: 0,
      consumedFrames: "2", copied: 2 });
    expect(completedBlock).not.toHaveBeenCalled();
    Node.latest.emit({
      kind: "progress",
      token: "generation",
      completedBlocks: 1,
      consumedFrames: "4",
      copied: 4,
    });
    await third;
    expect(completedBlock).toHaveBeenCalledOnce();
    expect(progress).toHaveBeenLastCalledWith({
      consumedFrames: "4",
      stalled: false,
      contextTime: 500,
    });
    expect(
      Node.latest.port.postMessage.mock.calls.filter(
        ([m]) => m.kind === "block",
      ),
    ).toHaveLength(3);
    Node.latest.emit({ kind: "progress", token: "generation", completedBlocks: 1,
      consumedFrames: "6", copied: 2 });
    expect(completedBlock).toHaveBeenCalledOnce();
    output.silence();
    Node.latest.emit({ kind: "progress", token: "generation", completedBlocks: 2,
      consumedFrames: "8", copied: 2 });
    expect(completedBlock).toHaveBeenCalledOnce();
    expect(Node.latest.disconnect).toHaveBeenCalledOnce();
    const stop = output.stop();
    expect(await stop).toBe("128");
    expect(output.stop()).toBe(stop);
    expect(
      Node.latest.port.postMessage.mock.calls.filter(
        ([m]) => m.kind === "stop",
      ),
    ).toHaveLength(1);
  } finally {
    abort.abort();
    await output.stop();
  }
});
it("rejects a device sample-rate mismatch before installing a worklet or playing service PCM", async () => {
  rate = 44100;
  const output = new PreviewAudioOutput("generation", 48000, vi.fn(), vi.fn());
  await expect(output.ready).rejects.toThrow("sample rate");
  expect(await output.stop()).toBe("0");
});
