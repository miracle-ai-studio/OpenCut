import { describe, expect, it } from "vitest";
import { readPreviewBytes } from "./visual-preview-bytes";

describe("admitted preview resource streams", () => {
  it("accepts exactly the admitted bytes across stream chunks", async () => {
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new Uint8Array([1, 2]));
        controller.enqueue(new Uint8Array([3]));
        controller.close();
      },
    });
    expect(
      await readPreviewBytes(
        new Response(body),
        "3",
        new AbortController().signal,
      ),
    ).toEqual(new Uint8Array([1, 2, 3]));
  });
  it("rejects truncated and oversized streams before exposing bytes", async () => {
    await expect(
      readPreviewBytes(
        new Response(new Uint8Array([1])),
        "2",
        new AbortController().signal,
      ),
    ).rejects.toThrow("ended early");
    await expect(
      readPreviewBytes(
        new Response(new Uint8Array([1, 2])),
        "1",
        new AbortController().signal,
      ),
    ).rejects.toThrow("exceeded");
  });
  it("cancels a stalled download when its preview owner is replaced", async () => {
    let cancelled = false;
    const body = new ReadableStream<Uint8Array>({
      cancel() {
        cancelled = true;
      },
    });
    const owner = new AbortController();
    const reading = readPreviewBytes(new Response(body), "1", owner.signal);
    owner.abort();
    await expect(reading).rejects.toMatchObject({ name: "AbortError" });
    expect(cancelled).toBe(true);
  });
});
