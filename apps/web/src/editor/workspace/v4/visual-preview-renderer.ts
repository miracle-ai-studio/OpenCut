import {
  openGpuRealm,
  renderClassicTextFrameV2,
  VisualDecoderCache,
  type GpuRealm,
} from "../../../../../../packages/classic-visual-renderer/src/index.mjs";
import type { EditorApi } from "../api";
import { readPreviewBytes } from "./visual-preview-bytes";
import { previewExecutionAdmission } from "./visual-preview-admission";
import type { ToolInput, ToolOutput } from "../../../lib/editor-sdk";
type Input = ToolInput<"opencut_visual_preview_v4_text_frame">;
type Frame = ToolOutput<"opencut_visual_preview_v4_text_frame">;
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
  private readonly fonts = new Map<string, { bytes: Uint8Array; face?: FontFace }>();
  private readonly decoders = new VisualDecoderCache();
  private readonly surface = new OffscreenCanvas(1, 1);
  async render(
    api: VisualPreviewExecutionApi,
    session: Input["session"],
    signal: AbortSignal,
  ) {
    const began = performance.now();
    const frame = await api.v4.visualPreviewTextFrame(
      { session, time: null, decoded: null, rasters: null },
      signal,
    );
    signal.throwIfAborted();
    const planned = performance.now();
    this.realm ??= await openGpuRealm();
    signal.throwIfAborted();
    const { width, height } = frame.plan.identity;
    const transfers = new Map<string, Uint8Array>();
    let execution: Record<string, string> = {};
    let decoded: Input["decoded"] = null;
    await renderClassicTextFrameV2(
      frame.plan,
      {
        admission: previewExecutionAdmission(frame.admission),
        surface: this.surface,
        observeExecution: (observation) => {
          execution = { ...execution, ...observation };
        },
        callback: async (name, args) => {
          signal.throwIfAborted();
          if (name === "admitDecode") {
            const permit = frame.decode_admissions[String(args.elementId)];
            if (!permit) throw new Error("Unknown visual decode admission");
            return permit;
          }
          if (name === "bindFrame") {
            const beganBind = performance.now();
            decoded = args.decoded as Input["decoded"];
            const bound = await api.v4.visualPreviewTextFrame(
              {
                session,
                time: frame.plan.identity.time,
                decoded,
                rasters: null,
              },
              signal,
            );
            if (!bound.bound_media)
              throw new Error("Missing bound visual media frame");
            execution.bindMs = String(performance.now() - beganBind);
            return bound.bound_media;
          }
          if (name === "bindTextFrame") {
            const bound = await api.v4.visualPreviewTextFrame(
              {
                session,
                time: frame.plan.identity.time,
                decoded,
                rasters: args.rasters as Input["rasters"],
              },
              signal,
            );
            if (!bound.bound)
              throw new Error("Missing bound visual text frame");
            return bound.bound;
          }
          if (name === "resolveVerifiedFontBytes") {
            const font = args.font as Frame["plan"]["texts"][number]["font"];
            const key = `${font.fontId}:${font.contentHash}:${font.size}`;
            if (this.fonts.has(key)) {
              const authorized = await api.sdk.call("opencut_visual_preview_v4_font_authorize", { session, font_id: font.fontId }, { signal });
              if (authorized.schema_version !== 2 || authorized.font_id !== font.fontId) throw new Error("Preview font authorization changed identity");
              return { transferId: `cached-font:${font.fontId}`, byteLength: font.size };
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
            return { transferId: resource.assetId, byteLength: resource.size };
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
    signal.throwIfAborted();
    const pixels = this.surface.transferToImageBitmap();
    if (pixels.width !== width || pixels.height !== height) {
      pixels.close();
      throw new Error("Invalid preview surface dimensions");
    }
    return {
      frame,
      pixels,
      runtime: {
        ...this.realm.observation,
        ...execution,
        planMs: String(planned - began),
        totalMs: String(performance.now() - began),
      },
    };
  }
  close() {
    this.decoders.close();
    const context = this.surface.getContext("webgpu");
    if (context && "unconfigure" in context) context.unconfigure();
    this.surface.width = 0;
    this.surface.height = 0;
    this.cache.clear();
    const fontSet = globalThis.document?.fonts;
    for (const font of this.fonts.values()) if (font.face) fontSet?.delete(font.face);
    this.fonts.clear();
    this.realm?.device.destroy();
    this.realm = null;
  }
}
