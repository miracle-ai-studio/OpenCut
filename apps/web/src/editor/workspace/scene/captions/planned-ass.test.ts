import { afterEach, expect, it, vi } from "vitest";
import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import type { EditorApi } from "../../api";
import { planAssSubtitles, type AssRequest } from "./planned-ass";
const mocks = vi.hoisted(() => ({
  load: vi.fn(),
  release: vi.fn(),
  measure: vi.fn(),
}));
vi.mock("../text/canvas-font", () => ({ loadCanvasFont: mocks.load }));
vi.mock("../visual-preview-renderer", () => ({
  measureClassicText: mocks.measure,
}));
afterEach(() => {
  vi.resetAllMocks();
  vi.unstubAllGlobals();
});
function fixture() {
  vi.stubGlobal("document", {
    createElement: () => ({ getContext: () => ({}) }),
  });
  mocks.load.mockResolvedValue({ family: "FixedFace", release: mocks.release });
  mocks.measure.mockReturnValue({ lines: [], block: { height: 20 } });
  const input: AssRequest = {
    project_id: "p",
    expected_revision: "r",
    sequence_id: "s",
    idempotency_key: "key",
    track: { kind: "new", track_id: "subtitles" },
    content: "original ASS",
    font_bindings: [],
    observations: null,
  };
  const text = {
    content: "Exact\ncue",
    transform: {
      centerX: 10,
      centerY: 20,
      scaleX: 1,
      scaleY: 1,
      rotationRadians: 0,
    },
  };
  const layer = {
    elementId: "cue",
    text,
    font: {
      fontId: "font",
      contentHash: "a".repeat(64),
      weight: 400,
      style: "normal",
      faceIndex: 0,
      validatedTextHash: bytesToHex(
        sha256(new TextEncoder().encode(text.content)),
      ),
    },
  };
  const prepared = {
    
    revision: "r",
    width: 320,
    height: 180,
    font_requirements: [],
    warnings: [],
    cues: [{ layer }],
    transaction: null,
    selection: null,
  };
  const bound = {
    ...prepared,
    transaction: {
      project_id: "p",
      expected_revision: "r",
      idempotency_key: "key",
    },
    selection: { track_id: "subtitles", element_id: "cue" },
  };
  const call = vi
    .fn()
    .mockResolvedValueOnce(prepared)
    .mockResolvedValueOnce(bound);
  return {
    input,
    prepared,
    bound,
    call,
    api: { sdk: { call } } as unknown as EditorApi,
  };
}
it("measures exact ASS content without wrapping and retains original transaction identity", async () => {
  const { input, call, api } = fixture();
  const result = await planAssSubtitles(
    api,
    input,
    new AbortController().signal,
  );
  expect(call).toHaveBeenCalledTimes(2);
  expect(mocks.load).toHaveBeenCalledTimes(1);
  expect(mocks.measure.mock.calls[0][1].content).toBe("Exact\ncue");
  expect(call.mock.calls[1][1].observations[0].layout.validatedTextHash).toBe(
    bytesToHex(sha256(new TextEncoder().encode("Exact\ncue"))),
  );
  expect(result.transaction?.idempotency_key).toBe("key");
  expect(mocks.release).toHaveBeenCalledTimes(1);
});
it("returns missing font requirements without loading or measuring synthetic fallbacks", async () => {
  const { input, prepared, call, api } = fixture();
  call
    .mockReset()
    .mockResolvedValue({
      ...prepared,
      cues: [],
      font_requirements: [
        {
          family: "Missing",
          weight: "bold",
          style: "italic",
          resolved_font_id: null,
        },
      ],
    });
  const result = await planAssSubtitles(
    api,
    input,
    new AbortController().signal,
  );
  expect(result.transaction).toBeNull();
  expect(call).toHaveBeenCalledTimes(1);
  expect(mocks.load).not.toHaveBeenCalled();
});
it("rejects changed text hash and mismatched bound scope, releasing loaded fonts", async () => {
  const first = fixture();
  first.prepared.cues[0].layer.font.validatedTextHash = "bad";
  await expect(
    planAssSubtitles(first.api, first.input, new AbortController().signal),
  ).rejects.toThrow("layout target changed");
  expect(mocks.load).not.toHaveBeenCalled();
  const second = fixture();
  second.bound.transaction.expected_revision = "changed";
  await expect(
    planAssSubtitles(second.api, second.input, new AbortController().signal),
  ).rejects.toThrow("fixed revision");
  expect(mocks.release).toHaveBeenCalledTimes(1);
});
it("propagates parser refusal and abort without applying anything", async () => {
  const { input, call, api } = fixture();
  call.mockReset().mockRejectedValue(new Error("Unsupported ASS tag"));
  await expect(
    planAssSubtitles(api, input, new AbortController().signal),
  ).rejects.toThrow("Unsupported ASS tag");
  const f = fixture(),
    abort = new AbortController();
  abort.abort();
  await expect(
    planAssSubtitles(f.api, f.input, abort.signal),
  ).rejects.toThrow();
  expect(mocks.load).not.toHaveBeenCalled();
});
