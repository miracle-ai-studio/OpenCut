import {
  openGpuRealm,
  renderClassicFrame,
  VisualDecoderCache,
  type GpuRealm,
} from "../../../../../../packages/classic-visual-renderer/src/index.mjs";
import type { EditorApi } from "../api";
import type { ToolInput, ToolOutput } from "../../../lib/editor-sdk";
type Input = ToolInput<"opencut_visual_preview_v4_frame">;
type Frame = ToolOutput<"opencut_visual_preview_v4_frame">;
export type VisualPreviewExecutionApi = {
  v4: Pick<EditorApi["v4"], "visualPreviewFrame" | "resource">;
  sdk: Pick<EditorApi["sdk"], "downloadProjectV4Resource">;
};

/** Only execution of application facts. No source clock, geometry or project
 * evaluation lives here. Cache is volatile, hash verified and session scoped. */
export class VisualPreviewRenderer {
  private realm: GpuRealm | null = null;
  private readonly cache = new Map<string, Uint8Array>();
  private readonly decoders = new VisualDecoderCache();
  private readonly surface = new OffscreenCanvas(1, 1);
  async render(api: VisualPreviewExecutionApi, session: Input["session"], signal: AbortSignal) {
    const began = performance.now();
    const frame = await api.v4.visualPreviewFrame(
      { session, time: null, decoded: null },
      signal,
    );
    signal.throwIfAborted();
    const planned = performance.now();
    this.realm ??= await openGpuRealm();
    signal.throwIfAborted();
    const { width, height } = frame.plan.identity;
    const transfers = new Map<string, Uint8Array>();
    let execution: Record<string, string> = {};
    await renderClassicFrame(
      frame.plan,
      {
        admission: frame.admission,
        surface: this.surface,
        observeExecution: (observation) => { execution = { ...execution, ...observation }; },
        callback: async (name, args) => {
          signal.throwIfAborted();
          if (name === "admitDecode") {
            const permit = frame.decode_admissions[String(args.elementId)];
            if (!permit) throw new Error("Unknown visual decode admission");
            return permit;
          }
          if (name === "bindFrame") {
            const beganBind = performance.now();
            const bound = await api.v4.visualPreviewFrame(
              {
                session,
                time: frame.plan.identity.time,
                decoded: args.decoded as Input["decoded"],
              },
              signal,
            );
            if (!bound.bound) throw new Error("Missing bound visual frame");
            execution.bindMs = String(performance.now() - beganBind);
            return bound.bound;
          }
          if (name === "resolveVerifiedBytes") {
            const resource =
              args.resource as Frame["plan"]["resources"][number];
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
            const reader = response.body?.getReader();
            if (!reader)
              throw new Error("Visual resource stream is unavailable");
            const bytes = new Uint8Array(Number(resource.size));
            let offset = 0;
            try {
              for (;;) {
                const { value, done } = await reader.read();
                signal.throwIfAborted();
                if (done) break;
                if (offset + value.length > bytes.length)
                  throw new Error("Visual resource exceeded its admitted size");
                bytes.set(value, offset);
                offset += value.length;
              }
              if (offset !== bytes.length)
                throw new Error("Visual resource ended early");
            } finally {
              await reader.cancel().catch(() => {});
            }
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
        rgba: async () => { throw new Error("Unexpected preview readback"); },
      },
      this.realm,
      signal,
      this.cache,
      this.decoders,
    );
    signal.throwIfAborted();
    const pixels = this.surface.transferToImageBitmap();
    if (pixels.width !== width || pixels.height !== height) {
      pixels.close(); throw new Error("Invalid preview surface dimensions");
    }
    return {
      frame,
      pixels,
      runtime: { ...this.realm.observation, ...execution, planMs: String(planned - began), totalMs: String(performance.now() - began) },
    };
  }
  close() {
    this.decoders.close();
    const context = this.surface.getContext("webgpu");
    if (context && "unconfigure" in context) context.unconfigure();
    this.surface.width = 0; this.surface.height = 0;
    this.cache.clear();
    this.realm?.device.destroy();
    this.realm = null;
  }
}
