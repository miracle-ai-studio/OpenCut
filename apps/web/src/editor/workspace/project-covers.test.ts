import { expect, it, vi } from "vitest";
import { ProjectCovers } from "./project-covers";
import type { ProjectRef } from "../model/types";
const ref = (id = "p", revision = "r"): ProjectRef => ({
  id,
  revision,
  name: id,
  format: "visual",
});
const png = () => new Blob(["synthetic"], { type: "image/png" });
const defer = <T>() => {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
};
const tick = async () => {
  for (let n = 0; n < 8; n++) await Promise.resolve();
};
it("deduplicates fixed identities, isolates projects/revisions, freezes input and evicts bounded derived bytes", async () => {
  const load = vi.fn(async (_project: ProjectRef) => png());
  const cache = new ProjectCovers(load),
    signal = new AbortController().signal;
  const mutable = ref(),
    a = cache.request(mutable, signal),
    b = cache.request(ref(), signal);
  mutable.revision = "changed";
  expect(await a).toBe(await b);
  expect(load).toHaveBeenCalledTimes(1);
  expect(load.mock.calls[0][0].revision).toBe("r");
  await cache.request(ref(), signal);
  expect(load).toHaveBeenCalledTimes(1);
  await cache.request(ref("copy"), signal);
  await cache.request(ref("p", "r2"), signal);
  expect(load).toHaveBeenCalledTimes(3);
  for (let n = 0; n < 32; n++) await cache.request(ref("bounded" + n), signal);
  await cache.request(ref(), signal);
  expect(load).toHaveBeenCalledTimes(36);
});
it("cancels one subscriber without cancelling another, cancels queued work before loading, and never caches stale results", async () => {
  const pending = new Map<string, ReturnType<typeof defer<Blob>>>(),
    signals = new Map<string, AbortSignal>();
  const load = vi.fn((p: ProjectRef, s: AbortSignal) => {
    const d = defer<Blob>();
    pending.set(p.id, d);
    signals.set(p.id, s);
    return d.promise;
  });
  const cache = new ProjectCovers(load),
    one = new AbortController(),
    two = new AbortController(),
    queued = new AbortController();
  const first = cache.request(ref(), one.signal).catch((e) => e),
    second = cache.request(ref(), two.signal);
  const other = cache.request(ref("other"), new AbortController().signal),
    third = cache.request(ref("queued"), queued.signal).catch((e) => e);
  await tick();
  expect(load).toHaveBeenCalledTimes(2);
  one.abort("one");
  expect(await first).toBe("one");
  expect(signals.get("p")!.aborted).toBe(false);
  queued.abort("queued");
  expect(await third).toBe("queued");
  two.abort("two");
  await expect(second).rejects.toBe("two");
  expect(signals.get("p")!.aborted).toBe(true);
  pending.get("p")!.resolve(png());
  pending.get("other")!.resolve(png());
  await other;
  await tick();
  expect(load).toHaveBeenCalledTimes(2);
  const again = cache.request(ref(), new AbortController().signal);
  await tick();
  expect(load).toHaveBeenCalledTimes(3);
  pending.get("p")!.resolve(png());
  await again;
});
it("prunes deleted and outdated identities, retries failures and releases all work on host disconnect", async () => {
  let failed = true;
  const load = vi.fn(async () => {
    if (failed) throw new Error("native render failure");
    return png();
  });
  const cache = new ProjectCovers(load),
    signal = new AbortController().signal;
  await expect(cache.request(ref(), signal)).rejects.toThrow(
    "native render failure",
  );
  failed = false;
  await cache.request(ref(), signal);
  cache.invalidate("p");
  await cache.request(ref(), signal);
  cache.retain([ref("copy")]);
  await cache.request(ref(), signal);
  expect(load).toHaveBeenCalledTimes(4);
  const d = defer<Blob>();
  let s!: AbortSignal;
  const live = new ProjectCovers(async (_p, signal) => {
    s = signal;
    return d.promise;
  });
  const result = live.request(ref(), signal);
  await tick();
  live.dispose();
  expect(s.aborted).toBe(true);
  d.resolve(png());
  await expect(result).rejects.toThrow();
});
it("rejects wrong MIME and oversized cache entries", async () => {
  const signal = new AbortController().signal;
  await expect(
    new ProjectCovers(async () => new Blob(["wrong"])).request(ref(), signal),
  ).rejects.toThrow("Invalid project cover");
  await expect(
    new ProjectCovers(
      async () =>
        new Blob([new Uint8Array(1024 * 1024 + 1)], { type: "image/png" }),
    ).request(ref(), signal),
  ).rejects.toThrow("Invalid project cover");
});
it("deletion suspends only selected covers and waits for their owned work to settle", async () => {
  const waiting = new Map<string, ReturnType<typeof defer<Blob>>>(),
    signals = new Map<string, AbortSignal>();
  const cache = new ProjectCovers(async (p, signal) => {
    const d = defer<Blob>();
    waiting.set(p.id, d);
    signals.set(p.id, signal);
    return d.promise;
  });
  const first = cache
    .request(ref(), new AbortController().signal)
    .catch((e) => e);
  const other = cache.request(ref("other"), new AbortController().signal);
  await tick();
  const resume = cache.hold(["p"]);
  let settled = false;
  const quiet = cache.quiesce(["p"]).then(() => {
    settled = true;
  });
  await tick();
  expect(signals.get("p")!.aborted).toBe(true);
  expect(signals.get("other")!.aborted).toBe(false);
  expect(settled).toBe(false);
  await expect(
    cache.request(ref(), new AbortController().signal),
  ).rejects.toThrow("suspended");
  waiting.get("p")!.resolve(png());
  await quiet;
  expect((await first).name).toBe("AbortError");
  waiting.get("other")!.resolve(png());
  await other;
  resume();
});
