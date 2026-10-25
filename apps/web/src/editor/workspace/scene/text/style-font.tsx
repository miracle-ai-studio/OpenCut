import { Select } from "../../../shared/controls";
import { Field } from "../../../shared/ui";
import { t, useLocale } from "../../../shared/locale";
import type { TextFont } from "./types";
import { requestedFontStyle, preferredFontVariant } from "./font-synthesis";

/** Variant controls select an authorized face or an explicitly proven Classic
 * synthesis binding. Native resource metadata remains its actual face. */
export function TextFontVariantFields({
  fontId,
  fonts,
  select,
}: {
  fontId: string;
  fonts: readonly TextFont[];
  select(fontId: string): void;
}) {
  useLocale();
  const font = fonts.find((item) => item.id === fontId);
  const variant = (weight: number, style: TextFont["style"]) =>
    font && preferredFontVariant(font, fonts, weight, style);
  return (
    <div className="field-pair">
      <Field label={t("Font weight")}>
        <Select
          disabled={!font}
          value={font?.weight ?? ""}
          onChange={(event) => {
            const next =
              font &&
              variant(Number(event.target.value), requestedFontStyle(font));
            if (next) select(next.id);
          }}
        >
          {!font && (
            <option value="">{t("Choose an imported font face")}</option>
          )}
          <option
            value={400}
            disabled={!font || !variant(400, requestedFontStyle(font))}
          >
            {t("Regular")}
          </option>
          <option
            value={700}
            disabled={!font || !variant(700, requestedFontStyle(font))}
          >
            {t("Bold")}
          </option>
        </Select>
      </Field>
      <Field label={t("Font style")}>
        <Select
          disabled={!font}
          value={font ? requestedFontStyle(font) : ""}
          onChange={(event) => {
            const next =
              font &&
              variant(font.weight, event.target.value as TextFont["style"]);
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
        </Select>
      </Field>
    </div>
  );
}
