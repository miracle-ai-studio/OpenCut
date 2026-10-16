import {
  measureClassicText,
  prepareCaptionTextV3,
  prepareClassicAssSubtitleV1,
  openGpuRealm,
  closeGpuRealm,
  renderClassicTextFrameV2,
  renderClassicCaptionFrameV3,
  renderClassicGraphicFrameV1,
  renderClassicBlendFrameV1,
  renderClassicMaskFrameV1,
  renderClassicSceneFrameV1,
  VisualDecoderCache,
  type GpuRealm,
} from "../../../../../../packages/classic-visual-renderer/src/index.mjs";
import type { EditorApi } from "../api";
import { readPreviewBytes } from "./visual-preview-bytes";
import { previewExecutionAdmission } from "./visual-preview-admission";
import type { ToolInput, ToolOutput } from "../../../lib/editor-sdk";
import type { VisualPreviewProfile } from "./visual-profile";
export type { VisualPreviewProfile } from "./visual-profile";
// Keep canvas editing on the same Classic measurement implementation as preview.
export {
  measureClassicText,
  prepareCaptionTextV3,
  prepareClassicAssSubtitleV1,
};

type Input = ToolInput<"opencut_visual_preview_v4_text_frame">;
type CaptionInput = ToolInput<"opencut_visual_preview_v4_caption_frame">;
type GraphicInput = ToolInput<"opencut_visual_preview_v4_graphic_frame">;
type SceneFrame = ToolOutput<"opencut_visual_preview_v4_scene_frame">;
type MaskFrame = ToolOutput<"opencut_visual_preview_v4_mask_frame">;
type BlendFrame = ToolOutput<"opencut_visual_preview_v4_blend_frame">;
type Frame = ToolOutput<"opencut_visual_preview_v4_text_frame">;
type ExecutionPlan =
  | Frame["plan"]
  | ToolOutput<"opencut_visual_preview_v4_caption_frame">["plan"]
  | ToolOutput<"opencut_visual_preview_v4_graphic_frame">["plan"]
  | BlendFrame["plan"]
  | MaskFrame["plan"]
  | SceneFrame["plan"];
type Font = Frame["plan"]["texts"][number]["font"];
type FontAuthorization = { ok: true } | { ok: false; error: unknown };
function textExecution(plan: ExecutionPlan): Frame["plan"] {
  return "texts" in plan ? plan : textExecution(plan.execution);
}
const fontKey = (font: Font) =>
  `${font.fontId}:${font.contentHash}:${font.size}`;
export type VisualPreviewExecutionApi = {
  v4: Pick<EditorApi["v4"], "visualPreviewTextFrame" | "resource">;
  sdk: Pick<
    EditorApi["sdk"],
    "downloadProjectV4Resource" | "downloadVisualPreviewFontV2" | "call"
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
  async render(
    api: VisualPreviewExecutionApi,
    session: Input["session"],
    signal: AbortSignal,
    profile: VisualPreviewProfile = "text",
    hitQuery?: { x: number; y: number },
    transformDraft?: {
      draftId: string;
      gestureIntent?: ToolInput<"opencut_visual_preview_v4_transform_draft_v2_frame">["gesture_intent"];
      transaction: ToolInput<"opencut_edit_v4_apply">;
      time: Input["time"];
      intentSignal?: AbortSignal;
    },
  ) {
    const checkIntent = () => {
      signal.throwIfAborted();
      transformDraft?.intentSignal?.throwIfAborted();
    };
    const began = performance.now();
    let draftDigest: string | null = null;
    const callFrame = (
      input: Input & Partial<Pick<GraphicInput, "captions" | "graphics">>,
    ) => {
      checkIntent();
      if (transformDraft) {
        if (profile !== "scene")
          throw new Error("Transform drafts require the scene renderer.");
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
        const pending = transformDraft.gestureIntent === undefined
          ? api.sdk.call("opencut_visual_preview_v4_transform_draft_frame", request, { signal })
          : api.sdk.call("opencut_visual_preview_v4_transform_draft_v2_frame",
            { ...request, gesture_intent: transformDraft.gestureIntent }, { signal });
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
      if (
        profile === "graphic" ||
        profile === "blend" ||
        profile === "mask" ||
        profile === "scene"
      )
        return api.sdk.call(
          profile === "scene"
            ? "opencut_visual_preview_v4_scene_frame"
            : profile === "mask"
              ? "opencut_visual_preview_v4_mask_frame"
              : profile === "blend"
                ? "opencut_visual_preview_v4_blend_frame"
                : "opencut_visual_preview_v4_graphic_frame",
          {
            ...input,
            captions: input.captions ?? null,
            graphics: input.graphics ?? null,
          },
          { signal },
        );
      if (profile === "caption")
        return api.sdk.call(
          "opencut_visual_preview_v4_caption_frame",
          { ...input, captions: input.captions ?? null },
          { signal },
        );
      return api.v4.visualPreviewTextFrame(input, signal);
    };
    const frame = await callFrame({
      session,
      time: null,
      decoded: null,
      rasters: null,
    });
    checkIntent();
    const planned = performance.now();
    const admission = previewExecutionAdmission(frame.admission);
    const plannedFonts = new Map(
      [
        ...textExecution(frame.plan).texts.map(({ font }) => font),
        ...("maskFonts" in frame.plan
          ? frame.plan.maskFonts.map((p) => p.font)
          : []),
      ].map((font) => [fontKey(font), font] as const),
    );
    // The renderer still admits the complete media/font set. Bound speculative
    // authorization by its existing resource limit before starting any calls.
    if (plannedFonts.size > 32)
      throw new Error("Preview font collection exceeds resource admission");
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
          "opencut_visual_preview_v4_font_authorize",
          { session, font_id: font.fontId },
          { signal: authorizationSignal },
        );
        authorizationSignal.throwIfAborted();
        if (
          authorized.schema_version !== 2 ||
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
    for (const [key, font] of plannedFonts)
      if (this.fonts.has(key)) authorizations.set(key, authorize(font));
    try {
      // This browser-owned realm can later switch between visual profiles.
      // Only graphic/blend frames use the worker; native hosts keep it disabled.
      this.realm ??= await openGpuRealm({ fontHashWorker: true });
      checkIntent();
      const { width, height } = frame.plan.identity;
      const transfers = new Map<string, Uint8Array>();
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
      let captions: CaptionInput["captions"] = null;
      let graphics: GraphicInput["graphics"] = null;
      // A complete graphic reply already carries its authorized blend wrapper.
      // Keep it local to this render, and consume it only in the final callback.
      let blendBound: BlendFrame["bound"] = null;
      let maskBound: MaskFrame["bound"] = null;
      let sceneBound: SceneFrame["bound"] = null;
      const render =
        profile === "scene"
          ? renderClassicSceneFrameV1
          : profile === "mask"
            ? renderClassicMaskFrameV1
            : profile === "blend"
              ? renderClassicBlendFrameV1
              : profile === "graphic"
                ? renderClassicGraphicFrameV1
                : profile === "caption"
                  ? renderClassicCaptionFrameV3
                  : renderClassicTextFrameV2;
      await render(
        frame.plan,
        {
          admission,
          surface: this.surface,
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
                rasters: null,
              });
              if (!bound.bound_media)
                throw new Error("Missing bound visual media frame");
              execution.bindMs = String(performance.now() - beganBind);
              return bound.bound_media;
            }
            if (
              name ===
              (profile === "text" ? "bindTextFrame" : "bindCaptionFrameV3")
            ) {
              rasters = args.rasters as Input["rasters"];
              captions =
                profile === "text"
                  ? null
                  : (args.captions as CaptionInput["captions"]);
              const bound = await callFrame({
                session,
                time: frame.plan.identity.time,
                decoded,
                rasters,
                ...(profile !== "text" ? { captions } : {}),
              });
              // Graphics delegate text/caption binding before their own raster
              // receipts exist; only the last bind returns the complete frame.
              const execution =
                profile === "graphic" ||
                profile === "blend" ||
                profile === "mask" ||
                profile === "scene"
                  ? "bound_execution" in bound
                    ? bound.bound_execution
                    : null
                  : bound.bound;
              if (!execution)
                throw new Error("Missing bound visual text frame");
              return execution;
            }
            if (
              (profile === "graphic" ||
                profile === "blend" ||
                profile === "mask" ||
                profile === "scene") &&
              name === "bindGraphicFrameV1"
            ) {
              blendBound = null;
              maskBound = null;
              sceneBound = null;
              graphics = args.graphics as GraphicInput["graphics"];
              const bound = await callFrame({
                session,
                time: frame.plan.identity.time,
                decoded,
                rasters,
                captions,
                graphics,
              });
              checkIntent();
              const graphic =
                profile === "blend" || profile === "mask" || profile === "scene"
                  ? "bound_graphic" in bound
                    ? bound.bound_graphic
                    : null
                  : bound.bound;
              if (!graphic)
                throw new Error("Missing bound visual graphic frame");
              if (profile === "scene") {
                if (
                  !("bound_blend" in bound) ||
                  !bound.bound_blend ||
                  !bound.bound ||
                  !("steps" in bound.bound)
                )
                  throw new Error("Missing bound visual scene frame");
                blendBound = bound.bound_blend;
                sceneBound = bound.bound;
              } else if (profile === "mask") {
                if (
                  !("bound_blend" in bound) ||
                  !bound.bound_blend ||
                  !bound.bound ||
                  !("masks" in bound.bound)
                )
                  throw new Error("Missing bound visual mask frame");
                blendBound = bound.bound_blend;
                maskBound = bound.bound;
              } else if (profile === "blend") {
                if (
                  !("bound_graphic" in bound) ||
                  "bound_blend" in bound ||
                  !bound.bound ||
                  !("blends" in bound.bound)
                )
                  throw new Error("Missing bound visual blend frame");
                blendBound = bound.bound;
              }
              return graphic;
            }
            if (
              (profile === "blend" ||
                profile === "mask" ||
                profile === "scene") &&
              name === "bindBlendFrameV1"
            ) {
              if (Array.isArray(args) || Object.keys(args).length !== 0)
                throw new Error("Invalid visual blend binding arguments");
              if (!blendBound)
                throw new Error("Missing bound visual blend frame");
              const bound = blendBound;
              blendBound = null;
              return bound;
            }
            if (profile === "scene" && name === "bindSceneFrameV1") {
              if (
                Array.isArray(args) ||
                Object.keys(args).length !== 0 ||
                !sceneBound ||
                blendBound
              )
                throw new Error("Invalid completed scene binding");
              const bound = sceneBound;
              sceneBound = null;
              return bound;
            }
            if (profile === "mask" && name === "bindMaskFrameV1") {
              if (Array.isArray(args) || Object.keys(args).length !== 0)
                throw new Error("Invalid visual mask binding arguments");
              if (!maskBound || blendBound)
                throw new Error("Missing completed visual mask binding");
              const bound = maskBound;
              maskBound = null;
              sceneBound = null;
              return bound;
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
              const response = await api.sdk.downloadVisualPreviewFontV2(
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
                args.resource as Frame["plan"]["media"]["resources"][number];
              const view = await api.v4.resource(
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
              const response = await api.sdk.downloadProjectV4Resource(view, {
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
            const id = String(args.transferId);
            if (name === "releaseVerifiedBytes") {
              transfers.delete(id);
              return { released: true };
            }
            if (name === "readVerifiedBytes") {
              const bytes = transfers.get(id),
                offset = Number(args.offset),
                count = Number(args.count);
              if (
                !bytes ||
                !Number.isSafeInteger(offset) ||
                offset < 0 ||
                !Number.isInteger(count) ||
                count < 1 ||
                count > 65536 ||
                offset + count > bytes.length
              )
                throw new Error("Invalid visual byte range");
              return {
                offset: String(offset),
                bytesHex: Array.from(
                  bytes.subarray(offset, offset + count),
                  (b) => b.toString(16).padStart(2, "0"),
                ).join(""),
                eof: offset + count === bytes.length,
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
      );
      for (const pending of authorizations.values()) {
        const authorized = await pending;
        checkIntent();
        if (!authorized.ok) throw authorized.error;
      }
      checkIntent();
      const pixels = this.surface.transferToImageBitmap();
      if (pixels.width !== width || pixels.height !== height) {
        pixels.close();
        throw new Error("Invalid preview surface dimensions");
      }
      return {
        frame,
        pixels,
        ...(hitTest ? { hitTest } : {}),
        runtime: {
          ...this.realm.observation,
          ...execution,
          planMs: String(planned - began),
          totalMs: String(performance.now() - began),
        },
      };
    } finally {
      authorizationAbort.abort();
    }
  }
  close() {
    this.decoders.close();
    const context = this.surface.getContext("webgpu");
    if (context && "unconfigure" in context) context.unconfigure();
    this.surface.width = 0;
    this.surface.height = 0;
    this.cache.clear();
    const fontSet = globalThis.document?.fonts;
    for (const font of this.fonts.values())
      if (font.face) fontSet?.delete(font.face);
    this.fonts.clear();
    if (this.realm) closeGpuRealm(this.realm);
    this.realm = null;
  }
}
