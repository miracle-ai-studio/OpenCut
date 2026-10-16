import type { ClassicAssPlan } from "./planner";
import { useClassicAssLocale } from "./locale";
export type ClassicAssImportMode = "classic" | "strict";
export interface ClassicAssImportOptionsProps {
  mode: ClassicAssImportMode;
  onModeChange(mode: ClassicAssImportMode): void;
  plan: ClassicAssPlan | null;
  onAcceptLosses?(): void;
}
/** A plan is read-only until the workspace's ordinary atomic apply action.
 * Warnings describe original Classic parser losses and stay reviewable. */
export function ClassicAssImportOptions({
  mode,
  onModeChange,
  plan,
  onAcceptLosses,
}: ClassicAssImportOptionsProps) {
  const { t, warning: translateWarning } = useClassicAssLocale();
  return (
    <fieldset className="v4-fieldset">
      <legend>{t("ASS import")}</legend>
      <label>
        <input
          type="radio"
          name="v4-ass-import-mode"
          checked={mode === "classic"}
          onChange={() => onModeChange("classic")}
        />
        {t("Classic compatibility")}
      </label>
      <label>
        <input
          type="radio"
          name="v4-ass-import-mode"
          checked={mode === "strict"}
          onChange={() => onModeChange("strict")}
        />
        {t("Strict static ASS")}
      </label>
      {mode === "classic" && (
        <p>
          {t(
            "Uses Classic styles and automatic wrapping. Review ignored features before saving.",
          )}
        </p>
      )}
      {plan && (
        <div role="status" aria-label={t("ASS import review")}>
          <p>
            {t(
              "{count} subtitle cues. {skipped} invalid or empty cues skipped.",
              { count: plan.cues.length, skipped: plan.skipped_cue_count },
            )}
          </p>
          {!!plan.warnings.length && (
            <ul>
              {plan.warnings.map((warning, index) => (
                <li key={index}>{translateWarning(warning)}</li>
              ))}
            </ul>
          )}
          {onAcceptLosses && (
            <button type="button" onClick={onAcceptLosses}>
              {t("Save reviewed subtitles")}
            </button>
          )}
        </div>
      )}
    </fieldset>
  );
}
