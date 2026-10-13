import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import { afterEach, expect, it, vi } from "vitest";
import type { EditorApi } from "../../api";
import { planEditableSubtitles } from "./planned-subtitles";
import type { SubtitleTextRequest } from "./planned-subtitles";
import { textInitial } from "../text/fixture";
const mocks = vi.hoisted(() => ({
  release: vi.fn(),
  load: vi.fn(),
  prepare: vi.fn(),
  measure: vi.fn(),
}));
vi.mock("../text/canvas-font", () => ({ loadCanvasFont: mocks.load }));
vi.mock("../visual-preview-renderer", () => ({
  prepareCaptionTextV3: mocks.prepare,
  measureClassicText: mocks.measure,
}));
afterEach(() => {
  vi.unstubAllGlobals();
  vi.resetAllMocks();
});
function fixture() {
  vi.stubGlobal("document", {
    createElement: () => ({ getContext: () => ({}) }),
  });
  mocks.load.mockResolvedValue({
    family: "AuthorizedFace",
    release: mocks.release,
  });
  mocks.prepare.mockImplementation((_ctx, text) => ({
    text: {
      ...text,
      content: "wrapped\ncue",
      transform: { ...text.transform, centerY: 160 },
    },
    wrappedText: "wrapped\ncue",
    measurements: [{ content: "wrapped\ncue", width: 80 }],
  }));
  mocks.measure.mockReturnValue({
    fontSizePx: 24,
    lineHeightPx: 28.8,
    letterSpacingPx: 0,
    align: "center",
    baseline: "middle",
    block: { maxWidth: 80, height: 28.8, visualCenterOffset: 0 },
    background: null,
    lines: [
      {
        content: "wrapped\ncue",
        width: 80,
        baselineY: 0,
        ascent: 18,
        descent: 4,
        decoration: null,
      },
    ],
  });
  const request: SubtitleTextRequest = {
    project_id: "project",
    expected_revision: "fixed",
    sequence_id: "scene",
    idempotency_key: "original-key",
    track: { kind: "new", track_id: "new-track" },
    source: {
      kind: "transcript",
      analysis_job_id: "original-asr",
      words_per_caption: 3,
    },
    style: { ...textInitial().text, content: "" },
    max_width_px: null,
    observations: null,
  };
  const layer = {
    elementId: "cue-one",
    opacity: 1,
    text: {
      content: "original cue",
      fontSizePx: 24,
      lineHeightPx: 28.8,
      letterSpacingPx: 0,
      align: "center",
      decoration: "none",
      color: [1, 1, 1, 1],
      background: { enabled: false },
      transform: {
        centerX: 160,
        centerY: 90,
        scaleX: 1,
        scaleY: 1,
        rotationRadians: 0,
      },
    },
    font: {
      fontId: "font",
      contentHash: "f".repeat(64),
      size: "200",
      weight: 400,
      style: "normal",
      faceIndex: 0,
      glyphCoverageValidated: true,
      validatedTextHash: "a".repeat(64),
    },
  };
  const prepared = {
    schema_version: 1,
    revision: "fixed",
    width: 320,
    height: 180,
    warnings: [],
    transaction: null,
    selection: null,
    cues: [
      {
        timeline_range: textInitial().timeline_range,
        layer,
        directive: { elementId: "cue-one" },
      },
      {
        timeline_range: textInitial().timeline_range,
        layer: {
          ...layer,
          elementId: "cue-two",
          font: { ...layer.font, validatedTextHash: "b".repeat(64) },
        },
        directive: { elementId: "cue-two" },
      },
    ],
  };
  const transaction = {
    project_id: "project",
    expected_revision: "fixed",
    idempotency_key: "original-key",
    label: "Editable subtitles",
    operations: [],
  };
  const bound = {
    ...prepared,
    transaction,
    selection: { track_id: "new-track", element_id: "cue-one" },
  };
  const call = vi
    .fn()
    .mockResolvedValueOnce(prepared)
    .mockResolvedValueOnce(bound);
  return {
    request,
    prepared,
    bound,
    call,
    api: { sdk: { call } } as unknown as EditorApi,
  };
}
it("observes the existing renderer with one authorized face and resubmits exactly the original request plus receipts", async () => {
  const { api, call, request, bound } = fixture(),
    original = structuredClone(request);
  const result = await planEditableSubtitles(
    api,
    request,
    new AbortController().signal,
  );
  expect(result).toEqual(bound);
  expect(request).toEqual(original);
  expect(call.mock.calls[0].slice(0, 2)).toEqual([
    "opencut_subtitle_text_v1_plan",
    original,
  ]);
  const second = call.mock.calls[1][1];
  expect({ ...second, observations: null }).toEqual(original);
  expect(second.observations).toHaveLength(2);
  expect(second.observations[0]).toMatchObject({
    caption: {
      elementId: "cue-one",
      originalTextHash: "a".repeat(64),
      wrappedText: "wrapped\ncue",
    },
    raster: {
      elementId: "cue-one",
      width: 320,
      height: 180,
      layout: {
        fontId: "font",
        fontContentHash: "f".repeat(64),
        validatedTextHash: bytesToHex(
          sha256(new TextEncoder().encode("wrapped\ncue")),
        ),
        transform: { centerY: 160 },
      },
    },
  });
  expect(second.observations[1].caption.originalTextHash).toBe("b".repeat(64));
  expect(mocks.load).toHaveBeenCalledOnce();
  expect(mocks.prepare).toHaveBeenCalledTimes(2);
  expect(mocks.measure).toHaveBeenCalledTimes(2);
  expect(mocks.release).toHaveBeenCalledOnce();
  expect(call.mock.calls.map(([name]) => name)).toEqual([
    "opencut_subtitle_text_v1_plan",
    "opencut_subtitle_text_v1_plan",
  ]);
});
it("releases the fixed font and never writes when the observation plan fails", async () => {
  const { api, request, call } = fixture();
  call
    .mockReset()
    .mockResolvedValueOnce(fixture().prepared)
    .mockRejectedValueOnce(new Error("Unauthorized observation"));
  await expect(
    planEditableSubtitles(api, request, new AbortController().signal),
  ).rejects.toThrow("Unauthorized observation");
  expect(mocks.release).toHaveBeenCalledOnce();
});
it("rejects a changed fixed revision or mismatched cue identity before loading font bytes", async () => {
  const { api, request, call, prepared } = fixture();
  call
    .mockReset()
    .mockResolvedValueOnce({ ...prepared, revision: "another-head" });
  await expect(
    planEditableSubtitles(api, request, new AbortController().signal),
  ).rejects.toThrow("fixed revision");
  expect(mocks.load).not.toHaveBeenCalled();
  call.mockReset().mockResolvedValueOnce({
    ...prepared,
    cues: [{ ...prepared.cues[0], directive: { elementId: "other" } }],
  });
  await expect(
    planEditableSubtitles(api, request, new AbortController().signal),
  ).rejects.toThrow("target changed");
  expect(mocks.load).not.toHaveBeenCalled();
});
it("does not return an edit for a different fixed project", async () => {
  const { api, request, call, prepared, bound } = fixture();
  call
    .mockReset()
    .mockResolvedValueOnce(prepared)
    .mockResolvedValueOnce({
      ...bound,
      transaction: { ...bound.transaction, project_id: "other" },
    });
  await expect(
    planEditableSubtitles(api, request, new AbortController().signal),
  ).rejects.toThrow("fixed revision");
  expect(mocks.release).toHaveBeenCalledOnce();
});

it.each(["schema", "key", "order", "selection"])(
  "rejects a stolen %s without returning a transaction",
  async (kind) => {
    const { api, request, call, prepared, bound } = fixture();
    const bad = structuredClone(bound);
    if (kind === "schema") bad.schema_version = 2;
    if (kind === "key") bad.transaction.idempotency_key = "different-key";
    if (kind === "order") bad.cues.reverse();
    if (kind === "selection") bad.selection.element_id = "outside-plan";
    call.mockReset().mockResolvedValueOnce(prepared).mockResolvedValueOnce(bad);
    await expect(
      planEditableSubtitles(api, request, new AbortController().signal),
    ).rejects.toThrow("fixed revision");
    expect(mocks.release).toHaveBeenCalledOnce();
  },
);
