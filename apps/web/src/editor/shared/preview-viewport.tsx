import { Select } from "./controls";
import { useEffect, useRef, useState } from "react";
import type { PointerEvent } from "react";
import {
  clampPreviewZoom,
  previewGeometry,
  previewZoomPresets,
} from "../model/viewport";
import { t, useLocale } from "./locale";
import { Button } from "./ui";
import { Minus, Plus, Scan } from "lucide-react";
export function usePreviewViewport(
  width: number,
  height: number,
  scope: string,
) {
  const ref = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [panning, setPanning] = useState(false);
  const panHeld = useRef(false);
  const drag = useRef<{
    id: number;
    x: number;
    y: number;
    pan: { x: number; y: number };
  } | null>(null);
  const geometry = previewGeometry(
    width,
    height,
    size.width,
    size.height,
    zoom,
    pan,
  );
  const releasePan = () => {
    panHeld.current = false;
    const active = drag.current;
    drag.current = null;
    if (active && ref.current?.hasPointerCapture?.(active.id))
      ref.current.releasePointerCapture(active.id);
    setPanning(false);
  };
  useEffect(() => {
    setZoom(1);
    setPan({ x: 0, y: 0 });
    drag.current = null;
    panHeld.current = false;
    setPanning(false);
  }, [scope]);
  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    const measure = () =>
      setSize({ width: element.clientWidth, height: element.clientHeight });
    const observer =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(measure);
    observer?.observe(element);
    measure();
    const release = () => {
      panHeld.current = false;
      const active = drag.current;
      drag.current = null;
      if (active && element.hasPointerCapture?.(active.id))
        element.releasePointerCapture(active.id);
      setPanning(false);
    };
    window.addEventListener("blur", release);
    return () => {
      observer?.disconnect();
      release();
      window.removeEventListener("blur", release);
    };
  }, []);
  const fit = () => {
    setZoom(1);
    setPan({ x: 0, y: 0 });
  };
  const change = (next: number) => {
    setZoom(clampPreviewZoom(next));
    setPan(geometry.pan);
  };
  const down = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 1 && !(event.button === 0 && panHeld.current)) return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.focus({ preventScroll: true });
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = {
      id: event.pointerId,
      x: event.clientX,
      y: event.clientY,
      pan: geometry.pan,
    };
    setPanning(true);
  };
  const move = (event: PointerEvent<HTMLDivElement>) => {
    const active = drag.current;
    if (!active || event.pointerId !== active.id) return;
    event.stopPropagation();
    setPan(
      previewGeometry(width, height, size.width, size.height, zoom, {
        x: active.pan.x + event.clientX - active.x,
        y: active.pan.y + event.clientY - active.y,
      }).pan,
    );
  };
  const end = (event: PointerEvent<HTMLDivElement>) => {
    if (drag.current?.id !== event.pointerId) return;
    drag.current = null;
    setPanning(false);
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
  };
  return {
    holdPan: () => {
      panHeld.current = true;
    },
    releasePan,
    ref,
    geometry,
    zoom,
    fit,
    change,
    panning,
    events: {
      onPointerDownCapture: down,
      onPointerMoveCapture: move,
      onPointerUpCapture: end,
      onPointerCancel: end,
      onLostPointerCapture: end,
    },
  };
}
export function ViewportControls({
  viewport,
}: {
  viewport: ReturnType<typeof usePreviewViewport>;
}) {
  useLocale();
  const percent = Math.round(viewport.geometry.scale * 100);
  const value = Math.abs(viewport.zoom - 1) < 0.001 ? "fit" : String(percent);
  return (
    <div className="toolbar viewport-controls">
      <Button
        aria-label={t("Zoom preview out")}
        disabled={viewport.zoom <= 0.25}
        icon={<Minus size={12} />}
        onClick={() => viewport.change(viewport.zoom / 1.25)}
      />
      <Select
        aria-label={t("Preview zoom")}
        value={value}
        onChange={(event) =>
          event.target.value === "fit"
            ? viewport.fit()
            : viewport.change(
                Number(event.target.value) / 100 / viewport.geometry.fit,
              )
        }
      >
        <option value="fit">{t("Fit")}</option>
        {!previewZoomPresets.includes(percent) && value !== "fit" && (
          <option value={percent}>{percent}%</option>
        )}
        {previewZoomPresets.map((preset) => (
          <option key={preset} value={preset}>
            {preset}%
          </option>
        ))}
      </Select>
      <Button
        aria-label={t("Zoom preview in")}
        disabled={viewport.zoom >= 16}
        icon={<Plus size={12} />}
        onClick={() => viewport.change(viewport.zoom * 1.25)}
      />
      <Button
        aria-label={t("Fit preview")}
        icon={<Scan size={13} />}
        onClick={viewport.fit}
      />
    </div>
  );
}
