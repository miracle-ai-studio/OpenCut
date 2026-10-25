import { memo, useLayoutEffect, useRef, useState } from "react";
import type { EditorApi } from "../api";
import type { Asset, Element, GainDraft } from "./types";
import type { TimelineScope } from "./timeline-edit/types";
import { ArtDisplay, useSourceArt } from "./source-art/view";
import type { ArtRequestSelection } from "./source-art/service";

/** A draft belongs to one exact element view; it never becomes an authored
 * gain or a second client waveform algorithm. */
export function waveformTemporaryGain(scope: TimelineScope, trackId: string, elementId: string, draft?: GainDraft | null) {
  return draft?.changed && draft.projectId === scope.projectId && draft.revision === scope.revision &&
    draft.sequenceId === scope.sequenceId && draft.trackId === trackId && draft.elementId === elementId ? draft.tenths : null;
}

/** Scroll is a presentation change, not a new audio projection for every pixel.
 * Short clips keep one bounded native drawing; long clips move in overscanned
 * tiles. Peaks, gain and source clocks are still computed by the native port. */
export function waveformViewport(width: number, from: number, through: number) {
  if (width <= 4096) return { from: 0, through: width };
  const left = Math.floor(Math.max(0, from - 256) / 512) * 512;
  return { from: left, through: Math.min(width, left + 8192, Math.ceil((through + 256) / 512) * 512) };
}

export const TimelineArt = memo(function TimelineArt({ api, scope, trackId, element, asset, scale, gainDraft }: {
  api: EditorApi; scope: TimelineScope; trackId: string; element: Element; asset: Asset; scale: number; gainDraft?: GainDraft | null;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const audio = element.content.kind === "audio";
  const layoutKey = JSON.stringify([scope.projectId, scope.revision, scope.sequenceId, trackId, element.id,
    element.timeline_range.start, element.timeline_range.duration, scale, audio]);
  const [measured, setWindow] = useState<{ key: string; from: number; through: number; height: number; dpr: number } | null>(null);
  const window = measured?.key === layoutKey ? measured : null;
  useLayoutEffect(() => {
    const node = ref.current, viewport = node?.closest(".editor-timeline-viewport");
    if (!node || !viewport) return;
    let frame = 0;
    const measure = () => {
      frame = 0;
      const clip = node.getBoundingClientRect(), visible = viewport.getBoundingClientRect();
      const from = Math.max(0, visible.left - clip.left), through = Math.min(clip.width, visible.right - clip.left);
      const value = clip.height > 0 && clip.bottom > visible.top && clip.top < visible.bottom && through > from
        ? { key: layoutKey, ...(audio ? waveformViewport(clip.width, from, through) : { from, through }), height: clip.height, dpr: globalThis.devicePixelRatio || 1 } : null;
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
  }, [layoutKey, audio]);
  const selected: ArtRequestSelection | null = !window ? null : audio ? { projectId: scope.projectId, revision: scope.revision, asset,
    display: { sequence_id: scope.sequenceId, track_id: trackId, element_id: element.id, sample_rate: 48000,
      temporary_gain: waveformTemporaryGain(scope, trackId, element.id, gainDraft), viewport: { clip_left_px: window.from,
        clip_right_px: window.through, pixels_per_second: scale, css_height: window.height, device_pixel_ratio: window.dpr } } }
    : { projectId: scope.projectId, revision: scope.revision, asset, query: { kind: "thumbnail" } };
  const state = useSourceArt(api, selected);
  const [failureWindow, setFailureWindow] = useState<{ key: string; from: number; through: number } | null>(null);
  const failed = !!state?.error;
  useLayoutEffect(() => {
    const node = ref.current, viewport = node?.closest(".editor-timeline-viewport");
    if (!failed || !node || !viewport) return;
    let frame = 0;
    const measure = () => {
      frame = 0;
      const clip = node.getBoundingClientRect(), visible = viewport.getBoundingClientRect();
      const header = node.closest(".editor-track")?.querySelector(".editor-track-header")?.getBoundingClientRect();
      const from = Math.max(0, Math.max(visible.left, header?.right ?? visible.left) - clip.left), through = Math.min(clip.width, visible.right - clip.left);
      const value = through > from ? { key: layoutKey, from, through } : null;
      setFailureWindow(old => old?.key === value?.key && old?.from === value?.from && old?.through === value?.through ? old : value);
    };
    const update = () => { if (!frame) frame = requestAnimationFrame(measure); };
    const resize = typeof ResizeObserver !== "undefined" ? new ResizeObserver(update) : null;
    resize?.observe(node); resize?.observe(viewport);
    viewport.addEventListener("scroll", update, { passive: true }); globalThis.addEventListener("resize", update);
    measure();
    return () => { if (frame) cancelAnimationFrame(frame); resize?.disconnect(); viewport.removeEventListener("scroll", update); globalThis.removeEventListener("resize", update); };
  }, [failed, layoutKey]);
  // Error controls follow the actual visible interval, never the audio
  // overscan tile. Leave both trim edges intact, including narrow clips.
  const failure = failureWindow?.key === layoutKey ? failureWindow : null;
  const retryFits = !!failure && failure.through - failure.from >= (audio ? 88 : 60);
  return <div ref={ref} className="v4-timeline-art" aria-hidden={!selected || undefined}
    style={{ position: "absolute", inset: 0, top: "var(--timeline-art-top, 26px)", bottom: "var(--timeline-art-bottom, 22px)",
      pointerEvents: "none", overflow: failed ? "visible" : "hidden" }}>
    {selected && (state?.error ? failure && <div style={{ position: "absolute", left: failure.from + (retryFits ? 8 : 0),
      width: Math.max(0, failure.through - failure.from - (retryFits ? 16 : 0)), height: 28 }}>
      <ArtDisplay error={state.error} onRetry={retryFits ? state.retry : undefined} retryName={element.name}
        retryHint={!retryFits ? "Zoom in to retry source artwork." : undefined} />
    </div> : audio ? <ArtDisplay art={state?.art} loading={!state?.art} />
      : <div style={{ position: "absolute", left: window!.from, width: window!.through - window!.from }}>
        <ArtDisplay art={state?.art} loading={!state?.art} tiled />
      </div>)}
  </div>;
}, (a, b) => a.api === b.api && a.trackId === b.trackId && a.element === b.element && a.asset === b.asset &&
  a.scale === b.scale && a.gainDraft === b.gainDraft && a.scope.projectId === b.scope.projectId &&
  a.scope.revision === b.scope.revision && a.scope.sequenceId === b.scope.sequenceId);
