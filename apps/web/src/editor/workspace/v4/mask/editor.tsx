import { t, useLocale } from "../../../shared/locale";
import { Field } from "../../../shared/ui";
import type { BoxGeometry, BoxMask } from "./types";

export function V4MaskEditor({
  mask,
  pending,
  error,
  change,
  save,
  cancel,
}: {
  mask: BoxMask;
  pending: boolean;
  error: string | null;
  change(mask: BoxMask): void;
  save(): void;
  cancel(): void;
}) {
  useLocale();
  const fields: {
    key: keyof BoxGeometry;
    label: string;
    percent?: boolean;
    min?: number;
    max?: number;
  }[] = [
    { key: "center_x", label: "Mask X (%)", percent: true },
    { key: "center_y", label: "Mask Y (%)", percent: true },
    { key: "width", label: "Mask width (%)", percent: true, min: 0.000001 },
    { key: "height", label: "Mask height (%)", percent: true, min: 0.000001 },
    {
      key: "rotation_degrees",
      label: "Mask rotation (degrees)",
    },
  ];
  return (
    <form
      className="v4-mask-editor"
      aria-label={t("Edit mask")}
      onSubmit={(event) => {
        event.preventDefault();
        save();
      }}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === "Escape") {
          event.preventDefault();
          cancel();
        }
      }}
    >
      <h3>
        {t(mask.shape.kind === "rectangle" ? "Rectangle mask" : "Ellipse mask")}
      </h3>
      <p>
        {t(
          "Edit the mask values or drag its outline and handles on the canvas.",
        )}
      </p>
      <fieldset disabled={pending}>
        {fields.map((field) => (
          <Field key={field.key} label={t(field.label)}>
            <input
              type="number"
              step="any"
              required
              min={field.min}
              max={field.max}
              value={mask.shape.geometry[field.key] * (field.percent ? 100 : 1)}
              onChange={(event) => {
                if (
                  event.target.value === "" ||
                  !Number.isFinite(event.target.valueAsNumber)
                )
                  return;
                const next = structuredClone(mask);
                next.shape.geometry[field.key] =
                  event.target.valueAsNumber / (field.percent ? 100 : 1);
                change(next);
              }}
            />
          </Field>
        ))}
        <Field label={t("Mask feather (pixels)")}>
          <input
            type="number"
            step="any"
            required
            min="0"
            max="1000"
            value={mask.feather.value}
            onChange={(event) => {
              if (
                event.target.value === "" ||
                !Number.isFinite(event.target.valueAsNumber)
              )
                return;
              change({
                ...mask,
                feather: { ...mask.feather, value: event.target.valueAsNumber },
              });
            }}
          />
        </Field>
      </fieldset>
      {error && <p role="alert">{t(error)}</p>}
      <div className="v4-mask-actions">
        <button type="submit" disabled={pending}>
          {t(pending ? "Saving mask…" : "Save mask")}
        </button>
        <button type="button" disabled={pending} onClick={cancel}>
          {t("Cancel")}
        </button>
      </div>
    </form>
  );
}
