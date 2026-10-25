// @vitest-environment jsdom
import {afterEach,expect,it,vi} from "vitest";
const bridge=vi.hoisted(()=>{const render=vi.fn();return {render,sceneRender:render,destroy:vi.fn(),open:vi.fn(),prepare:vi.fn(async():Promise<{token:object;close():Promise<void>}|null>=>null)}});
vi.mock("../../../../../../packages/classic-visual-renderer/src/index.mjs",()=>({
 openGpuRealm:async(options:{fontHashWorker?:boolean})=>{bridge.open(options);return {device:{destroy:bridge.destroy},observation:{}}},
 closeGpuRealm:(realm:{device:{destroy():void}})=>realm.device.destroy(),enableImmutablePreviewFonts:()=>{},
 VisualDecoderCache:class{close(){}},renderClassicSceneFrame:bridge.sceneRender,preparePreviewFontFrame:bridge.prepare,
}));
import {VisualPreviewRenderer,type VisualPreviewExecutionApi} from "./visual-preview-renderer";
function planFor(identity:object,fonts:object[]=[]) {
 const text={identity,texts:fonts.map(font=>({font})),media:{identity,resources:[],layers:[]}};
 const caption={identity,execution:text,captions:[]};
 const graphic={identity,execution:caption,graphics:[]},blend={identity,execution:graphic,blends:[]};
 return {identity,execution:blend,stacks:[],steps:[],maskFonts:[],fontSynthesis:[],background:{}};
}
afterEach(()=>{vi.unstubAllGlobals();vi.clearAllMocks();bridge.prepare.mockReset().mockResolvedValue(null)});
function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
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
    authorize = vi.fn(async (_input:unknown,_options:unknown) => ({ font_id: "font" }));
  const api = {
    scene:{},sdk:{downloadVisualPreviewFont:download,call:vi.fn(async(name,input,options)=>
      name==="opencut_visual_preview_frame" ? {plan:planFor({width:2,height:2},[font]),admission:{maxEncodedBytes:"1024"}} : authorize(input,options))},
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
      font_id: input.font_id,
    }),
  );
  const reply = () => ({
    plan: planFor(
      { width: 2, height: 2, time: { ticks: "1", timebase: 2 } },
      planned,
    ),
    admission: { maxEncodedBytes: "1024" },
    bound_media: { media: true },
  });
  const call = vi.fn(async (name, input, options) => {
    if (name === "opencut_visual_preview_font_authorize") return authorize(input, options);
    if (name === "opencut_visual_preview_font_batch_authorize") {
      const results = await Promise.all(input.font_ids.map((font_id: string) => authorize({ font_id }, options)));
      return {font_ids: results.map((r) => r.font_id) };
    }
    return reply();
  });
  const download = vi.fn(async () => new Response(new Uint8Array([1, 2, 3])));
  const api = {
    scene: {},
    sdk: { call, downloadVisualPreviewFont: download },
  } as unknown as VisualPreviewExecutionApi;
  const render = bridge.sceneRender;
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
  await renderer.render(api, session, new AbortController().signal);
  pixels.mockClear();
  return {
    renderer,
    session,
    api,
    fonts,
    render,
    authorize,
    call,
    download,
    pixels,
    plan: (next: typeof fonts) => {
      planned = next;
    },
    run: (signal = new AbortController().signal) =>
      renderer.render(api, session, signal),
  };
}

it("samples a fresh service frame after font preparation and forwards one scoped proof without retaining its old time", async () => {
  const f = await warmFontFixture(), gate = deferred<void>();
  const token = {}, close = vi.fn(async () => {}), ready = deferred<void>();
  bridge.prepare.mockImplementationOnce(async () => { ready.resolve(); await gate.promise; return { token, close }; });
  let sampled = 0;
  f.call.mockImplementation(async (name, input) => {
    if (name === "opencut_visual_preview_font_batch_authorize") return { font_ids: input.font_ids };
    const identity = { projectId: "p", revision: "r", sequenceId: "s", width: 2, height: 2, time: { ticks: String(++sampled), timebase: 10 } };
    return { plan: planFor(identity, f.fonts), admission: { maxEncodedBytes: "1024" }, bound_media: { media: true } };
  });
  f.render.mockImplementation(async (plan, host) => {
    expect(plan.identity.time.ticks).toBe("2");expect(host.preparedFontFrame).toBe(token);
    for (const font of f.fonts) await host.callback("resolveVerifiedFontBytes", { font });
  });
  try {
    const pending = f.run();await ready.promise;expect(sampled).toBe(1);expect(f.pixels).not.toHaveBeenCalled();
    gate.resolve();const result = await pending;expect(sampled).toBe(2);expect(result.frame.plan.identity.time.ticks).toBe("2");expect(close).toHaveBeenCalledOnce();
  } finally { f.renderer.close(); }
});

it("starts all warm font authorizations before media and renews them on the next frame",async()=>{
    const f = await warmFontFixture(),
      entered = deferred<void>(),
      media = deferred<void>();
    const approvals = f.fonts.map(() => deferred<void>());
    f.authorize.mockImplementation(async ({ font_id }) => {
      await approvals[f.fonts.findIndex((font) => font.fontId === font_id)]
        .promise;
      return { font_id };
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
      expect(f.call.mock.calls.filter(([name]) => name === "opencut_visual_preview_font_batch_authorize")).toHaveLength(1);
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
      font_id,
    }));
    await f.run();
    expect(f.authorize).toHaveBeenCalledTimes(4);
  } finally {
    f.renderer.close();
  }
});

it.each([
  { font_id: "another-font" },
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
    return { font_id };
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
    return { font_id };
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
  const f = await warmFontFixture(33);
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
    getContext() { return null; }
    transferToImageBitmap() { return { width: 2, height: 2, close: vi.fn() }; }
  });
  const time = { ticks: "1", timebase: 2 };
  const identity = { width: 2, height: 2, time };
  const plan=planFor(identity);
  let phase = 0;
  const call = vi.fn(async (_name: string, input: { draft_id: string; intent?: unknown; transaction: unknown; frame: Record<string, unknown> }) => ({
    draft_id: input.draft_id, transaction_digest: digest(++phase),
    frame: { plan, admission: { maxEncodedBytes: "1024" },
      bound_media: { media: true }, bound_execution: { execution: true },
      bound_graphic: { graphic: true }, bound_blend: { blend: true }, bound: { steps: [] },
    },
  }));
  const api = { sdk: { call }, scene: {} } as unknown as VisualPreviewExecutionApi;
  const session = { session_id: "s", generation: "1", overlay_epoch: "0", owner_id: "o", lease_id: "l" };
  const transaction = { project_id: "p", expected_revision: "r", idempotency_key: "preview-only", label: "transform", operations: [] };
  const draft = { draftId: "drag-1", intent: {kind:"gesture" as const,gesture:"translate" as const}, transaction, time } as Parameters<VisualPreviewRenderer["render"]>[4];
  return { api, session, call, draft, time };
}

it("binds empty text and graphics with decoded media once per live frame, without reusing another frame's authorization", async () => {
  const h = draftSceneHarness();
  const call = vi.fn(async (_name, input) => {
    const reply = await h.call("frame", { frame: input } as never);
    return reply.frame;
  });
  const api = { sdk: { call }, scene: {} } as unknown as VisualPreviewExecutionApi;
  bridge.sceneRender.mockImplementation(async (_plan, host) => {
    expect(host.rangePrefetch).toBe(false);
    expect(host.rangeReadAhead).toBe(true);
    expect(host.rangeChunkBytes).toBe(524288);
    expect(await host.callback("bindFrame", { decoded: [] })).toEqual({ media: true });
    expect(await host.callback("bindCaptionFrameV3", { rasters: [], captions: [] })).toEqual({ execution: true });
    expect(await host.callback("bindGraphicFrameV1", { graphics: [] })).toEqual({ graphic: true });
    expect(await host.callback("bindBlendFrameV1", {})).toEqual({ blend: true });
    expect(await host.callback("bindSceneFrameV1", {})).toEqual({ steps: [] });
  });
  const renderer = new VisualPreviewRenderer();
  for (let i = 0; i < 2; i++) await renderer.render(api, h.session, new AbortController().signal);
  expect(call).toHaveBeenCalledTimes(4);
  for (const index of [1, 3]) expect(call.mock.calls[index][1]).toMatchObject({
    decoded: [], rasters: [], captions: [], graphics: [], time: h.time,
  });
  bridge.sceneRender.mockImplementation(async (_plan, host) => {
    await host.callback("bindFrame", { decoded: [] });
    await host.callback("bindCaptionFrameV3", { rasters: [{ forged: true }], captions: [] });
  });
  await expect(renderer.render(api, h.session, new AbortController().signal)).rejects.toThrow("Empty text binding changed observations");
  renderer.close();
});

it("keeps retained video reads alive past a frame deadline and closes them on generation change", async () => {
  const h = draftSceneHarness(), hosts: any[] = [];
  const readVisualPreviewResource = vi.fn(async (_input, options) => {
    options.signal.throwIfAborted();
    return { bytes: new Uint8Array([17]) };
  });
  const call = vi.fn(async (name, input) => name === "opencut_visual_preview_resource_open"
    ? { transferId: "video", byteLength: "1" }
    : (await h.call("frame", { frame: input } as never)).frame);
  const api = { sdk: { call, readVisualPreviewResource }, scene: {} } as unknown as VisualPreviewExecutionApi;
  bridge.sceneRender.mockImplementation(async (_plan, host) => { hosts.push(host); });
  const renderer = new VisualPreviewRenderer(), deadline = new AbortController();
  await renderer.render(api, h.session, deadline.signal);
  await hosts[0].callback("resolveVerifiedBytes", { resource: { kind: "video", assetId: "v", size: "1" } });
  deadline.abort();
  await renderer.render(api, h.session, new AbortController().signal);
  expect(hosts[0].rangeSignal).toBe(hosts[1].rangeSignal);
  expect(await hosts[0].readVerifiedBytes("video", 0, 1)).toEqual(new Uint8Array([17]));
  await renderer.render(api, { ...h.session, generation: "2" }, new AbortController().signal);
  expect(hosts[0].rangeSignal.aborted).toBe(true);
  await expect(hosts[0].readVerifiedBytes("video", 0, 1)).rejects.toThrow();
  expect(readVisualPreviewResource).toHaveBeenCalledTimes(1);
  renderer.close();
  expect(hosts[2].rangeSignal.aborted).toBe(true);
});

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
  await renderer.render(h.api, h.session, new AbortController().signal, undefined, h.draft);
  expect(h.call).toHaveBeenCalledTimes(4);
  for (const [name, input] of h.call.mock.calls) {
    expect(name).toBe("opencut_visual_preview_edit_draft_frame");
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
  await expect(renderer.render(h.api, h.session, new AbortController().signal, undefined, h.draft)).rejects.toThrow("Transform draft binding changed identity");
  expect(h.call).toHaveBeenCalledTimes(2);
  renderer.close();
});

it("keeps a started draft HTTP phase alive but stops further stale phases after intent cancellation", async () => {
  const h = draftSceneHarness(), intent = new AbortController(), lifetime = new AbortController();
  let finish!: (value: unknown) => void;
  const call = vi.fn((_name, _input, _options) => new Promise((resolve) => { finish = resolve; }));
  const api = { sdk: { call }, scene: {} } as unknown as VisualPreviewExecutionApi;
  const renderer = new VisualPreviewRenderer();
  const pending = renderer.render(api, h.session, lifetime.signal, undefined, { ...h.draft!, intentSignal: intent.signal });
  intent.abort(new DOMException("Superseded", "AbortError"));
  expect(call.mock.calls[0][2].signal.aborted).toBe(false);
  finish({ draft_id: "drag-1", transaction_digest: `sha256:${"a".repeat(64)}`, frame: {} });
  await expect(pending).rejects.toThrow("Superseded");
  expect(call).toHaveBeenCalledTimes(1);
  expect(bridge.sceneRender).not.toHaveBeenCalled();
  renderer.close();
});

it.each(["translate", "rotate", "scale_x", "scale_y", "scale_xy"] as const)("captures explicit %s intent in every visual edit draft binding", async (gestureIntent) => {
  const h = draftSceneHarness();
  bridge.sceneRender.mockImplementation(async (_plan, host) => {
    await host.callback("bindFrame", { decoded: [] });
    await host.callback("bindCaptionFrameV3", { rasters: [], captions: [] });
    await host.callback("bindGraphicFrameV1", { graphics: [] });
    await host.callback("bindBlendFrameV1", {});
    await host.callback("bindSceneFrameV1", {});
  });
  const renderer = new VisualPreviewRenderer();
  await renderer.render(h.api, h.session, new AbortController().signal, undefined, { ...h.draft!, intent: {kind:"gesture",gesture:gestureIntent} });
  expect(h.call).toHaveBeenCalledTimes(4);
  for (const [name, input] of h.call.mock.calls) {
    expect(name).toBe("opencut_visual_preview_edit_draft_frame");
    expect(input.intent).toEqual({kind:"gesture",gesture:gestureIntent});
    expect(input.transaction).toBe(h.draft!.transaction);
  }
  renderer.close();
});

it("binds static opacity intent and the original transaction through every native frame phase", async () => {
  const h = draftSceneHarness();
  bridge.sceneRender.mockImplementation(async (_plan, host) => {
    await host.callback("bindFrame", {decoded:[]});
    await host.callback("bindCaptionFrameV3", {rasters:[], captions:[]});
    await host.callback("bindGraphicFrameV1", {graphics:[]});
    await host.callback("bindBlendFrameV1", {});
    await host.callback("bindSceneFrameV1", {});
  });
  const renderer = new VisualPreviewRenderer();
  await renderer.render(h.api,h.session,new AbortController().signal,undefined,
    {...h.draft!,intent:{kind:"property",property:"opacity"}});
  expect(h.call).toHaveBeenCalledTimes(4);
  for (const [name,input] of h.call.mock.calls) {
    expect(name).toBe("opencut_visual_preview_edit_draft_frame");
    expect(input.intent).toEqual({kind:"property",property:"opacity"});
    expect(input.transaction).toBe(h.draft!.transaction);
  }
  renderer.close();
});

it("keeps preparation time and native buffer reservation through all bindings", async () => {
  vi.stubGlobal("OffscreenCanvas", class {
    width=2; height=2;
    getContext(){ return null; }
    transferToImageBitmap(){return {width:2,height:2,close:vi.fn()};}
  });
  const time = { ticks:"1", timebase:3 }, session = {session_id:"s",generation:"1",overlay_epoch:"0",owner_id:"o",lease_id:"l"};
  const call = vi.fn(async () => ({prepare_ahead:true,next_time:{ticks:"11",timebase:30},
    plan:planFor({width:2,height:2,time}),admission:{maxEncodedBytes:"1024",maxRgbaBytes:"256",maxGpuBytes:"512"},
    bound_media:{},bound_execution:{},bound_graphic:{},bound_blend:{},bound:{steps:[]}}));
  bridge.sceneRender.mockImplementationOnce(async (_plan,host) => {
    expect(host.admission).toMatchObject({maxRgbaBytes:"256",maxGpuBytes:"512"});
    await host.callback("bindFrame",{decoded:[]});
  });
  const renderer = new VisualPreviewRenderer();
  await renderer.render({scene:{},sdk:{call}} as unknown as VisualPreviewExecutionApi,session,new AbortController().signal,
    undefined,undefined,{time,reservedBytes:32});
  expect(call).toHaveBeenCalledTimes(2);
  for(const args of call.mock.calls as unknown as Array<[unknown,unknown]>)
    expect(args[1]).toMatchObject({prepare_ahead:true,retained_picture_bytes:32,time});
  renderer.close();
});

it("overlaps only next-frame preparation with GPU work and still rebinds every frame under conservative memory admission",async()=>{
 vi.stubGlobal("OffscreenCanvas",class{width=2;height=2;getContext(){return null;}transferToImageBitmap(){return {width:2,height:2,close:vi.fn()};}});
 const session={session_id:"s",generation:"1",overlay_epoch:"0",owner_id:"o",lease_id:"l"},next=deferred<unknown>();
 const frame=(time:{ticks:string;timebase:number})=>({prepare_ahead:true,next_time:{ticks:String(Number(time.ticks)+1),timebase:25},
  plan:planFor({projectId:"p",revision:"r",sequenceId:"scene",width:2,height:2,time}),admission:{maxEncodedBytes:"1024",maxRgbaBytes:"256",maxGpuBytes:"512"},
  bound_media:{},bound_execution:{},bound_graphic:{},bound_blend:{},bound:{steps:[]}});
 const call=vi.fn(async(_name:string,input:{decoded:unknown;time:{ticks:string;timebase:number};retained_picture_bytes:number})=>input.decoded===null&&input.time.ticks==="1"?next.promise:frame(input.time));
 const api={scene:{},sdk:{call}} as unknown as VisualPreviewExecutionApi;
 bridge.sceneRender.mockImplementation(async(_plan,host)=>{await host.callback("bindFrame",{decoded:[]});await host.beforeGpu();});
 const renderer=new VisualPreviewRenderer(),signal=new AbortController().signal,end={ticks:"2",timebase:25};
 await renderer.render(api,session,signal,undefined,undefined,{time:{ticks:"0",timebase:25},reservedBytes:32,end});
 expect(call).toHaveBeenCalledTimes(3); // The current frame did not await the pending next plan.
 next.resolve(frame({ticks:"1",timebase:25}));
 await renderer.render(api,session,signal,undefined,undefined,{time:{ticks:"1",timebase:25},reservedBytes:32,end});
 expect(call).toHaveBeenCalledTimes(4);
 expect(call.mock.calls.filter(([,input])=>input.decoded===null)).toHaveLength(2);
 expect(call.mock.calls.filter(([,input])=>Array.isArray(input.decoded))).toHaveLength(2);
 expect(call.mock.calls.at(-1)?.[1]).toMatchObject({time:{ticks:"1",timebase:25},retained_picture_bytes:48});
 renderer.close();
});

it("a next-frame authorization rejection cannot be bypassed by a fresh-looking prepared plan",async()=>{
 vi.stubGlobal("OffscreenCanvas",class{width=2;height=2;getContext(){return null;}transferToImageBitmap(){return {width:2,height:2,close:vi.fn()};}});
 const session={session_id:"s",generation:"1",overlay_epoch:"0",owner_id:"o",lease_id:"l"};
 const call=vi.fn(async(_name:string,input:{decoded:unknown;time:{ticks:string;timebase:number}})=>{
  if(input.decoded===null&&input.time.ticks==="1")throw Object.assign(Error("revoked"),{code:"NOT_AUTHORIZED"});
  return {prepare_ahead:true,next_time:{ticks:"1",timebase:25},plan:planFor({width:2,height:2,time:input.time}),admission:{maxEncodedBytes:"1024",maxRgbaBytes:"256",maxGpuBytes:"512"},bound_media:{},bound_execution:{},bound_graphic:{},bound_blend:{},bound:{steps:[]}};
 });
 bridge.sceneRender.mockImplementation(async(_plan,host)=>{await host.callback("bindFrame",{decoded:[]});await host.beforeGpu();});
 const renderer=new VisualPreviewRenderer(),api={scene:{},sdk:{call}} as unknown as VisualPreviewExecutionApi,signal=new AbortController().signal;
 await renderer.render(api,session,signal,undefined,undefined,{time:{ticks:"0",timebase:25},reservedBytes:0});
 await expect(renderer.render(api,session,signal,undefined,undefined,{time:{ticks:"1",timebase:25},reservedBytes:0})).rejects.toThrow("revoked");
 expect(call).toHaveBeenCalledTimes(3);renderer.close();
});

it("rebinds identical text observations with each native media frame and verifies current rasters",async()=>{
 const h=draftSceneHarness();
 const base=(await h.call("frame",{frame:{}} as never)).frame;
 base.plan.execution.execution.execution.execution.texts.push({elementId:"title",text:{content:"Hello"},font:fixedFont("f")} as never);
 const call=vi.fn(async(_name:string,_input:unknown)=>({...base,prepare_ahead:true,next_time:{ticks:"14",timebase:25}}));
 const api={sdk:{call},scene:{}} as unknown as VisualPreviewExecutionApi;
 let observed=[{elementId:"title",width:2,height:2,layout:{fontId:"f",x:1}}];
 bridge.sceneRender.mockImplementation(async(_plan,host)=>{
  await host.callback("bindFrame",{decoded:[]});
  await host.callback("bindCaptionFrameV3",{rasters:structuredClone(observed),captions:[]});
  await host.callback("bindGraphicFrameV1",{graphics:[]});
  await host.callback("bindBlendFrameV1",{});await host.callback("bindSceneFrameV1",{});
 });
 const renderer=new VisualPreviewRenderer(),preparation={time:h.time,reservedBytes:0};
 await renderer.render(api,h.session,new AbortController().signal,undefined,undefined,preparation);
 expect(call).toHaveBeenCalledTimes(3);
 await renderer.render(api,h.session,new AbortController().signal,undefined,undefined,preparation);
 expect(call).toHaveBeenCalledTimes(5);
 expect(call.mock.calls[4][1]).toMatchObject({decoded:[],rasters:observed,captions:[],graphics:[]});
 observed=[{...observed[0],layout:{fontId:"f",x:2}}];
 await expect(renderer.render(api,h.session,new AbortController().signal,undefined,undefined,preparation)).rejects.toThrow("Retained text observations changed");
 Object.assign(base.plan.execution.execution.execution.execution.texts[0],{text:{content:"Changed"}});
 await renderer.render(api,h.session,new AbortController().signal,undefined,undefined,preparation);
 expect(call.mock.calls.at(-1)![1]).toMatchObject({rasters:observed});
 await renderer.render(api,{...h.session,generation:"2"},new AbortController().signal,undefined,undefined,preparation);
 expect(call.mock.calls.at(-2)![1]).toMatchObject({rasters:null});
 renderer.close();
});
