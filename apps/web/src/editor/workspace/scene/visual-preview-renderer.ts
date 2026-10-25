import {
  measureClassicText,
  prepareCaptionText,
  prepareClassicAssSubtitle,
  openGpuRealm,
  gpuSurfacePresenter,
  gpuSurfaceBitmap,
  enableImmutablePreviewFonts,
  closeGpuRealm,
  renderClassicSceneFrame,
  preparePreviewFontFrame,
  VisualDecoderCache,
  PreviewSegmentDecoder,
  type GpuRealm,
} from "../../../../../../packages/classic-visual-renderer/src/index.mjs";
import type { EditorApi } from "../api";
import { readPreviewBytes } from "./visual-preview-bytes";
import { previewExecutionAdmission } from "./visual-preview-admission";
import type { ToolInput, ToolOutput } from "../../../lib/editor-sdk";
// Keep canvas editing on the same Classic measurement implementation as preview.
export {
  PreviewSegmentDecoder,
  measureClassicText,
  prepareCaptionText,
  prepareClassicAssSubtitle,
};

type Input = ToolInput<"opencut_visual_preview_frame">;
type Frame = ToolOutput<"opencut_visual_preview_frame">;
type ExecutionPlan = Frame["plan"];
type TextPlan = ExecutionPlan["execution"]["execution"]["execution"]["execution"];
type Font = TextPlan["texts"][number]["font"];
type FontAuthorization = {ok:true}|{ok:false;error:unknown};
function textExecution(plan:ExecutionPlan):TextPlan {return plan.execution.execution.execution.execution;}
const fontKey = (font: Font) =>
  `${font.fontId}:${font.contentHash}:${font.size}`;
export type VisualPreviewExecutionApi = {
  scene: Pick<EditorApi["scene"], "resource">;
  sdk: Pick<
    EditorApi["sdk"],
    "downloadProjectResource" | "downloadVisualPreviewFont" | "readVisualPreviewResource" | "call"
  >;
};

/** Only execution of application facts. No source clock, geometry or project
 * evaluation lives here. Cache is volatile, hash verified and session scoped. */
export class VisualPreviewRenderer {
  private realm: GpuRealm | null = null;
  private readonly cache = new Map<string, Uint8Array>();
  private readonly fonts = new Map<
    string,
    { bytes: Uint8Array; face?: FontFace }
  >();
  private readonly decoders = new VisualDecoderCache();
  private readonly surface = new OffscreenCanvas(1, 1);
  private lastDeferredLog = -Infinity;
  private resourceScope: string | null = null;
  private resourceAbort = new AbortController();
  private prefetched: {api:VisualPreviewExecutionApi;scope:string;time:NonNullable<Input["time"]>;reserved:number;signal:AbortSignal;
    result:Promise<{ok:true;frame:Frame}|{ok:false;error:unknown}>}|null=null;
  private textObservation:{key:string;rasters:Input["rasters"];captions:Input["captions"]}|null=null;
  private rangeReads = { count: 0, bytes: 0, elapsed: 0 };
  async render(
    api: VisualPreviewExecutionApi,
    session: Input["session"],
    signal: AbortSignal,
    hitQuery?: { x: number; y: number },
    transformDraft?: {
      draftId: string;
      intent: ToolInput<"opencut_visual_preview_edit_draft_frame">["intent"];
      transaction: ToolInput<"opencut_edit_apply">;
      time: Input["time"];
      intentSignal?: AbortSignal;
    },
    preparation?: { time: Input["time"]; reservedBytes: number; end?:NonNullable<Input["time"]>; captureSurface?:boolean },
  ) {
    const checkIntent = () => {
      signal.throwIfAborted();
      transformDraft?.intentSignal?.throwIfAborted();
    };
    const began = performance.now();
    const readsBefore = { ...this.rangeReads };
    const rangeMetrics = () => ({
      rangeReads: this.rangeReads.count - readsBefore.count,
      rangeBytes: this.rangeReads.bytes - readsBefore.bytes,
      rangeReadMs: this.rangeReads.elapsed - readsBefore.elapsed,
    });
    const resourceScope = JSON.stringify([
      session.session_id, session.generation, session.overlay_epoch,
      session.owner_id, session.lease_id,
    ]);
    if (resourceScope !== this.resourceScope) {
      if (this.resourceScope !== null) {
        this.resourceAbort.abort();
        await this.decoders.close();
        this.resourceAbort = new AbortController();
      }
      this.resourceScope = resourceScope;
      this.prefetched=null;
      this.textObservation=null;
    }
    // Retained decoder reads outlive a frame's startup deadline. They remain
    // bound to this exact session generation and are reauthorized by the host.
    const rangeSignal = this.resourceAbort.signal;
    let draftDigest: string | null = null;
    const actualReserved=preparation?.reservedBytes??0;
    let declaredReserved=actualReserved;
    const callFrame = (
      input: Omit<Input,"captions"|"graphics"> & Partial<Pick<Input,"captions"|"graphics">>,
      requestSignal:AbortSignal=signal,reserved=declaredReserved,
    ) => {
      checkIntent();
      if (transformDraft) {
        const request = {
          draft_id: transformDraft.draftId,
          transaction: transformDraft.transaction,
          frame: {
            ...input,
            time: input.time ?? transformDraft.time,
            captions: input.captions ?? null,
            graphics: input.graphics ?? null,
          },
        };
        const pending = api.sdk.call("opencut_visual_preview_edit_draft_frame",
          { ...request, intent: transformDraft.intent }, { signal });
        return pending
          .then((reply) => {
            checkIntent();
            if (
              reply.draft_id !== transformDraft.draftId ||
              !/^sha256:[a-f0-9]{64}$/u.test(reply.transaction_digest) ||
              (draftDigest !== null && draftDigest !== reply.transaction_digest)
            )
              throw new Error("Transform draft binding changed identity.");
            draftDigest = reply.transaction_digest;
            return reply.frame;
          });
      }
      return api.sdk.call("opencut_visual_preview_frame",{
        ...input,resolution:preparation?"preview540":"source",prepare_ahead:!!preparation,retained_picture_bytes:reserved,captions:input.captions??null,graphics:input.graphics??null,
      },{signal:requestSignal});
    };
    let preparedFrame:Frame|undefined;
    const prefetched=this.prefetched;this.prefetched=null;
    if(preparation?.time&&!hitQuery&&!transformDraft&&prefetched&&prefetched.api===api&&prefetched.scope===resourceScope&&!prefetched.signal.aborted&&
      BigInt(prefetched.time.ticks)*BigInt(preparation.time.timebase)===BigInt(preparation.time.ticks)*BigInt(prefetched.time.timebase)) {
      const cached=await prefetched.result;checkIntent();
      if(cached.ok){preparedFrame=cached.frame;declaredReserved=Math.max(actualReserved,prefetched.reserved);}
      else if(!(cached.error instanceof Error&&"code" in cached.error&&["VISUAL_BUDGET_EXCEEDED","PREVIEW_FRAME_STALE"].includes(String(cached.error.code))))throw cached.error;
    }
    let frame = preparedFrame ?? await callFrame({
      session,
      time: preparation?.time ?? null,
      decoded: null,
      rasters: null,
    });
    checkIntent();
    const executionAdmission = previewExecutionAdmission;
    if (preparation && !frame.prepare_ahead) throw Error("Service did not authorize picture preparation");
    let admission = executionAdmission(frame.admission);
    const fontsFor = (plan: ExecutionPlan) => new Map(
      [
        ...textExecution(plan).texts.map(({ font }) => font),
        ...("maskFonts" in plan
          ? plan.maskFonts.map((p) => p.font)
          : []),
      ].map((font) => [fontKey(font), font] as const),
    );
    let plannedFonts = fontsFor(frame.plan);
    // The renderer still admits the complete media/font set. Bound speculative
    // authorization by its existing resource limit before starting any calls.
    if (plannedFonts.size > 32)
      throw new Error("Preview font collection exceeds resource admission");
    let fontPreparation: Awaited<ReturnType<typeof preparePreviewFontFrame>> = null;
    let resourcePrepareMs = 0;
    const captured={pixels:null as (ImageBitmap&{ready:Promise<void>})|null};
    let sampledAt = began;
    try {
      // Resource hashing is independent of sampled geometry/time. Within this
      // single call, prepare it before asking Rust for the final consumer time.
      // Picking and transform drafts retain their original exact-time path.
      if (!hitQuery && !transformDraft && !preparation && plannedFonts.size &&
        [...plannedFonts.keys()].every((key) => this.fonts.has(key))) {
        this.realm ??= await openGpuRealm({ fontHashWorker: true });
        enableImmutablePreviewFonts(this.realm);
        const start = performance.now();
        fontPreparation = await preparePreviewFontFrame(
          [...plannedFonts.values()], frame.plan.identity, admission,
          this.realm, signal, this.cache, this.decoders, this.fonts,
        );
        resourcePrepareMs = performance.now() - start;
        checkIntent();
        if (fontPreparation) {
          const initial = frame;
          sampledAt = performance.now();
          frame = await callFrame({ session, time: null, decoded: null, rasters: null });
          checkIntent();
          admission = executionAdmission(frame.admission);
          const next = fontsFor(frame.plan);
          const sameScope = (["projectId", "revision", "sequenceId", "width", "height", "profile"] as const)
            .every((key) => frame.plan.identity[key] === initial.plan.identity[key]);
          const sameFonts = next.size === plannedFonts.size && [...next].every(([key, font]) => {
            const previous = plannedFonts.get(key);
            return previous && (["weight", "style", "faceIndex"] as const)
              .every((field) => font[field] === previous[field]);
          });
          const initialAdmission = previewExecutionAdmission(initial.admission);
          const sameAdmission = Object.keys(admission).length === Object.keys(initialAdmission).length &&
            Object.entries(admission).every(([key, value]) =>
              value === initialAdmission[key as keyof typeof admission]);
          if (!sameScope) throw new Error("Refreshed preview plan changed fixed identity");
          if (!sameFonts || !sameAdmission) {
            await fontPreparation.close();
            fontPreparation = null;
          }
          plannedFonts = next;
          if (plannedFonts.size > 32) throw new Error("Preview font collection exceeds resource admission");
        }
      }
    } catch (error) {
      await fontPreparation?.close();
      throw error;
    }
    const planned = performance.now();
    const authorizationAbort = new AbortController();
    const authorizationSignal = AbortSignal.any([
      signal,
      authorizationAbort.signal,
    ]);
    const authorizations = new Map<string, Promise<FontAuthorization>>();
    const authorize = async (font: Font): Promise<FontAuthorization> => {
      try {
        authorizationSignal.throwIfAborted();
        const authorized = await api.sdk.call(
          "opencut_visual_preview_font_authorize",
          { session, font_id: font.fontId },
          { signal: authorizationSignal },
        );
        authorizationSignal.throwIfAborted();
        if (
          authorized.font_id !== font.fontId
        )
          throw new Error("Preview font authorization changed identity");
        return { ok: true };
      } catch (error) {
        // A speculative request may reject before its text layer is reached,
        // or after another stage fails. Retain its failure without an unhandled
        // rejection; actual use and successful frame completion must observe it.
        return { ok: false, error };
      }
    };
    const warmFonts = [...plannedFonts].filter(([key]) => this.fonts.has(key));
    if (warmFonts.length > 1) {
      // One current frame/owner check covers the complete bounded set. Do not
      // fan out N identical workspace/session checks behind the native host.
      const pending = (async (): Promise<FontAuthorization> => {
        try {
          const font_ids = warmFonts.map(([, font]) => font.fontId);
          authorizationSignal.throwIfAborted();
          const authorized = await api.sdk.call(
            "opencut_visual_preview_font_batch_authorize",
            { session, font_ids },
            { signal: authorizationSignal },
          );
          authorizationSignal.throwIfAborted();
          if (!Array.isArray(authorized.font_ids) ||
            authorized.font_ids.length !== font_ids.length ||
            authorized.font_ids.some((id, index) => id !== font_ids[index]))
            throw new Error("Preview font authorization changed identity");
          return { ok: true };
        } catch (error) { return { ok: false, error }; }
      })();
      for (const [key] of warmFonts) authorizations.set(key, pending);
    } else {
      for (const [key, font] of warmFonts) authorizations.set(key, authorize(font));
    }
    try {
      // This browser-owned realm hashes each complete frame. Native hosts retain their own execution policy.
      this.realm ??= await openGpuRealm({ fontHashWorker: true });
      enableImmutablePreviewFonts(this.realm);
      checkIntent();
      const { width, height } = frame.plan.identity;
      const transfers = new Map<string, Uint8Array>();
      const mediaTransfers = new Map<string, TextPlan["media"]["resources"][number]>();
      const readTransfer = async (id: string, offset: number, count: number) => {
        if (mediaTransfers.has(id)) {
          rangeSignal.throwIfAborted();
          const started = performance.now();
          try {
            const bytes = (await api.sdk.readVisualPreviewResource({ session, transfer_id: id, offset: String(offset), count },
              { signal: AbortSignal.any([rangeSignal, AbortSignal.timeout(10000)]) })).bytes;
            this.rangeReads.count++;
            this.rangeReads.bytes += bytes.byteLength;
            return bytes;
          } finally { this.rangeReads.elapsed += performance.now() - started; }
        }
        checkIntent();
        const bytes = transfers.get(id);
        if (!bytes || !Number.isSafeInteger(offset) || offset < 0 ||
          !Number.isInteger(count) || count < 1 || count > 65536 ||
          offset + count > bytes.length)
          throw new Error("Invalid visual byte range");
        return bytes.subarray(offset, offset + count);
      };
      let execution: Record<string, string> = {};
      let hitTest:
        | {
            identity: unknown;
            point: { x: number; y: number };
            hits: { elementId: string; alpha: number }[];
          }
        | undefined;
      let decoded: Input["decoded"] = null;
      let rasters: Input["rasters"] = null;
      let captions: Input["captions"] = null;
      let graphics: Input["graphics"] = null;
      // A complete graphic reply already carries its authorized blend wrapper.
      // Keep it local to this render, and consume it only in the final callback.
      let blendBound: Frame["bound_blend"] = null;
      let sceneBound: Frame["bound"] = null;
      let earlyText: Frame["bound_execution"] = null;
      let earlyGraphic: Frame["bound_graphic"] = null;
      const graphicsPlan = frame.plan.execution.execution;
      const emptyText = !hitQuery && !transformDraft &&
        graphicsPlan.execution.execution.texts.length === 0 &&
        graphicsPlan.execution.captions.length === 0;
      const emptyGraphics = !hitQuery && !transformDraft && graphicsPlan.graphics.length === 0;
      const textKey=preparation&&!hitQuery&&!transformDraft&&!emptyText
        ?JSON.stringify([resourceScope,width,height,textExecution(frame.plan).texts,graphicsPlan.execution.captions,frame.plan.fontSynthesis]):null;
      const previousText=textKey&&this.textObservation?.key===textKey?this.textObservation:null;
      this.textObservation=null;
      const retainComplete = (bound: Frame) => {
        if (!bound.bound_graphic || !bound.bound_blend || !bound.bound || !("steps" in bound.bound))
          throw Error("Missing bound visual scene frame");
        earlyGraphic = bound.bound_graphic;
        blendBound = bound.bound_blend;
        sceneBound = bound.bound;
      };
      try { await renderClassicSceneFrame(
        frame.plan,
        {
          admission,
          beforeGpu:async()=>{
            // Finish every current-frame authorization before preparing another
            // immutable native plan. Bindings themselves are never cached.
            for(const pending of authorizations.values()){const checked=await pending;checkIntent();if(!checked.ok)throw checked.error;}
            const next=frame!.next_time;
            const beforeEnd=next&&(!preparation?.end||BigInt(next.ticks)*BigInt(preparation.end.timebase)<BigInt(preparation.end.ticks)*BigInt(next.timebase));
            if(preparation&&!hitQuery&&!transformDraft&&next&&beforeEnd) {
              const preparedTime=structuredClone(next),reserved=Math.min(64*1024*1024,actualReserved+width*height*4);
              const requestSignal=AbortSignal.any([signal,rangeSignal]);
              const result=callFrame({session,time:preparedTime,decoded:null,rasters:null},requestSignal,reserved)
                .then(frame=>{requestSignal.throwIfAborted();return {ok:true as const,frame};})
                .catch(error=>({ok:false as const,error}));
              this.prefetched={api,scope:resourceScope,time:preparedTime,reserved,signal:requestSignal,result};
            }
          },
          // A completed frame has no consumer for speculative demux reads.
          // The pinned reader rethrows their retirement errors globally. Keep
          // demand reads and the admitted two-line transport cache instead.
          rangePrefetch: false,
          rangeCacheLimit: 512 * 1024,
          rangeChunkBytes: 524288,
          rangeReadAhead: true,
          rangeSignal,
          ...(preparation?.captureSurface?{outputBufferBytes:width*height*4,captureSurface:(pixels:ImageBitmap&{ready:Promise<void>})=>{checkIntent();if(captured.pixels){pixels.close();throw Error("Duplicate GPU picture");}captured.pixels=pixels;}}:{surface:this.surface}),
          readVerifiedBytes: readTransfer,
          ...(fontPreparation ? { preparedFontFrame: fontPreparation.token } : {}),
          ...(hitQuery
            ? {
                hitQuery,
                observeHitTest: (observation: NonNullable<typeof hitTest>) => {
                  hitTest = observation;
                },
              }
            : {}),
          observeExecution: (observation) => {
            execution = { ...execution, ...observation };
          },
          callback: async (name, args) => {
            const id = String(args.transferId);
            if (name === "releaseVerifiedBytes") {
              if (mediaTransfers.delete(id)) {
                // Cleanup is allowed after render cancellation. The server
                // still checks the original scope and closes terminal sessions.
                await api.sdk.call("opencut_visual_preview_resource_release", { session, transfer_id: id }, { keepalive: true }).catch(() => {});
                return { released: true };
              }
              transfers.delete(id);
              return { released: true };
            }
            checkIntent();
            if (name === "admitDecode") {
              const permit = frame.decode_admissions[String(args.elementId)];
              if (!permit) throw new Error("Unknown visual decode admission");
              return permit;
            }
            if (name === "bindFrame") {
              const beganBind = performance.now();
              decoded = args.decoded as Input["decoded"];
              const bound = await callFrame({
                session,
                time: frame.plan.identity.time,
                decoded,
                rasters: emptyText ? [] : previousText?.rasters??null,
                captions: emptyText ? [] : previousText?.captions??null,
                graphics: (emptyText||previousText)&&emptyGraphics ? [] : null,
              });
              if (!bound.bound_media)
                throw new Error("Missing bound visual media frame");
              if (emptyText||previousText) {
                if (!bound.bound_execution) throw Error("Missing bound visual text frame");
                earlyText = bound.bound_execution;
                if (emptyGraphics) retainComplete(bound);
              }
              execution.bindMs = String(performance.now() - beganBind);
              return bound.bound_media;
            }
            if (name === "bindCaptionFrameV3") {
              rasters=args.rasters as Input["rasters"];
              captions=args.captions as Input["captions"];
              if (emptyText) {
                if (!Array.isArray(rasters) || rasters.length || !Array.isArray(captions) || captions.length || !earlyText)
                  throw Error("Empty text binding changed observations");
                const bound = earlyText; earlyText = null; return bound;
              }
              if(previousText){
                // Reuse observations only for exactly identical sampled text
                // inputs. Native binds them afresh with this frame's decoded
                // media; actual current rasters must still match before output.
                if(JSON.stringify(rasters)!==JSON.stringify(previousText.rasters)||JSON.stringify(captions)!==JSON.stringify(previousText.captions)||!earlyText)
                  throw Error("Retained text observations changed during rasterization");
                const bound=earlyText;earlyText=null;return bound;
              }
              const bound=await callFrame({session,time:frame.plan.identity.time,decoded,rasters,captions,
                graphics: emptyGraphics ? [] : null});
              if(!bound.bound_execution) throw Error("Missing bound visual text frame");
              if (emptyGraphics) retainComplete(bound);
              return bound.bound_execution;
            }
            if(name === "bindGraphicFrameV1") {
              graphics=args.graphics as Input["graphics"];
              if (emptyGraphics) {
                if (!Array.isArray(graphics) || graphics.length || !earlyGraphic)
                  throw Error("Empty graphic binding changed observations");
                const bound = earlyGraphic; earlyGraphic = null; return bound;
              }
              blendBound=null;sceneBound=null;
              const bound=await callFrame({session,time:frame.plan.identity.time,decoded,rasters,captions,graphics});
              checkIntent();
              if(!bound.bound_graphic) throw Error("Missing bound visual graphic frame");
              if(!bound.bound_blend || !bound.bound || !("steps" in bound.bound)) throw Error("Missing bound visual scene frame");
              blendBound=bound.bound_blend;sceneBound=bound.bound;
              return bound.bound_graphic;
            }
            if(name === "bindBlendFrameV1") {
              if(Array.isArray(args)||Object.keys(args).length!==0) throw Error("Invalid visual blend binding arguments");
              if(!blendBound) throw Error("Missing bound visual blend frame");
              const bound=blendBound;blendBound=null;return bound;
            }
            if(name === "bindSceneFrameV1") {
              if(Array.isArray(args)||Object.keys(args).length!==0||!sceneBound||blendBound) throw Error("Invalid completed scene binding");
              const bound=sceneBound;sceneBound=null;return bound;
            }
            if (name === "resolveVerifiedFontBytes") {
              const font = args.font as Font;
              const key = fontKey(font),
                plannedFont = plannedFonts.get(key);
              if (
                !plannedFont ||
                (["weight", "style", "faceIndex"] as const).some(
                  (field) => font[field] !== plannedFont[field],
                )
              )
                throw new Error("Unplanned preview font resource");
              if (this.fonts.has(key)) {
                let pending = authorizations.get(key);
                if (!pending) {
                  pending = authorize(font);
                  authorizations.set(key, pending);
                }
                const authorized = await pending;
                checkIntent();
                if (!authorized.ok) throw authorized.error;
                return {
                  transferId: `cached-font:${font.fontId}`,
                  byteLength: font.size,
                };
              }
              const response = await api.sdk.downloadVisualPreviewFont(
                { session, font_id: font.fontId },
                { signal },
              );
              const bytes = await readPreviewBytes(response, font.size, signal);
              const id = `font:${font.fontId}`;
              transfers.set(id, bytes);
              return { transferId: id, byteLength: font.size };
            }
            if (name === "resolveVerifiedBytes") {
              const resource =
                args.resource as TextPlan["media"]["resources"][number];
              if (resource.kind === "video") {
                const opened = await api.sdk.call("opencut_visual_preview_resource_open", { session, resource }, { signal });
                mediaTransfers.set(opened.transferId, resource);
                return opened;
              }
              const view = await api.scene.resource(
                frame.plan.identity.projectId,
                frame.plan.identity.revision,
                resource.assetId,
                signal,
              );
              if (
                view.version.content_hash !== resource.contentHash ||
                String(view.version.size) !== resource.size
              )
                throw new Error("Visual resource identity changed");
              const response = await api.sdk.downloadProjectResource(view, {
                signal,
              });
              const bytes = await readPreviewBytes(
                response,
                resource.size,
                signal,
              );
              transfers.set(resource.assetId, bytes);
              return {
                transferId: resource.assetId,
                byteLength: resource.size,
              };
            }
            if (name === "readVerifiedBytes") {
              const offset = Number(args.offset),
                count = Number(args.count);
              const bytes = await readTransfer(id, offset, count);
              return {
                offset: String(offset),
                bytesHex: Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join(""),
                eof: offset + count === (mediaTransfers.has(id) ? Number(mediaTransfers.get(id)!.size) : transfers.get(id)!.length),
              };
            }
            throw new Error("Unknown visual callback");
          },
          rgba: async () => {
            throw new Error("Unexpected preview readback");
          },
        },
        this.realm,
        signal,
        this.cache,
        this.decoders,
        this.fonts,
      ); } catch (error) {
        if (error instanceof Error) Object.defineProperty(error, "previewMetrics", { value: {
          ...execution, ...rangeMetrics(), planMs: planned - began, resourcePrepareMs,
          sampleToCompleteMs: performance.now() - sampledAt, totalMs: performance.now() - began,
        }, configurable: true });
        if (error instanceof Error && "code" in error &&
          error.code === "PREVIEW_FRAME_STALE" && performance.now() - this.lastDeferredLog > 2000) {
          this.lastDeferredLog = performance.now();
          console.debug("Preview picture missed its consumer window " + JSON.stringify({
            ...execution, ...rangeMetrics(), planMs: planned - began, resourcePrepareMs,
            sampleToCompleteMs: performance.now() - sampledAt,
          }));
        }
        throw error;
      }
      for (const pending of authorizations.values()) {
        const authorized = await pending;
        checkIntent();
        if (!authorized.ok) throw authorized.error;
      }
      checkIntent();
      const pixels = preparation?.captureSurface?captured.pixels:this.surface.transferToImageBitmap();
      if(!pixels)throw Error("Missing owned GPU picture");
      if (pixels.width !== width || pixels.height !== height) {
        pixels.close();
        throw new Error("Invalid preview surface dimensions");
      }
      if(textKey&&textKey.length<=1024*1024&&rasters&&captions&&JSON.stringify([rasters,captions]).length<=1024*1024)
        this.textObservation={key:textKey,rasters:structuredClone(rasters),captions:structuredClone(captions)};
      const ready=captured.pixels?.ready;captured.pixels=null;
      const runtime={
          ...this.realm.observation,
          ...execution,
          nativeRgbaBytes:admission.maxRgbaBytes,nativeGpuBytes:admission.maxGpuBytes,reservedPictureBytes:String(actualReserved),declaredPictureBytes:String(declaredReserved),
          ...Object.fromEntries(Object.entries(rangeMetrics()).map(([key, value]) => [key, String(value)])),
          planMs: String(planned - began),resourcePrepareMs: String(resourcePrepareMs),
          sampleToCompleteMs: String(performance.now() - sampledAt),totalMs: String(performance.now() - began),
          gpuProofWaitMs:"0",
      };
      if(ready){const pendingAt=performance.now();void ready.then(()=>{runtime.gpuProofWaitMs=String(performance.now()-pendingAt);},()=>{});}
      return {
        frame,
        pixels,
        ...(ready?{ready}:{}),
        ...(hitTest ? { hitTest } : {}),
        runtime,
      };
    } finally {
      captured.pixels?.close();
      authorizationAbort.abort();
      await fontPreparation?.close();
    }
  }
  surfacePresenter(canvas:OffscreenCanvas){
    if(!this.realm)throw Error("Scene GPU renderer is not initialized");
    return gpuSurfacePresenter(this.realm,canvas);
  }
  async surfaceBitmap(pixels:ImageBitmap){
    if(!this.realm)throw Error("Scene GPU renderer is not initialized");
    return gpuSurfaceBitmap(this.realm,pixels,this.surface);
  }
  close() {
    this.prefetched=null;
    this.textObservation=null;
    this.resourceAbort.abort();
    void Promise.resolve(this.decoders.close()).catch(() => {});
    const context = this.surface.getContext("webgpu");
    if (context && "unconfigure" in context) context.unconfigure();
    this.surface.width = 0;
    this.surface.height = 0;
    this.cache.clear();
    const fontSet = (globalThis as typeof globalThis & { fonts?: FontFaceSet }).fonts ?? globalThis.document?.fonts;
    for (const font of this.fonts.values())
      if (font.face) fontSet?.delete(font.face);
    this.fonts.clear();
    if (this.realm) closeGpuRealm(this.realm);
    this.realm = null;
  }
}
