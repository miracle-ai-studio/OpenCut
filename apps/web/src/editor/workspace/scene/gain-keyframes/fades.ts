import type { Element } from "../types";
import type { ExactTime, ScalarChannel, ScalarKey } from "../keyframes/types";
import { difference, inputTime, seconds } from "../keyframes/types";

const zero: ExactTime = { ticks: "0", timebase: 1 };
export class FadeInputError extends Error {
  constructor(
    message: string,
    readonly field: "fadeIn" | "fadeOut",
  ) {
    super(message);
    this.name = "FadeInputError";
  }
}
const compare = (a: ExactTime, b: ExactTime) => {
  const d =
    BigInt(a.ticks) * BigInt(b.timebase) - BigInt(b.ticks) * BigInt(a.timebase);
  return d < 0n ? -1 : d > 0n ? 1 : 0;
};
export type SimpleFades = {
  channel: ScalarChannel | null;
  level: number;
  floor: number;
  parameter: "gain_decibels" | "gain_linear_legacy";
  duration: ExactTime;
  fadeIn: ExactTime;
  fadeOut: ExactTime;
};

/** Recognize only an unambiguous linear envelope, never fit or sample a custom
 * curve. These are authoring controls over the existing shared gain channel. */
export function simpleFades(element: Element): SimpleFades | null {
  if (element.content.kind !== "audio" && element.content.kind !== "video")
    return null;
  const gain = element.content.audio.gain;
  const parameter =
    gain.unit === "decibels" ? "gain_decibels" : "gain_linear_legacy";
  const channels = element.channels.filter(
    (c) =>
      c.kind === "scalar" &&
      c.binding.kind === "property" &&
      ["gain_decibels", "gain_linear_legacy"].includes(c.binding.parameter),
  );
  if (channels.length > 1) return null;
  const channel = channels[0] as ScalarChannel | undefined;
  const duration = element.timeline_range.duration;
  if (compare(duration, zero) <= 0) return null;
  const floor = gain.unit === "decibels" ? -60 : 0;
  if (!channel || !channel.keys.length) {
    if (
      channel &&
      (channel.binding.kind !== "property" ||
        channel.binding.parameter !== parameter ||
        channel.target.kind !== "element" ||
        channel.target.element_id !== element.id)
    )
      return null;
    if (!(gain.value > floor) || !Number.isFinite(gain.value)) return null;
    return {
      channel: channel ?? null,
      level: gain.value,
      floor,
      parameter,
      duration,
      fadeIn: zero,
      fadeOut: zero,
    };
  }
  if (
    channel.target.kind !== "element" ||
    channel.target.element_id !== element.id ||
    channel.binding.kind !== "property" ||
    channel.binding.parameter !== parameter ||
    channel.extrapolation.before !== "hold" ||
    channel.extrapolation.after !== "hold"
  )
    return null;
  const keys = channel.keys;
  if (
    keys.length < 2 ||
    keys.length > 4 ||
    compare(keys[0].time, zero) !== 0 ||
    compare(keys.at(-1)!.time, duration) !== 0 ||
    keys.some(
      (key, i) =>
        !Number.isFinite(key.value) ||
        key.left_handle ||
        key.right_handle ||
        key.segment_to_next.kind !== "linear" ||
        (i > 0 && compare(key.time, keys[i - 1].time) <= 0),
    )
  )
    return null;
  const level = Math.max(...keys.map((k) => k.value));
  if (!(level > floor)) return null;
  const start = keys[0].value === floor ? 1 : 0;
  const end = keys.at(-1)!.value === floor ? keys.length - 2 : keys.length - 1;
  if (
    start > end ||
    keys.slice(start, end + 1).some((k) => k.value !== level) ||
    (start === 0 && keys[0].value !== level) ||
    (end === keys.length - 1 && keys[end].value !== level)
  )
    return null;
  return {
    channel,
    level,
    floor,
    parameter,
    duration,
    fadeIn: start ? keys[start].time : zero,
    fadeOut:
      end < keys.length - 1 ? difference(duration, keys[end].time) : zero,
  };
}

export const fadeInput = (time: ExactTime) =>
  String(Number(seconds(time).toFixed(6)));

/** Changing a numeric fade authors ordinary linear keys in clip-local time.
 * Playback evaluation, gain units, retiming and validation remain in Rust. */
export function fadeChannel(
  element: Element,
  model: SimpleFades,
  field: "fadeIn" | "fadeOut",
  value: string,
  freshId: () => string = () => crypto.randomUUID(),
): ScalarChannel | null {
  if (Number(value) < 0)
    throw new FadeInputError("Fade duration must be zero or positive.", field);
  let time: ExactTime;
  try {
    time = inputTime(value);
  } catch {
    throw new FadeInputError(
      "Enter a finite fade duration within the editing range.",
      field,
    );
  }
  if (compare(time, zero) < 0)
    throw new FadeInputError("Fade duration must be zero or positive.", field);
  const fadeIn = field === "fadeIn" ? time : model.fadeIn;
  const fadeOut = field === "fadeOut" ? time : model.fadeOut;
  if (compare(fadeIn, difference(model.duration, fadeOut)) > 0)
    throw new FadeInputError(
      "Fade in and fade out must fit inside the clip.",
      field,
    );
  if (compare(time, model[field]) === 0) return null;
  const points: { time: ExactTime; value: number }[] = [
    {
      time: zero,
      value: compare(fadeIn, zero) > 0 ? model.floor : model.level,
    },
  ];
  if (compare(fadeIn, zero) > 0)
    points.push({ time: fadeIn, value: model.level });
  const end = difference(model.duration, fadeOut);
  if (compare(fadeOut, zero) > 0 && compare(end, points.at(-1)!.time) > 0)
    points.push({ time: end, value: model.level });
  if (compare(model.duration, points.at(-1)!.time) > 0)
    points.push({
      time: model.duration,
      value: compare(fadeOut, zero) > 0 ? model.floor : model.level,
    });
  const used = new Set<string>();
  const keys: ScalarKey[] = points.map((p, i) => {
    const previous =
      model.channel?.keys.find(
        (k) => !used.has(k.id) && compare(k.time, p.time) === 0,
      ) ?? (i === points.length - 1 ? model.channel?.keys.at(-1) : undefined);
    const id = previous && !used.has(previous.id) ? previous.id : freshId();
    used.add(id);
    return {
      ...p,
      id,
      left_handle: null,
      right_handle: null,
      segment_to_next: { kind: "linear" },
      tangent_mode: "auto",
    };
  });
  return {
    kind: "scalar",
    id: model.channel?.id ?? freshId(),
    target: { kind: "element", element_id: element.id },
    binding: { kind: "property", parameter: model.parameter },
    extrapolation: { before: "hold", after: "hold" },
    keys,
  };
}
