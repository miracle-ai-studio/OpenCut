import type { Language } from "../../shared/locale";
import type { Document, Snapshot } from "./types";
import type { ToolOutput } from "../../../lib/editor-sdk";
import { historyText as text, hasHistoryText } from "./history-copy";

export type HistoryChange = { object: string; field: string; before: string; after: string; kind: "added" | "removed" | "changed" };
export type RevisionActor = NonNullable<ToolOutput<"opencut_project_revision_actors">["actors"][string]>;
type RecordValue = Record<string, unknown>;
const record = (value: unknown): value is RecordValue => !!value && typeof value === "object" && !Array.isArray(value);
const list = (value: unknown): RecordValue[] => Array.isArray(value) ? value.filter(record) : [];
const same = (a: unknown, b: unknown): boolean => {
  if (a === b) return true;
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((v, i) => same(v, b[i]));
  if (!record(a) || !record(b)) return false;
  const keys = [...new Set([...Object.keys(a), ...Object.keys(b)])];
  return keys.every(key => same(a[key], b[key]));
};
const number = (value: number) => Number.isFinite(value) ? String(Number(value.toFixed(6))) : "—";
const ignored = new Set(["id", "segment_id", "schema_version", "source_locator", "uri", "source_url", "content_hash", "generation_id"]);
const literal = new Set(["name", "content", "text", "note", "family", "families", "location", "original_name", "notice", "attribution", "provider"]);
const containers = new Set(["content", "visual", "audio", "transform"]);

export function actorLabel(actor: RevisionActor | null | undefined, language: Language) {
  if (actor === undefined) return text("actorUnavailable", language);
  if (actor === null) return text("unknownActor", language);
  const who = actor.principal_id === "local-owner" ? text("localAccount", language) : text("account", language, { name: actor.principal_id });
  const source = ({ http: "http", cli: "cli", mcp_http: "mcp", mcp_stdio: "mcp", embedded: "embedded", worker: "worker" } as Record<string, string>)[actor.source] ?? "unknownSource";
  return `${who} · ${text(source, language)}`;
}

/** Validate every returned document against its requested public fixed identity. */
export async function readHistoryPair(
  inspect: (project: string, revision: string) => Promise<Snapshot>,
  project: string, before: string | null, after: string,
): Promise<{ before: Document | null; after: Document }> {
  const read = async (revision: string) => {
    const value = await inspect(project, revision);
    if (value.project.id !== project || value.revision !== revision) throw Error("History snapshot identity changed");
    return value.project;
  };
  const [previous, next] = await Promise.all([before ? read(before) : Promise.resolve(null), read(after)]);
  return { before: previous, after: next };
}

/** Presentation only: compare authorized public snapshots, never reconstruct edits. */
export function historyChanges(before: Document | null, after: Document, language: Language): HistoryChange[] {
  const changes: HistoryChange[] = [];
  const previousRefs = new Map<string, string>(), nextRefs = new Map<string, string>();
  for (const [doc, refs] of [[before, previousRefs], [after, nextRefs]] as const) if (doc) {
    for (const [kind, entries] of [["asset", doc.assets], ["fontResource", doc.fonts], ["scene", doc.sequences]] as const)
      entries.forEach((item, index) => refs.set(item.id, String("name" in item ? item.name : "family" in item ? item.family : `${text(kind, language)} ${index + 1}`)));
    for (const scene of doc.sequences) for (const track of scene.tracks) {
      refs.set(track.id, track.name);
      track.elements.forEach((item, index) => {
        refs.set(item.id, item.name || `${text("clip", language)} ${index + 1}`);
        item.visual?.masks.forEach((mask, i) => refs.set(mask.id, `${item.name} · ${text("masks", language)} ${i + 1}`));
        item.visual?.effects.forEach((effect, i) => refs.set(effect.id, `${item.name} · ${text("effects", language)} ${i + 1}`));
      });
    }
  }
  const label = (key: string) => text(hasHistoryText(key) ? key : "other", language);
  const name = (item: RecordValue, kind: string, index: number) => {
    const title = item.name ?? item.family ?? item.text ?? (kind === "bookmark" ? item.note : undefined);
    return `${label(kind)}${typeof title === "string" && title.trim() ? ` “${title}”` : ` ${index + 1}`}`;
  };
  const atom = (value: unknown, key: string, previous: boolean): string | null => {
    const refs = previous ? previousRefs : nextRefs;
    if (value === null || value === undefined) return text("unset", language);
    if (typeof value === "boolean") return text(value ? "on" : "off", language);
    if (typeof value === "number") return key === "opacity" ? `${number(value * 100)}%` : key.endsWith("_px") ? `${number(value)} px` : key === "rotation_degrees" ? `${number(value)}°` : number(value);
    if (typeof value === "string") {
      if (literal.has(key)) return value || text("unset", language);
      if (key.endsWith("_id")) return refs.get(value) ?? label(key);
      if (value === "none") return text("unset", language);
      return hasHistoryText(value) ? label(value) : text(previous ? "oldValue" : "newValue", language);
    }
    if (!record(value)) return null;
    if (typeof value.ticks === "string" && typeof value.timebase === "number") return `${number(Number(value.ticks) / value.timebase)} s`;
    if (typeof value.numerator === "number" && typeof value.denominator === "number") return `${number(value.numerator / value.denominator)}${key === "fps" ? " fps" : "×"}`;
    if (key === "canvas_size" && typeof value.width === "number" && typeof value.height === "number") return `${value.width} × ${value.height} px`;
    if (key === "gain" && typeof value.value === "number") return `${number(value.value)}${value.unit === "decibels" ? " dB" : "×"}`;
    if ((key === "font_size" || key === "feather") && typeof value.value === "number") return String(value.unit).includes("pixels") ? `${number(value.value)} px` : text(key === "font_size" ? "relativeFontSize" : "relativeFeather", language, { value: number(value.value * (key === "feather" ? 100 : 1)) });
    if (typeof value.red === "number" && typeof value.green === "number" && typeof value.blue === "number") {
      const channel = (v: number) => Math.round(Math.max(0, Math.min(1, value.encoding === "linear_srgb" || value.encoding === "linear" ? v <= 0.0031308 ? v * 12.92 : 1.055 * Math.pow(v, 1 / 2.4) - 0.055 : v)) * 255).toString(16).padStart(2, "0");
      return `#${channel(value.red)}${channel(value.green)}${channel(value.blue)} · ${number(Number(value.alpha ?? 1) * 100)}%`;
    }
    if (key === "font") return typeof value.font_id === "string" ? refs.get(value.font_id) ?? label("font") : typeof value.family === "string" ? value.family : label("font");
    if (key === "version" || key === "content" && typeof value.content_hash === "string") return text(previous ? "oldValue" : "sourceChanged", language);
    return null;
  };
  const push = (object: string, field: string, left: string, right: string, kind: HistoryChange["kind"] = "changed") => changes.push({ object, field, before: left, after: right, kind });
  const describe = (item: RecordValue, previous: boolean) => {
    const refs = previous ? previousRefs : nextRefs;
    const parts: string[] = [];
    if (typeof item.text === "string") parts.push(item.text);
    if (record(item.content)) {
      if (typeof item.content.asset_id === "string") parts.push(refs.get(item.content.asset_id) ?? label("asset"));
      if (record(item.content.text) && typeof item.content.text.content === "string") parts.push(item.content.text.content);
    }
    if (record(item.timeline_range)) parts.push(`${label("start")}: ${atom(item.timeline_range.start, "start", false)}, ${label("duration")}: ${atom(item.timeline_range.duration, "duration", false)}`);
    return parts.join(" · ") || text("present", language);
  };
  function fields(left: RecordValue, right: RecordValue, object: string, prefix = "", omit: string[] = []) {
    for (const key of new Set([...Object.keys(left), ...Object.keys(right)])) {
      if (ignored.has(key) || omit.includes(key) || same(left[key], right[key])) continue;
      const a = left[key], b = right[key], field = [prefix, label(key)].filter(Boolean).join(" · ");
      if (key === "path" && (typeof a === "string" || typeof b === "string")) { push(object, field, text("oldValue", language), text("newValue", language)); continue; }
      if (Array.isArray(a) || Array.isArray(b)) {
        const aa = list(a), bb = list(b);
        if (aa.length || bb.length) collection(aa, bb, key, object, prefix);
        else {
          const values = (value: unknown, previous: boolean) => Array.isArray(value) && value.length ? value.map(item => atom(item, key, previous) ?? text(previous ? "oldValue" : "newValue", language)).join(", ") : text("none", language);
          const av = values(a, true), bv = values(b, false);
          if (av !== bv) push(object, field, av, bv);
        }
        continue;
      }
      const av = atom(a, key, true), bv = atom(b, key, false);
      if (av !== null && bv !== null && av !== bv) push(object, field, av, bv);
      else if (record(a) || record(b)) {
        const count = changes.length;
        fields(record(a) ? a : {}, record(b) ? b : {}, object, containers.has(key) ? prefix : field);
        // A protected locator/provenance change remains visible without exposing its stored value.
        if (changes.length === count) push(object, field, text("oldValue", language), text("newValue", language));
      }
      else if (av === null || bv === null) push(object, field, text("oldValue", language), text("newValue", language));
    }
  }
  function collection(left: RecordValue[], right: RecordValue[], kind: string, parent = "", prefix = "", omit: string[] = [], ordered = true) {
    const key = (item: RecordValue, index: number) => String(item.id ?? index);
    const a = new Map(left.map((item, i) => [key(item, i), { item, i }])), b = new Map(right.map((item, i) => [key(item, i), { item, i }]));
    for (const id of new Set([...a.keys(), ...b.keys()])) {
      const old = a.get(id), next = b.get(id), entry = next ?? old!;
      const object = [parent, name(entry.item, kind, entry.i)].filter(Boolean).join(" · ");
      if (!old || !next) {
        push(object, label(kind), old ? describe(old.item, true) : text("none", language), next ? describe(next.item, false) : text("none", language), next ? "added" : "removed");
        fields(old?.item ?? {}, next?.item ?? {}, object, prefix, omit);
      }
      else {
        if (ordered && old.i !== next.i) push(object, label("order"), String(old.i + 1), String(next.i + 1));
        fields(old.item, next.item, object, prefix, omit);
      }
    }
  }
  const previous = before as unknown as RecordValue | null, next = after as unknown as RecordValue;
  if (!before) push(`${label("project")} “${after.name}”`, label("project"), text("none", language), after.name, "added");
  fields(previous ?? {}, next, `${label("project")} “${after.name}”`, "", ["assets", "fonts", "sequences", "settings"]);
  const settings = (doc: Document | null) => !doc ? [] : doc.settings.profile === "classic_global" ? [{ id: "global", ...doc.settings.settings }] : doc.settings.formats.map(value => ({ id: value.sequence_id, name: doc.sequences.find(scene => scene.id === value.sequence_id)?.name, ...value }));
  collection(settings(before), settings(after), "settings");
  collection(list(before?.assets), list(after.assets), "asset", "", "", [], false);
  collection(list(before?.fonts), list(after.fonts), "fontResource", "", "", [], false);
  collection(list(before?.sequences), list(after.sequences), "scene", "", "", ["tracks", "captions", "transitions", "bookmarks"]);
  // Match clips across all tracks so a move is not presented as delete + add.
  const tracks = (doc: Document | null) => doc?.sequences.flatMap(scene => scene.tracks.map((track, index) => ({ ...track, location: scene.name, order: index + 1 }))) ?? [];
  const clips = (doc: Document | null) => doc?.sequences.flatMap(scene => scene.tracks.flatMap(track => track.elements.map((clip, index) => ({ ...clip, location: `${scene.name} · ${track.name}`, order: index + 1 })))) ?? [];
  collection(tracks(before), tracks(after), "track", "", "", ["elements"], false);
  collection(clips(before), clips(after), "clip", "", "", [], false);
  for (const [field, kind] of [["captions", "caption"], ["bookmarks", "bookmark"], ["transitions", "transition"]] as const) {
    const entries = (doc: Document | null) => doc?.sequences.flatMap(scene => scene[field].map(item => ({ ...item, location: scene.name }))) ?? [];
    collection(entries(before), entries(after), kind, "", "", [], false);
  }
  return changes;
}

export function changeSummary(changes: HistoryChange[], language: Language) {
  if (!changes.length) return text("noChanges", language);
  const first = changes[0];
  const summary = text(first.kind === "added" ? "addedSummary" : first.kind === "removed" ? "removedSummary" : "changedSummary", language, { object: first.object, field: first.field });
  return changes.length > 1 ? `${summary} · ${text("more", language, { count: changes.length - 1 })}` : summary;
}
