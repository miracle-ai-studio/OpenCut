/** Private execution bridge. Public frame DTOs are generated from Rust. */
export type GpuRealm = { device: { destroy(): void }; observation: { gpuBackend: string; gpuDevice: string; textureFormat: string } };
export type ClassicTextFontCache = Map<string, { bytes: Uint8Array; face?: FontFace }>;
export function measureClassicText(ctx: CanvasRenderingContext2D, text: unknown, fontFamily: string, font: { style: string; weight: number }): { block: { maxWidth: number; height: number } };
/** Web preview may enable a local font digest worker; native hosts default off. */
export function openGpuRealm(options?: { fontHashWorker?: boolean }): Promise<GpuRealm>;
/** Releases retained graphics/GPU textures and destroys the realm once. */
export function closeGpuRealm(realm: GpuRealm): void;
export class VisualDecoderCache {
  close(): void;
}
export function renderClassicFrame(plan: unknown, host: {
  admission: unknown;
  surface?: OffscreenCanvas;
  hitQuery?: {x:number;y:number};
  observeHitTest?(observation: {identity:unknown;point:{x:number;y:number};hits:{elementId:string;alpha:number}[]}):void;
  requireResolveCached?: boolean;
  callback(name: string, args: Record<string, unknown>): Promise<unknown>;
  rgba(offset: number, bytes: Uint8Array): Promise<void>;
  observeExecution?(observation: Record<string, string>): void;
}, realm: GpuRealm, signal: AbortSignal, cache?: Map<string, Uint8Array>, decoders?: VisualDecoderCache): Promise<unknown>;
export function renderClassicTextFrameV2(plan: unknown, host: {
  admission: unknown;
  surface?: OffscreenCanvas;
  hitQuery?: {x:number;y:number};
  observeHitTest?(observation: {identity:unknown;point:{x:number;y:number};hits:{elementId:string;alpha:number}[]}):void;
  requireResolveCached?: boolean;
  callback(name: string, args: Record<string, unknown>): Promise<unknown>;
  rgba(offset: number, bytes: Uint8Array): Promise<void>;
  observeExecution?(observation: Record<string, string>): void;
}, realm: GpuRealm, signal: AbortSignal, cache?: Map<string, Uint8Array>, decoders?: VisualDecoderCache, fontCache?: ClassicTextFontCache): Promise<unknown>;

export function renderClassicCaptionFrameV3(plan: unknown, host: {
  admission: unknown;
  surface?: OffscreenCanvas;
  hitQuery?: {x:number;y:number};
  observeHitTest?(observation: {identity:unknown;point:{x:number;y:number};hits:{elementId:string;alpha:number}[]}):void;
  requireResolveCached?: boolean;
  callback(name: string, args: Record<string, unknown>): Promise<unknown>;
  rgba(offset: number, bytes: Uint8Array): Promise<void>;
  observeExecution?(observation: Record<string, string>): void;
}, realm: GpuRealm, signal: AbortSignal, cache?: Map<string, Uint8Array>, decoders?: VisualDecoderCache, fontCache?: ClassicTextFontCache): Promise<unknown>;

/** Private Classic caption preparation; public receipt types remain generated. */
export function prepareCaptionTextV3(ctx: CanvasRenderingContext2D, original: unknown, fontFamily: string, font: { style: string; weight: number }, directive: unknown, dimensions: { width: number; height: number }): { text: unknown; wrappedText: string; measurements: { content: string; width: number }[] };

export function renderClassicGraphicFrameV1(plan: unknown, host: {
  admission: unknown;
  surface?: OffscreenCanvas;
  hitQuery?: {x:number;y:number};
  observeHitTest?(observation: {identity:unknown;point:{x:number;y:number};hits:{elementId:string;alpha:number}[]}):void;
  requireResolveCached?: boolean;
  callback(name: string, args: Record<string, unknown>): Promise<unknown>;
  rgba(offset: number, bytes: Uint8Array): Promise<void>;
  observeExecution?(observation: Record<string, string>): void;
}, realm: GpuRealm, signal: AbortSignal, cache?: Map<string, Uint8Array>, decoders?: VisualDecoderCache, fontCache?: ClassicTextFontCache): Promise<unknown>;

export function renderClassicBlendFrameV1(plan: unknown, host: {
  admission: unknown;
  surface?: OffscreenCanvas;
  hitQuery?: {x:number;y:number};
  observeHitTest?(observation: {identity:unknown;point:{x:number;y:number};hits:{elementId:string;alpha:number}[]}):void;
  requireResolveCached?: boolean;
  callback(name: string, args: Record<string, unknown>): Promise<unknown>;
  rgba(offset: number, bytes: Uint8Array): Promise<void>;
  observeExecution?(observation: Record<string, string>): void;
}, realm: GpuRealm, signal: AbortSignal, cache?: Map<string, Uint8Array>, decoders?: VisualDecoderCache, fontCache?: ClassicTextFontCache): Promise<unknown>;

export function renderClassicMaskFrameV1(plan: unknown, host: {
  admission: unknown;
  surface?: OffscreenCanvas;
  hitQuery?: {x:number;y:number};
  observeHitTest?(observation: {identity:unknown;point:{x:number;y:number};hits:{elementId:string;alpha:number}[]}):void;
  requireResolveCached?: boolean;
  callback(name: string, args: Record<string, unknown>): Promise<unknown>;
  rgba(offset: number, bytes: Uint8Array): Promise<void>;
  observeExecution?(observation: Record<string, string>): void;
}, realm: GpuRealm, signal: AbortSignal, cache?: Map<string, Uint8Array>, decoders?: VisualDecoderCache, fontCache?: ClassicTextFontCache): Promise<unknown>;

export function renderClassicSceneFrameV1(plan: unknown, host: {
  admission: unknown;
  surface?: OffscreenCanvas;
  hitQuery?: {x:number;y:number};
  observeHitTest?(observation: {identity:unknown;point:{x:number;y:number};hits:{elementId:string;alpha:number}[]}):void;
  requireResolveCached?: boolean;
  callback(name: string, args: Record<string, unknown>): Promise<unknown>;
  rgba(offset: number, bytes: Uint8Array): Promise<void>;
  observeExecution?(observation: Record<string, string>): void;
}, realm: GpuRealm, signal: AbortSignal, cache?: Map<string, Uint8Array>, decoders?: VisualDecoderCache, fontCache?: ClassicTextFontCache): Promise<unknown>;

export function prepareClassicAssSubtitleV1(ctx: CanvasRenderingContext2D, original: unknown, fontFamily: string, font: {style: string; weight: number}, directive: unknown, placement: {left_margin_px:number;right_margin_px:number;vertical_margin_px:number;vertical_align:"top"|"middle"|"bottom"}, dimensions: {width:number;height:number}): {text:unknown;wrappedText:string;measurements:{content:string;width:number}[]};
