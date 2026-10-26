// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { sourceThumbnail } from "./source-thumbnail";
afterEach(() => vi.restoreAllMocks());
function fixture() {
  const create = document.createElement.bind(document);
  const video = create("video");
  Object.defineProperties(video, { videoWidth: { value: 640 }, videoHeight: { value: 360 }, duration: { value: 4 } });
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  const draw = vi.fn();
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({ drawImage: draw } as never);
  vi.spyOn(HTMLCanvasElement.prototype, "toBlob").mockImplementation(callback => callback(new Blob(["pixels"], { type: "image/png" })));
  vi.spyOn(document, "createElement").mockImplementation(((name: string) => name === "video" ? video : create(name)) as typeof document.createElement);
  return { video, draw };
}
it("waits for the selected video frame instead of capturing the initial loaded frame", async () => {
  const { video, draw } = fixture();
  const result = sourceThumbnail("http://localhost/source", "video", new AbortController().signal, 2.5);
  video.dispatchEvent(new Event("loadeddata"));
  expect(video.currentTime).toBe(2.5);
  expect(draw).not.toHaveBeenCalled();
  video.dispatchEvent(new Event("seeked"));
  expect((await result).type).toBe("image/png");
  expect(draw).toHaveBeenCalledTimes(1);
  expect(video.onseeked).toBeNull();
  expect(video.getAttribute("src")).toBeNull();
});
it("aborts a pending seek and never draws its late frame", async () => {
  const { video, draw } = fixture();
  const abort = new AbortController();
  const result = sourceThumbnail("http://localhost/source", "video", abort.signal, 1).catch(error => error);
  video.dispatchEvent(new Event("loadeddata"));
  abort.abort("navigation");
  video.dispatchEvent(new Event("seeked"));
  expect(await result).toBe("navigation");
  expect(draw).not.toHaveBeenCalled();
});
it("rejects timestamps at or beyond EOF without decoding a different frame", async () => {
  const { video, draw } = fixture();
  const result = sourceThumbnail("http://localhost/source", "video", new AbortController().signal, 4).catch(error => error);
  video.dispatchEvent(new Event("loadeddata"));
  expect((await result).message).toContain("outside the video");
  expect(draw).not.toHaveBeenCalled();
});
