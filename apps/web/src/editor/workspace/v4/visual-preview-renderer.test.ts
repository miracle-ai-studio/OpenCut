// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
const bridge = vi.hoisted(() => ({
  render: vi.fn(),
  blendRender: vi.fn(),
  maskRender: vi.fn(),
  sceneRender: vi.fn(),
  destroy: vi.fn(),
  open: vi.fn(),
}));
vi.mock(
  "../../../../../../packages/classic-visual-renderer/src/index.mjs",
  () => ({
    openGpuRealm: async (options: { fontHashWorker?: boolean }) => {
      bridge.open(options);
      return { device: { destroy: bridge.destroy }, observation: {} };
    },
    closeGpuRealm: (realm: { device: { destroy(): void } }) =>
      realm.device.destroy(),
    VisualDecoderCache: class {
      close() {}
    },
    renderClassicTextFrameV2: bridge.render,
    renderClassicCaptionFrameV3: bridge.render,
    renderClassicGraphicFrameV1: bridge.render,
    renderClassicBlendFrameV1: bridge.blendRender,
    renderClassicMaskFrameV1: bridge.maskRender,
    renderClassicSceneFrameV1: bridge.sceneRender,
  }),
);
import {
  VisualPreviewRenderer,
  type VisualPreviewExecutionApi,
  type VisualPreviewProfile,
} from "./visual-preview-renderer";
function planFor(
  profile: VisualPreviewProfile,
  identity: object,
  fonts: object[] = [],
) {
  const text = { identity, texts: fonts.map((font) => ({ font })) };
  const caption = { identity, execution: text, captions: [] };
  const graphic = { identity, execution: caption, graphics: [] };
  const blend = { identity, execution: graphic, blends: [] };
  return profile === "text"
    ? text
    : profile === "caption"
      ? caption
      : profile === "graphic"
        ? graphic
        : profile === "blend"
          ? blend
          : { identity, execution: blend, masks: [] };
}
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
        plan: planFor("text", { width: 2, height: 2 }, [font]),
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
  expect(bridge.open).toHaveBeenCalledExactlyOnceWith({ fontHashWorker: true });
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
    plan: planFor("caption", identity),
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
  await renderer.render(api, session, new AbortController().signal, "caption");
  expect(old).not.toHaveBeenCalled();
  expect(call).toHaveBeenLastCalledWith(
    "opencut_visual_preview_v4_caption_frame",
    { session, time: identity.time, decoded: [], rasters, captions },
    expect.anything(),
  );
  renderer.close();
});

it("graphic preview binds media, delegated captions and graphics on one fixed time and session", async () => {
  vi.stubGlobal(
    "OffscreenCanvas",
    class {
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
      { elementId: "caption", wrappedText: "Hello", measurements: [] },
    ],
    graphics = [
      { elementId: "rectangle", width: 60, height: 50, originX: 0, originY: 0 },
    ];
  const call = vi.fn(async (_name, input) => ({
    schema_version: 1,
    plan: planFor("graphic", identity),
    admission: { maxEncodedBytes: "1024" },
    bound_media: input.decoded ? { media: true } : null,
    bound_execution: input.rasters ? { text: true } : null,
    bound: input.graphics ? { graphic: true } : null,
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
    expect(await host.callback("bindGraphicFrameV1", { graphics })).toEqual({
      graphic: true,
    });
    await expect(host.callback("bindTextFrame", { rasters })).rejects.toThrow(
      "Unknown visual callback",
    );
  });
  const renderer = new VisualPreviewRenderer(),
    session = {
      session_id: "s",
      generation: "2",
      overlay_epoch: "1",
      owner_id: "o",
      lease_id: "l",
    };
  await renderer.render(api, session, new AbortController().signal, "graphic");
  expect(old).not.toHaveBeenCalled();
  expect(call.mock.calls.map(([name]) => name)).toEqual(
    Array(4).fill("opencut_visual_preview_v4_graphic_frame"),
  );
  expect(call).toHaveBeenLastCalledWith(
    "opencut_visual_preview_v4_graphic_frame",
    {
      session,
      time: identity.time,
      decoded: [],
      rasters,
      captions,
      graphics,
    },
    expect.anything(),
  );
  expect(bridge.render).toHaveBeenCalledTimes(1);
  renderer.close();
});

it.each([true, false])(
  "graphic preview rejects missing delegated bind without substituting final bound (field=%s)",
  async (field) => {
    vi.stubGlobal(
      "OffscreenCanvas",
      class {
        getContext() {
          return null;
        }
      },
    );
    const call = vi.fn(async () => ({
      schema_version: 1,
      plan: planFor("graphic", {
        width: 2,
        height: 2,
        time: { ticks: "0", timebase: 1 },
      }),
      admission: { maxEncodedBytes: "1024" },
      ...(field ? { bound_execution: null } : {}),
      bound: { wrongStage: true },
    }));
    const api = {
      v4: {},
      sdk: { call },
    } as unknown as VisualPreviewExecutionApi;
    bridge.render.mockImplementation(async (_plan, host) =>
      host.callback("bindCaptionFrameV3", { rasters: [], captions: [] }),
    );
    const renderer = new VisualPreviewRenderer();
    await expect(
      renderer.render(
        api,
        {
          session_id: "s",
          generation: "1",
          overlay_epoch: "0",
          owner_id: "o",
          lease_id: "l",
        },
        new AbortController().signal,
        "graphic",
      ),
    ).rejects.toThrow("Missing bound visual text frame");
    renderer.close();
  },
);

it("binds the blend wrapper after its complete graphic stage on the same authorized frame", async () => {
  vi.stubGlobal(
    "OffscreenCanvas",
    class {
      getContext() {
        return null;
      }
      transferToImageBitmap() {
        return { width: 2, height: 2 };
      }
    },
  );
  const identity = {
    width: 2,
    height: 2,
    time: { ticks: "1001", timebase: 30000 },
  };
  const decoded = [{ elementId: "video" }],
    rasters = [{ elementId: "title" }],
    captions = [
      { elementId: "caption", wrappedText: "Hello", measurements: [] },
    ],
    graphics = [
      { elementId: "star", width: 512, height: 512, originX: 0, originY: 0 },
    ];
  const graphic = { identity: "graphic-stage" };
  const wrapper = {
    identity: "blend-stage",
    execution: graphic,
    blends: [{ elementId: "star", blendMode: "multiply" }],
  };
  const call = vi.fn(async (_name, input) => ({
    schema_version: 1,
    plan: planFor("blend", identity),
    admission: { maxEncodedBytes: "1024" },
    bound_media: input.decoded ? { media: true } : null,
    bound_execution: input.rasters ? { text: true } : null,
    bound_graphic: input.graphics ? graphic : null,
    bound: input.graphics ? wrapper : null,
  }));
  const old = vi.fn(),
    api = {
      v4: { visualPreviewTextFrame: old },
      sdk: { call },
    } as unknown as VisualPreviewExecutionApi;
  bridge.blendRender.mockImplementation(async (_plan, host) => {
    expect(await host.callback("bindFrame", { decoded })).toEqual({
      media: true,
    });
    expect(
      await host.callback("bindCaptionFrameV3", { rasters, captions }),
    ).toEqual({ text: true });
    await expect(host.callback("bindBlendFrameV1", {})).rejects.toThrow(
      "Missing bound visual blend frame",
    );
    expect(await host.callback("bindGraphicFrameV1", { graphics })).toBe(
      graphic,
    );
    await expect(
      host.callback("bindBlendFrameV1", { blends: [] }),
    ).rejects.toThrow("Invalid visual blend binding arguments");
    expect(await host.callback("bindBlendFrameV1", {})).toBe(wrapper);
    await expect(host.callback("bindBlendFrameV1", {})).rejects.toThrow(
      "Missing bound visual blend frame",
    );
    await expect(host.callback("bindTextFrame", { rasters })).rejects.toThrow(
      "Unknown visual callback",
    );
  });
  const renderer = new VisualPreviewRenderer(),
    session = {
      session_id: "s",
      generation: "2",
      overlay_epoch: "1",
      owner_id: "o",
      lease_id: "l",
    };
  await renderer.render(api, session, new AbortController().signal, "blend");
  expect(bridge.blendRender).toHaveBeenCalledTimes(1);
  expect(bridge.render).not.toHaveBeenCalled();
  expect(old).not.toHaveBeenCalled();
  expect(call.mock.calls.map(([name]) => name)).toEqual(
    Array(4).fill("opencut_visual_preview_v4_blend_frame"),
  );
  expect(call).toHaveBeenLastCalledWith(
    "opencut_visual_preview_v4_blend_frame",
    { session, time: identity.time, decoded, rasters, captions, graphics },
    expect.anything(),
  );
  renderer.close();
});

it.each(["bound_graphic", "bound"] as const)(
  "rejects a missing %s instead of substituting another blend binding stage",
  async (missing) => {
    vi.stubGlobal(
      "OffscreenCanvas",
      class {
        getContext() {
          return null;
        }
      },
    );
    const call = vi.fn(async () => ({
      plan: planFor("blend", {
        width: 2,
        height: 2,
        time: { ticks: "0", timebase: 1 },
      }),
      admission: { maxEncodedBytes: "1024" },
      bound_graphic: missing === "bound_graphic" ? null : { graphic: true },
      bound: missing === "bound" ? null : { blend: true },
    }));
    const api = {
      v4: {},
      sdk: { call },
    } as unknown as VisualPreviewExecutionApi;
    bridge.blendRender.mockImplementation(async (_plan, host) => {
      await host.callback("bindGraphicFrameV1", { graphics: [] });
      await host.callback("bindBlendFrameV1", {});
    });
    const renderer = new VisualPreviewRenderer();
    await expect(
      renderer.render(
        api,
        {
          session_id: "s",
          generation: "1",
          overlay_epoch: "0",
          owner_id: "o",
          lease_id: "l",
        },
        new AbortController().signal,
        "blend",
      ),
    ).rejects.toThrow(
      missing === "bound_graphic"
        ? "Missing bound visual graphic frame"
        : "Missing bound visual blend frame",
    );
    renderer.close();
  },
);

function blendRoundtripFixture(onGraphicReply: () => void = () => {}) {
  vi.stubGlobal(
    "OffscreenCanvas",
    class {
      getContext() {
        return null;
      }
      transferToImageBitmap() {
        return { width: 2, height: 2 };
      }
    },
  );
  let number = 0;
  const wrappers: object[] = [];
  const call = vi.fn(async (_name, input) => {
    if (input.time === null) number++;
    const graphic = { render: number };
    const bound = input.graphics
      ? { identity: { render: number }, execution: graphic, blends: [] }
      : null;
    if (bound) {
      wrappers.push(bound);
      onGraphicReply();
    }
    return {
      plan: planFor("blend", {
        width: 2,
        height: 2,
        time: { ticks: String(number), timebase: 1 },
      }),
      admission: { maxEncodedBytes: "1024" },
      bound_graphic: input.graphics ? graphic : null,
      bound,
    };
  });
  const api = { v4: {}, sdk: { call } } as unknown as VisualPreviewExecutionApi;
  const session = {
    session_id: "s",
    generation: "1",
    overlay_epoch: "0",
    owner_id: "o",
    lease_id: "l",
  };
  return {
    api,
    session,
    call,
    wrappers,
    renderer: new VisualPreviewRenderer(),
  };
}

it.each(["during", "after"])(
  "aborts %s graphic binding without replaying the complete blend request",
  async (when) => {
    const cancel = new AbortController(),
      reason = new Error("Preview replaced");
    const fixture = blendRoundtripFixture(() => {
      if (when === "during") cancel.abort(reason);
    });
    bridge.blendRender.mockImplementation(async (_plan, host) => {
      await host.callback("bindGraphicFrameV1", { graphics: [] });
      cancel.abort(reason);
      await host.callback("bindBlendFrameV1", {});
    });
    try {
      await expect(
        fixture.renderer.render(
          fixture.api,
          fixture.session,
          cancel.signal,
          "blend",
        ),
      ).rejects.toBe(reason);
      expect(fixture.call).toHaveBeenCalledTimes(2);
    } finally {
      fixture.renderer.close();
    }
  },
);

it("does not reuse an unconsumed blend wrapper from an abandoned render", async () => {
  const fixture = blendRoundtripFixture();
  bridge.blendRender.mockImplementationOnce(async (_plan, host) => {
    await host.callback("bindGraphicFrameV1", { graphics: [] });
    throw new Error("Abandoned after graphic binding");
  });
  bridge.blendRender.mockImplementationOnce(async (_plan, host) => {
    await expect(host.callback("bindBlendFrameV1", {})).rejects.toThrow(
      "Missing bound visual blend frame",
    );
    await host.callback("bindGraphicFrameV1", { graphics: [] });
    expect(await host.callback("bindBlendFrameV1", {})).toBe(
      fixture.wrappers[1],
    );
    expect(fixture.wrappers[1]).not.toBe(fixture.wrappers[0]);
  });
  try {
    await expect(
      fixture.renderer.render(
        fixture.api,
        fixture.session,
        new AbortController().signal,
        "blend",
      ),
    ).rejects.toThrow("Abandoned after graphic binding");
    await fixture.renderer.render(
      fixture.api,
      fixture.session,
      new AbortController().signal,
      "blend",
    );
    expect(fixture.call).toHaveBeenCalledTimes(4);
  } finally {
    fixture.renderer.close();
  }
});

function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

function maskRoundtripFixture(
  finalReply: (value: Record<string, unknown>) => Record<string, unknown> = (
    value,
  ) => value,
) {
  vi.stubGlobal(
    "OffscreenCanvas",
    class {
      getContext() {
        return null;
      }
      transferToImageBitmap() {
        return { width: 2, height: 2 };
      }
    },
  );
  let number = 0;
  const bindings: {
    graphic: object;
    blend: object;
    mask: { execution: object; masks: object[] };
  }[] = [];
  const call = vi.fn(async (_name, input) => {
    if (input.time === null) {
      number++;
      const graphic = { identity: { render: number, profile: "graphic" } };
      const blend = {
        identity: { render: number, profile: "blend" },
        execution: graphic,
        blends: [{ elementId: "shape", blendMode: "multiply" }],
      };
      const mask = {
        identity: { render: number, profile: "mask" },
        execution: blend,
        masks: [
          {
            elementId: "shape",
            mask: { id: "mask", shape: { kind: "ellipse" } },
          },
        ],
      };
      bindings.push({ graphic, blend, mask });
    }
    const bound = bindings[number - 1];
    const reply = {
      schema_version: 1,
      plan: planFor("mask", {
        width: 2,
        height: 2,
        time: { ticks: String(number), timebase: 30 },
      }),
      admission: { maxEncodedBytes: "1024" },
      bound_media: input.decoded ? { media: true } : null,
      bound_execution: input.rasters ? { text: true } : null,
      bound_graphic: input.graphics ? bound.graphic : null,
      bound_blend: input.graphics ? bound.blend : null,
      bound: input.graphics ? bound.mask : null,
    };
    return input.graphics ? finalReply(reply) : reply;
  });
  return {
    api: {
      v4: { visualPreviewTextFrame: vi.fn() },
      sdk: { call },
    } as unknown as VisualPreviewExecutionApi,
    session: {
      session_id: "s",
      generation: "1",
      overlay_epoch: "0",
      owner_id: "o",
      lease_id: "l",
    },
    call,
    bindings,
    renderer: new VisualPreviewRenderer(),
  };
}

it("consumes nested blend and mask bindings from the single final graphic reply in order", async () => {
  const fixture = maskRoundtripFixture();
  const decoded = [{ elementId: "video" }],
    rasters = [{ elementId: "title" }],
    captions = [
      { elementId: "caption", wrappedText: "Caption", measurements: [] },
    ],
    graphics = [
      { elementId: "shape", width: 512, height: 512, originX: 0, originY: 0 },
    ];
  bridge.maskRender.mockImplementation(async (plan, host) => {
    expect(plan.execution.execution.execution.execution.texts).toEqual([]);
    await expect(host.callback("bindMaskFrameV1", {})).rejects.toThrow(
      "Missing completed visual mask binding",
    );
    await expect(host.callback("bindBlendFrameV1", {})).rejects.toThrow(
      "Missing bound visual blend frame",
    );
    expect(await host.callback("bindFrame", { decoded })).toEqual({
      media: true,
    });
    expect(
      await host.callback("bindCaptionFrameV3", { rasters, captions }),
    ).toEqual({ text: true });
    expect(await host.callback("bindGraphicFrameV1", { graphics })).toBe(
      fixture.bindings[0].graphic,
    );
    await expect(host.callback("bindMaskFrameV1", {})).rejects.toThrow(
      "Missing completed visual mask binding",
    );
    await expect(
      host.callback("bindBlendFrameV1", { blends: [] }),
    ).rejects.toThrow("Invalid visual blend binding arguments");
    const blend = await host.callback("bindBlendFrameV1", {});
    expect(blend).toBe(fixture.bindings[0].blend);
    expect(blend.execution).toBe(fixture.bindings[0].graphic);
    for (const args of [[], { masks: [] }])
      await expect(host.callback("bindMaskFrameV1", args)).rejects.toThrow(
        "Invalid visual mask binding arguments",
      );
    const mask = await host.callback("bindMaskFrameV1", {});
    expect(mask).toBe(fixture.bindings[0].mask);
    expect(mask.execution).toBe(blend);
    expect(mask.masks).toBe(fixture.bindings[0].mask.masks);
    await expect(host.callback("bindMaskFrameV1", {})).rejects.toThrow(
      "Missing completed visual mask binding",
    );
    await expect(host.callback("bindBlendFrameV1", {})).rejects.toThrow(
      "Missing bound visual blend frame",
    );
  });
  try {
    await fixture.renderer.render(
      fixture.api,
      fixture.session,
      new AbortController().signal,
      "mask",
    );
    expect(fixture.call.mock.calls.map(([name]) => name)).toEqual(
      Array(4).fill("opencut_visual_preview_v4_mask_frame"),
    );
    expect(fixture.call).toHaveBeenLastCalledWith(
      "opencut_visual_preview_v4_mask_frame",
      {
        session: fixture.session,
        time: { ticks: "1", timebase: 30 },
        decoded,
        rasters,
        captions,
        graphics,
      },
      expect.anything(),
    );
    expect(bridge.maskRender).toHaveBeenCalledOnce();
    expect(bridge.blendRender).not.toHaveBeenCalled();
    expect(bridge.render).not.toHaveBeenCalled();
    expect(fixture.api.v4.visualPreviewTextFrame).not.toHaveBeenCalled();
  } finally {
    fixture.renderer.close();
  }
});

it.each([
  ["bound_graphic", "null"],
  ["bound_graphic", "absent"],
  ["bound_blend", "null"],
  ["bound_blend", "absent"],
  ["bound", "null"],
  ["bound", "absent"],
])(
  "rejects %s=%s without substituting another nested mask stage",
  async (field, value) => {
    const fixture = maskRoundtripFixture((reply) => {
      if (value === "null") reply[field] = null;
      else delete reply[field];
      return reply;
    });
    bridge.maskRender.mockImplementation(async (_plan, host) => {
      await host.callback("bindGraphicFrameV1", { graphics: [] });
      await host.callback("bindBlendFrameV1", {});
      await host.callback("bindMaskFrameV1", {});
    });
    try {
      await expect(
        fixture.renderer.render(
          fixture.api,
          fixture.session,
          new AbortController().signal,
          "mask",
        ),
      ).rejects.toThrow(
        field === "bound_graphic"
          ? "Missing bound visual graphic frame"
          : "Missing bound visual mask frame",
      );
      expect(fixture.call).toHaveBeenCalledTimes(2);
    } finally {
      fixture.renderer.close();
    }
  },
);

it.each(["graphic", "blend"])(
  "does not reuse a mask binding abandoned after its %s stage",
  async (stage) => {
    const fixture = maskRoundtripFixture();
    bridge.maskRender.mockImplementationOnce(async (_plan, host) => {
      await host.callback("bindGraphicFrameV1", { graphics: [] });
      if (stage === "blend") await host.callback("bindBlendFrameV1", {});
      throw new Error("Abandoned mask frame");
    });
    bridge.maskRender.mockImplementationOnce(async (_plan, host) => {
      await expect(host.callback("bindMaskFrameV1", {})).rejects.toThrow(
        "Missing completed visual mask binding",
      );
      await expect(host.callback("bindBlendFrameV1", {})).rejects.toThrow(
        "Missing bound visual blend frame",
      );
      expect(await host.callback("bindGraphicFrameV1", { graphics: [] })).toBe(
        fixture.bindings[1].graphic,
      );
      expect(await host.callback("bindBlendFrameV1", {})).toBe(
        fixture.bindings[1].blend,
      );
      expect(await host.callback("bindMaskFrameV1", {})).toBe(
        fixture.bindings[1].mask,
      );
      expect(fixture.bindings[1].mask).not.toBe(fixture.bindings[0].mask);
    });
    try {
      await expect(
        fixture.renderer.render(
          fixture.api,
          fixture.session,
          new AbortController().signal,
          "mask",
        ),
      ).rejects.toThrow("Abandoned mask frame");
      await fixture.renderer.render(
        fixture.api,
        fixture.session,
        new AbortController().signal,
        "mask",
      );
      expect(fixture.call).toHaveBeenCalledTimes(4);
    } finally {
      fixture.renderer.close();
    }
  },
);

it.each(["during-graphic", "after-blend"])(
  "discards the final mask reply when cancelled %s",
  async (stage) => {
    const cancel = new AbortController(),
      reason = new Error("Mask preview replaced");
    const fixture = maskRoundtripFixture((reply) => {
      if (stage === "during-graphic") cancel.abort(reason);
      return reply;
    });
    bridge.maskRender.mockImplementation(async (_plan, host) => {
      await host.callback("bindGraphicFrameV1", { graphics: [] });
      await host.callback("bindBlendFrameV1", {});
      cancel.abort(reason);
      await host.callback("bindMaskFrameV1", {});
    });
    try {
      await expect(
        fixture.renderer.render(
          fixture.api,
          fixture.session,
          cancel.signal,
          "mask",
        ),
      ).rejects.toBe(reason);
      expect(fixture.call).toHaveBeenCalledTimes(2);
    } finally {
      fixture.renderer.close();
    }
  },
);

function fixedFont(id: string) {
  return {
    fontId: id,
    contentHash: "a".repeat(64),
    size: "3",
    weight: 400,
    style: "normal",
    faceIndex: 0,
  };
}

async function warmFontFixture(
  profile: VisualPreviewProfile = "text",
  count = 2,
) {
  const pixels = vi.fn(() => ({ width: 2, height: 2 }));
  vi.stubGlobal(
    "OffscreenCanvas",
    class {
      getContext() {
        return null;
      }
      transferToImageBitmap() {
        return pixels();
      }
    },
  );
  const fonts = Array.from({ length: count }, (_, i) => fixedFont(`font-${i}`));
  let planned = fonts.slice(0, 2);
  const authorize = vi.fn(
    async (input: { font_id: string }, _options: { signal: AbortSignal }) => ({
      schema_version: 2,
      font_id: input.font_id,
    }),
  );
  const reply = () => ({
    plan: planFor(
      profile,
      { width: 2, height: 2, time: { ticks: "1", timebase: 2 } },
      planned,
    ),
    admission: { maxEncodedBytes: "1024" },
    bound_media: { media: true },
  });
  const call = vi.fn(async (name, input, options) =>
    name === "opencut_visual_preview_v4_font_authorize"
      ? authorize(input, options)
      : reply(),
  );
  const download = vi.fn(async () => new Response(new Uint8Array([1, 2, 3])));
  const api = {
    v4: { visualPreviewTextFrame: vi.fn(async () => reply()) },
    sdk: { call, downloadVisualPreviewFontV2: download },
  } as unknown as VisualPreviewExecutionApi;
  const render = profile === "blend" ? bridge.blendRender : bridge.render;
  render.mockImplementation(
    async (_plan, _host, _realm, _signal, _media, _decoder, cache) => {
      for (const font of fonts)
        cache.set(`${font.fontId}:${font.contentHash}:${font.size}`, {
          bytes: new Uint8Array([1, 2, 3]),
        });
    },
  );
  const renderer = new VisualPreviewRenderer(),
    session = {
      session_id: "font-session",
      generation: "1",
      overlay_epoch: "0",
      owner_id: "owner",
      lease_id: "lease",
    };
  await renderer.render(api, session, new AbortController().signal, profile);
  pixels.mockClear();
  return {
    renderer,
    session,
    api,
    fonts,
    render,
    authorize,
    download,
    pixels,
    plan: (next: typeof fonts) => {
      planned = next;
    },
    run: (signal = new AbortController().signal) =>
      renderer.render(api, session, signal, profile),
  };
}

it.each(["text", "caption", "graphic", "blend"] as const)(
  "starts both warm %s font authorizations before media and renews them on the next frame",
  async (profile) => {
    const f = await warmFontFixture(profile),
      entered = deferred<void>(),
      media = deferred<void>();
    const approvals = f.fonts.map(() => deferred<void>());
    f.authorize.mockImplementation(async ({ font_id }) => {
      await approvals[f.fonts.findIndex((font) => font.fontId === font_id)]
        .promise;
      return { schema_version: 2, font_id };
    });
    let uses = 0;
    f.render.mockImplementation(async (_plan, host) => {
      expect(f.authorize).toHaveBeenCalledTimes(2);
      entered.resolve();
      await media.promise;
      expect(await host.callback("bindFrame", { decoded: [] })).toEqual({
        media: true,
      });
      for (const font of f.fonts) {
        await host.callback("resolveVerifiedFontBytes", { font });
        uses++;
      }
    });
    try {
      const pending = f.run();
      await entered.promise;
      expect(uses).toBe(0);
      approvals.forEach((approval) => approval.resolve());
      media.resolve();
      await pending;
      expect(uses).toBe(2);
      expect(f.download).not.toHaveBeenCalled();
      f.render.mockImplementation(async (_plan, host) => {
        for (const font of f.fonts)
          await host.callback("resolveVerifiedFontBytes", { font });
      });
      await f.run();
      expect(f.authorize).toHaveBeenCalledTimes(4);
      expect(f.pixels).toHaveBeenCalledTimes(2);
    } finally {
      f.renderer.close();
    }
  },
);

it("prefetches only distinct planned cached fonts and keeps cold downloads at actual use", async () => {
  const f = await warmFontFixture(),
    cold = fixedFont("cold");
  f.plan([f.fonts[0], f.fonts[0], cold]);
  f.render.mockImplementation(async (_plan, host) => {
    expect(f.authorize).toHaveBeenCalledTimes(1);
    expect(f.authorize.mock.calls[0][0].font_id).toBe(f.fonts[0].fontId);
    expect(f.download).not.toHaveBeenCalled();
    await host.callback("bindFrame", { decoded: [] });
    await host.callback("resolveVerifiedFontBytes", { font: f.fonts[0] });
    await host.callback("resolveVerifiedFontBytes", { font: cold });
  });
  try {
    await f.run();
    expect(f.authorize).toHaveBeenCalledTimes(1);
    expect(f.download).toHaveBeenCalledTimes(1);
  } finally {
    f.renderer.close();
  }
});

it("surfaces early speculative revocation at font use without unhandled rejection or presentation", async () => {
  const f = await warmFontFixture(),
    failure = new Error("Font permission revoked");
  f.authorize.mockRejectedValue(failure);
  f.render.mockImplementation(async (_plan, host) => {
    await new Promise((resolve) => setTimeout(resolve, 0));
    await host.callback("resolveVerifiedFontBytes", { font: f.fonts[0] });
  });
  try {
    await expect(f.run()).rejects.toBe(failure);
    expect(f.pixels).not.toHaveBeenCalled();
    expect(f.download).not.toHaveBeenCalled();
    f.authorize.mockImplementation(async ({ font_id }) => ({
      schema_version: 2,
      font_id,
    }));
    await f.run();
    expect(f.authorize).toHaveBeenCalledTimes(4);
  } finally {
    f.renderer.close();
  }
});

it.each([
  { schema_version: 1, font_id: "font-0" },
  { schema_version: 2, font_id: "another-font" },
])("rejects changed cached-font authorization identity %j", async (reply) => {
  const f = await warmFontFixture();
  f.authorize.mockResolvedValue(reply);
  f.render.mockImplementation(async (_plan, host) =>
    host.callback("resolveVerifiedFontBytes", { font: f.fonts[0] }),
  );
  try {
    await expect(f.run()).rejects.toThrow(
      "Preview font authorization changed identity",
    );
    expect(f.pixels).not.toHaveBeenCalled();
  } finally {
    f.renderer.close();
  }
});

it("checks abort after a cached authorization resolves and cancels all speculative requests", async () => {
  const f = await warmFontFixture(),
    approval = deferred<void>(),
    entered = deferred<void>();
  const cancel = new AbortController(),
    reason = new Error("Preview replaced");
  f.authorize.mockImplementation(async ({ font_id }) => {
    await approval.promise;
    return { schema_version: 2, font_id };
  });
  f.render.mockImplementation(async (_plan, host) => {
    entered.resolve();
    await host.callback("resolveVerifiedFontBytes", { font: f.fonts[0] });
  });
  try {
    const pending = f.run(cancel.signal),
      rejected = expect(pending).rejects.toBe(reason);
    await entered.promise;
    cancel.abort(reason);
    expect(
      f.authorize.mock.calls.every(([, options]) => options.signal.aborted),
    ).toBe(true);
    approval.resolve();
    await rejected;
    expect(f.pixels).not.toHaveBeenCalled();
  } finally {
    f.renderer.close();
  }
});

it("cancels abandoned speculative requests when media fails without masking the original error", async () => {
  const f = await warmFontFixture(),
    failure = new Error("Media failed");
  f.authorize.mockImplementation(
    (_input, { signal }) =>
      new Promise((_resolve, reject) =>
        signal.addEventListener("abort", () => reject(signal.reason), {
          once: true,
        }),
      ),
  );
  f.render.mockImplementation(async () => {
    throw failure;
  });
  try {
    await expect(f.run()).rejects.toBe(failure);
    expect(
      f.authorize.mock.calls.every(([, options]) => options.signal.aborted),
    ).toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(f.pixels).not.toHaveBeenCalled();
  } finally {
    f.renderer.close();
  }
});

it("requires every prefetched authorization to succeed before publishing even if a callback is skipped", async () => {
  const f = await warmFontFixture(),
    failure = new Error("Unused font revoked");
  f.authorize.mockImplementation(async ({ font_id }) => {
    if (font_id === f.fonts[1].fontId) throw failure;
    return { schema_version: 2, font_id };
  });
  f.render.mockImplementation(async (_plan, host) =>
    host.callback("resolveVerifiedFontBytes", { font: f.fonts[0] }),
  );
  try {
    await expect(f.run()).rejects.toBe(failure);
    expect(f.pixels).not.toHaveBeenCalled();
  } finally {
    f.renderer.close();
  }
});

it.each(["unplanned", "changed-face"])(
  "rejects a %s font callback without an extra authorization",
  async (kind) => {
    const f = await warmFontFixture();
    f.plan([f.fonts[0]]);
    f.render.mockImplementation(async (_plan, host) =>
      host.callback("resolveVerifiedFontBytes", {
        font:
          kind === "unplanned" ? f.fonts[1] : { ...f.fonts[0], weight: 700 },
      }),
    );
    try {
      await expect(f.run()).rejects.toThrow("Unplanned preview font resource");
      expect(f.authorize).toHaveBeenCalledTimes(1);
      expect(f.download).not.toHaveBeenCalled();
    } finally {
      f.renderer.close();
    }
  },
);

it("rejects more than 32 distinct planned fonts before starting speculative authorization", async () => {
  const f = await warmFontFixture("text", 33);
  f.plan(f.fonts);
  f.render.mockClear();
  try {
    await expect(f.run()).rejects.toThrow(
      "Preview font collection exceeds resource admission",
    );
    expect(f.authorize).not.toHaveBeenCalled();
    expect(f.render).not.toHaveBeenCalled();
  } finally {
    f.renderer.close();
  }
});

function draftSceneHarness(digest: (phase: number) => string = () => `sha256:${"a".repeat(64)}`) {
  vi.stubGlobal("OffscreenCanvas", class {
    width = 2; height = 2;
    transferToImageBitmap() { return { width: 2, height: 2, close: vi.fn() }; }
  });
  const time = { ticks: "1", timebase: 2 };
  const identity = { width: 2, height: 2, time };
  const text = { identity, texts: [] };
  const caption = { identity, execution: text, captions: [] };
  const graphic = { identity, execution: caption, graphics: [] };
  const blend = { identity, execution: graphic, blends: [] };
  const plan = { identity, execution: blend, stacks: [], steps: [], maskFonts: [], fontSynthesis: [], background: {} };
  let phase = 0;
  const call = vi.fn(async (_name: string, input: { draft_id: string; gesture_intent?: string; transaction: unknown; frame: Record<string, unknown> }) => ({
    draft_id: input.draft_id, transaction_digest: digest(++phase),
    frame: { plan, admission: { maxEncodedBytes: "1024" },
      bound_media: { media: true }, bound_execution: { execution: true },
      bound_graphic: { graphic: true }, bound_blend: { blend: true }, bound: { steps: [] },
    },
  }));
  const api = { sdk: { call }, v4: {} } as unknown as VisualPreviewExecutionApi;
  const session = { session_id: "s", generation: "1", overlay_epoch: "0", owner_id: "o", lease_id: "l" };
  const transaction = { project_id: "p", expected_revision: "r", idempotency_key: "preview-only", label: "transform", operations: [] };
  const draft = { draftId: "drag-1", transaction, time } as Parameters<VisualPreviewRenderer["render"]>[5];
  return { api, session, call, draft, time };
}

it("uses the same transform draft for every Scene bind and the captured exact time", async () => {
  const h = draftSceneHarness();
  bridge.sceneRender.mockImplementation(async (_plan, host) => {
    await host.callback("bindFrame", { decoded: [] });
    await host.callback("bindCaptionFrameV3", { rasters: [], captions: [] });
    await host.callback("bindGraphicFrameV1", { graphics: [] });
    expect(await host.callback("bindBlendFrameV1", {})).toEqual({ blend: true });
    expect(await host.callback("bindSceneFrameV1", {})).toEqual({ steps: [] });
  });
  const renderer = new VisualPreviewRenderer();
  await renderer.render(h.api, h.session, new AbortController().signal, "scene", undefined, h.draft);
  expect(h.call).toHaveBeenCalledTimes(4);
  for (const [name, input] of h.call.mock.calls) {
    expect(name).toBe("opencut_visual_preview_v4_transform_draft_frame");
    expect(input.transaction).toBe(h.draft!.transaction);
    expect(input.draft_id).toBe("drag-1");
    expect(input.frame.time).toEqual(h.time);
    expect(input.frame.session).toBe(h.session);
  }
  renderer.close();
});

it("rejects changed transaction digest during a later binding phase", async () => {
  const h = draftSceneHarness((phase) => `sha256:${(phase === 1 ? "a" : "b").repeat(64)}`);
  bridge.sceneRender.mockImplementation(async (_plan, host) => host.callback("bindFrame", { decoded: [] }));
  const renderer = new VisualPreviewRenderer();
  await expect(renderer.render(h.api, h.session, new AbortController().signal, "scene", undefined, h.draft)).rejects.toThrow("Transform draft binding changed identity");
  expect(h.call).toHaveBeenCalledTimes(2);
  renderer.close();
});

it("keeps a started draft HTTP phase alive but stops further stale phases after intent cancellation", async () => {
  const h = draftSceneHarness(), intent = new AbortController(), lifetime = new AbortController();
  let finish!: (value: unknown) => void;
  const call = vi.fn((_name, _input, _options) => new Promise((resolve) => { finish = resolve; }));
  const api = { sdk: { call }, v4: {} } as unknown as VisualPreviewExecutionApi;
  const renderer = new VisualPreviewRenderer();
  const pending = renderer.render(api, h.session, lifetime.signal, "scene", undefined, { ...h.draft!, intentSignal: intent.signal });
  intent.abort(new DOMException("Superseded", "AbortError"));
  expect(call.mock.calls[0][2].signal.aborted).toBe(false);
  finish({ draft_id: "drag-1", transaction_digest: `sha256:${"a".repeat(64)}`, frame: {} });
  await expect(pending).rejects.toThrow("Superseded");
  expect(call).toHaveBeenCalledTimes(1);
  expect(bridge.sceneRender).not.toHaveBeenCalled();
  renderer.close();
});

it.each(["translate", "rotate", "scale_x", "scale_y", "scale_xy"] as const)("captures explicit %s intent in every V2 draft binding", async (gestureIntent) => {
  const h = draftSceneHarness();
  bridge.sceneRender.mockImplementation(async (_plan, host) => {
    await host.callback("bindFrame", { decoded: [] });
    await host.callback("bindCaptionFrameV3", { rasters: [], captions: [] });
    await host.callback("bindGraphicFrameV1", { graphics: [] });
    await host.callback("bindBlendFrameV1", {});
    await host.callback("bindSceneFrameV1", {});
  });
  const renderer = new VisualPreviewRenderer();
  await renderer.render(h.api, h.session, new AbortController().signal, "scene", undefined, { ...h.draft!, gestureIntent });
  expect(h.call).toHaveBeenCalledTimes(4);
  for (const [name, input] of h.call.mock.calls) {
    expect(name).toBe("opencut_visual_preview_v4_transform_draft_v2_frame");
    expect(input.gesture_intent).toBe(gestureIntent);
    expect(input.transaction).toBe(h.draft!.transaction);
  }
  renderer.close();
});
