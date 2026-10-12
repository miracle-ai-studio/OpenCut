import { useLayoutEffect, useRef, useState } from "react";
import type { EditorApi } from "../api";
import type { V4Asset, V4Element, V4GainDraft } from "./types";
import type { TimelineScope } from "./timeline-edit/types";
import { ArtDisplay, useSourceArt } from "./source-art/view";
import type { ArtRequestSelection } from "./source-art/service";

/** A draft belongs to one exact element view; it never becomes an authored
 * gain or a second client waveform algorithm. */
export function waveformTemporaryGain(scope: TimelineScope, trackId: string, elementId: string, draft?: V4GainDraft | null) {
  return draft?.changed && draft.projectId === scope.projectId && draft.revision === scope.revision &&
    draft.sequenceId === scope.sequenceId && draft.trackId === trackId && draft.elementId === elementId ? draft.tenths : null;
}

export function TimelineArt({ api, scope, trackId, element, asset, scale, gainDraft }: {
  api: EditorApi; scope: TimelineScope; trackId: string; element: V4Element; asset: V4Asset; scale: number; gainDraft?: V4GainDraft | null;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const layoutKey = JSON.stringify([scope.projectId, scope.revision, scope.sequenceId, trackId, element.id,
    element.timeline_range.start, element.timeline_range.duration, scale]);
  const [measured, setWindow] = useState<{ key: string; from: number; through: number; height: number; dpr: number } | null>(null);
  const window = measured?.key === layoutKey ? measured : null;
  useLayoutEffect(() => {
    const node = ref.current, viewport = node?.closest(".v4-timeline-viewport");
    if (!node || !viewport) return;
    let frame = 0;
    const measure = () => {
      frame = 0;
      const clip = node.getBoundingClientRect(), visible = viewport.getBoundingClientRect();
      const from = Math.max(0, visible.left - clip.left), through = Math.min(clip.width, visible.right - clip.left);
      const value = clip.height > 0 && clip.bottom > visible.top && clip.top < visible.bottom && through > from
        ? { key: layoutKey, from, through, height: clip.height, dpr: globalThis.devicePixelRatio || 1 } : null;
      setWindow(old => old?.key === value?.key && old?.from === value?.from && old?.through === value?.through && old?.height === value?.height && old?.dpr === value?.dpr ? old : value);
    };
    const update = () => { if (!frame) frame = requestAnimationFrame(measure); };
    const resize = typeof ResizeObserver !== "undefined" ? new ResizeObserver(update) : null;
    resize?.observe(node); resize?.observe(viewport);
    viewport.addEventListener("scroll", update, { passive: true }); globalThis.addEventListener("resize", update);
    // New layout facts must be measured before subscribing with the new scale
    // or placement; the prior clipped window belongs to the prior layout only.
    measure();
    return () => { if (frame) cancelAnimationFrame(frame); resize?.disconnect(); viewport.removeEventListener("scroll", update); globalThis.removeEventListener("resize", update); };
  }, [layoutKey]);
  const audio = element.content.kind === "audio";
  const selected: ArtRequestSelection | null = !window ? null : audio ? { projectId: scope.projectId, revision: scope.revision, asset,
    display: { sequence_id: scope.sequenceId, track_id: trackId, element_id: element.id, sample_rate: 48000,
      temporary_gain: waveformTemporaryGain(scope, trackId, element.id, gainDraft), viewport: { clip_left_px: window.from,
        clip_right_px: window.through, pixels_per_second: scale, css_height: window.height, device_pixel_ratio: window.dpr } } }
    : { projectId: scope.projectId, revision: scope.revision, asset, query: { kind: "thumbnail" } };
  const state = useSourceArt(api, selected);
  return <div ref={ref} className="v4-timeline-art" aria-hidden={!selected || undefined}
    style={{ position: "absolute", inset: 0, top: 26, bottom: 22, pointerEvents: "none", overflow: "hidden" }}>
    {selected && (audio ? <ArtDisplay art={state?.art} error={state?.error} loading={!state?.art && !state?.error} />
      : <div style={{ position: "absolute", left: window!.from, width: window!.through - window!.from }}>
        <ArtDisplay art={state?.art} error={state?.error} loading={!state?.art && !state?.error} tiled />
      </div>)}
  </div>;
}
