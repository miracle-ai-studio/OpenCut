import { expect, it, vi } from "vitest";
import { PreviewDevicePort } from "./device-port";
function device() {
  const port = {
    onmessage: null as ((e: MessageEvent) => void) | null,
    start: vi.fn(),
    postMessage: vi.fn(),
    close: vi.fn(),
  };
  const failure = vi.fn(),
    progress = vi.fn();
  const output = new PreviewDevicePort(
    port as unknown as MessagePort,
    "device",
    progress,
    failure,
    vi.fn(),
  );
  const emit = (body: object) =>
    port.onmessage?.({ data: { token: "device", ...body } } as MessageEvent);
  return { port, output, failure, progress, emit };
}
it("permits one transfer before PCM and refuses further use by the old owner", async () => {
  const f = device();
  expect(f.output.detach()).toBe(f.port);
  expect(f.port.onmessage).toBeNull();
  expect(() => f.output.detach()).toThrow("ownership");
  await expect(
    f.output.push(new Float32Array(8), new AbortController().signal),
  ).rejects.toThrow("belongs");
});
it("rejects transfer after PCM and completion beyond the two sent slots", async () => {
  const f = device();
  await f.output.push(new Float32Array(8), new AbortController().signal);
  expect(() => f.output.detach()).toThrow("ownership");
  f.emit({
    kind: "progress",
    consumedFrames: "4",
    copied: 4,
    completedBlocks: 2,
  });
  expect(f.failure).toHaveBeenCalledOnce();
  expect(f.progress).not.toHaveBeenCalled();
  f.output.silence();
  f.emit({ kind: "stopped", consumedFrames: "4" });
  await f.output.stop();
});
it("holds a frozen stream until cancellation and accepts only the final processor acknowledgement", async () => {
  const f = device(),
    abort = new AbortController();
  await f.output.push(new Float32Array(8), abort.signal);
  f.emit({
    kind: "progress",
    consumedFrames: "4",
    copied: 4,
    completedBlocks: 1,
  });
  f.output.silence();
  let settled = false;
  const held = f.output.waitForCredit(abort.signal).catch(() => {
    settled = true;
  });
  await Promise.resolve();
  expect(settled).toBe(false);
  const stop = f.output.stop();
  f.emit({ kind: "stopped", consumedFrames: "2" });
  expect(f.failure).toHaveBeenCalledOnce();
  f.emit({ kind: "stopped", consumedFrames: "4" });
  expect(await stop).toBe("4");
  expect(settled).toBe(false);
  abort.abort();
  await held;
  expect(
    f.port.postMessage.mock.calls.filter(([m]) => m.kind === "stop"),
  ).toHaveLength(1);
});
