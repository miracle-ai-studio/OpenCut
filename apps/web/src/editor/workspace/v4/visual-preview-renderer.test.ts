// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
const bridge = vi.hoisted(() => ({ render: vi.fn(), destroy: vi.fn() }));
vi.mock(
  "../../../../../../packages/classic-visual-renderer/src/index.mjs",
  () => ({
    openGpuRealm: async () => ({
      device: { destroy: bridge.destroy },
      observation: {},
    }),
    VisualDecoderCache: class {
      close() {}
    },
    renderClassicTextFrameV2: bridge.render,
    renderClassicCaptionFrameV3: bridge.render,
  }),
);
import {
  VisualPreviewRenderer,
  type VisualPreviewExecutionApi,
} from "./visual-preview-renderer";
afterEach(() => {
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});
it("authorizes cached font uses, downloads once and releases faces and bytes at close", async () => {
  const deleted = vi.fn(),
    face = {};
  vi.stubGlobal("document", { fonts: { delete: deleted } });
  vi.stubGlobal(
    "OffscreenCanvas",
    class {
      width = 2;
      height = 2;
      getContext() {
        return null;
      }
      transferToImageBitmap() {
        return { width: 2, height: 2 };
      }
    },
  );
  const font = { fontId: "font", contentHash: "a".repeat(64), size: "3" };
  bridge.render.mockImplementation(
    async (_plan, host, _realm, _signal, _media, _decoder, fonts) => {
      const transfer = await host.callback("resolveVerifiedFontBytes", {
        font,
      });
      if (!fonts.size) {
        const chunk = await host.callback("readVerifiedBytes", {
          transferId: transfer.transferId,
          offset: "0",
          count: 3,
        });
        expect(chunk.bytesHex).toBe("010203");
        fonts.set(`font:${font.contentHash}:3`, {
          bytes: new Uint8Array([1, 2, 3]),
          face,
        });
      }
      await host.callback("releaseVerifiedBytes", {
        transferId: transfer.transferId,
      });
    },
  );
  const download = vi.fn(async () => new Response(new Uint8Array([1, 2, 3]))),
    authorize = vi.fn(async () => ({ schema_version: 2, font_id: "font" }));
  const api = {
    v4: {
      visualPreviewTextFrame: vi.fn(async () => ({
        plan: { identity: { width: 2, height: 2 } },
        admission: { maxEncodedBytes: "1024" },
      })),
    },
    sdk: { downloadVisualPreviewFontV2: download, call: authorize },
  } as unknown as VisualPreviewExecutionApi;
  const renderer = new VisualPreviewRenderer(),
    session = {
      session_id: "s",
      generation: "1",
      overlay_epoch: "0",
      owner_id: "o",
      lease_id: "l",
    };
  await renderer.render(api, session, new AbortController().signal);
  await renderer.render(api, session, new AbortController().signal);
  expect(download).toHaveBeenCalledTimes(1);
  expect(authorize).toHaveBeenCalledWith(
    "opencut_visual_preview_v4_font_authorize",
    { session, font_id: "font" },
    expect.anything(),
  );
  authorize.mockRejectedValueOnce(new Error("Owner replaced"));
  await expect(
    renderer.render(api, session, new AbortController().signal),
  ).rejects.toThrow("Owner replaced");
  expect(download).toHaveBeenCalledTimes(1);
  renderer.close();
  expect(deleted).toHaveBeenCalledWith(face);
  expect(bridge.destroy).toHaveBeenCalledTimes(1);
  await renderer.render(api, session, new AbortController().signal);
  expect(download).toHaveBeenCalledTimes(2);
});

it("caption preview sends layout measurements to the fixed session without V2 binding", async () => {
  vi.stubGlobal(
    "OffscreenCanvas",
    class {
      width = 2;
      height = 2;
      getContext() {
        return null;
      }
      transferToImageBitmap() {
        return { width: 2, height: 2 };
      }
    },
  );
  const identity = { width: 2, height: 2, time: { ticks: "1", timebase: 2 } };
  const rasters = [{ elementId: "caption" }],
    captions = [
      { elementId: "caption", wrappedText: "Hello\nworld", measurements: [] },
    ];
  const call = vi.fn(async (_name, input) => ({
    schema_version: 3,
    plan: { identity, execution: { identity }, captions: [] },
    admission: { maxEncodedBytes: "1024" },
    bound_media: input.decoded ? { media: true } : null,
    bound: input.rasters ? { text: true } : null,
  }));
  const old = vi.fn();
  const api = {
    v4: { visualPreviewTextFrame: old },
    sdk: { call },
  } as unknown as VisualPreviewExecutionApi;
  bridge.render.mockImplementation(async (_plan, host) => {
    expect(await host.callback("bindFrame", { decoded: [] })).toEqual({
      media: true,
    });
    expect(
      await host.callback("bindCaptionFrameV3", { rasters, captions }),
    ).toEqual({ text: true });
    await expect(host.callback("bindTextFrame", { rasters })).rejects.toThrow(
      "Unknown visual callback",
    );
  });
  const renderer = new VisualPreviewRenderer(),
    session = {
      session_id: "s",
      generation: "1",
      overlay_epoch: "0",
      owner_id: "o",
      lease_id: "l",
    };
  await renderer.render(api, session, new AbortController().signal, true);
  expect(old).not.toHaveBeenCalled();
  expect(call).toHaveBeenLastCalledWith(
    "opencut_visual_preview_v4_caption_frame",
    { session, time: identity.time, decoded: [], rasters, captions },
    expect.anything(),
  );
  renderer.close();
});
