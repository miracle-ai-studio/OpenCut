import { useMediaPreview } from "../workspace/media-preview";
import { MediaArt } from "../shared/media-art";
import { t, useLocale } from "../shared/locale";
import { TimelineCommands } from "./commands";
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
import type { Asset, Clip, Operation, Track } from "../model/types";
import {
  addTime,
  atFrame,
  compareTime,
  formatTime,
  secondsForDisplay,
  fromSeconds,
} from "../model/time";
import { Button, Empty } from "../shared/ui";
import { placeAsset } from "../model/edit-requests";
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
};
export function TimelinePanel() {
  useLocale();
  const {
    controller,
    snapshot,
    historical,
    sequenceId,
    selection,
    playhead,
    busy,
  } = useWorkspace();
  const view = historical ?? snapshot,
    sequence = activeSequence(view, sequenceId);
  const [zoom, setZoom] = useState(1),
    [snap, setSnap] = useState(true),
    [scroll, setScroll] = useState({ left: 0, width: 900 }),
    [drag, setDrag] = useState<Drag | null>(null);
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
  const clipboard = useRef<{
    clip: Clip;
    project: string;
    trackId: string;
  } | null>(null);
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
      .mutate(label, operations)
      .catch((error) => controller.report(error));
  const selected = sequence?.tracks
    .flatMap((track) => track.clips.map((clip) => ({ track, clip })))
    .find(
      (item) => selection?.kind === "clip" && item.clip.id === selection.id,
    );
  const split = () => {
    if (selected && sequence)
      run("Split clip", [
        {
          type: "clip.split",
          sequence_id: sequence.id,
          track_id: selected.track.id,
          clip_id: selected.clip.id,
          at: atFrame(secondsForDisplay(playhead), sequence.fps),
          right_clip_id: crypto.randomUUID(),
        },
      ]);
  };
  const remove = () => {
    if (selected && sequence)
      run("Delete clip", [
        {
          type: "clip.delete",
          sequence_id: sequence.id,
          track_id: selected.track.id,
          clip_id: selected.clip.id,
        },
      ]);
  };
  const copy = () => {
    if (selected && view)
      clipboard.current = {
        clip: structuredClone(selected.clip),
        project: view.project.id,
        trackId: selected.track.id,
      };
  };
  const paste = (duplicate = false) => {
    const source =
      duplicate && selected && view
        ? {
            clip: selected.clip,
            project: view.project.id,
            trackId: selected.track.id,
          }
        : clipboard.current;
    if (!source || !sequence || source.project !== view?.project.id) return;
    const track = sequence.tracks.find((track) => track.id === source.trackId);
    if (!track || track.locked) return;
    const id = crypto.randomUUID();
    const start = duplicate
      ? track.clips.reduce(
          (end, clip) =>
            Math.max(
              end,
              secondsForDisplay(
                addTime(
                  clip.timeline_range.start,
                  clip.timeline_range.duration,
                ),
              ),
            ),
          0,
        )
      : secondsForDisplay(playhead);
    const keyIds = new Map<string, string>();
    const clip = {
      ...structuredClone(source.clip),
      id,
      timeline_range: {
        ...source.clip.timeline_range,
        start: fromSeconds(start),
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
    const graph = view.visual.find(
      (graph) => graph.sequence_id === sequence.id,
    );
    const visual = [
      ...(graph?.effects
        .filter((effect) => effect.target_id === source.clip.id)
        .map((effect) => ({
          type: "effect.put" as const,
          sequence_id: sequence.id,
          effect: { ...effect, id: crypto.randomUUID(), target_id: id },
        })) ?? []),
      ...(graph?.masks
        .filter((mask) => mask.target_id === source.clip.id)
        .map((mask) => ({
          type: "mask.put" as const,
          sequence_id: sequence.id,
          mask: { ...mask, id: crypto.randomUUID(), target_id: id },
        })) ?? []),
      ...view.curves
        .filter(
          (curve) =>
            curve.clip_id === source.clip.id && keyIds.has(curve.keyframe_id),
        )
        .map((curve) => ({
          type: "keyframe.curve_set" as const,
          sequence_id: sequence.id,
          curve: {
            ...curve,
            id: crypto.randomUUID(),
            clip_id: id,
            keyframe_id: keyIds.get(curve.keyframe_id)!,
          },
        })),
    ];
    void controller
      .mutate(
        duplicate ? "Duplicate clip" : "Paste clip",
        [
          {
            type: "clip.insert",
            sequence_id: sequence.id,
            track_id: track.id,
            clip,
          },
        ],
        visual,
      )
      .then(() => controller.select({ kind: "clip", trackId: track.id, id }))
      .catch((error) => controller.report(error));
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
        if (["z", "c", "v", "d"].includes(key)) event.preventDefault();
        if (key === "z")
          void (event.shiftKey ? controller.redo() : controller.undo()).catch(
            (error) => controller.report(error),
          );
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
    controller.select({ kind: "clip", id: clip.id, trackId: track.id });
    if (event.button !== 0 || track.locked || historical || busy) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    const value = {
      clip,
      track,
      kind,
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
    if (!value || !sequence) return;
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
        initial + (event.clientX - value.x) / scale,
        sequence,
        value.clip.id,
        scale,
        snap,
      ),
    );
    const next = { ...value, time };
    dragRef.current = next;
    setDrag(next);
  };
  const endDrag = () => {
    const value = dragRef.current;
    dragRef.current = null;
    setDrag(null);
    if (!value || !sequence) return;
    const time = atFrame(value.time, sequence.fps);
    try {
      if (value.kind === "move") {
        if (compareTime(time, value.clip.timeline_range.start) !== 0)
          run("Move clip", [
            {
              type: "clip.move",
              sequence_id: sequence.id,
              track_id: value.track.id,
              clip_id: value.clip.id,
              destination_track_id: value.track.id,
              start: time,
            },
          ]);
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
            disabled={!selected || !!historical || busy}
            onClick={split}
            icon={<Scissors size={15} />}
          />
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
        <output className="timecode">{formatTime(playhead)}</output>
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
        <div className="timeline-body" style={{ width: width + 160 }}>
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
                </div>
              </div>
              <div
                className="track-lane"
                style={{ width }}
                onDragOver={(event) => {
                  if (!historical && !track.locked) event.preventDefault();
                }}
                onDrop={(event) => {
                  event.preventDefault();
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
                    const moving = drag?.clip.id === clip.id ? drag : null,
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
                        aria-pressed={selection?.id === clip.id}
                        className={`timeline-clip ${selection?.id === clip.id ? "selected" : ""}`}
                        key={clip.id}
                        style={{
                          left: start * scale,
                          width: Math.max(5, (end - start) * scale),
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
                          if (event.key === "Enter")
                            controller.select({
                              kind: "clip",
                              id: clip.id,
                              trackId: track.id,
                            });
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
