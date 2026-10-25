import type { Element, SessionState } from "../types";

export const visualFields = [
  { key: "position_x_px", label: "Horizontal position (pixels)", unit: "px" },
  { key: "position_y_px", label: "Vertical position (pixels)", unit: "px" },
  { key: "scale_x", label: "Horizontal scale (percent)", unit: "%" },
  { key: "scale_y", label: "Vertical scale (percent)", unit: "%" },
  { key: "rotation_degrees", label: "Rotation (degrees)", unit: "°" },
  { key: "opacity", label: "Opacity (percent)", unit: "%" },
] as const;
export type VisualField = (typeof visualFields)[number]["key"];
export type VisualTarget = { trackId: string; element: Element };
export const selectionIdentity = (scene: SessionState) =>
  JSON.stringify([scene.selectedId, scene.timelineEdit.selection]);

export function visualTargets(scene: SessionState, ids: readonly string[]): VisualTarget[] | null {
  const sequence = (scene.historical ?? scene.head).project.sequences.find(s => s.id === scene.sequenceId);
  if (!sequence || !ids.length || new Set(ids).size !== ids.length) return null;
  const targets: VisualTarget[] = [];
  for (const id of ids) {
    const track = sequence.tracks.find(t => t.elements.some(e => e.id === id));
    const element = track?.elements.find(e => e.id === id);
    if (!track || !element?.visual) return null;
    targets.push({ trackId: track.id, element });
  }
  return targets;
}
export const fieldValue = (element: Element, field: VisualField) =>
  field === "opacity" ? element.visual!.opacity : element.visual!.transform[field];
const percent = (field: VisualField) => field === "opacity" || field === "scale_x" || field === "scale_y";
export function commonVisualValue(targets: readonly VisualTarget[], field: VisualField) {
  const value = fieldValue(targets[0].element, field);
  if (!targets.every(t => fieldValue(t.element, field) === value)) return "";
  return String(Number((value * (percent(field) ? 100 : 1)).toFixed(6)));
}
export function animatedVisualField(targets: readonly VisualTarget[], field: VisualField) {
  return targets.some(({ element }) => element.channels.some(channel =>
    channel.kind === "scalar" && channel.keys.length > 0 &&
    channel.target.kind === "element" && channel.target.element_id === element.id &&
    channel.binding.kind === "property" && channel.binding.parameter === field));
}
export function parseVisualValue(field: VisualField, text: string) {
  if (!text.trim() || !Number.isFinite(Number(text))) throw Error("Enter a finite property value.");
  const number = Number(text);
  if (field === "opacity" && (number < 0 || number > 100)) throw Error("Opacity must be between 0% and 100%.");
  return number / (percent(field) ? 100 : 1);
}
/** UI unit conversion only. Rust validates the authored property intent. */
export function expectedVisualElement(element: Element, field: VisualField, value: number) {
  const next = structuredClone(element);
  if (field === "opacity") next.visual!.opacity = value;
  else next.visual!.transform[field] = value;
  return next;
}
