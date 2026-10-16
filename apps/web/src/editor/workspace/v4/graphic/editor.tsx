import { useRef, useState } from "react";
import { Button, Field } from "../../../shared/ui";
import { t, useLocale } from "../../../shared/locale";
import { graphicDraft, graphicDraftValues } from "./draft";
import type { GraphicDraft } from "./draft";
import type { GraphicInitial, GraphicValues } from "./types";
export function V4GraphicEditor({
  initial,
  disabled,
  save,
  close,
}: {
  initial: GraphicInitial;
  disabled: boolean;
  save(values: GraphicValues): Promise<void>;
  close(): void;
}) {
  useLocale();
  const [draft, setDraft] = useState<GraphicDraft>(() => graphicDraft(initial)),
    [error, setError] = useState<string | null>(null);
  const composing = useRef(false),
    flight = useRef(false);
  const field = (key: keyof GraphicDraft, label: string, type = "text") => (
    <Field label={t(label)}>
      <input
        type={type}
        value={draft[key]}
        onChange={(e) => setDraft({ ...draft, [key]: e.target.value })}
      />
    </Field>
  );
  return (
    <form
      aria-label={t("Edit graphic")}
      onCompositionStart={() => {
        composing.current = true;
      }}
      onCompositionEnd={() => {
        composing.current = false;
      }}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (
          e.key === "Escape" &&
          !e.nativeEvent.isComposing &&
          !composing.current
        ) {
          e.preventDefault();
          if (!flight.current && !disabled) close();
        }
      }}
      onSubmit={(e) => {
        e.preventDefault();
        if (disabled || flight.current || composing.current) return;
        flight.current = true;
        setError(null);
        void (async () => {
          try {
            await save(graphicDraftValues(initial, draft));
          } catch (value) {
            setError(value instanceof Error ? value.message : String(value));
          } finally {
            flight.current = false;
          }
        })();
      }}
    >
      <h3>
        {t("Edit graphic")} · {t(initial.graphic.geometry.kind)}
      </h3>
      {error && <p role="alert">{t(error)}</p>}
      <fieldset disabled={disabled}>
        {field("name", "Graphic name")}
        {field("start", "Start (seconds)")}
        {field("duration", "Duration (seconds)")}
        {field("width", "Graphic width (pixels)")}
        {field("height", "Graphic height (pixels)")}
        {"corner_radius_percent" in initial.graphic.geometry &&
          field("rounding", "Corner radius (percent)")}
        {initial.graphic.geometry.kind === "polygon" &&
          field("sides", "Polygon sides")}
        {initial.graphic.geometry.kind === "star" && (
          <>
            {field("points", "Star points")}
            {field("depth", "Star depth (percent)")}
          </>
        )}
        {field("fill", "Fill color", "color")}
        {field("fillAlpha", "Fill opacity (0–1)")}
        {field("stroke", "Stroke color", "color")}
        {field("strokeAlpha", "Stroke opacity (0–1)")}
        {field("strokeWidth", "Stroke width (pixels)")}
        <Field label={t("Stroke alignment")}>
          <select
            value={draft.strokeAlign}
            onChange={(e) =>
              setDraft({ ...draft, strokeAlign: e.target.value })
            }
          >
            {["inside", "center", "outside"].map((value) => (
              <option key={value} value={value}>
                {t(value)}
              </option>
            ))}
          </select>
        </Field>
        {field("x", "Horizontal center offset (pixels)")}
        {field("y", "Vertical center offset (pixels)")}
        {field("scaleX", "Horizontal scale (multiplier)")}
        {field("scaleY", "Vertical scale (multiplier)")}
        {field("rotation", "Rotation (degrees)")}
        {field("opacity", "Graphic opacity (0–1)")}
        <div className="toolbar">
          <Button type="submit">{t("Review graphic changes")}</Button>
          <Button type="button" onClick={close}>
            {t("Cancel")}
          </Button>
        </div>
      </fieldset>
    </form>
  );
}
