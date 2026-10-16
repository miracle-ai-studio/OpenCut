import type { V4Element } from "../types";
import type {
  KeyframeEdit,
  ScalarChannel,
  ScalarKey,
  ExactTime,
} from "../keyframes/types";
import { difference, add, inputTime, seconds } from "../keyframes/types";
export function gainChannel(element: V4Element): ScalarChannel | null {
  const audio =
    element.content.kind === "audio" || element.content.kind === "video"
      ? element.content.audio
      : null;
  if (!audio) return null;
  const parameter =
    audio.gain.unit === "decibels" ? "gain_decibels" : "gain_linear_legacy";
  const channels = element.channels.filter(
    (channel): channel is ScalarChannel =>
      channel.kind === "scalar" &&
      channel.target.kind === "element" &&
      channel.target.element_id === element.id &&
      channel.binding.kind === "property" &&
      channel.binding.parameter === parameter &&
      channel.keys.length > 0,
  );
  return channels.length === 1 ? channels[0] : null;
}
export function sampleTimes(
  duration: ExactTime,
  keys: readonly ScalarKey[],
  extra?: ExactTime,
): ExactTime[] {
  const all: ExactTime[] = [];
  for (let i = 0; i <= 128; i++)
    all.push({
      ticks: String(
        (BigInt(duration.ticks) * BigInt(i) * 120000n) /
          (BigInt(duration.timebase) * 128n),
      ),
      timebase: 120000,
    });
  all.push(duration, ...keys.map((k) => k.time));
  if (extra) all.push(extra);
  all.sort((a, b) => {
    const d =
      BigInt(a.ticks) * BigInt(b.timebase) -
      BigInt(b.ticks) * BigInt(a.timebase);
    return d < 0n ? -1 : d > 0n ? 1 : 0;
  });
  return all.filter(
    (time, i) =>
      i === 0 ||
      BigInt(time.ticks) * BigInt(all[i - 1].timebase) !==
        BigInt(all[i - 1].ticks) * BigInt(time.timebase),
  );
}
export function axisBounds(
  channel: ScalarChannel,
  samples: readonly { value: number }[],
) {
  const linear =
      channel.binding.kind === "property" &&
      channel.binding.parameter === "gain_linear_legacy",
    values = [
      ...samples.map((s) => s.value),
      ...channel.keys.map((k) => k.value),
    ];
  return {
    min: Math.min(linear ? 0 : -60, ...values),
    max: Math.max(linear ? 4 : 20, ...values),
    unit: linear ? "linear" : "dB",
  };
}
/** A batch only replaces changed keys. Existing IDs, segments, raw time/value
 * handles, tangent modes and every unselected key remain verbatim. */
export function dragKeys(
  channel: ScalarChannel,
  selected: ReadonlySet<string>,
  delta: ExactTime,
  valueDelta: number,
): KeyframeEdit[] {
  return channel.keys
    .filter((key) => selected.has(key.id))
    .map((key) => ({
      kind: "put_scalar",
      channel_id: channel.id,
      key: {
        ...structuredClone(key),
        time:
          BigInt(delta.ticks) === 0n
            ? structuredClone(key.time)
            : add(key.time, delta),
        value: key.value + valueDelta,
      },
    }));
}
export function boundedTimeDelta(
  keys: readonly ScalarKey[],
  selected: ReadonlySet<string>,
  duration: ExactTime,
  deltaSeconds: number,
): ExactTime {
  const values = keys
    .filter((k) => selected.has(k.id))
    .map((k) => seconds(k.time));
  if (!values.length) return { ticks: "0", timebase: 1 };
  return inputTime(
    Math.max(
      -Math.min(...values),
      Math.min(seconds(duration) - Math.max(...values), deltaSeconds),
    ).toFixed(6),
  );
}
export function localPlayhead(time: ExactTime, element: V4Element): ExactTime {
  return difference(time, element.timeline_range.start);
}
