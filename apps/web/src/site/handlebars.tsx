import { useEffect, useLayoutEffect, useRef, useState } from "react";
export function Handlebars({
  children,
  label,
}: {
  children: string;
  label: string;
}) {
  const box = useRef<HTMLSpanElement>(null),
    drag = useRef<{
      side: 0 | 1;
      start: number;
      positions: [number, number];
    } | null>(null);
  const [positions, setPositions] = useState<[number, number]>([0, 0]);
  const current = useRef(positions);
  current.current = positions;
  useLayoutEffect(() => {
    const element = box.current;
    if (!element) return;
    const resize = new ResizeObserver(() =>
      setPositions([0, element.clientWidth]),
    );
    resize.observe(element);
    return () => resize.disconnect();
  }, []);
  useEffect(() => {
    function move(event: PointerEvent) {
      if (!drag.current || !box.current) return;
      const next: [number, number] = [...drag.current.positions];
      const side = drag.current.side;
      next[side] = Math.max(
        side ? next[0] + 60 : 0,
        Math.min(
          side ? box.current.clientWidth : next[1] - 60,
          drag.current.positions[side] + event.clientX - drag.current.start,
        ),
      );
      setPositions(next);
    }
    function stop() {
      drag.current = null;
    }
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
    };
  }, []);
  return (
    <span className="site-handlebars" ref={box}>
      <span
        className="site-hero-clip"
        style={{
          clipPath: `inset(0 ${Math.max(0, (box.current?.clientWidth ?? positions[1]) - positions[1])}px 0 ${positions[0]}px)`,
        }}
      >
        {children}
      </span>
      <span
        className="site-handle-outline"
        style={{ left: positions[0], width: positions[1] - positions[0] }}
      />
      {([0, 1] as const).map((side) => (
        <button
          key={side}
          className="site-handle"
          style={{ left: positions[side] - 14 }}
          aria-label={`${label} ${side ? "→" : "←"}`}
          role="slider"
          aria-valuemin={side ? positions[0] + 60 : 0}
          aria-valuemax={
            side
              ? (box.current?.clientWidth ?? positions[1])
              : positions[1] - 60
          }
          aria-valuenow={Math.round(positions[side])}
          onPointerDown={(event) => {
            drag.current = {
              side,
              start: event.clientX,
              positions: [...current.current],
            };
            event.currentTarget.setPointerCapture(event.pointerId);
          }}
          onKeyDown={(event) => {
            if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key))
              return;
            event.preventDefault();
            const next: [number, number] = [...positions],
              min = side ? positions[0] + 60 : 0,
              max = side
                ? (box.current?.clientWidth ?? positions[1])
                : positions[1] - 60;
            next[side] =
              event.key === "Home"
                ? min
                : event.key === "End"
                  ? max
                  : Math.max(
                      min,
                      Math.min(
                        max,
                        next[side] + (event.key === "ArrowRight" ? 10 : -10),
                      ),
                    );
            setPositions(next);
          }}
        >
          <span />
        </button>
      ))}
    </span>
  );
}
