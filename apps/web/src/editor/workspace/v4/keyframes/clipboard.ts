import { add, difference, refKey, zero } from "./types";
import type {
  Channel,
  ExactTime,
  KeyframeEdit,
  Row,
  ScalarChannel,
  DiscreteChannel,
} from "./types";
type Entry = { channel: Channel };
export type KeyframeClipboard = { schema_version: 1; entries: Entry[] };
let fallback: KeyframeClipboard | null = null;
function earlier(a: ExactTime, b: ExactTime) {
  return (
    BigInt(a.ticks) * BigInt(b.timebase) < BigInt(b.ticks) * BigInt(a.timebase)
  );
}
export function copySelection(
  rows: Row[],
  selected: Set<string>,
): KeyframeClipboard {
  const active = rows
    .map((row) => ({
      row,
      keys: row.channel.keys.filter((key) =>
        selected.has(refKey({ channel_id: row.channel.id, key_id: key.id })),
      ),
    }))
    .filter((g) => g.keys.length);
  if (!active.length) throw new Error("Select keys to copy.");
  let first: ExactTime | null = null;
  let count = 0;
  for (const { row, keys } of active)
    for (const key of keys) {
      const time = add(key.time, row.start);
      if (!first || earlier(time, first)) first = time;
      if (++count > 4096) throw new Error("Copy at most 4096 keys.");
    }
  return {
    schema_version: 1,
    entries: active.map(({ row, keys }) => ({
      channel: {
        ...structuredClone(row.channel),
        keys: keys.map((key) => ({
          ...structuredClone(key),
          time: difference(add(key.time, row.start), first!),
        })),
      } as Channel,
    })),
  };
}
export async function writeClipboard(value: KeyframeClipboard) {
  fallback = structuredClone(value);
  if (navigator.clipboard?.writeText)
    await navigator.clipboard.writeText(
      JSON.stringify({ opencut_keyframes_v1: value }),
    );
}
export function parseClipboard(raw: string): KeyframeClipboard {
  if (raw.length > 524288)
    throw new Error("Keyframe clipboard exceeds 512 KiB.");
  const envelope: unknown = JSON.parse(raw);
  if (
    !envelope ||
    typeof envelope !== "object" ||
    Object.keys(envelope).join() !== "opencut_keyframes_v1"
  )
    throw new Error("Clipboard has no OpenCut keyframes.");
  const value = (envelope as { opencut_keyframes_v1: unknown })
    .opencut_keyframes_v1;
  if (!value || typeof value !== "object")
    throw new Error("Invalid keyframe clipboard.");
  const clip = value as KeyframeClipboard;
  if (
    clip.schema_version !== 1 ||
    !Array.isArray(clip.entries) ||
    !clip.entries.length ||
    clip.entries.length > 128
  )
    throw new Error("Invalid keyframe clipboard entries.");
  let count = 0;
  for (const entry of clip.entries) {
    const c = entry?.channel;
    if (
      !c ||
      !["scalar", "discrete"].includes(c.kind) ||
      !Array.isArray(c.keys) ||
      !c.keys.length
    )
      throw new Error("Invalid keyframe channel.");
    for (const key of c.keys) {
      if (
        ++count > 4096 ||
        !key ||
        typeof key.id !== "string" ||
        key.id.length > 128 ||
        !key.time ||
        typeof key.time.ticks !== "string" ||
        !/^\d+$/.test(key.time.ticks) ||
        !Number.isInteger(key.time.timebase) ||
        key.time.timebase < 1 ||
        key.time.timebase > 4294967295
      )
        throw new Error("Invalid clipboard key time.");
      if (
        c.kind === "scalar" &&
        typeof (key as ScalarChannel["keys"][number]).value !== "number"
      )
        throw new Error("Invalid scalar clipboard value.");
    }
  }
  // Full target, binding, key, tangent and physical validation stays with the
  // typed Rust planner. JSON size/shape limits here prevent unbounded UI work.
  return structuredClone(clip);
}
export async function readClipboard() {
  if (navigator.clipboard?.readText) {
    const raw = await navigator.clipboard.readText();
    return parseClipboard(raw);
  }
  if (fallback) return structuredClone(fallback);
  throw new Error("Copy keyframes before pasting.");
}
export function pasteSelection(
  clipboard: KeyframeClipboard,
  rows: Row[],
  elementId: string,
  at: ExactTime,
): KeyframeEdit[] {
  const edits: KeyframeEdit[] = [];
  for (const entry of clipboard.entries) {
    const source = entry.channel;
    const matches = rows.filter(
      (row) =>
        row.owner.kind === "element" &&
        row.owner.element_id === elementId &&
        row.channel.target.kind === "element" &&
        row.channel.kind === source.kind &&
        (source.kind === "scalar" && row.channel.kind === "scalar"
          ? JSON.stringify(source.binding) ===
            JSON.stringify(row.channel.binding)
          : source.kind === "discrete" &&
            row.channel.kind === "discrete" &&
            source.parameter === row.channel.parameter),
    );
    if (source.target.kind !== "element")
      throw new Error(
        "Paste effect, mask and track keyframes into an explicitly selected matching channel.",
      );
    let target = matches[0]?.channel;
    if (matches.length > 1)
      throw new Error("Choose an unambiguous destination channel.");
    if (!target) {
      target = {
        ...structuredClone(source),
        id: crypto.randomUUID(),
        target: { kind: "element", element_id: elementId },
        keys: [],
      } as Channel;
      edits.push({
        kind: "channel_put",
        owner: { kind: "element", element_id: elementId },
        channel: target,
      });
    }
    edits.push(pasteInto(source, target, at));
  }
  return edits;
}
export function pasteInto(
  source: Channel,
  target: Channel,
  at: ExactTime,
): KeyframeEdit {
  if (
    source.kind !== target.kind ||
    (source.kind === "scalar" && target.kind === "scalar"
      ? JSON.stringify(source.binding) !== JSON.stringify(target.binding)
      : source.kind === "discrete" &&
        target.kind === "discrete" &&
        source.parameter !== target.parameter)
  )
    throw new Error("Clipboard and destination parameter types differ.");
  if (source.kind === "scalar")
    return {
      kind: "paste_scalar",
      channel_id: target.id,
      keys: (source as ScalarChannel).keys.map((k) => ({
        ...structuredClone(k),
        id: crypto.randomUUID(),
        ...(k.segment_id ? { segment_id: crypto.randomUUID() } : {}),
      })),
      source_anchor: zero,
      destination_anchor: at,
    };
  return {
    kind: "paste_discrete",
    channel_id: target.id,
    keys: (source as DiscreteChannel).keys.map((k) => ({
      ...structuredClone(k),
      id: crypto.randomUUID(),
    })),
    source_anchor: zero,
    destination_anchor: at,
  };
}
