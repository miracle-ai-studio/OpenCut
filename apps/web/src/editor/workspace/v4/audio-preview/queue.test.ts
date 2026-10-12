import { expect, it } from "vitest";
import { PreviewPcmQueue } from "./queue";

it("renders actual interleaved frames across blocks, with a stopped clock on underrun", () => {
  const q = new PreviewPcmQueue();
  q.reset("g1");
  q.append("g1", Float32Array.of(1, -1, 0.5, -0.5));
  q.append("g1", Float32Array.of(0.25, -0.25));
  const left = new Float32Array(2),
    right = new Float32Array(2);
  expect(q.render(left, right)).toEqual({
    copied: 2,
    completedBlocks: 1,
    consumedFrames: "2",
  });
  expect([...left]).toEqual([1, 0.5]);
  expect([...right]).toEqual([-1, -0.5]);
  expect(q.render(left, right)).toEqual({
    copied: 1,
    completedBlocks: 2,
    consumedFrames: "3",
  });
  expect([...left]).toEqual([0.25, 0]);
  expect([...right]).toEqual([-0.25, 0]);
  expect(q.render(left, right).consumedFrames).toBe("3");
  expect([...left]).toEqual([0, 0]);
});
it("enforces two blocks, preserves a partially consumed slot, and flushes stale generations", () => {
  const q = new PreviewPcmQueue();
  q.reset("g1");
  q.append("g1", new Float32Array(8192));
  q.append("g1", new Float32Array(2));
  expect(() => q.append("g1", new Float32Array(2))).toThrow("full");
  q.render(new Float32Array(128), new Float32Array(128));
  expect(q.freeBlocks).toBe(0);
  q.reset("g2");
  expect(q.append("g1", Float32Array.of(1, 1))).toBe(false);
  const left = new Float32Array(128),
    right = new Float32Array(128);
  expect(q.render(left, right)).toMatchObject({
    copied: 0,
    consumedFrames: "0",
  });
  expect(left.every((n) => n === 0)).toBe(true);
  expect(q.freeBlocks).toBe(2);
  q.reset(null);
  expect(q.append("g2", Float32Array.of(1, 1))).toBe(false);
});
it("rejects malformed, oversized and non-finite PCM before entering the render queue", () => {
  const q = new PreviewPcmQueue();
  q.reset("g1");
  for (const block of [
    new Float32Array(0),
    new Float32Array(1),
    new Float32Array(8194),
    Float32Array.of(Infinity, 0),
    Float32Array.of(NaN, 0),
  ])
    expect(() => q.append("g1", block)).toThrow();
  expect(q.freeBlocks).toBe(2);
});
