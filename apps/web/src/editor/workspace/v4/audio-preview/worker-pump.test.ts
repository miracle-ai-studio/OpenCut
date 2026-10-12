import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { WorkerPreviewPump } from "./worker-pump";
import { PUMP_CHANNEL } from "./worker-protocol";
import type { V4AudioPreviewSession } from "../types";

class PumpWorker {
  static latest: PumpWorker;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: (() => void) | null = null;
  postMessage = vi.fn();
  terminate = vi.fn();
  constructor() {
    PumpWorker.latest = this;
  }
  emit(body: object, token = "device") {
    this.onmessage?.({
      data: { channel: PUMP_CHANNEL, version: 1, token, ...body },
    } as MessageEvent);
  }
}
beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("Worker", PumpWorker);
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});
function bridge() {
  const port = {} as MessagePort,
    take = vi.fn(() => port),
    progress = vi.fn(),
    updated = vi.fn(),
    failure = vi.fn();
  const pump = new WorkerPreviewPump(
    "device",
    { baseUrl: "http://localhost:1234", token: "" },
    take,
    progress,
    updated,
    failure,
  );
  return {
    pump,
    port,
    take,
    progress,
    updated,
    failure,
    worker: PumpWorker.latest,
  };
}
it("transfers exactly one device port after initialization and uses the worker's exact stopped cursor", async () => {
  const f = bridge();
  expect(() => f.pump.start({} as V4AudioPreviewSession)).toThrow("not ready");
  expect(f.take).not.toHaveBeenCalled();
  f.worker.emit({ kind: "ready" });
  await f.pump.ready;
  f.pump.start({ generation: "1" } as V4AudioPreviewSession);
  expect(f.worker.postMessage.mock.calls.at(-1)?.[1]).toEqual([f.port]);
  expect(() => f.pump.start({} as V4AudioPreviewSession)).toThrow(
    "already owns",
  );
  expect(f.take).toHaveBeenCalledOnce();
  // Main-thread observations/completions cannot create a second credit owner.
  const commands = f.worker.postMessage.mock.calls.length;
  f.pump.observe({ consumedFrames: "4096", stalled: false, contextTime: 100 });
  f.pump.completed();
  expect(f.worker.postMessage).toHaveBeenCalledTimes(commands);
  f.pump.freeze();
  const stop = f.pump.stopOutput();
  expect(f.pump.stopOutput()).toBe(stop);
  const id = f.worker.postMessage.mock.calls.at(-1)![0].id;
  f.worker.emit({ kind: "reply", id, frames: "9007199254740993" });
  expect(await stop).toBe("9007199254740993");
  const final = f.pump.reportStopped("9007199254740993");
  const report = f.worker.postMessage.mock.calls.at(-1)![0];
  expect(report).toMatchObject({
    kind: "reportStopped",
    frames: "9007199254740993",
    id: id + 1,
  });
  f.worker.emit({ kind: "reply", id: report.id, session: { generation: "1" } });
  expect(await final).toEqual({ generation: "1" });
  f.pump.abort();
  expect(f.worker.terminate).toHaveBeenCalledOnce();
});
it("aborts pending stop and ignores late receipt/publication", async () => {
  const f = bridge();
  f.worker.emit({ kind: "ready" });
  await f.pump.ready;
  const stop = f.pump.stopOutput(),
    rejected = expect(stop).rejects.toThrow("stopped");
  const id = f.worker.postMessage.mock.calls.at(-1)![0].id;
  f.pump.abort();
  await rejected;
  f.worker.emit({ kind: "reply", id, frames: "8192" });
  f.worker.emit({ kind: "progress", receipt: { consumedFrames: "8192" } });
  f.worker.emit({ kind: "session", session: { generation: "old" } });
  expect(f.updated).not.toHaveBeenCalled();
  expect(f.progress).not.toHaveBeenCalled();
  await expect(f.pump.reportStopped("8192")).rejects.toThrow("stopped");
});
it("fails a mismatched channel token and rejects initialization", async () => {
  const f = bridge(),
    rejected = expect(f.pump.ready).rejects.toThrow("reply");
  f.worker.emit({ kind: "ready" }, "old");
  await rejected;
  expect(f.failure).toHaveBeenCalledOnce();
  expect(f.worker.terminate).toHaveBeenCalledOnce();
});
it("fails a timed-out final device receipt without inventing a cursor", async () => {
  const f = bridge();
  f.worker.emit({ kind: "ready" });
  await f.pump.ready;
  const stop = f.pump.stopOutput(),
    rejected = expect(stop).rejects.toThrow("timed out");
  await vi.advanceTimersByTimeAsync(4000);
  await rejected;
  expect(f.failure).toHaveBeenCalledOnce();
  expect(f.worker.terminate).toHaveBeenCalledOnce();
});
