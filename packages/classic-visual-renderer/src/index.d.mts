/** Private execution bridge. Public frame DTOs are generated from Rust. */
export type GpuRealm = { device: { destroy(): void }; observation: { gpuBackend: string; gpuDevice: string; textureFormat: string } };
export type ClassicTextFontCache = Map<string, { bytes: Uint8Array; face?: FontFace }>;
export function openGpuRealm(): Promise<GpuRealm>;
export class VisualDecoderCache {
  close(): void;
}
export function renderClassicFrame(plan: unknown, host: {
  admission: unknown;
  surface?: OffscreenCanvas;
  requireResolveCached?: boolean;
  callback(name: string, args: Record<string, unknown>): Promise<unknown>;
  rgba(offset: number, bytes: Uint8Array): Promise<void>;
  observeExecution?(observation: Record<string, string>): void;
}, realm: GpuRealm, signal: AbortSignal, cache?: Map<string, Uint8Array>, decoders?: VisualDecoderCache): Promise<unknown>;
export function renderClassicTextFrameV2(plan: unknown, host: {
  admission: unknown;
  surface?: OffscreenCanvas;
  requireResolveCached?: boolean;
  callback(name: string, args: Record<string, unknown>): Promise<unknown>;
  rgba(offset: number, bytes: Uint8Array): Promise<void>;
  observeExecution?(observation: Record<string, string>): void;
}, realm: GpuRealm, signal: AbortSignal, cache?: Map<string, Uint8Array>, decoders?: VisualDecoderCache, fontCache?: ClassicTextFontCache): Promise<unknown>;
