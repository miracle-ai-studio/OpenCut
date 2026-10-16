import { useRef, useState } from "react";
import { useWorkspace } from "../../context";
import { t, useLocale } from "../../../shared/locale";
import { fontRecoveryBlocked } from "../text/font-recovery";
import type { V4Document } from "../types";
type Settings = V4Document["settings"];
const number = (value: string) => {
  if (!/^\d+$/.test(value) || Number(value) < 1 || Number(value) > 4294967295)
    throw new Error("Enter positive 32-bit dimensions and frame-rate values.");
  return Number(value);
};
export function settingsPatch(
  source: Settings,
  sequenceId: string,
  width: string,
  height: string,
  numerator: string,
  denominator: string,
  mode: "preset" | "custom",
): Settings {
  const next = structuredClone(source),
    size = { width: number(width), height: number(height) },
    fps = { numerator: number(numerator), denominator: number(denominator) };
  if (next.profile === "classic_global") {
    next.settings = {
      ...next.settings,
      canvas_size: size,
      fps,
      canvas_size_mode: mode,
      ...(mode === "custom" ? { last_custom_canvas_size: size } : {}),
    };
  } else {
    const format = next.formats.find((f) => f.sequence_id === sequenceId);
    if (!format) throw new Error("The scene format changed.");
    format.canvas_size = size;
    format.fps = fps;
  }
  return next;
}
export function V4ProjectSettingsEditor() {
  const { api, v4 } = useWorkspace();
  return (
    <SettingsForm
      key={`${api?.connectionIdentity}/${v4?.head.project.id}/${v4?.head.revision}/${v4?.sequenceId}`}
    />
  );
}
function SettingsForm() {
  useLocale();
  const { api, v4, controller, busy } = useWorkspace();
  const [captured] = useState(() => ({
    api,
    projectId: v4?.head.project.id,
    revision: v4?.head.revision,
    sequenceId: v4?.sequenceId,
    settings: v4 ? structuredClone(v4.head.project.settings) : null,
  }));
  const source = captured.settings,
    format =
      source?.profile === "classic_global"
        ? source.settings
        : source?.formats.find((f) => f.sequence_id === captured.sequenceId);
  const background = format?.background;
  const [backgroundKind,setBackgroundKind] = useState(background?.kind ?? "color"),
    [backgroundDirty,setBackgroundDirty] = useState(false),
    [backgroundColor,setBackgroundColor] = useState(background?.kind === "color" ? `#${[background.color.red,background.color.green,background.color.blue].map(v=>Math.round(v*255).toString(16).padStart(2,"0")).join("")}` : "#000000"),
    [backgroundAlpha,setBackgroundAlpha] = useState(String(background?.kind === "color" ? background.color.alpha : 1)),
    [blurIntensity,setBlurIntensity] = useState(String(background?.kind === "main_track_blur" ? background.intensity : 10));
  const [width, setWidth] = useState(String(format?.canvas_size.width ?? 1920)),
    [height, setHeight] = useState(String(format?.canvas_size.height ?? 1080)),
    [numerator, setNumerator] = useState(String(format?.fps.numerator ?? 30)),
    [denominator, setDenominator] = useState(
      String(format?.fps.denominator ?? 1),
    ),
    [mode, setMode] = useState<"preset" | "custom">(
      source?.profile === "classic_global"
        ? source.settings.canvas_size_mode
        : "custom",
    ),
    [pending, setPending] = useState(false),
    [error, setError] = useState<string | null>(null);
  const flight = useRef(false);
  const blocked =
    busy ||
    pending ||
    !v4 ||
    !!v4.historical ||
    !!v4.gainDraft ||
    !!v4.recovery?.record ||
    !!v4.recovery?.error ||
    fontRecoveryBlocked(v4?.fontRecovery) ||
    controller.v4.captionRecovery.blocked() ||
    ["loading", "playing", "stalled", "unconfirmed"].includes(
      v4?.audioPreview.phase ?? "",
    ) ||
    ["applying", "unconfirmed"].includes(
      v4?.timelineEdit.attempt?.phase ?? "",
    ) ||
    ["applying", "unconfirmed"].includes(v4?.clipboard.attempt?.phase ?? "");
  const fence = () => {
    const state = controller.getSnapshot();
    if (
      state.api !== captured.api ||
      !state.v4 ||
      state.v4.head.project.id !== captured.projectId ||
      state.v4.head.revision !== captured.revision ||
      state.v4.sequenceId !== captured.sequenceId ||
      state.v4.historical
    )
      throw new Error("The project format changed. Reopen its settings.");
  };
  async function save() {
    if (blocked || flight.current || !source || !captured.sequenceId) return;
    flight.current = true;
    setPending(true);
    setError(null);
    try {
      fence();
      const settings = settingsPatch(
        source,
        captured.sequenceId,
        width,
        height,
        numerator,
        denominator,
        mode,
      );
      if (backgroundDirty) {
        const alpha=Number(backgroundAlpha),intensity=Number(blurIntensity);
        if (!backgroundAlpha.trim() || !Number.isFinite(alpha) || alpha<0 || alpha>1 || !blurIntensity.trim() || !Number.isFinite(intensity) || intensity<0 || !/^#[0-9a-f]{6}$/i.test(backgroundColor)) throw Error("Enter a valid background color, opacity and blur intensity.");
        const nextBackground = backgroundKind === "main_track_blur" ? {kind:"main_track_blur" as const,intensity} : {kind:"color" as const,color:{encoding:"srgb" as const,red:parseInt(backgroundColor.slice(1,3),16)/255,green:parseInt(backgroundColor.slice(3,5),16)/255,blue:parseInt(backgroundColor.slice(5,7),16)/255,alpha}};
        if(settings.profile === "classic_global") settings.settings.background=nextBackground;
        else settings.formats.find(f=>f.sequence_id === captured.sequenceId)!.background=nextBackground;
      }
      if (JSON.stringify(settings) === JSON.stringify(source)) return;
      await controller.v4.write("Edit project canvas and frame rate", [
        { type: "settings_set", settings },
      ]);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      flight.current = false;
      setPending(false);
    }
  }
  if (!source || !format) return null;
  return (
    <section
      className="v4-project-settings"
      aria-label={t("Canvas and frame rate")}
    >
      <h4>{t("Canvas and frame rate")}</h4>
      <fieldset disabled={blocked}>
        <label>
          {t("Canvas preset")}
          <select
            value=""
            onChange={(e) => {
              const [w, h] = e.target.value.split("x");
              if (w && h) {
                setWidth(w);
                setHeight(h);
                setMode("preset");
              }
            }}
          >
            <option value="">{t("Choose dimensions")}</option>
            <option value="1920x1080">1920 × 1080 · 16:9</option>
            <option value="1080x1920">1080 × 1920 · 9:16</option>
            <option value="1080x1080">1080 × 1080 · 1:1</option>
            <option value="1440x1080">1440 × 1080 · 4:3</option>
          </select>
        </label>
        <label>
          {t("Canvas width")}
          <input
            type="number"
            min="1"
            step="1"
            value={width}
            onChange={(e) => {
              setWidth(e.target.value);
              setMode("custom");
            }}
          />
        </label>
        <label>
          {t("Canvas height")}
          <input
            type="number"
            min="1"
            step="1"
            value={height}
            onChange={(e) => {
              setHeight(e.target.value);
              setMode("custom");
            }}
          />
        </label>
        {source.profile === "classic_global" && (
          <>
            <button
              type="button"
              onClick={() => {
                const size =
                  source.settings.last_custom_canvas_size ??
                  source.settings.canvas_size;
                setWidth(String(size.width));
                setHeight(String(size.height));
                setMode("custom");
              }}
            >
              {t("Restore last custom size")}
            </button>
            <button
              type="button"
              onClick={() => {
                const size =
                  source.settings.original_canvas_size ??
                  source.settings.canvas_size;
                setWidth(String(size.width));
                setHeight(String(size.height));
                setMode("preset");
              }}
            >
              {t("Use original size")}
            </button>
            {!source.settings.original_canvas_size && (
              <p className="muted">
                {t(
                  "No original size is recorded. Original size keeps the current canvas dimensions.",
                )}
              </p>
            )}
          </>
        )}
        <label>
          {t("Frame-rate numerator")}
          <input
            type="number"
            min="1"
            step="1"
            value={numerator}
            onChange={(e) => setNumerator(e.target.value)}
          />
        </label>
        <label>
          {t("Frame-rate denominator")}
          <input
            type="number"
            min="1"
            step="1"
            value={denominator}
            onChange={(e) => setDenominator(e.target.value)}
          />
        </label>
        <label>{t("Background")}<select value={backgroundKind} onChange={e=>{setBackgroundKind(e.target.value as typeof backgroundKind);setBackgroundDirty(true);}}><option value="color">{t("Solid color")}</option><option value="main_track_blur">{t("Blur main track")}</option></select></label>
        {backgroundKind === "color" ? <><label>{t("Background color")}<input type="color" value={backgroundColor} onChange={e=>{setBackgroundColor(e.target.value);setBackgroundDirty(true);}} /></label><label>{t("Background opacity")}<input type="number" min="0" max="1" step="0.01" value={backgroundAlpha} onChange={e=>{setBackgroundAlpha(e.target.value);setBackgroundDirty(true);}} /></label></> : <label>{t("Background blur intensity")}<input type="number" min="0" step="1" value={blurIntensity} onChange={e=>{setBlurIntensity(e.target.value);setBackgroundDirty(true);}} /></label>}
        <p className="muted">
          {t(
            "Frame-rate changes preserve exact authored times. Playback and export use the saved frame rate.",
          )}
        </p>
        <button type="button" onClick={() => void save()}>
          {t("Save canvas and frame rate")}
        </button>
      </fieldset>
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
