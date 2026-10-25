import type { Element } from "../types";
import type {
  KeyframeEdit,
  ScalarChannel,
  ScalarKey,
  ExactTime,
} from "../keyframes/types";
import { difference, add, inputTime, seconds } from "../keyframes/types";
import { BUILTIN_PRESETS } from "../keyframes/presets";
export function gainChannel(element: Element): ScalarChannel | null {
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
  // Short fades need their own time samples even inside a long clip. Values
  // still come exclusively from Rust; this only requests a bounded time grid.
  const divisions = Math.min(
    16,
    Math.floor(Math.max(0, 1024 - all.length) / Math.max(1, keys.length - 1)),
  );
  for (let i = 0; i + 1 < keys.length; i++) {
    const a = keys[i].time,
      b = keys[i + 1].time;
    for (let part = 1; part < divisions; part++) {
      all.push({
        ticks: String(
          ((BigInt(a.ticks) * BigInt(b.timebase) * BigInt(divisions - part) +
            BigInt(b.ticks) * BigInt(a.timebase) * BigInt(part)) *
            120000n) /
            (BigInt(a.timebase) * BigInt(b.timebase) * BigInt(divisions)),
        ),
        timebase: 120000,
      });
    }
  }
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

/** Reuse the existing Classic easing input and Rust curve planner. No client
 * evaluation, key retiming or level normalization is introduced. */
export function curveEdits(
  channel: ScalarChannel,
  smooth: boolean,
): KeyframeEdit[] {
  if (!smooth)
    return channel.keys.map((key) => ({
      kind: "put_scalar",
      channel_id: channel.id,
      key: {
        ...structuredClone(key),
        segment_to_next: { kind: "linear" },
        left_handle: null,
        right_handle: null,
        tangent_mode: "broken",
      },
    }));
  const curve = BUILTIN_PRESETS.find(
    (preset) => preset.id === "ease-in-out",
  )!.curve;
  return channel.keys.slice(0, -1).map((key, index) => ({
    kind: "curve",
    channel_id: channel.id,
    left_key_id: key.id,
    right_key_id: channel.keys[index + 1].id,
    curve: { ...curve },
    // Flat segments require an explicit reference. Both value handles are zero
    // for this preset, so the plateau remains exactly at the original level.
    reference_span_value: 1,
  }));
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
  let min = -Math.min(...values),
    max = seconds(duration) - Math.max(...values);
  // Moving a Bezier point must not push an unchanged tangent through its
  // neighbor. Preserve raw handles while keeping every pointer draft valid.
  keys.forEach((key, index) => {
    if (!selected.has(key.id)) return;
    const time = seconds(key.time),
      previous = keys[index - 1],
      next = keys[index + 1];
    if (previous && !selected.has(previous.id)) {
      min = Math.max(
        min,
        seconds(previous.time) - time + 1 / 120000,
        seconds(previous.time) +
          seconds(previous.right_handle?.dt ?? { ticks: "0", timebase: 1 }) -
          time,
        seconds(previous.time) -
          time -
          seconds(key.left_handle?.dt ?? { ticks: "0", timebase: 1 }),
      );
    }
    if (next && !selected.has(next.id)) {
      max = Math.min(
        max,
        seconds(next.time) - time - 1 / 120000,
        seconds(next.time) +
          seconds(next.left_handle?.dt ?? { ticks: "0", timebase: 1 }) -
          time,
        seconds(next.time) -
          time -
          seconds(key.right_handle?.dt ?? { ticks: "0", timebase: 1 }),
      );
    }
  });
  return inputTime(Math.max(min, Math.min(max, deltaSeconds)).toFixed(6));
}
export function localPlayhead(time: ExactTime, element: Element): ExactTime {
  return difference(time, element.timeline_range.start);
}

/** SVG geometry for the pointer draft, using the authored controls verbatim.
 * Saved/playback values still come from the Rust evaluator. */
export function authoringPath(
  keys: readonly ScalarKey[],
  extrapolation: ScalarChannel["extrapolation"],
  start: number,
  end: number,
  x: (t: number) => number,
  y: (v: number) => number,
): string {
  if (!keys.length) return "";
  const first = keys[0],
    last = keys[keys.length - 1];
  const before = Math.min(start, seconds(first.time)), after = Math.max(end, seconds(last.time));
  const extended = (a:ScalarKey,b:ScalarKey,at:number)=>a.value + (at-seconds(a.time))/(seconds(b.time)-seconds(a.time))*(b.value-a.value);
  const firstValue=keys.length>1 && extrapolation.before === "linear" ? extended(first,keys[1],before) : first.value;
  const lastValue=keys.length>1 && extrapolation.after === "linear" ? extended(keys[keys.length-2],last,after) : last.value;
  let path = `M ${x(before)} ${y(firstValue)} L ${x(seconds(first.time))} ${y(first.value)}`;
  for (let i = 0; i + 1 < keys.length; i++) {
    const a = keys[i],
      b = keys[i + 1],
      ta = seconds(a.time),
      tb = seconds(b.time),
      dt = tb - ta,
      dv = b.value - a.value;
    if (a.segment_to_next.kind === "step")
      path += ` L ${x(tb)} ${y(a.value)} L ${x(tb)} ${y(b.value)}`;
    else if (a.segment_to_next.kind === "bezier") {
      path += ` C ${x(ta + (a.right_handle ? seconds(a.right_handle.dt) : dt / 3))} ${y(a.value + (a.right_handle?.dv ?? dv / 3))} ${x(tb + (b.left_handle ? seconds(b.left_handle.dt) : -dt / 3))} ${y(b.value + (b.left_handle?.dv ?? -dv / 3))} ${x(tb)} ${y(b.value)}`;
    } else if (a.segment_to_next.kind === "legacy_normalized_bezier") {
      const c = a.segment_to_next.curve;
      path += ` C ${x(ta + dt * c.x1)} ${y(a.value + dv * c.y1)} ${x(ta + dt * c.x2)} ${y(a.value + dv * c.y2)} ${x(tb)} ${y(b.value)}`;
    } else path += ` L ${x(tb)} ${y(b.value)}`;
  }
  return path + ` L ${x(after)} ${y(lastValue)}`;
}
