import { useEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import type { V4Element } from "../types";
import { canvasTextViewport } from "../text/canvas-geometry";
import { useSceneStack } from "./workspace";
import { dragSceneMask, sceneMaskOverlay } from "./geometry";
import type { Handle, XY } from "./geometry";
import { useSceneText } from "./messages";

type Gesture = {
  pointerId: number;
  target: Element;
  scope: object;
  start: V4Element;
  maskId: string;
  handle: Handle;
  from: XY;
};
export function SceneMaskCanvasOverlay({
  width,
  height,
}: {
  width: number;
  height: number;
}) {
  const stack = useSceneStack(),
    draft = stack?.draft,
    t = useSceneText(),
    ref = useRef<HTMLDivElement>(null),
    gesture = useRef<Gesture | null>(null),
    latest = useRef(stack);
  latest.current = stack;
  const [viewport, setViewport] = useState({ width: 0, height: 0 });
  const cancelGesture = () => {
    const g = gesture.current;
    gesture.current = null;
    if (!g) return;
    if (g.target.hasPointerCapture?.(g.pointerId))
      g.target.releasePointerCapture(g.pointerId);
    const current = latest.current;
    if (current?.draft?.scope !== g.scope) return;
    if (current.valid()) current.change(g.start);
    else current.cancel();
  };
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    const update = () => {
      const rect = node.getBoundingClientRect();
      setViewport({ width: rect.width, height: rect.height });
    };
    update();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(update);
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    cancelGesture();
    return () => cancelGesture();
  }, [
    draft?.scope,
    stack?.disabled,
    width,
    height,
    viewport.width,
    viewport.height,
  ]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || !gesture.current) return;
      event.preventDefault();
      event.stopPropagation();
      cancelGesture();
    };
    const cancel = () => cancelGesture();
    window.addEventListener("keydown", key, true);
    window.addEventListener("blur", cancel);
    window.addEventListener("pagehide", cancel);
    return () => {
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("blur", cancel);
      window.removeEventListener("pagehide", cancel);
      cancelGesture();
    };
  }, []);
  const fitted = canvasTextViewport({ width, height }, viewport),
    scale = fitted.width / width;
  const point = (event: ReactPointerEvent): XY => {
    const bounds = ref.current!.getBoundingClientRect();
    return {
      x: (event.clientX - bounds.left - fitted.left) / scale,
      y: (event.clientY - bounds.top - fitted.top) / scale,
    };
  };
  const mask = draft?.element.visual?.masks.find(
      (m) => m.id === draft.activeMaskId,
    ),
    outline =
      mask && draft?.geometry
        ? sceneMaskOverlay(mask, draft.geometry.quad)
        : null;
  const down = (event: ReactPointerEvent, handle: Handle) => {
    if (
      event.button !== 0 ||
      !stack?.valid() ||
      stack.disabled ||
      !draft?.geometry ||
      !mask ||
      !scale
    )
      return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    gesture.current = {
      pointerId: event.pointerId,
      target: event.currentTarget,
      scope: draft.scope,
      start: structuredClone(draft.element),
      maskId: mask.id,
      handle,
      from: point(event),
    };
  };
  const move = (event: ReactPointerEvent) => {
    const g = gesture.current,
      current = latest.current,
      value = current?.draft;
    if (!g || event.pointerId !== g.pointerId) return;
    event.preventDefault();
    event.stopPropagation();
    if (!current?.valid() || !value?.geometry || value.scope !== g.scope) {
      cancelGesture();
      return;
    }
    const start = g.start.visual!.masks.find((m) => m.id === g.maskId)!;
    const next = structuredClone(g.start),
      index = next.visual!.masks.findIndex((m) => m.id === g.maskId);
    next.visual!.masks[index] = dragSceneMask(
      start,
      value.geometry.quad,
      g.handle,
      g.from,
      point(event),
    );
    current.change(next);
  };
  const up = (event: ReactPointerEvent) => {
    if (gesture.current?.pointerId !== event.pointerId) return;
    move(event);
    const g = gesture.current;
    gesture.current = null;
    if (!g) return;
    if (g.target.hasPointerCapture?.(g.pointerId))
      g.target.releasePointerCapture(g.pointerId);
    if (latest.current?.draft?.scope === g.scope && latest.current.valid())
      void latest.current.save();
  };
  return (
    <div ref={ref} className="scene-mask-overlay">
      {draft && outline && scale > 0 && !stack?.disabled && (
        <svg
          aria-label={t("Mask canvas controls")}
          style={{
            left: fitted.left,
            top: fitted.top,
            width: fitted.width,
            height: fitted.height,
          }}
          viewBox={`0 0 ${width} ${height}`}
          onPointerMove={move}
          onPointerUp={up}
          onPointerCancel={cancelGesture}
          onLostPointerCapture={cancelGesture}
        >
          <path className="scene-mask-outline" d={outline.path} />
          <path
            data-handle="move"
            d={outline.path}
            fill="none"
            stroke="transparent"
            strokeWidth={12 / scale}
            onPointerDown={(event) => down(event, "move")}
          />
          {outline.lines.map(([a, b], i) => (
            <line
              key={i}
              x1={a.x}
              y1={a.y}
              x2={b.x}
              y2={b.y}
              className="scene-mask-outline"
            />
          ))}
          {outline.handles.map(({ handle, point, label }, index) => (
            <circle
              key={index}
              role="button"
              aria-label={t(label)}
              data-handle={typeof handle === "string" ? handle : handle.kind}
              className="scene-mask-handle"
              cx={point.x}
              cy={point.y}
              r={
                (typeof handle === "object" && handle.kind !== "anchor"
                  ? 3
                  : 5) / scale
              }
              onPointerDown={(event) => down(event, handle)}
            />
          ))}
        </svg>
      )}
    </div>
  );
}
