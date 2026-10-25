import { expect, it } from "vitest";
import { PreviewOutputClock } from "./output-clock";
import type { DeviceReceipt } from "./device-port";

const receipt = (patch: Partial<DeviceReceipt> = {}): DeviceReceipt => ({
  kind: "progress", token: "device", consumedFrames: "4800", copied: 128,
  completedBlocks: 1, renderFrame: 28672, quantumFrames: 128,
  runStartFrame: 24000, runStartConsumed: "0", phase: "playing", ...patch,
});
it("maps the device output timestamp rather than message arrival or submitted samples, including graph look-ahead", () => {
  const clock = new PreviewOutputClock(48000, .006);
  clock.observe(receipt());
  // At C=.556 the sample entering the graph at C=.550 is heard: 50ms=2400 frames.
  expect(clock.sample({ contextTime: .556, performanceTime: 1000 }, 1000)?.output?.frames).toBe("2400");
  // A delayed observation can advance 10ms, never jump to the submitted 4800.
  expect(clock.sample({ contextTime: .556, performanceTime: 1000 }, 1010)?.output?.frames).toBe("2880");
});
it("freezes content through an underrun and resumes without treating inserted silence as timeline content", () => {
  const clock = new PreviewOutputClock(48000, 0);
  clock.observe(receipt({ consumedFrames: "480", runStartFrame: 48000, renderFrame: 48352, phase: "buffering" }));
  clock.observe(receipt({ consumedFrames: "960", runStartFrame: 52800, runStartConsumed: "480", renderFrame: 53152 }));
  const gap = clock.sample({ contextTime: 1.05, performanceTime: 1000 }, 1000);
  expect(gap?.output?.frames).toBe("480"); expect(gap?.stalled).toBe(true);
  expect(clock.sample({ contextTime: 1.105, performanceTime: 1055 }, 1055)?.output?.frames).toBe("720");
});
it("does not start the playhead before the first sample or consume a partial quantum's empty tail", () => {
  const clock = new PreviewOutputClock(48000, .006);
  clock.observe(receipt({ consumedFrames: "16", copied: 16, renderFrame: 48000, runStartFrame: 48000, phase: "ended" }));
  expect(clock.sample({ contextTime: 1.005, performanceTime: 1000 }, 1000)?.started).toBe(false);
  expect(clock.sample({ contextTime: 1.01, performanceTime: 1005 }, 1005)?.output?.frames).toBe("16");
  expect(clock.sample({ contextTime: 1.02, performanceTime: 1015 }, 1015)?.stalled).toBe(false);
});
it("rejects missing intervals and clock reversal, and never substitutes a wall clock for an unavailable output timestamp", () => {
  const clock = new PreviewOutputClock(48000, 0);
  expect(clock.sample({ contextTime: 0, performanceTime: 0 }, 1000)).toBeNull();
  expect(() => clock.observe(receipt({ runStartFrame: null }))).toThrow("lost");
  clock.observe(receipt());
  clock.sample({ contextTime: .55, performanceTime: 1000 }, 1000);
  expect(() => clock.sample({ contextTime: .54, performanceTime: 1001 }, 1001)).toThrow("changed");
});
it("retains coalesced contiguous run anchors and 44.1kHz sample boundaries without accumulated drift", () => {
  const clock = new PreviewOutputClock(44100, 0);
  clock.observe(receipt({ consumedFrames: "26460000", runStartFrame: 44100, renderFrame: 26503972 }));
  expect(clock.sample({ contextTime: 601, performanceTime: 600000 }, 600000)?.output?.frames).toBe("26460000");
});
it("tracks every quantum through a device gap without losing the previous run's terminal samples",()=>{
  const clock=new PreviewOutputClock(48000,0);
  for(const through of [128,256,384])clock.observe(receipt({consumedFrames:String(through),runStartFrame:48000,
    runStartConsumed:"0",renderFrame:48000+through-128}));
  clock.observe(receipt({consumedFrames:"512",runStartFrame:49024,runStartConsumed:"384",renderFrame:49024}));
  expect(clock.sample({contextTime:1.015,performanceTime:1000},1000)?.output?.frames).toBe("384");
  expect(clock.sample({contextTime:49024/48000+64/48000,performanceTime:1005},1005)?.output?.frames).toBe("448");
});
it("keeps low-latency output within the latest submitted quantum instead of a UI-rate ceiling",()=>{
  const clock=new PreviewOutputClock(48000,0);
  clock.observe(receipt({consumedFrames:"128",runStartFrame:48000,renderFrame:48000}));
  clock.observe(receipt({consumedFrames:"256",runStartFrame:48000,renderFrame:48128}));
  expect(clock.sample({contextTime:1.004,performanceTime:1000},1000)?.output?.frames).toBe("192");
});

it("still rejects a genuine overlapping submission after device-quantum deduplication",()=>{
  const clock=new PreviewOutputClock(48000,0);
  clock.observe(receipt({consumedFrames:"491008",runStartFrame:275456,renderFrame:766336}));
  expect(()=>clock.observe(receipt({consumedFrames:"491136",runStartFrame:766336,
    runStartConsumed:"491008",renderFrame:766336}))).toThrow("overlap or skip");
  expect(JSON.parse(clock.diagnostics().clockIntervalError)).toMatchObject({previousEnd:766464,startFrame:766336});
});
