import { useEffect, useRef, type DragEvent, type RefObject } from "react";
import type { Sequence } from "../types";
import { addTime } from "../../../model/time";
import { pointerTime, TRACK_LABEL_WIDTH } from "../timeline-edit/geometry";
import { RESOURCE_DRAG_TYPE, useResourceDrag } from "./resource-drag";
import type { ResourceHit } from "./resource-drop-model";

/** Browser hit testing only; the provider captures scope and plans the write. */
export function useResourceDropTarget(
  viewport: RefObject<HTMLDivElement | null>,
  sequence: Sequence,
  scale: number,
  snapping: boolean,
  disabled: boolean,
) {
  const drag = useResourceDrag();
  const point = useRef<{ x: number; y: number } | null>(null);
  const animation = useRef<number | null>(null);
  const lastFrame = useRef<number | null>(null);
  const latest = useRef({ drag, sequence, scale, snapping, disabled });
  latest.current = { drag, sequence, scale, snapping, disabled };
  const stop = () => {
    point.current = null;
    lastFrame.current = null;
    if (animation.current !== null) cancelAnimationFrame(animation.current);
    animation.current = null;
  };
  const hit = (
    x: number,
    y: number,
    target?: EventTarget | null,
  ): ResourceHit | null => {
    const settings = latest.current;
    const node =
      target instanceof HTMLElement ? target : document.elementFromPoint(x, y);
    const lane = node?.closest<HTMLElement>(
      "[data-file-track], [data-resource-new-track]",
    );
    if (!lane || !viewport.current?.contains(lane)) return null;
    const trackId = lane.dataset.fileTrack ?? null;
    const seconds = Math.max(
      0,
      (x - lane.getBoundingClientRect().left) / settings.scale,
    );
    const edges = settings.snapping && settings.drag?.drag?.resource.kind !== "transition"
      ? settings.sequence.tracks.flatMap((track) =>
        track.elements.flatMap((element) => [
          element.timeline_range.start,
          addTime(element.timeline_range.start, element.timeline_range.duration),
        ]),
      ) : [];
    return {
      trackId,
      at: pointerTime(seconds, edges, 8 / settings.scale),
      elementId:
        node?.closest<HTMLElement>("[data-element-id]")?.dataset.elementId,
      tolerance: 14 / settings.scale,
    };
  };
  const scroll = (timestamp: number) => {
    animation.current = null;
    const node = viewport.current,
      position = point.current;
    if (
      !node ||
      !position ||
      !latest.current.drag?.drag ||
      latest.current.disabled
    )
      return;
    const rect = node.getBoundingClientRect();
    if (position.x < rect.left || position.x > rect.right || position.y < rect.top || position.y > rect.bottom) {
      stop(); latest.current.drag.hover(null); return;
    }
    const edge = (value: number, low: number, high: number) =>
      value < low + 28 ? -Math.min(1, (low + 28 - value) / 28)
        : value > high - 28 ? Math.min(1, (value - high + 28) / 28) : 0;
    const elapsed = lastFrame.current === null ? 16 : Math.min(32, timestamp - lastFrame.current);
    lastFrame.current = timestamp;
    const dx = edge(position.x, rect.left + TRACK_LABEL_WIDTH, rect.right),
      dy = edge(position.y, rect.top, rect.bottom);
    if (dx || dy) {
      const left = node.scrollLeft, top = node.scrollTop;
      node.scrollLeft += dx * elapsed * 0.6;
      node.scrollTop += dy * elapsed * 0.6;
      if (left !== node.scrollLeft || top !== node.scrollTop)
        latest.current.drag.hover(hit(position.x, position.y));
      animation.current = requestAnimationFrame(scroll);
    } else {
      lastFrame.current = null;
    }
  };
  useEffect(() => {
    if (!drag?.drag || disabled) stop();
  }, [drag?.drag, disabled]);
  useEffect(() => {
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape") stop(); };
    window.addEventListener("blur", stop);
    window.addEventListener("keydown", escape, true);
    return () => { stop(); window.removeEventListener("blur", stop); window.removeEventListener("keydown", escape, true); };
  }, []);
  return {
    state: drag,
    over(event: DragEvent): boolean {
      if (!event.dataTransfer.types.includes(RESOURCE_DRAG_TYPE)) return false;
      event.preventDefault();
      event.stopPropagation();
      const target = disabled
        ? null
        : hit(event.clientX, event.clientY, event.target);
      const result = drag?.hover(target);
      event.dataTransfer.dropEffect =
        result && !result.reason ? "copy" : "none";
      point.current = { x: event.clientX, y: event.clientY };
      if (animation.current === null)
        animation.current = requestAnimationFrame(scroll);
      return true;
    },
    drop(event: DragEvent): boolean {
      if (!event.dataTransfer.types.includes(RESOURCE_DRAG_TYPE)) return false;
      event.preventDefault();
      event.stopPropagation();
      stop();
      const target = disabled
        ? null
        : hit(event.clientX, event.clientY, event.target);
      if (target)
        void drag?.drop(target, event.dataTransfer.getData(RESOURCE_DRAG_TYPE));
      else drag?.cancel();
      return true;
    },
    leave(event: DragEvent) {
      if (
        event.relatedTarget instanceof Node &&
        event.currentTarget.contains(event.relatedTarget)
      )
        return;
      stop();
      drag?.hover(null);
    },
  };
}
