import { t, useLocale } from "../shared/locale";
import { useState } from "react";
import type { Sequence } from "../model/types";
import { useWorkspace } from "../workspace/context";
import { activeSequence } from "../model/types";
import { Button, Field, Panel } from "../shared/ui";
export function SequenceSettings({
  sequence,
  readonly,
}: {
  sequence: Sequence;
  readonly: boolean;
}) {
  useLocale();
  const { controller } = useWorkspace();
  const [width, setWidth] = useState(sequence.width),
    [height, setHeight] = useState(sequence.height),
    [rate, setRate] = useState(
      `${sequence.fps.numerator}/${sequence.fps.denominator}`,
    );
  return (
    <fieldset disabled={readonly}>
      <legend>{t("Sequence settings")}</legend>
      <div className="field-pair">
        <Field label={t("Width")}>
          <input
            type="number"
            min="16"
            max="8192"
            value={width}
            onChange={(event) => setWidth(Number(event.target.value))}
          />
        </Field>
        <Field label={t("Height")}>
          <input
            type="number"
            min="16"
            max="8192"
            value={height}
            onChange={(event) => setHeight(Number(event.target.value))}
          />
        </Field>
      </div>
      <Field label={t("Frame rate")}>
        <select value={rate} onChange={(event) => setRate(event.target.value)}>
          {[
            "24/1",
            "24000/1001",
            "25/1",
            "30/1",
            "30000/1001",
            "50/1",
            "60/1",
            "60000/1001",
          ].map((value) => (
            <option key={value} value={value}>
              {(
                Number(value.split("/")[0]) / Number(value.split("/")[1])
              ).toFixed(3)}{" "}
              {t("fps")}
            </option>
          ))}
        </select>
      </Field>
      <Button
        onClick={() => {
          const [numerator, denominator] = rate.split("/").map(Number);
          void controller
            .mutate("Sequence settings", [
              {
                type: "sequence.update_settings",
                sequence_id: sequence.id,
                width,
                height,
                fps: { numerator, denominator },
              },
            ])
            .catch((error) => controller.report(error));
        }}
      >
        {t("Apply settings")}
      </Button>
    </fieldset>
  );
}

export function SettingsPanel() {
  useLocale();
  const { snapshot, historical, sequenceId, busy } = useWorkspace();
  const sequence = activeSequence(historical ?? snapshot, sequenceId);
  return (
    <Panel title={t("Settings")}>
      <div className="padded scroll">
        {sequence && (
          <SequenceSettings
            key={`${sequence.id}-${sequence.width}-${sequence.height}`}
            sequence={sequence}
            readonly={busy || !!historical}
          />
        )}
      </div>
    </Panel>
  );
}
