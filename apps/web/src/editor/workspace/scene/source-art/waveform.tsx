import { t, useLocale } from "../../../shared/locale";
import { useEffect, useRef, useState } from "react";
import type { DisplayArt } from "./service";

/** Classic colors and drawing operations. Geometry, gain and burn are facts
 * from the shared display operation, never recalculated in the browser. */
export function ClassicWaveform({ art }: { art: Extract<DisplayArt, { kind: "classic_waveform" }> }) {
  const { theme } = useLocale();
  const ref = useRef<HTMLCanvasElement>(null), [failed, setFailed] = useState(false);
  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    canvas.width = art.canvas.width; canvas.height = art.canvas.height;
    const context = canvas.getContext("2d");
    if (!context) { canvas.width = 0; canvas.height = 0; setFailed(true); return; }
    setFailed(false);
    context.setTransform(1, 0, 0, 1, 0, 0); context.clearRect(0, 0, canvas.width, canvas.height);
    for (const bar of art.bars) {
      if (!bar.device_height) continue;
      context.fillStyle = theme === "light" ? "rgba(33, 107, 100, 0.85)" : "rgba(255, 255, 255, 0.7)";
      context.fillRect(bar.device_left, bar.device_top, bar.device_right - bar.device_left, bar.device_height);
      if (bar.burn) { context.fillStyle = "rgba(255, 110, 20, 0.9)";
        context.fillRect(bar.device_left, bar.device_top, bar.device_right - bar.device_left, bar.burn_height); }
    }
    return () => { canvas.width = 0; canvas.height = 0; };
  }, [art, theme]);
  return <>{failed && <small aria-label="Source artwork unavailable">{t("Canvas display unavailable")}</small>}
    <canvas ref={ref} hidden={failed} role="img" aria-label="Classic element waveform" width={art.canvas.width} height={art.canvas.height}
    data-display-profile="classic-waveform-display-v1" data-display-bars={art.bars.length}
    data-burn-bars={art.bars.filter(bar => bar.burn).length} data-temporary-gain={art.temporaryGain ?? "saved"}
    style={{ position: "absolute", bottom: 0, left: art.canvas.css_left, width: art.canvas.css_width, height: art.canvas.css_height, pointerEvents: "none" }} /></>;
}
