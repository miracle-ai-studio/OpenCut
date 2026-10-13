import { useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode, KeyboardEvent as ReactKeyboardEvent, PointerEvent as ReactPointerEvent } from "react";
import { Button } from "../../../shared/ui";
import { t, useLocale } from "../../../shared/locale";
import { addTime, formatTime, secondsForDisplay } from "../../../model/time";
import type { Time } from "../../../model/types";
import type { V4Asset, V4Element, V4Sequence, V4GainDraft } from "../types";
import type { EditorApi } from "../../api";
import { TimelineArt } from "../timeline-art";
import type { V4TimelineEdit } from "./controller";
import type { TimelineAttempt, TimelineCommand, TimelineScope, TimelineTarget } from "./types";
import { TimelineEditControls } from "./controls";
import { ignoresTimelineShortcut, shortcutCommand } from "./actions";
import { TimelineGesture, sameGestureFence } from "./gesture";
import type { GestureDraft, GestureFence } from "./gesture";
import { anchoredScroll, intersectingMedia, MARKER_HEIGHT, pixelsPerSecond, pointerTime, RULER_HEIGHT, sequenceEnd, TRACK_HEIGHT, TRACK_LABEL_WIDTH } from "./geometry";
import type { TimelineRect } from "./geometry";
import type { MediaFileTarget } from "../media";

export type TimelinePanelProps = {
  controller: V4TimelineEdit;
  sequence: V4Sequence;
  scope: TimelineScope;
  selection: TimelineTarget[];
  selectedId: string | null;
  playhead: Time;
  attempt?: TimelineAttempt | null;
  disabled: boolean;
  gainDraftActive: boolean;
  onSelect(target: TimelineTarget, mode: "replace" | "toggle" | "extend"): void;
  onSelectMany(targets: TimelineTarget[], primaryId?: string): void;
  onSelectOther(id: string): void;
  onSeek(time: Time): void;
  onReview(): void;
  onFiles?(files: File[], target: MediaFileTarget): Promise<void>;
  renderAudioControls(element: V4Element): ReactNode;
  sourceArt?: { api: EditorApi; assets: V4Asset[]; gainDraft?: V4GainDraft | null };
};
type Rubber = { pointerId: number; fence: GestureFence; x: number; y: number; original: TimelineTarget[]; additive: boolean };
const media = (element: V4Element) => ["video", "audio", "image"].includes(element.content.kind);
const editable = (element: V4Element) => media(element) || element.content.kind === "text";
const keyOf = (target: TimelineTarget) => `${target.track_id}\u0000${target.element_id}`;

/** One Session owns the project, selection, playhead and transactions. Local
 * state here is only a view (zoom/scroll) or an uncommitted pointer intention. */
export function V4TimelinePanel(props: TimelinePanelProps) {
  const { language, theme } = useLocale();
  const latest = useRef(props); latest.current = props;
  const root = useRef<HTMLDivElement>(null), viewport = useRef<HTMLDivElement>(null), tracks = useRef<HTMLDivElement>(null);
  const gesture = useRef<TimelineGesture | null>(null), rubber = useRef<Rubber | null>(null);
  const clickConsumed = useRef(false), movedRubber = useRef(false);
  const [zoom, setZoom] = useState(1), [ripple, setRipple] = useState(false);
  const [draft, setDraft] = useState<GestureDraft | null>(null), [box, setBox] = useState<TimelineRect | null>(null);
  const [error, setError] = useState<string | null>(null);
  const scale = pixelsPerSecond(zoom), end = sequenceEnd(props.sequence);
  const currentIdentity = props.controller.identity();
  const width = Math.max(640, end * scale + 160), selected = new Set(props.selection.map(keyOf));
  const fence = (): GestureFence => ({ ...latest.current.scope, ...latest.current.controller.identity() });
  const blocked = () => latest.current.disabled || latest.current.gainDraftActive || !!latest.current.attempt;
  const cancel = (restore = true) => {
    const current = rubber.current;
    if (restore && current && sameGestureFence(current.fence, fence())) latest.current.onSelectMany(current.original);
    if (gesture.current?.hasMoved || movedRubber.current) clickConsumed.current = true;
    gesture.current?.cancel(); gesture.current = null; rubber.current = null; movedRubber.current = false;
    setDraft(null); setBox(null);
  };
  const cancelRef = useRef(cancel); cancelRef.current = cancel;
  useEffect(() => {
    cancelRef.current(false);
    setError(null);
  }, [props.scope.projectId, props.scope.revision, props.scope.sequenceId, props.controller, props.disabled, props.gainDraftActive,
    currentIdentity.api, currentIdentity.generation, currentIdentity.viewGeneration]);
  useEffect(() => {
    const blur = () => cancelRef.current();
    window.addEventListener("blur", blur);
    return () => { window.removeEventListener("blur", blur); gesture.current?.cancel(); };
  }, []);
  const review = async (command: TimelineCommand, label: string, captured: GestureFence) => {
    if (!sameGestureFence(captured, fence()) || blocked()) return;
    setError(null);
    try { await latest.current.controller.plan(command, label, captured); }
    catch (cause) {
      if (sameGestureFence(captured, fence())) setError(cause instanceof Error ? cause.message : String(cause));
    }
    if (sameGestureFence(captured, fence())) latest.current.onReview();
  };
  const point = (clientX: number, clientY: number) => {
    const trackRect = tracks.current!.getBoundingClientRect();
    return { x: clientX - trackRect.left - TRACK_LABEL_WIDTH, y: clientY - trackRect.top };
  };
  const start = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || !(event.target instanceof Element)) return;
    if (gesture.current || rubber.current) return;
    if (event.target.closest(".v4-audio-controls,[role=slider],input,select,textarea,[data-timeline-no-gesture]")) return;
    if (props.gainDraftActive || props.attempt?.phase === "applying") return;
    const clip = event.target.closest<HTMLElement>("[data-timeline-editable]");
    if (clip) {
      const target = { track_id: clip.dataset.trackId!, element_id: clip.dataset.elementId! };
      const modifier = event.metaKey || event.ctrlKey ? "toggle" : event.shiftKey ? "extend" : null;
      event.preventDefault(); root.current?.focus({ preventScroll: true });
      clickConsumed.current = false;
      if (modifier) { props.onSelect(target, modifier); clickConsumed.current = true; return; }
      const already = props.selection.some(item => keyOf(item) === keyOf(target));
      const targets = already ? props.selection : [target];
      if (!already) props.onSelect(target, "replace");
      const edge = event.target.closest<HTMLElement>("[data-trim-edge]")?.dataset.trimEdge as "left" | "right" | undefined;
      if (blocked()) return;
      gesture.current = new TimelineGesture({ fence: fence(), sequence: props.sequence, selected: targets, primary: target,
        kind: edge ?? "move", pointerId: event.pointerId, clientX: event.clientX, scrollLeft: viewport.current!.scrollLeft, zoom, ripple });
      event.currentTarget.setPointerCapture(event.pointerId);
      return;
    }
    if (!event.target.closest("[data-timeline-background]")) return;
    if (event.target.closest("button,a")) return;
    event.preventDefault(); root.current?.focus({ preventScroll: true });
    if (props.gainDraftActive) return;
    const at = point(event.clientX, event.clientY);
    if (at.x < 0 || at.y < 0) return;
    rubber.current = { pointerId: event.pointerId, fence: fence(), ...at, original: structuredClone(props.selection),
      additive: event.shiftKey || event.ctrlKey || event.metaKey };
    movedRubber.current = false; clickConsumed.current = false;
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const update = (event: ReactPointerEvent<HTMLDivElement>) => {
    const g = gesture.current, r = rubber.current;
    if (!g && !r) return;
    if (event.pointerId !== (g?.start.pointerId ?? r?.pointerId)) return;
    const view = viewport.current!, bounds = view.getBoundingClientRect();
    // Auto-scroll changes the same content coordinate; it never changes a source clock.
    if (event.type === "pointermove" && event.clientX >= bounds.left && event.clientX <= bounds.right) {
      if (event.clientX > bounds.right - 24) view.scrollLeft += 12;
      else if (event.clientX < bounds.left + TRACK_LABEL_WIDTH + 24) view.scrollLeft = Math.max(0, view.scrollLeft - 12);
    }
    if (g) {
      const p = point(event.clientX, event.clientY);
      const sourceIndex = props.sequence.tracks.findIndex(track => track.id === g.start.primary.track_id);
      const inViewport = event.clientY >= bounds.top && event.clientY <= bounds.bottom &&
        event.clientX >= bounds.left + TRACK_LABEL_WIDTH && event.clientX <= bounds.right;
      const targetIndex = inViewport ? Math.floor(p.y / TRACK_HEIGHT) : -1;
      const next = g.update(event.clientX, view.scrollLeft, targetIndex < 0 ? -props.sequence.tracks.length - 1 : targetIndex - sourceIndex, fence());
      setDraft(next);
      if (!next) cancel(false);
    } else if (r) {
      if (!sameGestureFence(r.fence, fence())) { cancel(false); return; }
      const p = point(event.clientX, event.clientY);
      const next = { x: Math.max(0, Math.min(r.x, p.x)), y: Math.max(0, Math.min(r.y, p.y)), width: Math.abs(p.x - r.x), height: Math.abs(p.y - r.y) };
      if (next.width < 4 && next.height < 4 && !movedRubber.current) return;
      movedRubber.current = true; setBox(next);
      const targets = intersectingMedia(props.sequence, zoom, next);
      const combined = r.additive ? [...r.original, ...targets] : targets;
      props.onSelectMany(combined, targets.at(-1)?.element_id);
    }
  };
  const finish = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (gesture.current?.start.pointerId === event.pointerId) {
      const g = gesture.current;
      update(event);
      const result = g.finish(fence()); gesture.current = null; setDraft(null);
      clickConsumed.current = g.hasMoved;
      if (result?.changed) {
        clickConsumed.current = true;
        if (result.error) setError(result.error);
        else if (result.command) void review(result.command, g.start.kind === "move" ? "Move selection" : "Trim element", g.start.fence);
      }
    } else if (rubber.current?.pointerId === event.pointerId) {
      const r = rubber.current;
      if (!movedRubber.current && sameGestureFence(r.fence, fence())) props.onSelectMany(r.additive ? r.original : []);
      clickConsumed.current = movedRubber.current;
      rubber.current = null; movedRubber.current = false; setBox(null);
    }
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  };
  const changeZoom = (next: number, anchorX = (viewport.current?.clientWidth ?? 640) / 2 - TRACK_LABEL_WIDTH) => {
    cancel();
    const bounded = Math.min(8, Math.max(0.1, next));
    const view = viewport.current!;
    const scroll = anchoredScroll(view.scrollLeft, Math.max(0, anchorX), zoom, bounded);
    setZoom(bounded);
    requestAnimationFrame(() => { if (viewport.current) viewport.current.scrollLeft = scroll; });
  };
  const zoomSelection = () => {
    const chosen = props.sequence.tracks.flatMap(track => track.elements.filter(element => selected.has(keyOf({ track_id: track.id, element_id: element.id }))));
    if (!chosen.length) return;
    const left = Math.min(...chosen.map(element => secondsForDisplay(element.timeline_range.start)));
    const right = Math.max(...chosen.map(element => secondsForDisplay(addTime(element.timeline_range.start, element.timeline_range.duration))));
    const available = Math.max(64, (viewport.current?.clientWidth ?? 640) - TRACK_LABEL_WIDTH - 64);
    const next = Math.min(8, Math.max(0.1, available / Math.max(0.001, right - left) / pixelsPerSecond(1)));
    cancel(); setZoom(next);
    requestAnimationFrame(() => { if (viewport.current) viewport.current.scrollLeft = Math.max(0, left * pixelsPerSecond(next) - 24); });
  };
  const keyboard = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape" && (gesture.current || rubber.current)) { event.preventDefault(); event.stopPropagation(); cancel(); return; }
    if (event.repeat || event.metaKey || event.ctrlKey || event.altKey || ignoresTimelineShortcut(event.target) || blocked()) return;
    try {
      const command = shortcutCommand(event.key, props.sequence, props.selection, props.playhead, ripple);
      if (command) { event.preventDefault(); void review(command, command.kind === "split" ? "Split selection" : "Delete selection", fence()); }
    } catch (cause) { event.preventDefault(); setError(cause instanceof Error ? cause.message : String(cause)); }
  };
  const labelInterval = Math.max(0.1, 10 ** Math.ceil(Math.log10(100 / scale)));
  const ticks = Array.from({ length: Math.min(2000, Math.ceil((width / scale) / labelInterval)) }, (_, index) => index * labelInterval);
  // Authored clip bodies do not depend on the continuous consumer cursor.
  // Handlers use the latest view; selection, policy, scope and draft still invalidate.
  const trackRows = useMemo(() => props.sequence.tracks.map(track => <div key={track.id} className="v4-track" style={{ height: TRACK_HEIGHT, minHeight: TRACK_HEIGHT, gap: 0 }}>
            <button className="v4-track-name" style={{ width: TRACK_LABEL_WIDTH, flexBasis: TRACK_LABEL_WIDTH }} data-selected={props.selectedId === track.id} aria-pressed={props.selectedId === track.id}
              onClick={() => latest.current.onSelectOther(track.id)}>{track.name} · {track.kind}{track.locked ? " 🔒" : ""}</button>
            <div className="v4-track-lane" data-timeline-background data-file-track={track.id} style={{ width, flex: "none", height: TRACK_HEIGHT }}>
              {track.elements.map(element => {
                const target = { track_id: track.id, element_id: element.id };
                const move = draft?.command?.kind === "move_selection" ? draft.command.moves.find(item => keyOf(item.target) === keyOf(target)) : undefined;
                const trim = draft?.command?.kind === "trim" && draft.command.target.element_id === element.id ? draft.command : null;
                const startSeconds = secondsForDisplay(element.timeline_range.start), duration = secondsForDisplay(element.timeline_range.duration);
                const left = move ? secondsForDisplay(move.new_start) : trim?.edge === "left" ? secondsForDisplay(trim.boundary) : startSeconds;
                const right = trim?.edge === "right" ? secondsForDisplay(trim.boundary) : startSeconds + duration;
                return <div key={element.id} className="v4-timeline-element" data-selected={selected.has(keyOf(target)) || props.selectedId === element.id}
                  data-timeline-media={media(element) ? "true" : undefined} data-timeline-editable={editable(element) ? "true" : undefined} data-track-id={track.id} data-element-id={element.id}
                  style={{ left: left * scale, width: (move ? duration : Math.max(0, right - left)) * scale,
                    ...(move ? { transform: `translateY(${draft!.trackOffset * TRACK_HEIGHT}px)`, zIndex: 4 } : {}) }}>
                  <button className="v4-element-select" aria-pressed={selected.has(keyOf(target)) || props.selectedId === element.id}
                    title={`${element.name} · ${formatTime(element.timeline_range.start)}`} onClick={event => {
                    if (props.gainDraftActive || props.attempt?.phase === "applying") return;
                    if (editable(element)) { if (!event.detail) latest.current.onSelect(target, event.metaKey || event.ctrlKey ? "toggle" : event.shiftKey ? "extend" : "replace"); }
                    else latest.current.onSelectOther(element.id);
                    if (!event.shiftKey && !event.metaKey && !event.ctrlKey) latest.current.onSeek(element.timeline_range.start);
                    root.current?.focus({ preventScroll: true });
                  }}>{element.name} · {element.content.kind}</button>
                  {props.renderAudioControls(element)}
                  {props.sourceArt && media(element) && (() => {
                    const assetId = element.content.kind === "video" || element.content.kind === "audio" || element.content.kind === "image" ? element.content.asset_id : null;
                    const asset = props.sourceArt.assets.find(item => item.id === assetId);
                    return asset ? <TimelineArt api={props.sourceArt.api} scope={props.scope} trackId={track.id} element={element} asset={asset} scale={scale} gainDraft={props.sourceArt.gainDraft} /> : null;
                  })()}
                  {editable(element) && !props.disabled && !props.gainDraftActive && !track.locked && <>
                    <span className="v4-trim-handle" data-trim-edge="left" title={t("Trim left edge")} />
                    <span className="v4-trim-handle" data-trim-edge="right" title={t("Trim right edge")} />
                  </>}
                </div>;
              })}
              {props.sequence.captions.filter(caption => caption.track_id === track.id).map(caption => <button key={caption.id} className="v4-caption-marker"
                style={{ left: secondsForDisplay(caption.timeline_range.start) * scale, width: secondsForDisplay(caption.timeline_range.duration) * scale }}
                onClick={() => { latest.current.onSelectOther(caption.id); latest.current.onSeek(caption.timeline_range.start); }}>{caption.text}</button>)}
            </div>
          </div>), [props.sequence, props.selection, props.selectedId, props.disabled, props.gainDraftActive,
    props.attempt?.phase, props.scope.projectId, props.scope.revision, props.scope.sequenceId,
    props.sourceArt?.api, props.sourceArt?.assets, props.sourceArt?.gainDraft, props.renderAudioControls, scale, draft, language, theme]);
  return <div className="v4-timeline-panel" ref={root} tabIndex={0} aria-label={t("Timeline")} onKeyDown={keyboard}
    onBlur={event => { if (gesture.current || rubber.current || !event.currentTarget.contains(event.relatedTarget as Node | null)) cancel(); }}>
    <TimelineEditControls controller={props.controller} sequence={props.sequence} scope={props.scope} playhead={props.playhead}
      selection={props.selection} attempt={props.attempt} selectedTrackId={props.selectedId} disabled={props.disabled || props.gainDraftActive} onReview={props.onReview} />
    <div className="toolbar v4-timeline-view-controls">
      <Button onClick={() => changeZoom(zoom / 1.5)}>{t("Zoom out")}</Button>
      <label>{t("Timeline zoom")} <input type="range" min="0.1" max="8" step="0.1" value={zoom} onChange={event => changeZoom(Number(event.target.value))} /></label>
      <Button onClick={() => changeZoom(zoom * 1.5)}>{t("Zoom in")}</Button>
      <Button disabled={!props.selection.length} onClick={zoomSelection}>{t("Zoom to selection")}</Button>
      <label><input type="checkbox" checked={ripple} onChange={event => setRipple(event.target.checked)} />{t("Ripple affected tracks")}</label>
      <span className="muted">{t("S: split · Q: keep right · W: keep left · Escape: cancel drag")}</span>
    </div>
    {(error || draft?.changed) && <div className="v4-timeline-feedback">
      {(error || draft?.error) ? <span role="alert">{t(draft?.error ?? error!)}</span>
        : <span role="status">{t("Release to review. Escape cancels without saving.")}</span>}
    </div>}
    <div className="v4-timeline-viewport" ref={viewport} onPointerDown={start} onPointerMove={update} onPointerUp={finish}
      onDragOver={event => {
        if (!event.dataTransfer.types.includes("Files")) return;
        event.preventDefault();
        event.dataTransfer.dropEffect = blocked() || !props.onFiles ? "none" : "copy";
      }}
      onDrop={event => {
        if (!event.dataTransfer.files.length) return;
        event.preventDefault(); event.stopPropagation();
        if (blocked() || !props.onFiles || gesture.current || rubber.current) return;
        const lane = event.target instanceof Element ? event.target.closest<HTMLElement>("[data-file-track]") : null;
        const track = props.sequence.tracks.find(item => item.id === lane?.dataset.fileTrack);
        if (!lane || !track) return;
        if (track.locked || !["video", "audio", "graphic"].includes(track.kind)) {
          setError("Choose an unlocked video, audio or graphic track for these files."); return;
        }
        const captured = fence(), at = pointerTime((event.clientX - lane.getBoundingClientRect().left) / scale);
        const files = Array.from(event.dataTransfer.files);
        setError(null);
        void props.onFiles(files, { scope: { ...props.scope }, trackId: track.id, at }).catch(cause => {
          if (sameGestureFence(captured, fence())) setError(cause instanceof Error ? cause.message : String(cause));
        });
      }}
      onPointerDownCapture={() => { if (!gesture.current && !rubber.current) clickConsumed.current = false; }}
      onPointerCancel={() => cancel()} onLostPointerCapture={() => { if (gesture.current || rubber.current) cancel(); }}
      onClickCapture={event => { if (clickConsumed.current && event.detail > 0) { clickConsumed.current = false; event.preventDefault(); event.stopPropagation(); } }}
      onWheel={event => { if (event.ctrlKey || event.metaKey) { event.preventDefault(); changeZoom(zoom * (event.deltaY < 0 ? 1.15 : 1 / 1.15), event.clientX - event.currentTarget.getBoundingClientRect().left - TRACK_LABEL_WIDTH); } }}>
      <div className="v4-timeline-content" style={{ width: width + TRACK_LABEL_WIDTH }}>
        <div className="v4-timeline-ruler" style={{ height: RULER_HEIGHT, marginLeft: TRACK_LABEL_WIDTH }} role="slider" tabIndex={0}
          aria-label={t("Scene playhead")} aria-valuemin={0} aria-valuemax={end} aria-valuenow={secondsForDisplay(props.playhead)}
          onPointerDown={event => { event.stopPropagation(); if (event.button !== 0 || props.gainDraftActive) return;
            const rect = event.currentTarget.getBoundingClientRect(); props.onSeek(pointerTime((event.clientX - rect.left) / scale)); }}
          onKeyDown={event => { if (event.key === "ArrowLeft" || event.key === "ArrowRight") { event.preventDefault(); event.stopPropagation();
            props.onSeek(pointerTime(secondsForDisplay(props.playhead) + (event.key === "ArrowRight" ? 1 : -1) / scale)); } }}>
          {ticks.map(seconds => <span key={seconds} className="v4-ruler-tick" style={{ left: seconds * scale }}>{formatTime(pointerTime(seconds))}</span>)}
        </div>
        <div className="v4-marker-lane" style={{ height: MARKER_HEIGHT, marginLeft: TRACK_LABEL_WIDTH }}>
          {props.sequence.bookmarks.map(bookmark => <button key={bookmark.id} className="v4-marker" data-selected={props.selectedId === bookmark.id}
            title={bookmark.note ?? formatTime(bookmark.time)} aria-pressed={props.selectedId === bookmark.id}
            aria-label={t("Bookmark at {time}", { time: formatTime(bookmark.time) }) + (bookmark.note ? ": " + bookmark.note : "")}
            style={{ left: secondsForDisplay(bookmark.time) * scale }} onClick={() => { props.onSelectOther(bookmark.id); props.onSeek(bookmark.time); }}>◆</button>)}
        </div>
        <div className="v4-timeline-tracks" ref={tracks} style={{ position: "relative" }}>
          {trackRows}
          {box && <div className="v4-selection-box" style={{ left: TRACK_LABEL_WIDTH + box.x, top: box.y, width: box.width, height: box.height }} />}
          <div className="v4-playhead" style={{ left: TRACK_LABEL_WIDTH + secondsForDisplay(props.playhead) * scale, height: props.sequence.tracks.length * TRACK_HEIGHT }} />
        </div>
      </div>
    </div>
    <details className="v4-timeline-item-list"><summary>{t("Select timeline items")}</summary>
      {props.sequence.tracks.flatMap(track => track.elements.map(element => <button key={element.id} aria-pressed={selected.has(keyOf({ track_id: track.id, element_id: element.id }))}
        onClick={event => { if (editable(element)) props.onSelect({ track_id: track.id, element_id: element.id }, event.metaKey || event.ctrlKey ? "toggle" : event.shiftKey ? "extend" : "replace");
          else props.onSelectOther(element.id); root.current?.focus({ preventScroll: true }); }}>{track.name} · {element.name} · {formatTime(element.timeline_range.start)}</button>))}
    </details>
  </div>;
}
