import { useResourceDropTarget } from "../workbench/resource-drop-target";
import { useEffect, useLayoutEffect, useMemo, useRef, useState,useId } from "react";
import type {
  ReactNode,
  KeyboardEvent as ReactKeyboardEvent,
  PointerEvent as ReactPointerEvent,
} from "react";
import { useShortcutActions } from "../../../shortcuts";
import { Button } from "../../../shared/ui";
import { t, useLocale, mediaKindLabel } from "../../../shared/locale";
import { readTimelinePreferences, saveTimelinePreferences, flushTimelinePreferences } from "../../../shared/view-preferences";
import { addTime,subtractTime,compareTime, formatTime, secondsForDisplay } from "../../../model/time";
import type { Time } from "../../../model/types";
import type { Asset, Element, Sequence, GainDraft } from "../types";
import type { EditorApi } from "../../api";
import { TimelineArt } from "../timeline-art";
import type { TimelineEdit } from "./controller";
import type {
  TimelineAttempt,
  TimelineCommand,
  TimelineScope,
  TimelineTarget,
} from "./types";
import { TimelineEditControls } from "./controls";
import { shortcutCommand } from "./actions";
import { TimelineGesture, sameGestureFence } from "./gesture";
import type { GestureDraft, GestureFence } from "./gesture";
import {
  anchoredScroll,
  clampTimelineZoom,
  timelineZoomAt,
  timelineZoomPosition,
  timelineZoomAnchor,
  intersectingMedia,
  MARKER_HEIGHT,
  pixelsPerSecond,
  pointerTime,
  RULER_HEIGHT,
  sequenceEnd,
  TRACK_HEIGHT,
  TRACK_LABEL_WIDTH,
} from "./geometry";
import type { TimelineRect } from "./geometry";
import type { MediaFileTarget } from "../media";
import {useTimelineScrub} from "./scrub";
import {TimelineScrollNavigator} from "./scroll-navigator";
import { TransitionSeams } from "../workbench/transition-seams";
import { useResourceCopy } from "../workbench/resources-copy";
import { Focus, Magnet, Minus, Plus, MoveHorizontal, AudioLines, Film, Type, Shapes, LockKeyhole, UnlockKeyhole, Eye, EyeOff, Volume2, VolumeX, ScanLine, Rows3 } from "lucide-react";

export type TimelinePanelProps = {
  toolbar?: ReactNode;
  controller: TimelineEdit;
  sequence: Sequence;
  scope: TimelineScope;
  selection: TimelineTarget[];
  selectedId: string | null;
  playhead: Time;
  presentation?:{time():Time;subscribe(listener:()=>void):()=>void};
  attempt?: TimelineAttempt | null;
  disabled: boolean;
  gainDraftActive: boolean;
  onSelect(target: TimelineTarget, mode: "replace" | "toggle" | "extend"): void;
  onSelectMany(targets: TimelineTarget[], primaryId?: string): void;
  onSelectOther(id: string): void;
  onSeek(time: Time): void;
  onScrubStart?():void;
  frameStep?:Time;
  onReview(): void;
  onFiles?(files: File[], target: MediaFileTarget): Promise<void>;
  renderAudioControls(element: Element): ReactNode;
  sourceArt?: {
    api: EditorApi;
    assets: Asset[];
    gainDraft?: GainDraft | null;
  };
};
type Rubber = {
  pointerId: number;
  fence: GestureFence;
  x: number;
  y: number;
  original: TimelineTarget[];
  additive: boolean;
};
const media = (element: Element) =>
  ["video", "audio", "image"].includes(element.content.kind);
const editable = (_element: Element) => true;
const keyOf = (target: TimelineTarget) =>
  `${target.track_id}\u0000${target.element_id}`;

/** One Session owns the project, selection, playhead and transactions. Local
 * state here is only a view (zoom/scroll) or an uncommitted pointer intention. */
export function TimelinePanel(props: TimelinePanelProps) {
  const { language, theme } = useLocale();
  const c = useResourceCopy();
  const latest = useRef(props);
  const viewportId=useId();
  latest.current = props;
  const root = useRef<HTMLDivElement>(null),
    viewport = useRef<HTMLDivElement>(null),
    tracks = useRef<HTMLDivElement>(null);
  const playheadLine=useRef<HTMLDivElement>(null),ruler=useRef<HTMLDivElement>(null);
  const gesture = useRef<TimelineGesture | null>(null),
    rubber = useRef<Rubber | null>(null);
  const clickConsumed = useRef(false),
    movedRubber = useRef(false);
  const [zoom, setZoom] = useState(1),
    [ripple, setRipple] = useState(() => readTimelinePreferences(props.scope.projectId, props.scope.sequenceId).ripple),
    [snapping, setSnapping] = useState(true);
  const zoomValue = useRef(1), pendingScroll = useRef<number | null>(null);
  useLayoutEffect(() => {
    if (viewport.current && pendingScroll.current !== null) {
      viewport.current.scrollLeft = pendingScroll.current;
      pendingScroll.current = null;
    }
  }, [zoom]);
  const [compact, setCompact] = useState(true);
  const trackHeight = compact ? 56 : TRACK_HEIGHT;
  const markerHeight = props.sequence.bookmarks.length ? MARKER_HEIGHT : 0;
  const [draft, setDraft] = useState<GestureDraft | null>(null),
    [box, setBox] = useState<TimelineRect | null>(null);
  const [error, setError] = useState<string | null>(null);
  const scale = pixelsPerSecond(zoom),
    end = sequenceEnd(props.sequence);
  const resourceDrop = useResourceDropTarget(viewport, props.sequence, scale, snapping,
    props.disabled || props.gainDraftActive || !!props.attempt || !!draft);
  const resourceTarget = resourceDrop.state?.target;
  const resourceTrack = resourceTarget?.trackId
    ? props.sequence.tracks.find(track => track.id === resourceTarget.trackId)?.name ?? resourceTarget.name
    : c("dropNewTrack");
  const resourceRange = resourceTarget
    ? `${c("dropStart")} ${formatTime(resourceTarget.at)} · ${resourceTarget.duration
      ? `${c("dropEnd")} ${formatTime(addTime(resourceTarget.at, resourceTarget.duration))} · ${c("dropDuration")} ${formatTime(resourceTarget.duration)}`
      : c("dropDurationPending")}` : "";
  const resourceAction = resourceTarget?.kind === "effect-clip" ? `${c("dropAttach")} · ${resourceTarget.name}`
    : resourceTarget?.kind === "transition" ? `${c(resourceTarget.replace ? "dropReplace" : "dropTransition")} · ${resourceTarget.name}` : "";
  const currentIdentity = props.controller.identity();
  const seekFrame=(direction:number)=>{
    props.onScrubStart?.();
    const current=props.presentation?.time()??props.playhead;
    if(!props.frameStep){props.onSeek(pointerTime(Math.max(0,Math.min(end,secondsForDisplay(current)+direction/scale))));return;}
    const next=direction>0?addTime(current,props.frameStep):subtractTime(current,props.frameStep),zero={ticks:"0",timebase:1},last=pointerTime(end);
    props.onSeek(compareTime(next,zero)<0?zero:compareTime(next,last)>0?last:next);
  };
  const scrub=useTimelineScrub({ruler,line:playheadLine,scale,end,blocked:props.gainDraftActive||!!props.attempt,
    seek:time=>latest.current.onSeek(time),begin:()=>latest.current.onScrubStart?.()});
  useLayoutEffect(()=>{
    let lastAccessibleAt=-Infinity;
    const draw=()=>{
      if(scrub.active())return;
      const at=props.presentation?.time()??props.playhead,seconds=secondsForDisplay(at);
      if(playheadLine.current)playheadLine.current.style.transform=`translateX(${seconds*scale}px)`;
      const now=performance.now();
      if(now-lastAccessibleAt>=100){lastAccessibleAt=now;ruler.current?.setAttribute("aria-valuenow",String(seconds));}
    };
    draw();return props.presentation?.subscribe(draw);
  },[props.presentation,props.playhead,scale]);
  const width = Math.max(640, end * scale + 160),
    selected = new Set(props.selection.map(keyOf));
  const fence = (): GestureFence => ({
    ...latest.current.scope,
    ...latest.current.controller.identity(),
  });
  const blocked = () =>
    latest.current.disabled ||
    latest.current.gainDraftActive ||
    !!latest.current.attempt;
  const cancel = (restore = true) => {
    const current = rubber.current;
    if (restore && current && sameGestureFence(current.fence, fence()))
      latest.current.onSelectMany(current.original);
    if (gesture.current?.hasMoved || movedRubber.current)
      clickConsumed.current = true;
    gesture.current?.cancel();
    gesture.current = null;
    rubber.current = null;
    movedRubber.current = false;
    setDraft(null);
    setBox(null);
  };
  const cancelRef = useRef(cancel);
  cancelRef.current = cancel;
  useEffect(() => {
    cancelRef.current(false);
    setError(null);
  }, [
    props.scope.projectId,
    props.scope.revision,
    props.scope.sequenceId,
    props.controller,
    props.disabled,
    props.gainDraftActive,
    currentIdentity.api,
    currentIdentity.generation,
    currentIdentity.viewGeneration,
  ]);
  useEffect(() => {
    const blur = () => cancelRef.current();
    window.addEventListener("blur", blur);
    return () => {
      window.removeEventListener("blur", blur);
      gesture.current?.cancel();
    };
  }, []);
  const review = async (
    command: TimelineCommand,
    label: string,
    captured: GestureFence,
  ) => {
    if (!sameGestureFence(captured, fence()) || blocked()) return;
    setError(null);
    try {
      const needsReview = await latest.current.controller.perform(command, label, captured);
      if (needsReview && sameGestureFence(captured, fence())) latest.current.onReview();
    } catch (cause) {
      if (sameGestureFence(captured, fence()))
        setError(cause instanceof Error ? cause.message : String(cause));
    }
  };
  const point = (clientX: number, clientY: number) => {
    const trackRect = tracks.current!.getBoundingClientRect();
    return {
      x: clientX - trackRect.left - TRACK_LABEL_WIDTH,
      y: clientY - trackRect.top,
    };
  };
  const start = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || !(event.target instanceof Element)) return;
    if (gesture.current || rubber.current) return;
    if (
      event.target.closest(
        ".editor-audio-controls,[role=slider],input,select,textarea,[data-timeline-no-gesture]",
      )
    )
      return;
    if (props.gainDraftActive || props.attempt?.phase === "applying") return;
    const clip = event.target.closest<HTMLElement>("[data-timeline-editable]");
    if (clip) {
      const target = {
        track_id: clip.dataset.trackId!,
        element_id: clip.dataset.elementId!,
      };
      const modifier =
        event.metaKey || event.ctrlKey
          ? "toggle"
          : event.shiftKey
            ? "extend"
            : null;
      event.preventDefault();
      root.current?.focus({ preventScroll: true });
      clickConsumed.current = false;
      if (modifier) {
        props.onSelect(target, modifier);
        clickConsumed.current = true;
        return;
      }
      const already = props.selection.some(
        (item) => keyOf(item) === keyOf(target),
      );
      const targets = already ? props.selection : [target];
      if (!already) props.onSelect(target, "replace");
      const edge = event.target.closest<HTMLElement>("[data-trim-edge]")
        ?.dataset.trimEdge as "left" | "right" | undefined;
      if (blocked()) return;
      gesture.current = new TimelineGesture({
        fence: fence(),
        sequence: props.sequence,
        selected: targets,
        primary: target,
        kind: edge ?? "move",
        pointerId: event.pointerId,
        clientX: event.clientX,
        scrollLeft: viewport.current!.scrollLeft,
        zoom,
        ripple,
        snapping,
      });
      event.currentTarget.setPointerCapture(event.pointerId);
      return;
    }
    if (!event.target.closest("[data-timeline-background]")) return;
    if (event.target.closest("button,a")) return;
    event.preventDefault();
    root.current?.focus({ preventScroll: true });
    if (props.gainDraftActive) return;
    const at = point(event.clientX, event.clientY);
    if (at.x < 0 || at.y < 0) return;
    rubber.current = {
      pointerId: event.pointerId,
      fence: fence(),
      ...at,
      original: structuredClone(props.selection),
      additive: event.shiftKey || event.ctrlKey || event.metaKey,
    };
    movedRubber.current = false;
    clickConsumed.current = false;
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const update = (event: ReactPointerEvent<HTMLDivElement>) => {
    const g = gesture.current,
      r = rubber.current;
    if (!g && !r) return;
    if (event.pointerId !== (g?.start.pointerId ?? r?.pointerId)) return;
    const view = viewport.current!,
      bounds = view.getBoundingClientRect();
    // Auto-scroll changes the same content coordinate; it never changes a source clock.
    if (
      event.type === "pointermove" &&
      event.clientX >= bounds.left &&
      event.clientX <= bounds.right
    ) {
      if (event.clientX > bounds.right - 24) view.scrollLeft += 12;
      else if (event.clientX < bounds.left + TRACK_LABEL_WIDTH + 24)
        view.scrollLeft = Math.max(0, view.scrollLeft - 12);
    }
    if (g) {
      const p = point(event.clientX, event.clientY);
      const sourceIndex = props.sequence.tracks.findIndex(
        (track) => track.id === g.start.primary.track_id,
      );
      const inViewport =
        event.clientY >= bounds.top + RULER_HEIGHT + markerHeight &&
        event.clientY <= bounds.bottom &&
        event.clientX >= bounds.left + TRACK_LABEL_WIDTH &&
        event.clientX <= bounds.right;
      const targetIndex = inViewport ? Math.floor(p.y / trackHeight) : -1;
      const next = g.update(
        event.clientX,
        view.scrollLeft,
        targetIndex < 0
          ? -props.sequence.tracks.length - 1
          : targetIndex - sourceIndex,
        fence(),
      );
      setDraft(next);
      if (!next) cancel(false);
    } else if (r) {
      if (!sameGestureFence(r.fence, fence())) {
        cancel(false);
        return;
      }
      const p = point(event.clientX, event.clientY);
      const next = {
        x: Math.max(0, Math.min(r.x, p.x)),
        y: Math.max(0, Math.min(r.y, p.y)),
        width: Math.abs(p.x - r.x),
        height: Math.abs(p.y - r.y),
      };
      if (next.width < 4 && next.height < 4 && !movedRubber.current) return;
      movedRubber.current = true;
      setBox(next);
      const targets = intersectingMedia(props.sequence, zoom, next, trackHeight);
      const combined = r.additive ? [...r.original, ...targets] : targets;
      props.onSelectMany(combined, targets.at(-1)?.element_id);
    }
  };
  const finish = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (gesture.current?.start.pointerId === event.pointerId) {
      const g = gesture.current;
      update(event);
      const result = g.finish(fence());
      gesture.current = null;
      setDraft(null);
      clickConsumed.current = g.hasMoved;
      if (result?.changed) {
        clickConsumed.current = true;
        if (result.error) setError(result.error);
        else if (result.command)
          void review(
            result.command,
            g.start.kind === "move" ? "Move selection" : "Trim element",
            g.start.fence,
          );
      }
    } else if (rubber.current?.pointerId === event.pointerId) {
      const r = rubber.current;
      if (!movedRubber.current && sameGestureFence(r.fence, fence()))
        props.onSelectMany(r.additive ? r.original : []);
      clickConsumed.current = movedRubber.current;
      rubber.current = null;
      movedRubber.current = false;
      setBox(null);
    }
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
  };
  const changeZoom = (
    next: number,
    anchorX?: number,
  ) => {
    cancel();
    resourceDrop.state?.cancel();
    const view = viewport.current;
    if (!view) return;
    const bounded = clampTimelineZoom(next);
    if (bounded === zoomValue.current) return;
    const left = pendingScroll.current ?? view.scrollLeft;
    const scroll = anchoredScroll(
      left,
      Math.max(0, anchorX ?? timelineZoomAnchor(left, view.clientWidth, zoomValue.current,
        latest.current.presentation?.time() ?? latest.current.playhead)),
      zoomValue.current,
      bounded,
    );
    zoomValue.current = bounded;
    pendingScroll.current = scroll;
    setZoom(bounded);
  };
  const wheelZoom = useRef(changeZoom);
  wheelZoom.current = changeZoom;
  useEffect(() => {
    const view = viewport.current;
    if (!view) return;
    const wheel = (event: WheelEvent) => {
      if (!(event.ctrlKey || event.metaKey) || !event.deltaY) return;
      event.preventDefault();
      const delta = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? view.clientHeight : 1);
      wheelZoom.current(zoomValue.current * Math.exp(Math.max(-2, Math.min(2, -delta * 0.002))),
        event.clientX - view.getBoundingClientRect().left - TRACK_LABEL_WIDTH);
    };
    view.addEventListener("wheel", wheel, { passive: false });
    return () => view.removeEventListener("wheel", wheel);
  }, []);
  const changeRipple = (value: boolean) => {
    cancel();
    setRipple(value);
    saveTimelinePreferences(props.scope.projectId, props.scope.sequenceId, { ripple: value });
    flushTimelinePreferences();
  };
  const zoomSelection = () => {
    const chosen = [
      ...props.sequence.tracks.flatMap((track) =>
        track.elements.filter((element) =>
          selected.has(keyOf({ track_id: track.id, element_id: element.id })),
        ),
      ),
      ...props.sequence.captions.filter((c) =>
        selected.has(keyOf({ track_id: c.track_id, element_id: c.id })),
      ),
    ];
    if (!chosen.length) return;
    const left = Math.min(
      ...chosen.map((element) =>
        secondsForDisplay(element.timeline_range.start),
      ),
    );
    const right = Math.max(
      ...chosen.map((element) =>
        secondsForDisplay(
          addTime(
            element.timeline_range.start,
            element.timeline_range.duration,
          ),
        ),
      ),
    );
    const available = Math.max(
      64,
      (viewport.current?.clientWidth ?? 640) - TRACK_LABEL_WIDTH - 64,
    );
    const next = Math.min(
      8,
      Math.max(
        0.1,
        available / Math.max(0.001, right - left) / pixelsPerSecond(1),
      ),
    );
    cancel();
    resourceDrop.state?.cancel();
    zoomValue.current = next;
    pendingScroll.current = Math.max(0, left * pixelsPerSecond(next) - 24);
    setZoom(next);
    if (next === zoom && viewport.current) {
      viewport.current.scrollLeft = pendingScroll.current;
      pendingScroll.current = null;
    }
  };
  const zoomTimeline = () => {
    cancel();
    resourceDrop.state?.cancel();
    const available = Math.max(64, (viewport.current?.clientWidth ?? 640) - TRACK_LABEL_WIDTH - 64);
    const next = clampTimelineZoom(available / end / pixelsPerSecond(1));
    zoomValue.current = next;
    pendingScroll.current = 0;
    setZoom(next);
    if (next === zoom && viewport.current) {
      viewport.current.scrollLeft = 0;
      pendingScroll.current = null;
    }
  };
  const shortcut = (key: string) => {
    if (blocked()) return;
    try {
      const command = shortcutCommand(
        key,
        latest.current.sequence,
        latest.current.selection,
        latest.current.presentation?.time()??latest.current.playhead,
        ripple,
      );
      if (command)
        void review(
          command,
          command.kind === "split" ? "Split selection" : "Delete selection",
          fence(),
        );
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };
  useShortcutActions({
    split: {
      run: () => shortcut("s"),
      enabled: !blocked() && !!props.selection.length,
    },
    "split-left": {
      run: () => shortcut("q"),
      enabled: !blocked() && !!props.selection.length,
    },
    "split-right": {
      run: () => shortcut("w"),
      enabled: !blocked() && !!props.selection.length,
    },
    "delete-selected": {
      run: () => shortcut("delete"),
      enabled: !blocked() && !!props.selection.length,
    },
    "toggle-ripple-editing": {
      run: () => changeRipple(!ripple),
      enabled: !blocked(),
      checked: ripple,
    },
    "toggle-snapping": {
      run: () => {
        cancel();
        setSnapping((v) => !v);
      },
      enabled: !blocked(),
      checked: snapping,
    },
    "zoom-in": {
      run: () => changeZoom(zoom * 1.5),
      enabled: !props.gainDraftActive,
    },
    "zoom-out": {
      run: () => changeZoom(zoom / 1.5),
      enabled: !props.gainDraftActive,
    },
    "zoom-selection": { run: zoomSelection, enabled: !!props.selection.length },
    // An inactive local owner must leave global deselection available. While
    // dragging, the local handler preserves the original selected group.
    "cancel-interaction": gesture.current || rubber.current
      ? { run: () => cancel() }
      : undefined,
  });
  const keyboard = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape" && (gesture.current || rubber.current)) {
      event.preventDefault();
      event.stopPropagation();
      cancel();
    }
  };
  const labelInterval = Math.max(0.1, 10 ** Math.ceil(Math.log10(100 / scale)));
  const ticks = Array.from(
    { length: Math.min(2000, Math.ceil(width / scale / labelInterval)) },
    (_, index) => index * labelInterval,
  );
  // Authored clip bodies do not depend on the continuous consumer cursor.
  // Handlers use the latest view; selection, policy, scope and draft still invalidate.
  const trackRows = useMemo(
    () =>
      props.sequence.tracks.map((track) => (
        <div
          key={track.id}
          className="editor-track"
          style={{ height: trackHeight, minHeight: trackHeight, gap: 0 }}
          data-enabled={track.enabled}
        >
          <div className="editor-track-header" style={{ width: TRACK_LABEL_WIDTH, flexBasis: TRACK_LABEL_WIDTH }} data-selected={props.selectedId === track.id}>
          <button
            className="editor-track-name"
            aria-label={`${track.name} · ${mediaKindLabel(track.kind)}`}
            aria-description={track.locked ? t("Locked") : undefined}
            data-selected={props.selectedId === track.id}
            aria-pressed={props.selectedId === track.id}
            onClick={() => latest.current.onSelectOther(track.id)}
          >
            <span className="workbench-track-icon">{track.kind === "audio" ? <AudioLines size={15} /> : track.kind === "video" ? <Film size={15} /> : track.kind === "text" || track.kind === "caption" ? <Type size={15} /> : <Shapes size={15} />}</span>
            <span className="workbench-track-label">{track.name}<span className="workbench-track-kind"> · {mediaKindLabel(track.kind)}</span></span>
            {track.locked ? <LockKeyhole size={12} aria-label={t("Locked")} /> : null}
          </button>
          <div className="editor-track-actions" onPointerDown={(event) => event.stopPropagation()}>
            <button type="button" title={t(track.locked ? "Unlock track" : "Lock track")} aria-label={`${t(track.locked ? "Unlock track" : "Lock track")} · ${track.name}`} aria-pressed={track.locked} disabled={props.disabled || props.gainDraftActive || !!props.attempt} onClick={() => void review({ kind: "set_track_locked", track_id: track.id, locked: !track.locked }, track.locked ? "Unlock track" : "Lock track", fence())}>
              {track.locked ? <LockKeyhole size={12} /> : <UnlockKeyhole size={12} />}
            </button>
            <button type="button" title={t(track.enabled ? "Disable track" : "Enable track")} aria-label={`${t(track.enabled ? "Disable track" : "Enable track")} · ${track.name}`} aria-pressed={!track.enabled} disabled={props.disabled || props.gainDraftActive || !!props.attempt || track.locked} onClick={() => void review({ kind: "set_track_enabled", track_id: track.id, enabled: !track.enabled }, track.enabled ? "Disable track" : "Enable track", fence())}>
              {track.enabled ? <Eye size={12} /> : <EyeOff size={12} />}
            </button>
            {["video", "audio"].includes(track.kind) && <button type="button" title={t(track.muted ? "Unmute track" : "Mute track")} aria-label={`${t(track.muted ? "Unmute track" : "Mute track")} · ${track.name}`} aria-pressed={track.muted} disabled={props.disabled || props.gainDraftActive || !!props.attempt || track.locked} onClick={() => void review({ kind: "set_track_muted", track_id: track.id, muted: !track.muted }, track.muted ? "Unmute track" : "Mute track", fence())}>
              {track.muted ? <VolumeX size={12} /> : <Volume2 size={12} />}
            </button>}
          </div>
          </div>
          <div
            className="editor-track-lane"
            data-timeline-background
            data-file-track={track.id}
            style={{ width, flex: "none", height: trackHeight }}
          >
            {track.elements.map((element) => {
              const target = { track_id: track.id, element_id: element.id };
              const move =
                draft?.command?.kind === "move_selection"
                  ? draft.command.moves.find(
                      (item) => keyOf(item.target) === keyOf(target),
                    )
                  : undefined;
              const trim =
                draft?.command?.kind === "trim" &&
                draft.command.target.element_id === element.id
                  ? draft.command
                  : null;
              const startSeconds = secondsForDisplay(
                  element.timeline_range.start,
                ),
                duration = secondsForDisplay(element.timeline_range.duration);
              const left = move
                ? secondsForDisplay(move.new_start)
                : trim?.edge === "left"
                  ? secondsForDisplay(trim.boundary)
                  : startSeconds;
              const right =
                trim?.edge === "right"
                  ? secondsForDisplay(trim.boundary)
                  : startSeconds + duration;
              return (
                <div
                  key={element.id}
                  className="editor-timeline-element"
                  data-kind={element.content.kind}
                  data-selected={
                    selected.has(keyOf(target)) ||
                    props.selectedId === element.id
                  }
                  data-timeline-media={media(element) ? "true" : undefined}
                  data-timeline-editable={
                    editable(element) ? "true" : undefined
                  }
                  data-track-id={track.id}
                  data-element-id={element.id}
                  style={{
                    left: left * scale,
                    width:
                      (move ? duration : Math.max(0, right - left)) * scale,
                    ...(move
                      ? {
                          transform: `translateY(${draft!.trackOffset * trackHeight}px)`,
                          zIndex: 4,
                        }
                      : {}),
                  }}
                >
                  <button
                    className="editor-element-select"
                    aria-pressed={
                      selected.has(keyOf(target)) ||
                      props.selectedId === element.id
                    }
                    title={`${element.name} · ${formatTime(element.timeline_range.start)}`}
                    onClick={(event) => {
                      if (
                        props.gainDraftActive ||
                        props.attempt?.phase === "applying"
                      )
                        return;
                      if (editable(element)) {
                        if (!event.detail)
                          latest.current.onSelect(
                            target,
                            event.metaKey || event.ctrlKey
                              ? "toggle"
                              : event.shiftKey
                                ? "extend"
                                : "replace",
                          );
                      } else latest.current.onSelectOther(element.id);
                      if (!event.shiftKey && !event.metaKey && !event.ctrlKey)
                        latest.current.onSeek(element.timeline_range.start);
                      root.current?.focus({ preventScroll: true });
                    }}
                  >
                    {element.name} · {mediaKindLabel(element.content.kind)}
                  </button>
                  {props.renderAudioControls(element)}
                  {props.sourceArt &&
                    media(element) &&
                    (() => {
                      const assetId =
                        element.content.kind === "video" ||
                        element.content.kind === "audio" ||
                        element.content.kind === "image"
                          ? element.content.asset_id
                          : null;
                      const asset = props.sourceArt.assets.find(
                        (item) => item.id === assetId,
                      );
                      return asset ? (
                        <TimelineArt
                          api={props.sourceArt.api}
                          scope={props.scope}
                          trackId={track.id}
                          element={element}
                          asset={asset}
                          scale={scale}
                          gainDraft={props.sourceArt.gainDraft}
                        />
                      ) : null;
                    })()}
                  {editable(element) &&
                    !props.disabled &&
                    !props.gainDraftActive &&
                    !track.locked && (
                      <>
                        <span
                          className="editor-trim-handle"
                          data-trim-edge="left"
                          title={t("Trim left edge")}
                        />
                        <span
                          className="editor-trim-handle"
                          data-trim-edge="right"
                          title={t("Trim right edge")}
                        />
                      </>
                    )}
                </div>
              );
            })}
            {props.sequence.captions
              .filter((caption) => caption.track_id === track.id)
              .map((caption) => (
                <div
                  key={caption.id}
                  className="editor-caption-marker"
                  data-timeline-editable="true"
                  data-track-id={track.id}
                  data-element-id={caption.id}
                  data-selected={selected.has(
                    keyOf({ track_id: track.id, element_id: caption.id }),
                  )}
                  style={{
                    left:
                      secondsForDisplay(caption.timeline_range.start) * scale,
                    width:
                      secondsForDisplay(caption.timeline_range.duration) *
                      scale,
                  }}
                >
                  <button
                    className="editor-element-select"
                    onClick={(event) => {
                      if (!event.detail)
                        latest.current.onSelect(
                          { track_id: track.id, element_id: caption.id },
                          event.metaKey || event.ctrlKey
                            ? "toggle"
                            : event.shiftKey
                              ? "extend"
                              : "replace",
                        );
                      if (!event.shiftKey && !event.metaKey && !event.ctrlKey)
                        latest.current.onSeek(caption.timeline_range.start);
                    }}
                  >
                    {caption.text}
                  </button>
                  {!props.disabled && !track.locked && (
                    <>
                      <span
                        className="editor-trim-handle"
                        data-trim-edge="left"
                        title={t("Trim left edge")}
                      />
                      <span
                        className="editor-trim-handle"
                        data-trim-edge="right"
                        title={t("Trim right edge")}
                      />
                    </>
                  )}
                </div>
              ))}
            <TransitionSeams sequence={props.sequence} trackId={track.id} scale={scale} disabled={props.gainDraftActive || !!props.attempt || !!draft} />
          </div>
        </div>
      )),
    [
      props.sequence,
      props.selection,
      props.selectedId,
      props.disabled,
      props.gainDraftActive,
      props.attempt?.phase,
      props.scope.projectId,
      props.scope.revision,
      props.scope.sequenceId,
      props.sourceArt?.api,
      props.sourceArt?.assets,
      props.sourceArt?.gainDraft,
      props.renderAudioControls,
      scale,
      trackHeight,
      props.attempt,
      draft,
      language,
      theme,
    ],
  );
  return (
    <div
      className="editor-timeline-panel"
      data-density={compact ? "compact" : "comfortable"}
      ref={root}
      tabIndex={0}
      data-shortcut-scope="timeline"
      aria-label={t("Timeline")}
      onKeyDown={keyboard}
      onBlur={(event) => {
        if (
          gesture.current ||
          rubber.current ||
          !event.currentTarget.contains(event.relatedTarget as Node | null)
        )
          cancel();
      }}
    >
      <div className="editor-timeline-toolbar">
      {props.toolbar}
      <TimelineEditControls
        controller={props.controller}
        sequence={props.sequence}
        scope={props.scope}
        playhead={props.playhead}
        selection={props.selection}
        attempt={props.attempt}
        selectedTrackId={props.selectedId}
        disabled={props.disabled || props.gainDraftActive}
        onReview={props.onReview}
        ripple={ripple}
        onRippleChange={changeRipple}
      />
      <div className="toolbar editor-timeline-view-controls">
        <Button aria-label={t("Fit timeline")} title={t("Fit timeline")} icon={<ScanLine size={15} />} onClick={zoomTimeline} />
        <Button aria-label={t("Compact tracks")} title={t("Compact tracks")} aria-pressed={compact} icon={<Rows3 size={15} />} onClick={() => { cancel(); setCompact(!compact); }} />
        <Button aria-label={t("Zoom out")} title={t("Zoom out")} icon={<Minus size={14} />} onClick={() => changeZoom(zoom / 1.5)} />
        <label>
          <span className="workbench-action-label">{t("Timeline zoom")}</span>
          <input
            type="range"
            aria-label={t("Timeline zoom")}
            min="0"
            max="1"
            step="0.001"
            value={timelineZoomPosition(zoom)}
            aria-valuetext={`${Math.round(zoom * 100)}%`}
            onChange={(event) => changeZoom(timelineZoomAt(Number(event.target.value)))}
          />
        </label>
        <Button aria-label={t("Zoom in")} title={t("Zoom in")} icon={<Plus size={14} />} onClick={() => changeZoom(zoom * 1.5)} />
        <Button title={t("Zoom to selection")} disabled={!props.selection.length} onClick={zoomSelection} icon={<Focus size={15} />}>
          <span className="workbench-action-label">{t("Zoom to selection")}</span>
        </Button>
        <label className="workbench-timeline-toggle" title={t("Ripple affected tracks")}>
          <input
            type="checkbox"
            checked={ripple}
            aria-label={t("Ripple affected tracks")}
            onChange={(event) => changeRipple(event.target.checked)}
          />
          <MoveHorizontal size={16} /><span className="workbench-action-label">{t("Ripple affected tracks")}</span>
        </label>
        <label className="workbench-timeline-toggle" title={t("Snap to element edges")}>
          <input
            type="checkbox"
            checked={snapping}
            aria-label={t("Snap to element edges")}
            onChange={(event) => {
              cancel();
              setSnapping(event.target.checked);
            }}
          />
          <Magnet size={16} /><span className="workbench-action-label">{t("Snap to element edges")}</span>
        </label>
      </div>
      </div>
      {(error || draft?.changed) && (
        <div className="editor-timeline-feedback">
          {error || draft?.error ? (
            <span role="alert">{t(draft?.error ?? error!)}</span>
          ) : (
            <span role="status">
              {t("Release to review. Escape cancels without saving.")}
            </span>
          )}
        </div>
      )}
      <div className="timeline-viewport-layer">
      {(resourceDrop.state?.drag || resourceDrop.state?.pending || resourceDrop.state?.error) && <div className="resource-drop-feedback" role={resourceDrop.state?.error || resourceTarget?.reason ? "alert" : "status"}>
        {resourceTarget && <span className="resource-drop-location">
          <strong>{resourceDrop.state?.drag?.resource.name} · {resourceTrack}</strong>
          <span>{resourceRange}{resourceAction && ` · ${resourceAction}`}</span>
        </span>}
        <span>{t(resourceDrop.state?.error ?? resourceTarget?.reason ?? (resourceDrop.state?.pending ? "Adding resource…" : resourceTarget ? "Release to add here. Escape cancels." : "Drag to a compatible track or a cut between videos."))}</span>
      </div>}
      <div
        className="editor-timeline-viewport"
        id={viewportId}
        ref={viewport}
        onPointerDown={start}
        onPointerMove={update}
        onPointerUp={finish}
        onDragLeave={resourceDrop.leave}
        onDragOver={(event) => {
          if (resourceDrop.over(event)) return;
          if (!event.dataTransfer.types.includes("Files")) return;
          event.preventDefault();
          event.dataTransfer.dropEffect =
            blocked() || !props.onFiles ? "none" : "copy";
        }}
        onDrop={(event) => {
          if (resourceDrop.drop(event)) return;
          if (!event.dataTransfer.files.length) return;
          event.preventDefault();
          event.stopPropagation();
          if (blocked() || !props.onFiles || gesture.current || rubber.current)
            return;
          const lane =
            event.target instanceof Element
              ? event.target.closest<HTMLElement>("[data-file-track]")
              : null;
          const track = props.sequence.tracks.find(
            (item) => item.id === lane?.dataset.fileTrack,
          );
          if (!lane || !track) return;
          if (
            track.locked ||
            !["video", "audio", "graphic"].includes(track.kind)
          ) {
            setError(
              "Choose an unlocked video, audio or graphic track for these files.",
            );
            return;
          }
          const captured = fence(),
            at = pointerTime(
              (event.clientX - lane.getBoundingClientRect().left) / scale,
            );
          const files = Array.from(event.dataTransfer.files);
          setError(null);
          void props
            .onFiles(files, {
              scope: { ...props.scope },
              trackId: track.id,
              at,
            })
            .catch((cause) => {
              if (sameGestureFence(captured, fence()))
                setError(
                  cause instanceof Error ? cause.message : String(cause),
                );
            });
        }}
        onPointerDownCapture={() => {
          if (!gesture.current && !rubber.current)
            clickConsumed.current = false;
        }}
        onPointerCancel={() => cancel()}
        onLostPointerCapture={() => {
          if (gesture.current || rubber.current) cancel();
        }}
        onClickCapture={(event) => {
          if (clickConsumed.current && event.detail > 0) {
            clickConsumed.current = false;
            event.preventDefault();
            event.stopPropagation();
          }
        }}
      >
        <div
          className="editor-timeline-content"
          style={{ width: width + TRACK_LABEL_WIDTH }}
        >
          <div
            className="editor-timeline-ruler"
            style={{ height: RULER_HEIGHT, marginLeft: TRACK_LABEL_WIDTH }}
            role="slider"
            tabIndex={0}
            aria-label={t("Scene playhead")}
            aria-valuemin={0}
            aria-valuemax={end}
            aria-valuenow={secondsForDisplay(props.playhead)}
            ref={ruler}
            onPointerDown={(event) => {
              scrub.start(event,props.presentation?.time()??props.playhead);
            }}
            onPointerMove={scrub.move}
            onPointerUp={scrub.end}
            onPointerCancel={scrub.cancel} onLostPointerCapture={scrub.cancel}
            onKeyDown={(event) => {
              if(event.key==="Escape"){event.preventDefault();event.stopPropagation();scrub.cancel();return;}
              if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
                event.preventDefault();
                event.stopPropagation();
                seekFrame(event.key==="ArrowRight"?1:-1);
              }
            }}
          >
            {ticks.map((seconds) => (
              <span
                key={seconds}
                className="editor-ruler-tick"
                style={{ left: seconds * scale }}
              >
                {formatTime(pointerTime(seconds))}
              </span>
            ))}
          </div>
          <div
            className="editor-marker-lane"
            hidden={!markerHeight}
            style={{ height: markerHeight, marginLeft: TRACK_LABEL_WIDTH }}
          >
            {props.sequence.bookmarks.map((bookmark) => (
              <button
                key={bookmark.id}
                className="editor-marker"
                data-selected={props.selectedId === bookmark.id}
                title={bookmark.note ?? formatTime(bookmark.time)}
                aria-pressed={props.selectedId === bookmark.id}
                aria-label={
                  t("Bookmark at {time}", { time: formatTime(bookmark.time) }) +
                  (bookmark.note ? ": " + bookmark.note : "")
                }
                style={{ left: secondsForDisplay(bookmark.time) * scale }}
                onClick={() => {
                  props.onSelectOther(bookmark.id);
                  props.onSeek(bookmark.time);
                }}
              >
                ◆
              </button>
            ))}
          </div>
          <div
            className="v4-timeline-tracks"
            ref={tracks}
            style={{ position: "relative" }}
          >
            {trackRows}
            {resourceDrop.state?.drag && <div className="resource-new-track" data-resource-new-track="true"
              style={{marginLeft:TRACK_LABEL_WIDTH,height:trackHeight}}>{t("Drop here to create a track")}</div>}
            {resourceTarget && <div className="resource-drop-preview" data-invalid={!!resourceTarget.reason} aria-hidden="true"
              style={{left:TRACK_LABEL_WIDTH + secondsForDisplay(resourceTarget.at)*scale,
                top:(resourceTarget.trackId ? props.sequence.tracks.findIndex(track=>track.id===resourceTarget.trackId) : props.sequence.tracks.length)*trackHeight + 3,
                width:resourceTarget.duration ? secondsForDisplay(resourceTarget.duration)*scale : 28,height:trackHeight-6}}>
              <span>{resourceDrop.state?.drag?.resource.name}</span>
              <small>{resourceTrack} · {resourceRange}</small>
            </div>}
            {box && (
              <div
                className="editor-selection-box"
                style={{
                  left: TRACK_LABEL_WIDTH + box.x,
                  top: box.y,
                  width: box.width,
                  height: box.height,
                }}
              />
            )}
            <div
              className="editor-playhead"
              ref={playheadLine}
              style={{
                left:TRACK_LABEL_WIDTH,
                top:-(RULER_HEIGHT+markerHeight),
                transform:`translateX(${secondsForDisplay(props.playhead)*scale}px)`,
                willChange:"transform",
                height: props.sequence.tracks.length * trackHeight+RULER_HEIGHT+markerHeight,
              }}
            ><button type="button" className="editor-playhead-grip" aria-label={t("Drag playhead")}
              onPointerDown={event=>scrub.start(event,props.presentation?.time()??props.playhead)}
              onPointerMove={scrub.move} onPointerUp={scrub.end} onPointerCancel={scrub.cancel} onLostPointerCapture={scrub.cancel}
              onKeyDown={event=>{if(event.key==="Escape"){event.preventDefault();event.stopPropagation();scrub.cancel();}else if(event.key==="ArrowLeft"||event.key==="ArrowRight"){
                event.preventDefault();event.stopPropagation();seekFrame(event.key==="ArrowRight"?1:-1);
              }}}/></div>
          </div>
        </div>
      </div>
      </div>
      <TimelineScrollNavigator viewport={viewport} controls={viewportId}/>
      <details className="editor-timeline-item-list">
        <summary>{t("Select timeline items")}</summary>
        {props.sequence.tracks.flatMap((track) =>
          track.elements.map((element) => (
            <button
              key={element.id}
              aria-pressed={selected.has(
                keyOf({ track_id: track.id, element_id: element.id }),
              )}
              onClick={(event) => {
                if (editable(element))
                  props.onSelect(
                    { track_id: track.id, element_id: element.id },
                    event.metaKey || event.ctrlKey
                      ? "toggle"
                      : event.shiftKey
                        ? "extend"
                        : "replace",
                  );
                else props.onSelectOther(element.id);
                root.current?.focus({ preventScroll: true });
              }}
            >
              {track.name} · {element.name} ·{" "}
              {formatTime(element.timeline_range.start)}
            </button>
          )),
        )}
      </details>
    </div>
  );
}
