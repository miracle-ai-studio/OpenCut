import { t, parameterLabel } from "../../../shared/locale";
import type { ToolInput } from "../../../../lib/editor-sdk";
import type { Element, Sequence } from "../types";
export type KeyframeRequest = ToolInput<"opencut_keyframe_plan">;
export type KeyframeEdit = KeyframeRequest["edits"][number];
export type Channel = Element["channels"][number];
export type ScalarChannel = Extract<Channel, { kind: "scalar" }>;
export type DiscreteChannel = Extract<Channel, { kind: "discrete" }>;
export type ScalarKey = ScalarChannel["keys"][number];
export type DiscreteKey = DiscreteChannel["keys"][number];
export type KeyRef = { channel_id: string; key_id: string };
export type Curve = Extract<KeyframeEdit, { kind: "curve" }>["curve"];
export type ExactTime = ScalarKey["time"];
export type Row = {
  channel: Channel;
  owner: Extract<KeyframeEdit, { kind: "channel_put" }>["owner"];
  label: string;
  start: ExactTime;
  duration: ExactTime;
};
export const zero: ExactTime = { ticks: "0", timebase: 1 };
export const seconds = (time: ExactTime) => Number(time.ticks) / time.timebase;
export function inputTime(value: string): ExactTime {
  const n = Number(value);
  if (
    !value.trim() ||
    !Number.isFinite(n) ||
    Math.abs(n) * 120000 > Number.MAX_SAFE_INTEGER
  )
    throw new Error("Enter a finite time within the exact editing range.");
  return {
    ticks: String(Math.sign(n) * Math.round(Math.abs(n) * 120000)),
    timebase: 120000,
  };
}
export function difference(a: ExactTime, b: ExactTime): ExactTime {
  const n =
      BigInt(a.ticks) * BigInt(b.timebase) -
      BigInt(b.ticks) * BigInt(a.timebase),
    d = BigInt(a.timebase) * BigInt(b.timebase);
  const gcd = (a: bigint, b: bigint): bigint =>
    b === 0n ? (a < 0n ? -a : a) : gcd(b, a % b);
  const g = gcd(n, d) || 1n;
  if (
    d / g > 4294967295n ||
    n / g < -9223372036854775808n ||
    n / g > 9223372036854775807n
  )
    throw new Error("The exact local time cannot be represented.");
  return { ticks: String(n / g), timebase: Number(d / g) };
}
export const refKey = (r: KeyRef) => `${r.channel_id}\u0000${r.key_id}`;
export function channelRows(sequence: Sequence, elementId: string): Row[] {
  const track = sequence.tracks.find((t) =>
      t.elements.some((e) => e.id === elementId),
    ),
    element = track?.elements.find((e) => e.id === elementId);
  if (!track || !element) return [];
  let trackDuration = zero;
  for (const track of sequence.tracks)
    for (const e of track.elements) {
      const end = add(e.timeline_range.start, e.timeline_range.duration);
      if (
        BigInt(end.ticks) * BigInt(trackDuration.timebase) >
        BigInt(trackDuration.ticks) * BigInt(end.timebase)
      )
        trackDuration = end;
    }
  for (const caption of sequence.captions) {
    const end = add(
      caption.timeline_range.start,
      caption.timeline_range.duration,
    );
    if (
      BigInt(end.ticks) * BigInt(trackDuration.timebase) >
      BigInt(trackDuration.ticks) * BigInt(end.timebase)
    )
      trackDuration = end;
  }
  return [
    ...element.channels.map((channel) => ({
      channel,
      owner: { kind: "element" as const, element_id: elementId },
      label: channelLabel(channel),
      start: element.timeline_range.start,
      duration: element.timeline_range.duration,
    })),
    ...track.channels.map((channel) => ({
      channel,
      owner: { kind: "track" as const, track_id: track.id },
      label: `${t("Track")}: ${channelLabel(channel)}`,
      start: zero,
      duration: trackDuration,
    })),
  ];
}
export function channelLabel(channel: Channel) {
  const target = channel.target;
  const path =
    target.kind === "effect"
      ? `${t("Effect")}: `
      : target.kind === "mask"
        ? `${t("Mask")}: `
        : "";
  return (
    path +
    (channel.kind === "discrete"
      ? parameterLabel(channel.parameter)
      : channel.binding.kind === "property"
        ? parameterLabel(channel.binding.parameter)
        : `${parameterLabel(channel.binding.parameter)} · ${parameterLabel(channel.binding.component)}`)
  );
}
export function copiedKeys(row: Row, selected: Set<string>) {
  return row.channel.keys.filter((k) =>
    selected.has(refKey({ channel_id: row.channel.id, key_id: k.id })),
  );
}
export function add(a: ExactTime, b: ExactTime): ExactTime {
  return difference(a, {
    ticks: String(-BigInt(b.ticks)),
    timebase: b.timebase,
  });
}
