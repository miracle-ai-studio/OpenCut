import { useEffect, useRef, useState } from "react";
import type { PointerEvent } from "react";
import { useWorkspace } from "../workspace/context";
import { activeSequence } from "../model/types";
import { secondsForDisplay } from "../model/time";
import { t, useLocale } from "../shared/locale";
export function TransformOverlay() {
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
  const sequence = activeSequence(snapshot, sequenceId);
  const clip = sequence?.tracks
    .flatMap((track) => track.clips.map((clip) => ({ clip, track })))
    .find(
      (item) => selection?.kind === "clip" && item.clip.id === selection.id,
    );
  const shape = snapshot?.visual
    .find((graph) => graph.sequence_id === sequenceId)
    ?.shapes.find(
      (shape) => selection?.kind === "shape" && shape.id === selection.id,
    );
  const entity = shape ?? clip?.clip;
  const frame = useRef<HTMLDivElement>(null);
  type Draft = {
    x: number;
    y: number;
    sx: number;
    sy: number;
    rotation: number;
  };
  const base: Draft = {
    x: shape?.position_x ?? Number(clip?.clip.params?.position_x ?? 0),
    y: shape?.position_y ?? Number(clip?.clip.params?.position_y ?? 0),
    sx: Number(clip?.clip.params?.scale_x ?? 1),
    sy: Number(clip?.clip.params?.scale_y ?? 1),
    rotation:
      shape?.rotation_degrees ??
      Number(clip?.clip.params?.rotation_degrees ?? 0),
  };
  const [draft, setDraft] = useState<Draft | null>(null);
  const drag = useRef<{
    mode: string;
    x: number;
    y: number;
    base: Draft;
    revision: string;
    draft: Draft;
  } | null>(null);
  const [bounds, setBounds] = useState({ width: 0, height: 0 });
  useEffect(() => {
    const parent = frame.current?.parentElement;
    if (!parent || !sequence) return;
    const measure = () => {
      setBounds({ width: parent.clientWidth, height: parent.clientHeight });
    };
    const observer = new ResizeObserver(measure);
    observer.observe(parent);
    measure();
    return () => observer.disconnect();
  }, [sequence?.id, entity?.id]);
  const time = secondsForDisplay(playhead);
  if (
    !entity ||
    !sequence ||
    !snapshot ||
    historical ||
    busy ||
    clip?.track.locked ||
    (shape &&
      sequence.tracks.find((track) => track.id === shape.track_id)?.locked) ||
    time < secondsForDisplay(entity.timeline_range.start) ||
    time >=
      secondsForDisplay(entity.timeline_range.start) +
        secondsForDisplay(entity.timeline_range.duration)
  )
    return null;
  const displayed = draft ?? base;
  const down = (event: PointerEvent, mode: string) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = {
      mode,
      x: event.clientX,
      y: event.clientY,
      base,
      revision: snapshot.revision,
      draft: base,
    };
  };
  const move = (event: PointerEvent) => {
    const value = drag.current;
    const rect = frame.current?.getBoundingClientRect();
    if (!value || !rect) return;
    const ratio = sequence.width / rect.width;
    const dx = (event.clientX - value.x) * ratio,
      dy = (event.clientY - value.y) * ratio;
    const next = { ...value.base };
    if (value.mode === "move") {
      next.x += dx;
      next.y += dy;
    } else if (value.mode === "rotate") {
      next.rotation += dx / 3;
    } else {
      const scale = Math.max(
        0.05,
        1 +
          (dx * (value.mode.endsWith("w") ? -1 : 1) +
            dy * (value.mode.startsWith("n") ? -1 : 1)) /
            sequence.width,
      );
      next.sx *= scale;
      next.sy *= scale;
    }
    value.draft = next;
    setDraft(next);
  };
  const end = () => {
    const value = drag.current;
    drag.current = null;
    setDraft(null);
    if (
      !value ||
      controller.getSnapshot().snapshot?.revision !== value.revision
    )
      return;
    if (shape)
      void controller
        .mutate(
          "Transform shape",
          [],
          [
            {
              type: "shape.put",
              sequence_id: sequence.id,
              shape: {
                ...shape,
                position_x: value.draft.x,
                position_y: value.draft.y,
                rotation_degrees: value.draft.rotation,
                primitive:
                  shape.primitive.kind === "rectangle"
                    ? {
                        ...shape.primitive,
                        width_px:
                          (shape.primitive.width_px * value.draft.sx) / base.sx,
                        height_px:
                          (shape.primitive.height_px * value.draft.sy) /
                          base.sy,
                      }
                    : {
                        ...shape.primitive,
                        radius_x_px:
                          (shape.primitive.radius_x_px * value.draft.sx) /
                          base.sx,
                        radius_y_px:
                          (shape.primitive.radius_y_px * value.draft.sy) /
                          base.sy,
                      },
              },
            },
          ],
        )
        .catch((error) => controller.report(error));
    else if (clip)
      void controller
        .mutate("Transform clip", [
          {
            type: "parameter.set_clip",
            sequence_id: sequence.id,
            track_id: clip.track.id,
            clip_id: clip.clip.id,
            params: {
              ...clip.clip.params,
              position_x: value.draft.x,
              position_y: value.draft.y,
              scale_x: value.draft.sx,
              scale_y: value.draft.sy,
              rotation_degrees: value.draft.rotation,
            },
          },
        ])
        .catch((error) => controller.report(error));
  };
  const w = shape
    ? shape.primitive.kind === "rectangle"
      ? shape.primitive.width_px
      : shape.primitive.radius_x_px * 2
    : sequence.width;
  const h = shape
    ? shape.primitive.kind === "rectangle"
      ? shape.primitive.height_px
      : shape.primitive.radius_y_px * 2
    : sequence.height;
  return (
    <div
      className="transform-canvas"
      ref={frame}
      style={{ width: bounds.width, height: bounds.height }}
    >
      <div
        className="transform-box"
        aria-label={t("Transform & audio")}
        style={{
          left: `${50 + (displayed.x / sequence.width) * 100}%`,
          top: `${50 + (displayed.y / sequence.height) * 100}%`,
          width: `${(w / sequence.width) * 100 * displayed.sx}%`,
          height: `${(h / sequence.height) * 100 * displayed.sy}%`,
          transform: `translate(-50%,-50%) rotate(${displayed.rotation}deg)`,
        }}
        onPointerDown={(e) => down(e, "move")}
        onPointerMove={move}
        onPointerUp={end}
        onPointerCancel={() => {
          drag.current = null;
          setDraft(null);
        }}
      >
        <span className="transform-origin" />
        <button
          className="transform-rotate"
          aria-label={t("Rotation")}
          onPointerDown={(e) => down(e, "rotate")}
        />
        {["nw", "ne", "sw", "se"].map((corner) => (
          <button
            key={corner}
            className={`transform-handle ${corner}`}
            aria-label={t("Scale X")}
            onPointerDown={(e) => down(e, corner)}
          />
        ))}
      </div>
    </div>
  );
}
