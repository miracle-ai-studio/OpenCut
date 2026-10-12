import { useMediaPreview } from "../workspace/media-preview";
import { MediaArt } from "../shared/media-art";
import { t, useLocale } from "../shared/locale";
import { TimelineCommands } from "./commands";
import { DeleteTrack } from "./delete-track";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import {
  Undo2,
  Redo2,
  Copy,
  Eye,
  EyeOff,
  Lock,
  Magnet,
  Minus,
  Plus,
  Scissors,
  Trash2,
  Unlock,
  Volume2,
  VolumeX,
} from "lucide-react";
import { useWorkspace } from "../workspace/context";
import { activeSequence } from "../model/types";
import type { Asset, Clip, Operation, Track, Snapshot } from "../model/types";
import {
  addTime,
  atFrame,
  compareTime,
  formatTime,
  secondsForDisplay,
  subtractTime,
} from "../model/time";
import { Button, Empty } from "../shared/ui";
import { placeAsset } from "../model/edit-requests";
import { copyClip, type CopiedClip } from "../model/clip-clipboard";
import {
  clipSelections,
  deleteClips,
  moveClips,
  sameSelection,
  type ClipSelection,
} from "../model/selection";
import {
  duration,
  PIXELS_PER_SECOND,
  snappedTime,
  trimmedRange,
} from "./geometry";
import { formatRulerLabel, getRulerConfig, shouldShowLabel } from "./ruler";
import {
  readTimelinePreferences,
  saveTimelinePreferences,
} from "../shared/view-preferences";

type Drag = {
  clip: Clip;
  track: Track;
  kind: "move" | "start" | "end";
  x: number;
  time: number;
  revision: string;
  sequenceId: string;
  destination: string;
  selected: ClipSelection[];
  moved: boolean;
  scrollLeft: number;
  error?: string;
};
type Box = {
  x: number;
  y: number;
  endX: number;
  endY: number;
  previous: ClipSelection[];
};
export function TimelinePanel() {
  useLocale();
  const {
    controller,
    snapshot,
    historical,
    sequenceId,
    selection,
    selections,
    playhead,
    busy,
    ripple,
  } = useWorkspace();
  const view = historical ?? snapshot,
    sequence = activeSequence(view, sequenceId);
  const [zoom, setZoom] = useState(1),
    [snap, setSnap] = useState(true),
    [scroll, setScroll] = useState({ left: 0, width: 900 }),
    [drag, setDrag] = useState<Drag | null>(null);
  const [trackDeletion, setTrackDeletion] = useState<{
    view: Snapshot;
    sequenceId: string;
    trackId: string;
  } | null>(null);
  useEffect(() => setTrackDeletion(null), [view?.project.id, sequenceId]);
  const viewport = useRef<HTMLDivElement>(null),
    dragRef = useRef<Drag | null>(null);
  const restoredZoom = useRef(1),
    restoredScroll = useRef<number | null>(null);
  useLayoutEffect(() => {
    if (!view || !sequence) return;
    const saved = readTimelinePreferences(view.project.id, sequence.id);
    restoredZoom.current = saved.zoom;
    restoredScroll.current = saved.scrollLeft;
    setZoom(saved.zoom);
  }, [view?.project.id, sequence?.id]);
  useLayoutEffect(() => {
    if (
      restoredScroll.current === null ||
      zoom !== restoredZoom.current ||
      !viewport.current
    )
      return;
    viewport.current.scrollLeft = restoredScroll.current;
    restoredScroll.current = null;
    setScroll({
      left: viewport.current.scrollLeft,
      width: viewport.current.clientWidth,
    });
  }, [zoom, view?.project.id, sequence?.id]);
  const changeZoom = (value: number) => {
    setZoom(value);
    if (view && sequence && !historical)
      saveTimelinePreferences(view.project.id, sequence.id, { zoom: value });
  };
  const clipboard = useRef<CopiedClip[]>([]);
  const importAbort = useRef<AbortController | null>(null);
  const [importProgress, setImportProgress] = useState<string | null>(null);
  useEffect(
    () => () => importAbort.current?.abort(),
    [snapshot?.project.id, sequenceId],
  );
  const boxRef = useRef<Box | null>(null);
  const [box, setBox] = useState<Box | null>(null);
  const [commands, setCommands] = useState(false);
  const scale = PIXELS_PER_SECOND * zoom;
  useEffect(() => {
    const element = viewport.current;
    if (!element) return;
    const update = () =>
      setScroll({ left: element.scrollLeft, width: element.clientWidth });
    const observer = new ResizeObserver(update);
    observer.observe(element);
    update();
    return () => observer.disconnect();
  }, [sequence?.id]);
  useEffect(() => {
    setDrag(null);
    dragRef.current = null;
    boxRef.current = null;
    setBox(null);
  }, [view?.project.id, view?.revision, sequenceId]);
  const end = sequence
      ? Math.max(
          duration(
            sequence,
            view?.visual.find((graph) => graph.sequence_id === sequence.id)
              ?.shapes,
          ) + 8,
          20,
        )
      : 20,
    width = Math.max(end * scale, scroll.width - 160);
  const ticks = useMemo(() => {
    if (!sequence) return [];
    const config = getRulerConfig({ zoomLevel: zoom, fps: sequence.fps }),
      step = config.tickIntervalSeconds;
    const from = Math.max(
        0,
        Math.floor((scroll.left - 160) / scale / step) * step,
      ),
      to = (scroll.left + scroll.width) / scale;
    const ticks = [];
    for (let time = from; time < Math.min(to, end); time += step) {
      ticks.push({
        time,
        label: shouldShowLabel({
          time,
          labelIntervalSeconds: config.labelIntervalSeconds,
        })
          ? formatRulerLabel({ timeInSeconds: time, fps: sequence.fps })
          : null,
      });
    }
    return ticks;
  }, [sequence, zoom, scroll, scale, end]);
  const run = (label: string, operations: Operation[]) =>
    void controller
      .mutate(label, operations, [], view?.revision)
      .catch((error) => controller.report(error));
  const selected = sequence?.tracks
    .flatMap((track) => track.clips.map((clip) => ({ track, clip })))
    .find(
      (item) => selection?.kind === "clip" && item.clip.id === selection.id,
    );
  const split = (retain: "both" | "left" | "right" = "both") =>
    void controller.split(retain).catch((error) => controller.report(error));
  const selectedClips =
    sequence?.tracks.flatMap((track) =>
      track.clips
        .filter((clip) =>
          selections.some(
            (item) =>
              item.kind === "clip" &&
              item.trackId === track.id &&
              item.id === clip.id,
          ),
        )
        .map((clip) => ({ track, clip })),
    ) ?? [];
  const remove = () => {
    if (!view || !sequence || !selectedClips.length) return;
    const selected = selectedClips.map(({ track, clip }) => ({
      kind: "clip" as const,
      trackId: track.id,
      id: clip.id,
    }));
    const ordered = deleteClips(view, sequence.id, selected);
    void controller
      .mutate(
        "Delete clips",
        view.format === "base"
          ? selected.map((item) => ({
              type: "clip.delete" as const,
              sequence_id: sequence.id,
              track_id: item.trackId,
              clip_id: item.id,
            }))
          : [],
        view.format === "visual" ? ordered : [],
        view.revision,
      )
      .catch((error) => controller.report(error));
  };
  const copy = () => {
    if (view && sequence && selectedClips.length)
      clipboard.current = selectedClips.map(({ track, clip }) =>
        copyClip(view, sequence.id, track.id, clip),
      );
  };
  const paste = (duplicate = false) => {
    if (!view || !sequence) return;
    const sources = duplicate
      ? selectedClips.map(({ track, clip }) =>
          copyClip(view, sequence.id, track.id, clip),
        )
      : clipboard.current;
    if (
      !sources.length ||
      sources.some((source) => source.project !== view.project.id)
    )
      return;
    try {
      const earliest = sources.reduce(
        (time, source) =>
          compareTime(source.clip.timeline_range.start, time) < 0
            ? source.clip.timeline_range.start
            : time,
        sources[0].clip.timeline_range.start,
      );
      const start = duplicate
        ? sources.reduce((end, source) => {
            const track = sequence.tracks.find(
              (track) => track.id === source.trackId,
            );
            return (
              track?.clips.reduce((end, clip) => {
                const value = addTime(
                  clip.timeline_range.start,
                  clip.timeline_range.duration,
                );
                return compareTime(end, value) < 0 ? value : end;
              }, end) ?? end
            );
          }, earliest)
        : atFrame(secondsForDisplay(playhead), sequence.fps);
      const operations: Operation[] = [],
        visual: import("../model/types").VisualOperation[] = [],
        next: ClipSelection[] = [];
      for (const source of sources) {
        const track = sequence.tracks.find(
          (track) => track.id === source.trackId,
        );
        if (!track || track.locked)
          throw new Error("Unlock the target track before adding clips.");
        const id = crypto.randomUUID(),
          keyIds = new Map<string, string>();
        const clip = {
          ...structuredClone(source.clip),
          id,
          timeline_range: {
            ...source.clip.timeline_range,
            start: addTime(
              start,
              subtractTime(source.clip.timeline_range.start, earliest),
            ),
          },
          channels: source.clip.channels?.map((channel) => ({
            ...channel,
            keyframes: channel.keyframes.map((frame) => {
              const keyId = crypto.randomUUID();
              keyIds.set(frame.id, keyId);
              return { ...frame, id: keyId };
            }),
          })),
        };
        operations.push({
          type: "clip.insert",
          sequence_id: sequence.id,
          track_id: track.id,
          clip,
        });
        visual.push(
          ...source.effects.map((effect) => ({
            type: "effect.put" as const,
            sequence_id: sequence.id,
            effect: { ...effect, id: crypto.randomUUID(), target_id: id },
          })),
          ...source.masks.map((mask) => ({
            type: "mask.put" as const,
            sequence_id: sequence.id,
            mask: { ...mask, id: crypto.randomUUID(), target_id: id },
          })),
          ...source.curves
            .filter((curve) => keyIds.has(curve.keyframe_id))
            .map((curve) => ({
              type: "keyframe.curve_set" as const,
              sequence_id: sequence.id,
              curve: {
                ...curve,
                sequence_id: sequence.id,
                id: crypto.randomUUID(),
                clip_id: id,
                keyframe_id: keyIds.get(curve.keyframe_id)!,
              },
            })),
        );
        next.push({ kind: "clip", trackId: track.id, id });
      }
      void controller
        .mutate(
          duplicate ? "Duplicate clips" : "Paste clips",
          operations,
          visual,
          view.revision,
        )
        .then(() => {
          if (
            controller.getSnapshot().snapshot?.project.id === view.project.id &&
            controller.getSnapshot().sequenceId === sequence.id
          )
            controller.selectMany(next);
        })
        .catch((error) => controller.report(error));
    } catch (error) {
      controller.report(error);
    }
  };
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (
        (event.target as HTMLElement).closest(
          "input,textarea,select,[contenteditable=true],dialog,[role=dialog],[role=menu]",
        )
      )
        return;
      if (event.key === "Escape") {
        setDrag(null);
        dragRef.current = null;
        boxRef.current = null;
        setBox(null);
        controller.select(null);
      }
      if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
        event.preventDefault();
        if (sequence)
          controller.seek(
            atFrame(
              Math.max(
                0,
                secondsForDisplay(playhead) +
                  ((event.key === "ArrowLeft" ? -1 : 1) *
                    sequence.fps.denominator) /
                    sequence.fps.numerator,
              ),
              sequence.fps,
            ),
          );
      }
      if (historical || busy) return;
      if (event.metaKey || event.ctrlKey) {
        const key = event.key.toLowerCase();
        if (["z", "c", "v", "d", "a"].includes(key)) event.preventDefault();
        if (key === "z")
          void (event.shiftKey ? controller.redo() : controller.undo()).catch(
            (error) => controller.report(error),
          );
        if (key === "a" && sequence)
          controller.selectMany(clipSelections(sequence));
        if (key === "c") copy();
        if (key === "v") paste();
        if (key === "d") paste(true);
      }
      if (event.key === "Delete" || event.key === "Backspace") {
        event.preventDefault();
        remove();
      }
      if (event.key.toLowerCase() === "s" && !event.metaKey && !event.ctrlKey) {
        event.preventDefault();
        split();
      }
      if (!event.metaKey && !event.ctrlKey && !event.altKey) {
        const key = event.key.toLowerCase();
        if (key === "q" || key === "w") {
          event.preventDefault();
          split(key === "q" ? "right" : "left");
        }
        if (key === "r") {
          event.preventDefault();
          controller.toggleRipple();
        }
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  });
  const startDrag = (
    event: ReactPointerEvent,
    clip: Clip,
    track: Track,
    kind: Drag["kind"],
  ) => {
    event.stopPropagation();
    if (event.button !== 0 || !sequence || !view) return;
    const item: ClipSelection = {
      kind: "clip",
      id: clip.id,
      trackId: track.id,
    };
    if (event.shiftKey) {
      const ordered = clipSelections(sequence),
        from = ordered.findIndex(
          (entry) => selection && sameSelection(entry, selection),
        ),
        to = ordered.findIndex((entry) => sameSelection(entry, item));
      controller.selectMany(
        from < 0
          ? [item]
          : [
              ...ordered
                .slice(Math.min(from, to), Math.max(from, to) + 1)
                .filter((entry) => !sameSelection(entry, item)),
              item,
            ],
      );
      return;
    }
    if (event.metaKey || event.ctrlKey) {
      controller.select(item, "toggle");
      return;
    }
    const group =
      kind === "move" &&
      selections.some((entry) => sameSelection(entry, item)) &&
      selections.every((entry) => entry.kind === "clip")
        ? (selections as ClipSelection[])
        : [item];
    controller.selectMany(group);
    if (track.locked || historical || busy) return;
    if (kind === "move") {
      try {
        moveClips(sequence, group, item, track.id, clip.timeline_range.start);
      } catch (error) {
        controller.report(error);
        return;
      }
    }
    event.currentTarget.setPointerCapture(event.pointerId);
    const value = {
      clip,
      track,
      kind,
      revision: view.revision,
      sequenceId: sequence.id,
      destination: track.id,
      selected: group,
      moved: false,
      scrollLeft: viewport.current?.scrollLeft ?? 0,
      x: event.clientX,
      time: secondsForDisplay(
        kind === "end"
          ? addTime(clip.timeline_range.start, clip.timeline_range.duration)
          : clip.timeline_range.start,
      ),
    };
    dragRef.current = value;
    setDrag(value);
  };
  const moveDrag = (event: ReactPointerEvent) => {
    const value = dragRef.current;
    if (
      !value ||
      !sequence ||
      view?.revision !== value.revision ||
      sequence.id !== value.sequenceId
    )
      return;
    const initial = secondsForDisplay(
      value.kind === "end"
        ? addTime(
            value.clip.timeline_range.start,
            value.clip.timeline_range.duration,
          )
        : value.clip.timeline_range.start,
    );
    const time = secondsForDisplay(
      snappedTime(
        initial +
          (event.clientX -
            value.x +
            (viewport.current?.scrollLeft ?? 0) -
            value.scrollLeft) /
            scale,
        sequence,
        value.kind === "move"
          ? value.selected.map((item) => item.id)
          : value.clip.id,
        scale,
        snap,
      ),
    );
    let destination = value.destination,
      error: string | undefined;
    if (value.kind === "move") {
      const lane = [
        ...(viewport.current?.querySelectorAll<HTMLElement>(
          ".track-lane[data-track-id]",
        ) ?? []),
      ].find((lane) => {
        const rect = lane.getBoundingClientRect();
        return (
          event.clientX >= rect.left &&
          event.clientX <= rect.right &&
          event.clientY >= rect.top &&
          event.clientY <= rect.bottom
        );
      });
      destination = lane?.dataset.trackId ?? "";
      try {
        moveClips(
          sequence,
          value.selected,
          { kind: "clip", id: value.clip.id, trackId: value.track.id },
          destination,
          atFrame(time, sequence.fps),
        );
      } catch (e) {
        error = e instanceof Error ? e.message : String(e);
      }
    }
    const next = {
      ...value,
      time,
      destination,
      error,
      moved:
        value.moved ||
        Math.abs(
          event.clientX -
            value.x +
            (viewport.current?.scrollLeft ?? 0) -
            value.scrollLeft,
        ) >= 4 ||
        destination !== value.track.id,
    };
    dragRef.current = next;
    setDrag(next);
  };
  const endDrag = () => {
    const value = dragRef.current;
    dragRef.current = null;
    setDrag(null);
    const live = controller.getSnapshot();
    if (
      !value ||
      !sequence ||
      live.snapshot?.revision !== value.revision ||
      live.snapshot.project.id !== view?.project.id ||
      live.sequenceId !== value.sequenceId ||
      live.historical ||
      live.busy
    )
      return;
    if (!value.moved && value.kind === "move") {
      controller.select({
        kind: "clip",
        id: value.clip.id,
        trackId: value.track.id,
      });
      return;
    }
    const time = atFrame(value.time, sequence.fps);
    try {
      if (value.kind === "move") {
        if (value.error) throw new Error(value.error);
        const operations = moveClips(
          sequence,
          value.selected,
          { kind: "clip", id: value.clip.id, trackId: value.track.id },
          value.destination,
          time,
        );
        if (operations.length)
          void controller
            .mutate("Move clips", operations, [], value.revision)
            .then(() => {
              if (
                controller.getSnapshot().snapshot?.project.id !==
                  view?.project.id ||
                controller.getSnapshot().sequenceId !== sequence.id
              )
                return;
              controller.selectMany(
                value.selected.map((item) => {
                  const op = operations.find(
                    (op) => op.type === "clip.move" && op.clip_id === item.id,
                  );
                  return op?.type === "clip.move"
                    ? { ...item, trackId: op.destination_track_id }
                    : item;
                }),
              );
            })
            .catch((error) => controller.report(error));
      } else {
        const range = trimmedRange(
          value.clip,
          value.kind === "start" ? time : value.clip.timeline_range.start,
          value.kind === "end"
            ? time
            : addTime(
                value.clip.timeline_range.start,
                value.clip.timeline_range.duration,
              ),
        );
        run("Trim clip", [
          {
            type: "clip.trim",
            sequence_id: sequence.id,
            track_id: value.track.id,
            clip_id: value.clip.id,
            ...range,
          },
        ]);
      }
    } catch (error) {
      controller.report(error);
    }
  };
  const trackUpdate = (track: Track, field: "locked" | "muted" | "enabled") => {
    if (sequence)
      run("Update track", [
        {
          type: "track.update",
          sequence_id: sequence.id,
          track_id: track.id,
          [field]: !track[field],
        },
      ]);
  };
  let moves: Operation[] = [];
  if (
    drag?.kind === "move" &&
    !drag.error &&
    sequence &&
    view?.revision === drag.revision &&
    sequence.id === drag.sequenceId
  )
    moves = moveClips(
      sequence,
      drag.selected,
      { kind: "clip", id: drag.clip.id, trackId: drag.track.id },
      drag.destination,
      atFrame(drag.time, sequence.fps),
    );
  const cancelBox = () => {
    if (boxRef.current) controller.selectMany(boxRef.current.previous);
    boxRef.current = null;
    setBox(null);
  };
  return (
    <section className="timeline panel" aria-label={t("Timeline")}>
      <div className="timeline-toolbar">
        <div className="toolbar">
          <Button
            aria-label={t("Undo")}
            disabled={busy || !!historical || !controller.canUndo}
            icon={<Undo2 size={15} />}
            onClick={() =>
              void controller.undo().catch((error) => controller.report(error))
            }
          />
          <Button
            aria-label={t("Redo")}
            disabled={busy || !!historical || !controller.canRedo}
            icon={<Redo2 size={15} />}
            onClick={() =>
              void controller.redo().catch((error) => controller.report(error))
            }
          />
          <Button
            aria-label={t("Duplicate")}
            disabled={busy || !!historical || !selected}
            icon={<Copy size={15} />}
            onClick={() => paste(true)}
          />

          <Button
            aria-label={t("Split clip")}
            title={t("Split at playhead · S")}
            disabled={!sequence || !!historical || busy}
            onClick={() => split()}
            icon={<Scissors size={15} />}
          />
          <Button
            aria-label={t("Split and remove left")}
            title={t("Split and remove left · Q")}
            disabled={!sequence || !!historical || busy}
            onClick={() => split("right")}
          >
            {t("Remove left")}
          </Button>
          <Button
            aria-label={t("Split and remove right")}
            title={t("Split and remove right · W")}
            disabled={!sequence || !!historical || busy}
            onClick={() => split("left")}
          >
            {t("Remove right")}
          </Button>
          <Button
            aria-label={t("Delete clip")}
            disabled={!selected || !!historical || busy}
            onClick={remove}
            icon={<Trash2 size={15} />}
          />
          <span className="divider" />
          <Button
            aria-label={t("Toggle snapping")}
            aria-pressed={snap}
            onClick={() => setSnap(!snap)}
            icon={<Magnet size={15} />}
          />
          <Button
            aria-label={t("Ripple editing")}
            title={t("Ripple editing · R")}
            aria-pressed={ripple}
            disabled={!!historical || busy}
            onClick={() => controller.toggleRipple()}
          >
            {t("Ripple")}
          </Button>
          <Button
            disabled={!!historical || busy}
            onClick={() =>
              void controller
                .track("video")
                .catch((error) => controller.report(error))
            }
            icon={<Plus size={15} />}
          >
            {t("Track")}
          </Button>
        </div>
        {snapshot?.format === "visual" && (
          <Button
            disabled={!!historical || busy}
            onClick={() => setCommands(true)}
          >
            {t("Timeline edit")}
          </Button>
        )}
        {importProgress && (
          <div className="toolbar" role="status">
            <span>{importProgress}</span>
            <Button onClick={() => importAbort.current?.abort()}>
              {t("Cancel import")}
            </Button>
          </div>
        )}
        <output className="timecode">{formatTime(playhead)}</output>
        <output aria-label={t("Selected clips")}>
          {selectedClips.length > 1
            ? t("{count} clips selected", { count: selectedClips.length })
            : ""}
        </output>
        {drag?.error && <output role="status">{t(drag.error)}</output>}
        <div className="toolbar">
          <Button
            aria-label={t("Zoom out timeline")}
            onClick={() => changeZoom(Math.max(0.1, zoom / 1.5))}
            icon={<Minus size={14} />}
          />
          <input
            aria-label={t("Timeline zoom")}
            type="range"
            min="0.1"
            max="8"
            step="0.1"
            value={zoom}
            onChange={(event) => changeZoom(Number(event.target.value))}
          />
          <Button
            aria-label={t("Zoom in timeline")}
            onClick={() => changeZoom(Math.min(8, zoom * 1.5))}
            icon={<Plus size={14} />}
          />
        </div>
      </div>
      <div
        ref={viewport}
        className="timeline-scroll"
        onScroll={(event) => {
          setScroll({
            left: event.currentTarget.scrollLeft,
            width: event.currentTarget.clientWidth,
          });
          if (
            view &&
            sequence &&
            !historical &&
            restoredScroll.current === null
          )
            saveTimelinePreferences(view.project.id, sequence.id, {
              scrollLeft: event.currentTarget.scrollLeft,
            });
        }}
      >
        <div
          className="timeline-body"
          style={{ width: width + 160 }}
          onPointerDown={(event) => {
            if (
              event.button !== 0 ||
              busy ||
              !(event.target as HTMLElement).closest(".track-lane") ||
              (event.target as HTMLElement).closest(".timeline-clip")
            )
              return;
            const rect = event.currentTarget.getBoundingClientRect();
            const previous = selections.filter(
              (item): item is ClipSelection => item.kind === "clip",
            );
            const value = {
              x: event.clientX - rect.left,
              y: event.clientY - rect.top,
              endX: event.clientX - rect.left,
              endY: event.clientY - rect.top,
              previous,
            };
            boxRef.current = value;
            setBox(value);
            if (!event.ctrlKey && !event.metaKey && !event.shiftKey)
              controller.selectMany([]);
            else value.previous = previous;
            event.currentTarget.setPointerCapture(event.pointerId);
            // Keep the original set for cancellation; additive selection is separate.
            event.currentTarget.dataset.additive = String(
              event.ctrlKey || event.metaKey || event.shiftKey,
            );
          }}
          onPointerMove={(event) => {
            const current = boxRef.current;
            if (!current) return;
            const rect = event.currentTarget.getBoundingClientRect();
            const next = {
              ...current,
              endX: event.clientX - rect.left,
              endY: event.clientY - rect.top,
            };
            boxRef.current = next;
            setBox(next);
            const left = rect.left + Math.min(next.x, next.endX),
              right = rect.left + Math.max(next.x, next.endX),
              top = rect.top + Math.min(next.y, next.endY),
              bottom = rect.top + Math.max(next.y, next.endY);
            const hit: ClipSelection[] = [];
            event.currentTarget
              .querySelectorAll<HTMLElement>("[data-clip-id]")
              .forEach((element) => {
                const bounds = element.getBoundingClientRect(),
                  trackId =
                    element.closest<HTMLElement>("[data-track-id]")?.dataset
                      .trackId;
                if (
                  trackId &&
                  bounds.left < right &&
                  bounds.right > left &&
                  bounds.top < bottom &&
                  bounds.bottom > top
                )
                  hit.push({
                    kind: "clip",
                    trackId,
                    id: element.dataset.clipId!,
                  });
              });
            controller.selectMany(
              event.currentTarget.dataset.additive === "true"
                ? [...current.previous, ...hit]
                : hit,
            );
          }}
          onPointerUp={() => {
            boxRef.current = null;
            setBox(null);
          }}
          onPointerCancel={cancelBox}
        >
          {box && (
            <div
              className="timeline-selection-box"
              style={{
                left: Math.min(box.x, box.endX),
                top: Math.min(box.y, box.endY),
                width: Math.abs(box.endX - box.x),
                height: Math.abs(box.endY - box.y),
              }}
            />
          )}
          <div className="timeline-ruler">
            <div className="track-heading ruler-corner">{t("TIMELINE")}</div>
            <div
              className="ruler-content"
              style={{ width }}
              onPointerDown={(event) => {
                if (sequence) {
                  const rect = event.currentTarget.getBoundingClientRect();
                  controller.seek(
                    atFrame((event.clientX - rect.left) / scale, sequence.fps),
                  );
                }
              }}
            >
              {ticks.map((tick) => (
                <span
                  className={tick.label ? "ruler-label" : "ruler-tick"}
                  key={tick.time.toFixed(7)}
                  style={{ left: tick.time * scale }}
                >
                  {tick.label}
                </span>
              ))}
            </div>
          </div>
          {sequence?.tracks.map((track) => (
            <div className={`track-row ${track.kind}`} key={track.id}>
              <div className="track-heading">
                <span title={track.name}>{track.name}</span>
                <div className="toolbar">
                  <Button
                    aria-label={t("{value0} {value1}", {
                      value0: track.enabled ? "Hide" : "Show",
                      value1: track.name,
                    })}
                    disabled={!!historical || busy}
                    onClick={() => trackUpdate(track, "enabled")}
                    icon={
                      track.enabled ? <Eye size={12} /> : <EyeOff size={12} />
                    }
                  />
                  <Button
                    aria-label={t("{value0} {value1}", {
                      value0: track.muted ? "Unmute" : "Mute",
                      value1: track.name,
                    })}
                    disabled={!!historical || busy}
                    onClick={() => trackUpdate(track, "muted")}
                    icon={
                      track.muted ? (
                        <VolumeX size={12} />
                      ) : (
                        <Volume2 size={12} />
                      )
                    }
                  />
                  <Button
                    aria-label={t("{value0} {value1}", {
                      value0: track.locked ? "Unlock" : "Lock",
                      value1: track.name,
                    })}
                    disabled={!!historical || busy}
                    onClick={() => trackUpdate(track, "locked")}
                    icon={
                      track.locked ? <Lock size={12} /> : <Unlock size={12} />
                    }
                  />
                  <Button
                    aria-label={t("Delete track {name}", { name: track.name })}
                    title={
                      track.locked
                        ? t("Unlock this track before deleting it.")
                        : t("Delete track")
                    }
                    disabled={!!historical || busy || track.locked}
                    onClick={() => {
                      if (view && sequence)
                        setTrackDeletion({
                          view,
                          sequenceId: sequence.id,
                          trackId: track.id,
                        });
                    }}
                    icon={<Trash2 size={12} />}
                  />
                </div>
              </div>
              <div
                className="track-lane"
                data-track-id={track.id}
                style={{ width }}
                onDragOver={(event) => {
                  if (!historical && !busy && !importAbort.current)
                    event.preventDefault();
                }}
                onDrop={(event) => {
                  event.preventDefault();
                  if (historical || busy || importAbort.current) return;
                  if (event.dataTransfer.files.length) {
                    const rect = event.currentTarget.getBoundingClientRect(),
                      request = new AbortController();
                    const at = atFrame(
                      Math.max(0, (event.clientX - rect.left) / scale),
                      sequence.fps,
                    );
                    importAbort.current = request;
                    setImportProgress(t("Preparing media…"));
                    void controller
                      .importMedia(
                        Array.from(event.dataTransfer.files),
                        setImportProgress,
                        request.signal,
                        { sequenceId: sequence.id, trackId: track.id, at },
                      )
                      .catch((error) => controller.report(error))
                      .finally(() => {
                        if (importAbort.current === request) {
                          importAbort.current = null;
                          setImportProgress(null);
                        }
                      });
                    return;
                  }
                  const asset = view?.project.assets.find(
                    (asset) =>
                      asset.id ===
                      event.dataTransfer.getData("application/opencut-asset"),
                  );
                  if (asset) {
                    const rect = event.currentTarget.getBoundingClientRect();
                    try {
                      run(
                        `Add ${asset.name}`,
                        placeAsset(
                          asset,
                          sequence,
                          atFrame(
                            (event.clientX - rect.left) / scale,
                            sequence.fps,
                          ),
                          track.id,
                        ),
                      );
                    } catch (error) {
                      controller.report(error);
                    }
                  }
                }}
              >
                {moves
                  .filter(
                    (op): op is Extract<Operation, { type: "clip.move" }> =>
                      op.type === "clip.move" &&
                      op.destination_track_id === track.id,
                  )
                  .map((op) => {
                    const clip = sequence.tracks
                      .flatMap((track) => track.clips)
                      .find((clip) => clip.id === op.clip_id)!;
                    return (
                      <div
                        key={op.clip_id}
                        className="timeline-clip move-ghost"
                        style={{
                          left: secondsForDisplay(op.start) * scale,
                          width: Math.max(
                            5,
                            secondsForDisplay(clip.timeline_range.duration) *
                              scale,
                          ),
                        }}
                      />
                    );
                  })}
                {track.clips
                  .filter((clip) => {
                    const start =
                        secondsForDisplay(clip.timeline_range.start) * scale,
                      end =
                        start +
                        secondsForDisplay(clip.timeline_range.duration) * scale;
                    return (
                      end > scroll.left - 200 &&
                      start < scroll.left + scroll.width + 200
                    );
                  })
                  .map((clip) => {
                    const moving =
                        drag?.kind !== "move" && drag?.clip.id === clip.id
                          ? drag
                          : null,
                      originalStart = secondsForDisplay(
                        clip.timeline_range.start,
                      ),
                      originalEnd =
                        originalStart +
                        secondsForDisplay(clip.timeline_range.duration);
                    const start =
                        moving && moving.kind !== "end"
                          ? moving.time
                          : originalStart,
                      end =
                        moving?.kind === "end"
                          ? moving.time
                          : moving?.kind === "move"
                            ? moving.time + originalEnd - originalStart
                            : originalEnd;
                    const asset = view?.project.assets.find(
                      (asset) => asset.id === clip.asset_id,
                    );
                    return (
                      <div
                        role="button"
                        tabIndex={0}
                        aria-label={t("Clip {value0}", {
                          value0: asset?.name ?? clip.id,
                        })}
                        aria-pressed={selections.some(
                          (item) => item.kind === "clip" && item.id === clip.id,
                        )}
                        className={`timeline-clip ${selections.some((item) => item.kind === "clip" && item.id === clip.id) ? "selected" : ""}`}
                        key={clip.id}
                        data-clip-id={clip.id}
                        style={{
                          left: start * scale,
                          width: Math.max(5, (end - start) * scale),
                          opacity:
                            drag?.kind === "move" &&
                            drag.moved &&
                            drag.selected.some((item) => item.id === clip.id)
                              ? 0.4
                              : 1,
                        }}
                        onPointerDown={(event) =>
                          startDrag(event, clip, track, "move")
                        }
                        onPointerMove={moveDrag}
                        onPointerUp={endDrag}
                        onPointerCancel={() => {
                          dragRef.current = null;
                          setDrag(null);
                        }}
                        onKeyDown={(event) => {
                          if (event.key === "Enter" || event.key === " ") {
                            event.preventDefault();
                            controller.select(
                              {
                                kind: "clip",
                                id: clip.id,
                                trackId: track.id,
                              },
                              event.metaKey || event.ctrlKey
                                ? "toggle"
                                : "replace",
                            );
                          }
                        }}
                      >
                        <span
                          className="trim-handle start"
                          role="slider"
                          aria-label={t("Trim start {value0}", {
                            value0: asset?.name ?? clip.id,
                          })}
                          aria-valuenow={start}
                          tabIndex={-1}
                          onPointerDown={(event) =>
                            startDrag(event, clip, track, "start")
                          }
                        />
                        <AssetArt asset={asset} clip={clip} />
                        <span className="clip-label">
                          {asset?.name ?? "Clip"}
                        </span>
                        <span
                          className="trim-handle end"
                          role="slider"
                          aria-label={t("Trim end {value0}", {
                            value0: asset?.name ?? clip.id,
                          })}
                          aria-valuenow={end}
                          tabIndex={-1}
                          onPointerDown={(event) =>
                            startDrag(event, clip, track, "end")
                          }
                        />
                      </div>
                    );
                  })}
              </div>
            </div>
          ))}
          {!!sequence?.caption_cues?.length && (
            <div className="track-row caption">
              <div className="track-heading">{t("Captions")}</div>
              <div className="track-lane" style={{ width }}>
                {sequence.caption_cues.map((cue) => (
                  <button
                    className={`timeline-clip ${selection?.id === cue.id ? "selected" : ""}`}
                    style={{
                      left: secondsForDisplay(cue.timeline_range.start) * scale,
                      width: Math.max(
                        6,
                        secondsForDisplay(cue.timeline_range.duration) * scale,
                      ),
                    }}
                    key={cue.id}
                    onClick={() =>
                      controller.select({ kind: "caption", id: cue.id })
                    }
                  >
                    {cue.text}
                  </button>
                ))}
              </div>
            </div>
          )}
          {view?.visual
            .find((graph) => graph.sequence_id === sequence?.id)
            ?.shapes.map((shape) => (
              <div className="track-row graphic" key={shape.id}>
                <div className="track-heading">{t("Shape")}</div>
                <div className="track-lane" style={{ width }}>
                  <button
                    className={`timeline-clip ${selection?.id === shape.id ? "selected" : ""}`}
                    style={{
                      left:
                        secondsForDisplay(shape.timeline_range.start) * scale,
                      width: Math.max(
                        6,
                        secondsForDisplay(shape.timeline_range.duration) *
                          scale,
                      ),
                    }}
                    onClick={() =>
                      controller.select({ kind: "shape", id: shape.id })
                    }
                  >
                    {t(
                      shape.primitive.kind === "rectangle"
                        ? "Rectangle"
                        : "Ellipse",
                    )}
                  </button>
                </div>
              </div>
            ))}
          {!sequence?.tracks.length && (
            <Empty title={t("Your timeline starts here")}>
              {t("Add media from the library to begin.")}
            </Empty>
          )}
          <div
            className="playhead"
            style={{ left: 160 + secondsForDisplay(playhead) * scale }}
          >
            <span />
          </div>
        </div>
      </div>
      {commands && sequence && (
        <TimelineCommands
          sequence={sequence}
          clip={selected?.clip}
          onClose={() => setCommands(false)}
        />
      )}
      {trackDeletion && (
        <DeleteTrack
          {...trackDeletion}
          onClose={() => setTrackDeletion(null)}
        />
      )}
    </section>
  );
}

function AssetArt({ asset, clip }: { asset: Asset | undefined; clip: Clip }) {
  const art = useMediaPreview(asset);
  const duration = asset?.duration ? secondsForDisplay(asset.duration) : 0;
  const from =
    clip.source_range && duration
      ? secondsForDisplay(clip.source_range.start) / duration
      : 0;
  const to =
    clip.source_range && duration
      ? from + secondsForDisplay(clip.source_range.duration) / duration
      : 1;
  return <MediaArt {...art} from={from} to={to} />;
}
