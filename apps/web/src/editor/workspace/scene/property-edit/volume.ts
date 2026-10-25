import type { Element, SessionState } from "../types";
import { audioGainAvailability } from "../audio-gain";
import { simpleFades } from "../gain-keyframes/fades";

export class VolumeInputError extends Error {}

export function volumeTargets(state: SessionState, ids: readonly string[]) {
  return ids.map((id) => audioGainAvailability(state, id));
}
export type VolumeTarget = NonNullable<
  ReturnType<typeof audioGainAvailability>
>;
export function volumeLevel(target: VolumeTarget): number | null {
  const model = simpleFades(target.element);
  if (target.animated && !model) return null;
  const value = model?.level ?? target.audio.gain.value;
  return target.audio.gain.unit === "decibels" ? 10 ** (value / 20) : value;
}
export const displayVolume = (value: number) =>
  String(Number(value.toFixed(3)));
export function parseVolume(field: "percent" | "db", text: string) {
  if (!text.trim() || !Number.isFinite(Number(text)))
    throw new VolumeInputError("Enter a finite volume value.");
  const value = Number(text);
  if (field === "percent") {
    if (value < 0 || value > 1000)
      throw new VolumeInputError("Volume must be between 0% and 1000%.");
    return value / 100;
  }
  if (value < -60 || value > 20)
    throw new VolumeInputError("Volume must be between −60 dB and 20 dB.");
  return 10 ** (value / 20);
}

/** Author an existing gain property or an unambiguous linear envelope plateau.
 * This does not sample/interpolate audio, convert its stored unit, or fit a curve. */
export function authoredVolume(
  target: VolumeTarget,
  amplitude: number,
): {
  expected: Element;
  channel: NonNullable<ReturnType<typeof simpleFades>>["channel"];
} {
  const original = target.element,
    gain = target.audio.gain;
  if (
    !Number.isFinite(amplitude) ||
    amplitude < 0 ||
    amplitude > (gain.unit === "linear_legacy" ? 4 : 10)
  )
    throw new VolumeInputError(
      gain.unit === "linear_legacy"
        ? "This clip supports volume from 0% to 400%."
        : "Volume must be between 0% and 1000%.",
    );
  const value =
    gain.unit === "decibels" ? 20 * Math.log10(amplitude) : amplitude;
  if (!Number.isFinite(value) || (gain.unit === "decibels" && value < -60))
    throw new VolumeInputError(
      "Use mute to silence the clip without losing its volume settings.",
    );
  const expected = structuredClone(original);
  if (expected.content.kind !== "audio" && expected.content.kind !== "video")
    throw Error("Select an audio element.");
  const model = simpleFades(original);
  if (target.animated) {
    if (!model?.channel)
      throw Error(
        "Edit this custom volume envelope with the keyframe controls.",
      );
    if (value <= model.floor)
      throw new VolumeInputError("Use mute to silence the clip without losing its fades.");
    const channel = structuredClone(model.channel);
    channel.keys = channel.keys.map((key) =>
      key.value === model.level ? { ...key, value } : key,
    );
    expected.channels = expected.channels.map((c) =>
      c.id === channel.id ? channel : c,
    );
    return { expected, channel };
  }
  expected.content.audio.gain = { unit: gain.unit, value };
  return { expected, channel: null };
}

/** Unit switching in the advanced form preserves amplitude and untouched input
 * precision. Animated units remain fixed by the original channel binding. */
export function convertGainUnit(
  value: string,
  from: "decibels" | "linear_legacy",
  to: typeof from,
) {
  if (from === to) return value;
  if (!value.trim() || !Number.isFinite(Number(value)))
    throw new VolumeInputError("Enter a finite volume value.");
  const amplitude =
    from === "decibels" ? 10 ** (Number(value) / 20) : Number(value);
  if (!(amplitude > 0) || amplitude > 4)
    throw new VolumeInputError("This gain cannot be represented in the requested unit.");
  return String(to === "decibels" ? 20 * Math.log10(amplitude) : amplitude);
}
