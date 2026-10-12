import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { V4Asset } from "../types";
import type { ArtAllocation } from "./cache";
import { canonicalArt, validateArtScope } from "./receipt";
import type { DisplayArt } from "./service";

export type WaveformDisplayRequest = ToolInput<"opencut_source_art_waveform_display_v1_get">;
export type WaveformDisplayResponse = ToolOutput<"opencut_source_art_waveform_display_v1_get">;
export type WaveformDisplaySelection = { projectId: string; revision: string; asset: V4Asset;
  display: Omit<WaveformDisplayRequest, "project_id" | "revision" | "asset_id" | "request_id"> };
const invalid = (): never => { throw new Error("The service returned invalid Classic waveform display."); };
const unsigned = (value: unknown): value is number => typeof value === "number" && Number.isInteger(value) && value >= 0 && value <= 4294967295;
const sample = (value: unknown) => typeof value === "string" && /^(0|[1-9][0-9]{0,19})$/.test(value) && BigInt(value) <= 18446744073709551615n;

/** Validate transported facts; no client source clock, gain curve, burn or
 * device-coordinate projection is derived from the authored document. */
export function allocateWaveformDisplay(response: WaveformDisplayResponse, selected: WaveformDisplaySelection): ArtAllocation<DisplayArt> {
  validateArtScope(response, selected);
  const requested = selected.display, canvas = response.canvas;
  if (response.identity.algorithm !== "source-peaks128-v1" || response.display_profile !== "classic-waveform-display-v1" ||
      response.sequence_id !== requested.sequence_id || response.track_id !== requested.track_id || response.element_id !== requested.element_id ||
      response.sample_rate !== requested.sample_rate || (response.temporary_gain ?? null) !== (requested.temporary_gain ?? null) ||
      canonicalArt(response.viewport) !== canonicalArt(requested.viewport) || response.bucket_size !== 128 ||
      !Number.isInteger(response.channels) || response.channels < 1 || response.channels > 64 || !sample(response.total_samples) ||
      !canvas || !unsigned(canvas.width) || !canvas.width || !unsigned(canvas.height) || !canvas.height || canvas.width * canvas.height > 8388608 ||
      ![canvas.css_left, canvas.css_width, canvas.css_height].every(Number.isFinite) ||
      canvas.css_left < 0 || canvas.css_width <= 0 || canvas.css_height <= 0 ||
      canvas.css_left !== requested.viewport.clip_left_px ||
      canvas.css_width !== requested.viewport.clip_right_px - requested.viewport.clip_left_px ||
      canvas.css_height !== requested.viewport.css_height ||
      !Array.isArray(response.bars) || !response.bars.length || response.bars.length > 4096) return invalid();
  for (const bar of response.bars) {
    if (!sample(bar.source_start_sample) || !sample(bar.source_end_sample) ||
        ![bar.peak, bar.gain, bar.output_amplitude, bar.height_fraction].every(value => Number.isFinite(value) && value >= 0) ||
        bar.height_fraction > 1 || typeof bar.burn !== "boolean" || !unsigned(bar.device_left) || !unsigned(bar.device_right) ||
        bar.device_right <= bar.device_left || !Number.isSafeInteger(bar.device_top) ||
        !unsigned(bar.device_height) || !unsigned(bar.burn_height) || (!bar.burn && bar.burn_height !== 0)) return invalid();
  }
  const executionIdentity = canonicalArt([response.version, response.identity]);
  const serialized = canonicalArt([executionIdentity, response.display_profile, requested, canvas, response.bars]);
  const bytes = new TextEncoder().encode(serialized).length * 2 + response.bars.length * 256 + canvas.width * canvas.height * 4;
  return { bytes, fingerprint: serialized, materialize: () => ({ bytes, fingerprint: serialized,
    value: { kind: "classic_waveform", executionIdentity, canvas: { ...canvas }, bars: response.bars.map(bar => ({ ...bar })),
      temporaryGain: response.temporary_gain ?? null }, dispose() {} }) };
}
