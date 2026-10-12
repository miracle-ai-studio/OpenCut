import type { Clip, Curve, Snapshot, VisualGraph } from "./types";

export type CopiedClip = {
  clip: Clip;
  trackId: string;
  project: string;
  effects: VisualGraph["effects"];
  masks: VisualGraph["masks"];
  curves: Curve[];
};

/** A clipboard owns copied values, never a reference to a later project head. */
export function copyClip(
  view: Snapshot,
  sequenceId: string,
  trackId: string,
  clip: Clip,
): CopiedClip {
  const graph = view.visual.find((graph) => graph.sequence_id === sequenceId);
  return structuredClone({
    clip,
    trackId,
    project: view.project.id,
    effects:
      graph?.effects.filter((effect) => effect.target_id === clip.id) ?? [],
    masks: graph?.masks.filter((mask) => mask.target_id === clip.id) ?? [],
    curves: view.curves.filter(
      (curve) => curve.sequence_id === sequenceId && curve.clip_id === clip.id,
    ),
  });
}
