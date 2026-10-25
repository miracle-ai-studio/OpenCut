import { ColorInput } from "../../../shared/controls";
import { Field } from "../../../shared/ui";
import { t, useLocale } from "../../../shared/locale";
import type { TextDraft } from "./draft";

type NumericBackgroundField =
  | "backgroundRadius"
  | "backgroundPaddingX"
  | "backgroundPaddingY"
  | "backgroundOffsetX"
  | "backgroundOffsetY";
const fields: readonly [NumericBackgroundField, string][] = [
  ["backgroundRadius", "Background corner radius (%)"],
  ["backgroundPaddingX", "Background horizontal padding (Classic size 15)"],
  ["backgroundPaddingY", "Background vertical padding (Classic size 15)"],
  ["backgroundOffsetX", "Background horizontal offset (pixels)"],
  ["backgroundOffsetY", "Background vertical offset (pixels)"],
];
export function TextBackgroundFields({
  draft,
  update,
}: {
  draft: TextDraft;
  update<K extends keyof TextDraft>(name: K, value: TextDraft[K]): void;
}) {
  useLocale();
  return (
    <fieldset>
      <legend>{t("Text background")}</legend>
      <label>
        <input
          type="checkbox"
          checked={draft.backgroundEnabled}
          onChange={(event) =>
            update("backgroundEnabled", event.target.checked)
          }
        />
        {t("Enable text background")}
      </label>
      <fieldset disabled={!draft.backgroundEnabled}>
        <div className="field-pair">
          <Field label={t("Background color")}>
            <ColorInput
              value={draft.backgroundColor}
              onValueChange={(value) => update("backgroundColor", value)}
            />
          </Field>
          <Field label={t("Background opacity (%)")}>
            <input
              required
              inputMode="decimal"
              value={draft.backgroundOpacity}
              onChange={(event) =>
                update("backgroundOpacity", event.target.value)
              }
            />
          </Field>
        </div>
        {fields.map(([name, label]) => (
          <Field key={name} label={t(label)}>
            <input
              required
              inputMode="decimal"
              value={draft[name]}
              onChange={(event) => update(name, event.target.value)}
            />
          </Field>
        ))}
      </fieldset>
    </fieldset>
  );
}
