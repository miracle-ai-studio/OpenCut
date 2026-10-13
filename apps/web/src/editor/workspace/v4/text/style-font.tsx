import { Field } from "../../../shared/ui";
import { t, useLocale } from "../../../shared/locale";
import type { V4TextFont } from "./types";

/** Variant controls select a real authorized face. Missing variants never become
 * browser-synthesized styles or a change to a locked font's authored identity. */
export function TextFontVariantFields({
  fontId,
  fonts,
  select,
}: {
  fontId: string;
  fonts: readonly V4TextFont[];
  select(fontId: string): void;
}) {
  useLocale();
  const font = fonts.find((item) => item.id === fontId);
  const variant = (weight: number, style: V4TextFont["style"]) =>
    font &&
    (font.weight === weight && font.style === style
      ? font
      : fonts.find(
          (item) =>
            item.family === font.family &&
            item.weight === weight &&
            item.style === style,
        ));
  return (
    <div className="field-pair">
      <Field label={t("Font weight")}>
        <select
          disabled={!font}
          value={font?.weight ?? ""}
          onChange={(event) => {
            const next =
              font && variant(Number(event.target.value), font.style);
            if (next) select(next.id);
          }}
        >
          {!font && (
            <option value="">{t("Choose an imported font face")}</option>
          )}
          <option value={400} disabled={!font || !variant(400, font.style)}>
            {t("Regular")}
          </option>
          <option value={700} disabled={!font || !variant(700, font.style)}>
            {t("Bold")}
          </option>
        </select>
      </Field>
      <Field label={t("Font style")}>
        <select
          disabled={!font}
          value={font?.style ?? ""}
          onChange={(event) => {
            const next =
              font &&
              variant(font.weight, event.target.value as V4TextFont["style"]);
            if (next) select(next.id);
          }}
        >
          {!font && (
            <option value="">{t("Choose an imported font face")}</option>
          )}
          <option
            value="normal"
            disabled={!font || !variant(font.weight, "normal")}
          >
            {t("Regular")}
          </option>
          <option
            value="italic"
            disabled={!font || !variant(font.weight, "italic")}
          >
            {t("Italic")}
          </option>
        </select>
      </Field>
    </div>
  );
}
