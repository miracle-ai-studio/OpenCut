import { t, useLocale, parameterLabel } from "../shared/locale";
import { useState } from "react";
import { BezierGraph, type NormalizedCubicBezier } from "./bezier-graph";
import { Trash2 } from "lucide-react";
import type { Clip } from "../model/types";
import { useWorkspace } from "../workspace/context";
import { Button, Empty, Field } from "../shared/ui";
import { fromSeconds, secondsForDisplay, compareTime } from "../model/time";
const easings = {
  Linear: { x1: 0, y1: 0, x2: 1, y2: 1 },
  "Ease in": { x1: 0.42, y1: 0, x2: 1, y2: 1 },
  "Ease out": { x1: 0, y1: 0, x2: 0.58, y2: 1 },
  "Ease in out": { x1: 0.42, y1: 0, x2: 0.58, y2: 1 },
};
export function KeyframeEditor({
  clip,
  trackId,
  sequenceId,
  readonly,
}: {
  clip: Clip;
  trackId: string;
  sequenceId: string;
  readonly: boolean;
}) {
  useLocale();
  const { controller, snapshot } = useWorkspace();
  const [custom, setCustom] = useState<{
    parameter: string;
    keyframeId: string;
    curve: NormalizedCubicBezier;
  } | null>(null);
  const [parameter, setParameter] = useState("opacity"),
    [time, setTime] = useState(0),
    [value, setValue] = useState(1);
  const save = () => {
    const channel = clip.channels?.find(
      (channel) => channel.parameter === parameter,
    );
    const existing = channel?.keyframes.find(
      (frame) => compareTime(frame.time, fromSeconds(time)) === 0,
    );
    if (existing?.value === value && existing.interpolation === "linear")
      return;
    void controller
      .mutate("Set animation keyframe", [
        {
          type: "keyframe.channel_set",
          sequence_id: sequenceId,
          track_id: trackId,
          clip_id: clip.id,
          channel: {
            parameter,
            keyframes: [
              ...(channel?.keyframes ?? []).filter(
                (frame) => compareTime(frame.time, fromSeconds(time)) !== 0,
              ),
              {
                id:
                  channel?.keyframes.find(
                    (frame) => compareTime(frame.time, fromSeconds(time)) === 0,
                  )?.id ?? crypto.randomUUID(),
                time: fromSeconds(time),
                value,
                interpolation: "linear" as const,
              },
            ].sort(
              (a, b) => secondsForDisplay(a.time) - secondsForDisplay(b.time),
            ),
          },
        },
      ])
      .catch((error) => controller.report(error));
  };
  return (
    <fieldset disabled={readonly}>
      <legend>{t("Animation")}</legend>
      <Field label={t("Parameter")}>
        <select
          value={parameter}
          onChange={(event) => setParameter(event.target.value)}
        >
          {[
            "opacity",
            "volume",
            "position_x",
            "position_y",
            "rotation_degrees",
            "scale_x",
            "scale_y",
          ].map((name) => (
            <option key={name} value={name}>
              {parameterLabel(name)}
            </option>
          ))}
        </select>
      </Field>
      <div className="field-pair">
        <Field label={t("Clip time")}>
          <input
            type="number"
            min="0"
            step=".01"
            value={time}
            onChange={(event) => setTime(Number(event.target.value))}
          />
        </Field>
        <Field label={t("Value")}>
          <input
            type="number"
            step=".05"
            value={value}
            onChange={(event) => setValue(Number(event.target.value))}
          />
        </Field>
      </div>
      <Button onClick={save}>{t("Add keyframe")}</Button>
      {!clip.channels?.length && (
        <Empty title={t("No animation yet")}>
          {t("Add keyframes to animate a property over time.")}
        </Empty>
      )}
      {clip.channels?.map((channel) => (
        <div className="keyframe-channel" key={channel.parameter}>
          <strong>{parameterLabel(channel.parameter)}</strong>
          <Button
            aria-label={t("Remove {value0} animation", {
              value0: channel.parameter,
            })}
            icon={<Trash2 size={12} />}
            onClick={() =>
              void controller
                .mutate(
                  "Remove animation",
                  [
                    {
                      type: "keyframe.channel_delete",
                      sequence_id: sequenceId,
                      track_id: trackId,
                      clip_id: clip.id,
                      parameter: channel.parameter,
                    },
                  ],
                  (snapshot?.curves ?? [])
                    .filter(
                      (curve) =>
                        curve.clip_id === clip.id &&
                        curve.parameter === channel.parameter,
                    )
                    .map((curve) => ({
                      type: "keyframe.curve_delete",
                      sequence_id: sequenceId,
                      id: curve.id,
                    })),
                )
                .catch((error) => controller.report(error))
            }
          />
          {channel.keyframes.map((keyframe, index) => (
            <div className="keyframe-row" key={keyframe.id}>
              <span>
                {secondsForDisplay(keyframe.time).toFixed(2)}
                {t("s")}
              </span>
              <span>{String(keyframe.value)}</span>
              <select
                aria-label={t("Interpolation at {value0} seconds", {
                  value0: secondsForDisplay(keyframe.time),
                })}
                value={keyframe.interpolation}
                onChange={(event) =>
                  void controller
                    .mutate(
                      "Change interpolation",
                      [
                        {
                          type: "keyframe.channel_set",
                          sequence_id: sequenceId,
                          track_id: trackId,
                          clip_id: clip.id,
                          channel: {
                            ...channel,
                            keyframes: channel.keyframes.map((item) =>
                              item.id === keyframe.id
                                ? {
                                    ...item,
                                    interpolation: event.target.value as
                                      "linear" | "hold",
                                  }
                                : item,
                            ),
                          },
                        },
                      ],
                      event.target.value === "hold"
                        ? (snapshot?.curves ?? [])
                            .filter(
                              (curve) =>
                                curve.clip_id === clip.id &&
                                curve.parameter === channel.parameter &&
                                curve.keyframe_id === keyframe.id,
                            )
                            .map((curve) => ({
                              type: "keyframe.curve_delete",
                              sequence_id: sequenceId,
                              id: curve.id,
                            }))
                        : [],
                    )
                    .catch((error) => controller.report(error))
                }
              >
                <option value="linear">{t("Linear")}</option>
                <option value="hold">{t("Hold")}</option>
              </select>
              {snapshot?.format === "visual" &&
                index + 1 < channel.keyframes.length &&
                keyframe.interpolation === "linear" && (
                  <Button
                    aria-label={t("Custom curve")}
                    onClick={() => {
                      const saved = snapshot.curves.find(
                        (curve) =>
                          curve.clip_id === clip.id &&
                          curve.keyframe_id === keyframe.id &&
                          curve.parameter === channel.parameter,
                      )?.curve;
                      setCustom({
                        parameter: channel.parameter,
                        keyframeId: keyframe.id,
                        curve: saved
                          ? [saved.x1, saved.y1, saved.x2, saved.y2]
                          : [0.42, 0, 0.58, 1],
                      });
                    }}
                  >
                    ⌁
                  </Button>
                )}
              {snapshot?.format === "visual" &&
                index + 1 < channel.keyframes.length &&
                keyframe.interpolation === "linear" && (
                  <select
                    aria-label={t("Curve at {value0} seconds", {
                      value0: secondsForDisplay(keyframe.time),
                    })}
                    defaultValue=""
                    onChange={(event) => {
                      const curve =
                        easings[event.target.value as keyof typeof easings];
                      if (curve)
                        void controller
                          .mutate(
                            "Set easing curve",
                            [],
                            [
                              {
                                type: "keyframe.curve_set",
                                sequence_id: sequenceId,
                                curve: {
                                  id: `curve-${clip.id}-${channel.parameter}-${keyframe.id}`,
                                  schema_version: 1,
                                  sequence_id: sequenceId,
                                  clip_id: clip.id,
                                  parameter: channel.parameter,
                                  keyframe_id: keyframe.id,
                                  curve,
                                },
                              },
                            ],
                          )
                          .catch((error) => controller.report(error));
                    }}
                  >
                    <option value="">{t("Easing…")}</option>
                    {Object.keys(easings).map((name) => (
                      <option key={name} value={name}>
                        {parameterLabel(name)}
                      </option>
                    ))}
                  </select>
                )}
            </div>
          ))}
        </div>
      ))}
      {custom && (
        <div className="curve-editor">
          <strong>{t("Custom curve")}</strong>
          <BezierGraph
            value={custom.curve}
            onChange={(curve) => setCustom({ ...custom, curve })}
          />
          <div className="curve-values">
            {custom.curve.map((value, index) => (
              <Field label={["X1", "Y1", "X2", "Y2"][index]} key={index}>
                <input
                  type="number"
                  min={0}
                  max={1}
                  step=".01"
                  value={value}
                  onChange={(e) => {
                    const curve: NormalizedCubicBezier = [...custom.curve];
                    curve[index] = Number(e.target.value);
                    setCustom({ ...custom, curve });
                  }}
                />
              </Field>
            ))}
          </div>
          <Button
            onClick={() =>
              void controller
                .mutate(
                  "Set easing curve",
                  [],
                  [
                    {
                      type: "keyframe.curve_set",
                      sequence_id: sequenceId,
                      curve: {
                        id: `curve-${clip.id}-${custom.parameter}-${custom.keyframeId}`,
                        schema_version: 1,
                        sequence_id: sequenceId,
                        clip_id: clip.id,
                        parameter: custom.parameter,
                        keyframe_id: custom.keyframeId,
                        curve: {
                          x1: custom.curve[0],
                          y1: custom.curve[1],
                          x2: custom.curve[2],
                          y2: custom.curve[3],
                        },
                      },
                    },
                  ],
                )
                .then(() => setCustom(null))
                .catch((error) => controller.report(error))
            }
          >
            {t("Apply curve")}
          </Button>
        </div>
      )}
    </fieldset>
  );
}
