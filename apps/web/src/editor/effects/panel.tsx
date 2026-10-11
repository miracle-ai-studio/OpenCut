import previewImage from "./preview.jpg";
import { useEffect, useState } from "react";
import { Sparkles, Trash2 } from "lucide-react";
import { useWorkspace } from "../workspace/context";
import { activeSequence } from "../model/types";
import { addTime, compareTime, fromSeconds } from "../model/time";
import { Button, Empty, Field, Panel } from "../shared/ui";
import { t, useLocale } from "../shared/locale";
export function EffectsPanel({
  mode,
}: {
  mode: "effects" | "transitions" | "adjustment";
}) {
  useLocale();
  const { controller, snapshot, historical, sequenceId, selection, busy } =
    useWorkspace();
  const sequence = activeSequence(historical ?? snapshot, sequenceId);
  const target =
    selection?.kind === "clip" || selection?.kind === "shape"
      ? selection.id
      : null;
  const graph = (historical ?? snapshot)?.visual.find(
    (graph) => graph.sequence_id === sequenceId,
  );
  const targetTrack = sequence?.tracks.find(
    (track) =>
      track.clips.some((clip) => clip.id === target) ||
      graph?.shapes.some(
        (shape) => shape.id === target && shape.track_id === track.id,
      ),
  );
  const [radius, setRadius] = useState(12),
    [duration, setDuration] = useState(0.5);
  const [exposure, setExposure] = useState(0),
    [contrast, setContrast] = useState(1),
    [saturation, setSaturation] = useState(1);
  const effect = graph?.effects.find(
    (effect) =>
      effect.target_id === target &&
      effect.effect.kind ===
        (mode === "effects" ? "gaussian_blur" : "color_adjust"),
  );
  const outgoing = sequence?.tracks
    .flatMap((track) => track.clips)
    .find((clip) => clip.id === target);
  const incoming =
    outgoing &&
    sequence?.tracks
      .find((track) => track.clips.includes(outgoing))
      ?.clips.find(
        (clip) =>
          clip.id !== target &&
          compareTime(
            clip.timeline_range.start,
            addTime(
              outgoing.timeline_range.start,
              outgoing.timeline_range.duration,
            ),
          ) === 0,
      );
  const transition = graph?.transitions.find(
    (item) => item.outgoing_clip_id === target,
  );
  useEffect(() => {
    if (effect?.effect.kind === "gaussian_blur")
      setRadius(effect.effect.radius_px);
    else if (effect?.effect.kind === "color_adjust") {
      setExposure(effect.effect.exposure_ev);
      setContrast(effect.effect.contrast);
      setSaturation(effect.effect.saturation);
    } else {
      setRadius(12);
      setExposure(0);
      setContrast(1);
      setSaturation(1);
    }
  }, [target, mode, JSON.stringify(effect)]);
  useEffect(() => {
    if (transition)
      setDuration(
        Number(transition.duration.ticks) / transition.duration.timebase,
      );
    else setDuration(0.5);
  }, [target, JSON.stringify(transition)]);
  const apply = () => {
    if (!target || !sequence) return;
    void controller
      .mutate(
        mode === "transitions" ? "Add transition" : "Apply effect",
        [],
        mode === "transitions"
          ? incoming
            ? [
                {
                  type: "transition.put",
                  sequence_id: sequence.id,
                  transition: {
                    id: transition?.id ?? crypto.randomUUID(),
                    outgoing_clip_id: target,
                    incoming_clip_id: incoming.id,
                    duration: fromSeconds(duration),
                    transition: { kind: "crossfade" },
                  },
                },
              ]
            : []
          : [
              {
                type: "effect.put",
                sequence_id: sequence.id,
                effect: {
                  id: effect?.id ?? crypto.randomUUID(),
                  target_id: target,
                  effect:
                    mode === "effects"
                      ? { kind: "gaussian_blur", radius_px: radius }
                      : {
                          kind: "color_adjust",
                          exposure_ev: exposure,
                          contrast,
                          saturation,
                        },
                },
              },
            ],
      )
      .catch((error) => controller.report(error));
  };
  return (
    <Panel
      title={t(
        mode === "effects"
          ? "Effects"
          : mode === "transitions"
            ? "Transitions"
            : "Adjustments",
      )}
    >
      <div className="padded scroll">
        {!target ? (
          <Empty title={t("Nothing selected")}>
            <Sparkles size={30} />
            {t(
              mode === "transitions"
                ? "Select the outgoing clip at a cut."
                : "Select a video, image or shape first.",
            )}
          </Empty>
        ) : (
          <fieldset
            disabled={
              busy ||
              !!historical ||
              targetTrack?.locked ||
              snapshot?.format !== "visual"
            }
          >
            {mode === "effects" ? (
              <>
                <div className="effect-preview">
                  <img
                    src={previewImage}
                    alt=""
                    style={{ filter: `blur(${radius / 5}px)` }}
                  />
                </div>
                <h3>{t("Blur")}</h3>
                <Field label={t("Blur radius")}>
                  <input
                    type="range"
                    min="0"
                    max="60"
                    value={radius}
                    onChange={(e) => setRadius(Number(e.target.value))}
                  />
                </Field>
                <output>{radius}px</output>
              </>
            ) : mode === "transitions" ? (
              <>
                <div className="transition-preview">
                  <span />
                  <span />
                </div>
                <h3>{t("Crossfade")}</h3>
                <Field label={t("Duration")}>
                  <input
                    type="number"
                    min=".04"
                    step=".04"
                    max="5"
                    value={duration}
                    onChange={(e) => setDuration(Number(e.target.value))}
                  />
                </Field>
                <p className="muted">
                  {t(
                    "Clips must be adjacent and have enough source media beyond the cut.",
                  )}
                </p>
              </>
            ) : (
              <>
                {[
                  ["Exposure", exposure, setExposure, -1, 1],
                  ["Contrast", contrast, setContrast, 0, 3],
                  ["Saturation", saturation, setSaturation, 0, 3],
                ].map(([name, value, set, min, max]) => (
                  <Field label={t(String(name))} key={String(name)}>
                    <input
                      type="range"
                      min={Number(min)}
                      max={Number(max)}
                      step=".05"
                      value={Number(value)}
                      onChange={(e) =>
                        (set as (value: number) => void)(Number(e.target.value))
                      }
                    />
                    <output>{Number(value).toFixed(2)}</output>
                  </Field>
                ))}
              </>
            )}
            <div className="modal-footer">
              <Button
                className="primary"
                disabled={mode === "transitions" && !incoming}
                onClick={apply}
              >
                {t(mode === "transitions" ? "Add transition" : "Apply effect")}
              </Button>
              {(mode === "transitions" ? transition : effect) && (
                <Button
                  aria-label={t("Remove effect")}
                  icon={<Trash2 size={14} />}
                  onClick={() => {
                    if (sequence)
                      void controller
                        .mutate(
                          "Remove effect",
                          [],
                          [
                            mode === "transitions"
                              ? {
                                  type: "transition.delete",
                                  sequence_id: sequence.id,
                                  id: transition!.id,
                                }
                              : {
                                  type: "effect.delete",
                                  sequence_id: sequence.id,
                                  id: effect!.id,
                                },
                          ],
                        )
                        .catch((error) => controller.report(error));
                  }}
                />
              )}
            </div>
          </fieldset>
        )}
      </div>
    </Panel>
  );
}
