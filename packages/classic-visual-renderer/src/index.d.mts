/** Private execution bridge. Public frame DTOs are generated from Rust. */
export type GpuRealm = { device: { destroy(): void }; observation: { gpuBackend: string; gpuDevice: string; textureFormat: string } };
export function openGpuRealm(): Promise<GpuRealm>;
export function renderClassicFrame(plan: unknown, host: {
  admission: unknown;
  callback(name: string, args: Record<string, unknown>): Promise<unknown>;
  rgba(offset: number, bytes: Uint8Array): Promise<void>;
}, realm: GpuRealm, signal: AbortSignal, cache?: Map<string, Uint8Array>): Promise<unknown>;
