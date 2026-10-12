import { afterEach, expect, it, vi } from "vitest";
afterEach(() => { vi.unstubAllGlobals(); vi.resetModules(); });
it("primes only the existing two-block FIFO; short EOF and stopped cursor cannot deadlock or advance silence", async () => {
  class Processor {
    port = { onmessage: null as null | ((event: { data: Record<string, unknown> }) => void), postMessage: vi.fn() };
    process(_inputs: Float32Array[][], _outputs: Float32Array[][]) { return true; }
  }
  let Current = Processor;
  vi.stubGlobal("AudioWorkletProcessor", Processor);
  vi.stubGlobal("currentFrame", 128);
  vi.stubGlobal("registerProcessor", (_name: string, ctor: typeof Processor) => { Current = ctor; });
  await import("./worklet");
  const p = new Current();
  const send = (data: Record<string, unknown>) => p.port.onmessage!({ data });
  const left = new Float32Array(2), right = new Float32Array(2);
  const quantum = () => p.process([], [[left, right]]);
  send({ kind: "reset", token: "first" });
  send({ kind: "block", token: "first", pcm: new Float32Array([.1, .2, .3, .4]).buffer });
  quantum(); expect([...left]).toEqual([0, 0]); expect(p.port.postMessage).not.toHaveBeenCalled();
  send({ kind: "block", token: "first", pcm: new Float32Array([.5, .6, .7, .8]).buffer });
  quantum(); expect(left[0]).toBeCloseTo(.1);
  expect(p.port.postMessage).toHaveBeenLastCalledWith(expect.objectContaining({ consumedFrames: "2", copied: 2, renderFrame: 128, quantumFrames: 2 }));
  send({ kind: "stop" });
  expect(p.port.postMessage).toHaveBeenLastCalledWith({ kind: "stopped", token: "first", consumedFrames: "2" });
  send({ kind: "reset", token: "short" });
  send({ kind: "block", token: "short", pcm: new Float32Array([.25, .75]).buffer });
  send({ kind: "end", token: "first" }); quantum(); expect([...left]).toEqual([0, 0]);
  send({ kind: "end", token: "short" }); quantum(); expect([...left]).toEqual([.25, 0]);
  expect(p.port.postMessage).toHaveBeenLastCalledWith(expect.objectContaining({ consumedFrames: "1", copied: 1 }));
  vi.stubGlobal("currentFrame", 256);
  quantum();
  expect(p.port.postMessage).toHaveBeenLastCalledWith(expect.objectContaining({ consumedFrames: "1", copied: 0, renderFrame: 256, quantumFrames: 2 }));
});
