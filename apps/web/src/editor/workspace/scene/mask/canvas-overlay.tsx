import { useEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import { t } from "../../../shared/locale";
import { canvasTextViewport } from "../text/canvas-geometry";
import { useMaskAuthoring } from "./workspace";
import { dragMask, maskOutline } from "./geometry";
import type { BoxMask, MaskHandle, Point } from "./types";

type Gesture = {
  key: number;
  pointerId: number;
  target: Element;
  handle: MaskHandle;
  start: BoxMask;
  from: Point;
  next: BoxMask;
};

/** Only an application-authorized logical outline is interactive. Source media
 * and final scene pixels stay owned by the normal preview/rendering pipeline. */
export function CanvasMaskOverlay({
  width,
  height,
}: {
  width: number;
  height: number;
}) {
  const authoring = useMaskAuthoring(),
    draft = authoring?.draft;
  const ref = useRef<HTMLDivElement>(null),
    drag = useRef<Gesture | null>(null);
  const latest = useRef(authoring);
  latest.current = authoring;
  const [viewport, setViewport] = useState({ width: 0, height: 0 });
  const cancelGesture = () => {
    const gesture = drag.current;
    drag.current = null;
    if (!gesture) return;
    if (gesture.target.hasPointerCapture?.(gesture.pointerId))
      gesture.target.releasePointerCapture(gesture.pointerId);
    if (latest.current?.draft?.key !== gesture.key) return;
    if (latest.current.valid()) latest.current.change(gesture.start);
    else latest.current.cancel();
  };
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    const update = () => {
      const bounds = node.getBoundingClientRect();
      setViewport({ width: bounds.width, height: bounds.height });
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
    draft?.key,
    authoring?.disabled,
    width,
    height,
    viewport.width,
    viewport.height,
  ]);
  useEffect(() => {
    const cancel = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || !latest.current?.draft) return;
      event.preventDefault();
      event.stopPropagation();
      cancelGesture();
      latest.current.cancel();
    };
    window.addEventListener("keydown", cancel, true);
    return () => {
      window.removeEventListener("keydown", cancel, true);
      cancelGesture();
    };
  }, []);
  const fitted = canvasTextViewport({ width, height }, viewport),
    scale = fitted.width / width;
  const point = (event: ReactPointerEvent): Point => {
    const bounds = ref.current!.getBoundingClientRect();
    return {
      x: (event.clientX - bounds.left - fitted.left) / scale,
      y: (event.clientY - bounds.top - fitted.top) / scale,
    };
  };
  const down = (event: ReactPointerEvent, handle: MaskHandle) => {
    if (
      event.button !== 0 ||
      !draft ||
      !authoring ||
      authoring.disabled ||
      !authoring.valid() ||
      drag.current
    )
      return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = {
      key: draft.key,
      pointerId: event.pointerId,
      target: event.currentTarget,
      handle,
      start: structuredClone(draft.mask),
      next: structuredClone(draft.mask),
      from: point(event),
    };
  };
  const move = (event: ReactPointerEvent) => {
    const gesture = drag.current;
    if (
      !gesture ||
      event.pointerId !== gesture.pointerId ||
      !draft ||
      gesture.key !== draft.key
    )
      return;
    event.preventDefault();
    event.stopPropagation();
    if (!authoring?.valid()) {
      cancelGesture();
      return;
    }
    gesture.next = dragMask(
      gesture.start,
      draft.plan.quad,
      gesture.handle,
      gesture.from,
      point(event),
    );
    authoring.change(gesture.next);
  };
  const up = (event: ReactPointerEvent) => {
    const gesture = drag.current;
    if (!gesture || event.pointerId !== gesture.pointerId) return;
    move(event);
    drag.current = null;
    if (gesture.target.hasPointerCapture?.(gesture.pointerId))
      gesture.target.releasePointerCapture(gesture.pointerId);
    if (
      authoring?.valid() &&
      draft?.key === gesture.key &&
      JSON.stringify(gesture.next) !== JSON.stringify(gesture.start)
    )
      void authoring.save(gesture.next);
  };
  const outline =
    draft && scale > 0 ? maskOutline(draft.mask, draft.plan.quad, scale) : null;
  return (
    <div ref={ref} className="editor-mask-overlay">
      {draft && outline && !authoring?.disabled && (
        <svg
          aria-label={t("Mask canvas controls")}
          role="group"
          width={fitted.width}
          height={fitted.height}
          viewBox={`0 0 ${width} ${height}`}
          style={{ left: fitted.left, top: fitted.top }}
          onPointerMove={move}
          onPointerUp={up}
          onPointerCancel={cancelGesture}
          onLostPointerCapture={cancelGesture}
        >
          <polygon
            className="editor-mask-outline"
            points={outline.points.map((p) => `${p.x},${p.y}`).join(" ")}
            role="button"
            aria-label={t("Move mask")}
            onPointerDown={(event) => down(event, "move")}
          />
          {outline.handles.map((handle) => (
            <circle
              key={handle.id}
              className={`editor-mask-handle v4-mask-handle-${handle.id}`}
              role="button"
              aria-label={t(handle.label)}
              cx={handle.point.x}
              cy={handle.point.y}
              r={5 / scale}
              onPointerDown={(event) => down(event, handle.id)}
            />
          ))}
        </svg>
      )}
    </div>
  );
}
