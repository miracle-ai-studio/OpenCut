import { afterEach, expect, it, vi } from "vitest";
import { PreviewConsumptionPump } from "./pump";
import { setupAudioPreview } from "../audio-preview-fixture";
afterEach(() => vi.useRealTimers());
const flush = async () => {
  for (let i = 0; i < 20; i++) await Promise.resolve();
};
async function session() {
  const f = await setupAudioPreview();
  await f.preview.play();
  const s = f.state().audioPreview.session!;
  f.cleanup();
  return s;
}
const open = (_s: unknown, signal: AbortSignal) =>
  new Promise<Response>((_resolve, reject) =>
    signal.addEventListener("abort", () => reject(signal.reason), {
      once: true,
    }),
  );
const output = () => ({
  waitForCredit: vi.fn(async () => {}),
  push: vi.fn(async () => {}),
});
it("clears report ownership after a synchronous transport exception", async () => {
  const s = await session();
  vi.useFakeTimers();
  const failure = vi.fn(),
    control = vi.fn(() => {
      throw new Error("transport");
    });
  const pump = new PreviewConsumptionPump(
    { open, control },
    output(),
    () => true,
    vi.fn(),
    failure,
  );
  pump.start(s);
  pump.completed();
  await flush();
  pump.completed();
  await flush();
  expect(control).toHaveBeenCalledTimes(2);
  expect(failure).toHaveBeenCalledTimes(2);
  pump.abort();
});
it("serializes the final cursor after an in-flight report and keeps the old stream until cancellation", async () => {
  const s = await session();
  vi.useFakeTimers();
  let release!: () => void, streamSignal!: AbortSignal;
  const control = vi.fn(async (input) => {
    if (input.command.sequence === "1")
      await new Promise<void>((resolve) => {
        release = resolve;
      });
    return { ...s, consumed_frames: input.command.consumed_frames };
  });
  const updated = vi.fn(),
    pump = new PreviewConsumptionPump(
      {
        control,
        open: (s, signal) => {
          streamSignal = signal;
          return open(s, signal);
        },
      },
      output(),
      () => true,
      updated,
      vi.fn(),
    );
  pump.start(s);
  pump.observe({ consumedFrames: "4096", stalled: false, contextTime: 1 });
  pump.completed();
  pump.freeze();
  const final = pump.reportStopped("4128");
  expect(control).toHaveBeenCalledOnce();
  expect(streamSignal.aborted).toBe(false);
  release();
  await final;
  expect(control.mock.calls.map(([i]) => i.command)).toEqual([
    {
      kind: "report_consumed",
      sequence: "1",
      consumed_frames: "4096",
      playing: true,
      stalled: false,
    },
    {
      kind: "report_consumed",
      sequence: "2",
      consumed_frames: "4128",
      playing: false,
      stalled: false,
    },
  ]);
  expect(streamSignal.aborted).toBe(false);
  pump.abort();
  expect(streamSignal.aborted).toBe(true);
  await vi.advanceTimersByTimeAsync(1000);
  expect(control).toHaveBeenCalledTimes(2);
});
it("does not publish a report completed after cancellation", async () => {
  const s = await session();
  vi.useFakeTimers();
  let release!: () => void;
  const updated = vi.fn(),
    failure = vi.fn(),
    control = vi.fn(async () => {
      await new Promise<void>((resolve) => {
        release = resolve;
      });
      return s;
    });
  const pump = new PreviewConsumptionPump(
    { open, control },
    output(),
    () => true,
    updated,
    failure,
  );
  pump.start(s);
  pump.completed();
  pump.abort();
  release();
  await flush();
  expect(updated).not.toHaveBeenCalled();
  expect(failure).not.toHaveBeenCalled();
});
