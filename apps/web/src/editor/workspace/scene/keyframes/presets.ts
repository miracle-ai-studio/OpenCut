import type { Curve } from "./types";
export type CurvePreset = { id: string; name: string; curve: Curve };
// Exact Classic easing-presets.ts values. These are UI inputs; Rust turns them
// into signed time/value controls and validates the fixed source aggregate.
export const BUILTIN_PRESETS: readonly CurvePreset[] = [
  {
    id: "smooth",
    name: "Smooth",
    curve: { x1: 0.25, y1: 0.1, x2: 0.25, y2: 1 },
  },
  { id: "ease-out", name: "Ease out", curve: { x1: 0, y1: 0, x2: 0.2, y2: 1 } },
  { id: "ease-in", name: "Ease in", curve: { x1: 0.8, y1: 0, x2: 1, y2: 1 } },
  {
    id: "ease-in-out",
    name: "In out",
    curve: { x1: 0.4, y1: 0, x2: 0.2, y2: 1 },
  },
  {
    id: "pop",
    name: "Pop",
    curve: { x1: 0.175, y1: 0.885, x2: 0.32, y2: 1.275 },
  },
  { id: "linear", name: "Linear", curve: { x1: 0, y1: 0, x2: 1, y2: 1 } },
];
function validCurve(c: unknown): c is Curve {
  if (!c || typeof c !== "object") return false;
  const n = c as Record<string, unknown>;
  return (
    Object.keys(n).length === 4 &&
    [n.x1, n.x2].every(
      (v) => typeof v === "number" && Number.isFinite(v) && v >= 0 && v <= 1,
    ) &&
    [n.y1, n.y2].every(
      (v) =>
        typeof v === "number" && Number.isFinite(v) && v >= -0.5 && v <= 1.5,
    )
  );
}
export function parsePresets(raw: string | null): CurvePreset[] {
  if (!raw) return [];
  if (raw.length > 32768 || new TextEncoder().encode(raw).byteLength > 32768)
    throw new Error("Curve preset preferences exceed 32 KiB.");
  const items: unknown = JSON.parse(raw);
  if (!Array.isArray(items) || items.length > 32)
    throw new Error("Save at most 32 custom curve presets.");
  const ids = new Set<string>();
  return items.map((item) => {
    if (
      !item ||
      typeof item !== "object" ||
      Object.keys(item).sort().join(",") !== "curve,id,name" ||
      typeof item.id !== "string" ||
      !/^[A-Za-z0-9-]{1,64}$/.test(item.id) ||
      ids.has(item.id) ||
      typeof item.name !== "string" ||
      !item.name.trim() ||
      item.name.length > 64 ||
      !validCurve(item.curve)
    )
      throw new Error("Invalid custom curve preset preferences.");
    ids.add(item.id);
    return { id: item.id, name: item.name, curve: { ...item.curve } };
  });
}
export function loadPresets() {
  return parsePresets(localStorage.getItem("opencut.curve-presets") ??
    localStorage.getItem("opencut.curve-presets.v1"));
}
export function savePresets(presets: CurvePreset[]) {
  const raw = JSON.stringify(presets);
  parsePresets(raw);
  localStorage.setItem("opencut.curve-presets", raw);
}
