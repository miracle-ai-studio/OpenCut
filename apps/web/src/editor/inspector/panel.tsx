import { t, useLocale, parameterLabel } from "../shared/locale";
import { SequenceSettings } from "./sequence-settings";
import { useEffect, useState } from "react";
import { Diamond, Plus, Settings2, Trash2 } from "lucide-react";
import type { ToolOutput } from "../../lib/editor-sdk";
import { useWorkspace } from "../workspace/context";
import { activeSequence } from "../model/types";
import {
  secondsForDisplay,
  fromSeconds,
  subtractTime,
  compareTime,
  zero,
} from "../model/time";
import { Button, Empty, Field, Panel } from "../shared/ui";
import { VisualProperties } from "./visual-properties";
import { KeyframeEditor } from "./keyframes";
export function InspectorPanel() {
  useLocale();
  const {
    controller,
    api,
    snapshot,
    historical,
    sequenceId,
    selection,
    selections,
    playhead,
    busy,
  } = useWorkspace();
  const view = historical ?? snapshot,
    sequence = activeSequence(view, sequenceId);
  const selected = sequence?.tracks
    .flatMap((track) => track.clips.map((clip) => ({ track, clip })))
    .find(
      (item) => selection?.kind === "clip" && selection.id === item.clip.id,
    );
  const [parameters, setParameters] = useState<
      ToolOutput<"opencut_capabilities">["parameters"]
    >([]),
    [tab, setTab] = useState("properties");
  useEffect(() => {
    let active = true;
    if (api)
      void api.sdk
        .call("opencut_capabilities", {})
        .then((cap) => {
          if (active) setParameters(cap.parameters);
        })
        .catch((error) => {
          if (active) controller.report(error);
        });
    return () => {
      active = false;
    };
  }, [api, controller]);
  if (!view || !sequence)
    return (
      <Panel title={t("Properties")}>
        <Empty title={t("Nothing selected")}>
          <Settings2 size={28} />
          {t("Select a clip to adjust its properties.")}
        </Empty>
      </Panel>
    );
  const readonly = !!historical || busy;
  if (selections.length > 1)
    return (
      <Panel title={t("Properties")}>
        <Empty
          title={t("{count} clips selected", { count: selections.length })}
        >
          {t(
            "Move the selection in the timeline, or select one element to edit its properties.",
          )}
        </Empty>
      </Panel>
    );
  return (
    <Panel title={t("Properties")}>
      {selected && (
        <div className="panel-tabs">
          {["properties", "animation", "effects"].map((name) => (
            <Button
              key={name}
              aria-pressed={tab === name}
              onClick={() => setTab(name)}
            >
              {t(name[0].toUpperCase() + name.slice(1))}
            </Button>
          ))}
        </div>
      )}
      <div className="inspector-content scroll">
        {selected ? (
          <>
            <div className="selection-heading">
              <strong>
                {view.project.assets.find(
                  (asset) => asset.id === selected.clip.asset_id,
                )?.name ?? "Clip"}
              </strong>
              <span className="muted">{selected.track.name}</span>
            </div>
            {tab === "properties" && (
              <>
                <fieldset disabled={readonly || selected.track.locked}>
                  <legend>{t("Timing")}</legend>
                  <Field label={t("Start (seconds)")}>
                    <input
                      key={`${selected.clip.id}-start-${selected.clip.timeline_range.start.ticks}`}
                      type="number"
                      min="0"
                      step="0.01"
                      defaultValue={secondsForDisplay(
                        selected.clip.timeline_range.start,
                      )}
                      onBlur={(event) => {
                        const value = Number(event.target.value);
                        if (
                          Number.isFinite(value) &&
                          value !==
                            secondsForDisplay(
                              selected.clip.timeline_range.start,
                            )
                        )
                          void controller
                            .mutate("Move clip", [
                              {
                                type: "clip.move",
                                sequence_id: sequence.id,
                                track_id: selected.track.id,
                                clip_id: selected.clip.id,
                                destination_track_id: selected.track.id,
                                start: fromSeconds(value),
                              },
                            ])
                            .catch((error) => controller.report(error));
                      }}
                    />
                  </Field>
                  <Field label={t("Duration (seconds)")}>
                    <input
                      key={`${selected.clip.id}-duration-${selected.clip.timeline_range.duration.ticks}`}
                      type="number"
                      min="0.001"
                      step="0.01"
                      defaultValue={secondsForDisplay(
                        selected.clip.timeline_range.duration,
                      )}
                      onBlur={(event) => {
                        const value = Number(event.target.value);
                        if (
                          value > 0 &&
                          value !==
                            secondsForDisplay(
                              selected.clip.timeline_range.duration,
                            )
                        )
                          void controller
                            .mutate("Change clip speed", [
                              {
                                type: "clip.retime",
                                sequence_id: sequence.id,
                                track_id: selected.track.id,
                                clip_id: selected.clip.id,
                                timeline_duration: fromSeconds(value),
                              },
                            ])
                            .catch((error) => controller.report(error));
                      }}
                    />
                  </Field>
                  <small className="muted">
                    {t("Changing duration adjusts playback speed.")}
                  </small>
                </fieldset>
                <fieldset disabled={readonly || selected.track.locked}>
                  <legend>{t("Transform & audio")}</legend>
                  {parameters
                    .filter((parameter) => parameter.target === "clip")
                    .map((parameter) => {
                      const value = Number(
                        selected.clip.params?.[parameter.name] ??
                          parameter.default_value,
                      );
                      return (
                        <div className="parameter-row" key={parameter.name}>
                          <Field label={parameterLabel(parameter.name)}>
                            <input
                              key={`${selected.clip.id}-${parameter.name}-${value}`}
                              type="number"
                              step="0.05"
                              min={parameter.minimum ?? undefined}
                              max={parameter.maximum ?? undefined}
                              defaultValue={value}
                              onBlur={(event) => {
                                const next = Number(event.target.value);
                                if (Number.isFinite(next) && next !== value)
                                  void controller
                                    .mutate(`Set ${parameter.name}`, [
                                      {
                                        type: "parameter.set_clip",
                                        sequence_id: sequence.id,
                                        track_id: selected.track.id,
                                        clip_id: selected.clip.id,
                                        params: { [parameter.name]: next },
                                      },
                                    ])
                                    .catch((error) => controller.report(error));
                              }}
                            />
                          </Field>
                          <Button
                            aria-label={t("Add {value0} keyframe", {
                              value0: parameter.name,
                            })}
                            disabled={!parameter.animatable}
                            icon={<Diamond size={13} />}
                            onClick={() => {
                              const time = subtractTime(
                                playhead,
                                selected.clip.timeline_range.start,
                              );
                              if (
                                compareTime(time, zero) < 0 ||
                                compareTime(
                                  time,
                                  selected.clip.timeline_range.duration,
                                ) > 0
                              ) {
                                controller.report(
                                  new Error(
                                    "Place the playhead inside the clip.",
                                  ),
                                );
                                return;
                              }
                              const channel = selected.clip.channels?.find(
                                (channel) =>
                                  channel.parameter === parameter.name,
                              );
                              const existing = channel?.keyframes.find(
                                (keyframe) =>
                                  compareTime(keyframe.time, time) === 0,
                              );
                              if (
                                existing?.value === value &&
                                existing.interpolation === "linear"
                              )
                                return;
                              void controller
                                .mutate("Add keyframe", [
                                  {
                                    type: "keyframe.channel_set",
                                    sequence_id: sequence.id,
                                    track_id: selected.track.id,
                                    clip_id: selected.clip.id,
                                    channel: {
                                      parameter: parameter.name,
                                      keyframes: [
                                        ...(channel?.keyframes ?? []).filter(
                                          (keyframe) =>
                                            compareTime(keyframe.time, time) !==
                                            0,
                                        ),
                                        {
                                          id:
                                            channel?.keyframes.find(
                                              (keyframe) =>
                                                compareTime(
                                                  keyframe.time,
                                                  time,
                                                ) === 0,
                                            )?.id ?? crypto.randomUUID(),
                                          time,
                                          value,
                                          interpolation: "linear" as const,
                                        },
                                      ].sort((a, b) =>
                                        compareTime(a.time, b.time),
                                      ),
                                    },
                                  },
                                ])
                                .catch((error) => controller.report(error));
                            }}
                          />
                        </div>
                      );
                    })}
                </fieldset>
              </>
            )}
            {tab === "animation" && (
              <KeyframeEditor
                key={selected.clip.id}
                clip={selected.clip}
                trackId={selected.track.id}
                sequenceId={sequence.id}
                readonly={readonly || selected.track.locked}
              />
            )}
            {tab === "effects" && (
              <VisualProperties
                key={selected.clip.id}
                targetId={selected.clip.id}
                sequenceId={sequence.id}
                readonly={readonly || selected.track.locked}
              />
            )}
          </>
        ) : selection?.kind === "shape" ? (
          <VisualProperties
            key={selection.id}
            targetId={selection.id}
            sequenceId={sequence.id}
            readonly={readonly}
          />
        ) : selection?.kind === "caption" ? (
          <CaptionProperties cueId={selection.id} />
        ) : (
          <>
            <Empty title={t("Select an element")}>
              {t("Clip properties, keyframes and effects appear here.")}
            </Empty>
            <SequenceSettings
              key={`${sequence.id}-${sequence.width}-${sequence.height}-${sequence.fps.numerator}-${sequence.fps.denominator}`}
              sequence={sequence}
              readonly={readonly}
            />
            <fieldset disabled={readonly}>
              <legend>{t("Tracks")}</legend>
              <p>
                {sequence.width} × {sequence.height} ·{" "}
                {sequence.fps.numerator / sequence.fps.denominator} {t("fps")}
              </p>
              <Button
                onClick={() =>
                  void controller
                    .track("audio")
                    .catch((error) => controller.report(error))
                }
                icon={<Plus size={14} />}
              >
                {t("Add audio track")}
              </Button>
            </fieldset>
          </>
        )}
      </div>
    </Panel>
  );
}
function CaptionProperties({ cueId }: { cueId: string }) {
  useLocale();
  const { controller, snapshot, historical, sequenceId, busy } = useWorkspace();
  const sequence = activeSequence(historical ?? snapshot, sequenceId),
    cue = sequence?.caption_cues?.find((cue) => cue.id === cueId);
  if (!cue || !sequence) return null;
  return (
    <fieldset disabled={!!historical || busy}>
      <legend>{t("Caption")}</legend>
      <Field label={t("Text")}>
        <textarea
          key={`${cue.id}-${cue.text}`}
          defaultValue={cue.text}
          onBlur={(event) => {
            if (event.target.value !== cue.text)
              void controller
                .mutate("Edit caption", [
                  {
                    type: "caption.update",
                    sequence_id: sequence.id,
                    cue: { ...cue, text: event.target.value },
                  },
                ])
                .catch((error) => controller.report(error));
          }}
        />
      </Field>
      <Field label={t("Font size")}>
        <input
          type="number"
          key={`${cue.id}-${cue.style.font_size}`}
          defaultValue={cue.style.font_size}
          onBlur={(event) =>
            void controller
              .mutate("Caption font size", [
                {
                  type: "caption.update",
                  sequence_id: sequence.id,
                  cue: {
                    ...cue,
                    style: {
                      ...cue.style,
                      font_size: Number(event.target.value),
                    },
                  },
                },
              ])
              .catch((error) => controller.report(error))
          }
        />
      </Field>
      <Button
        className="danger"
        icon={<Trash2 size={14} />}
        onClick={() =>
          void controller
            .mutate("Delete caption", [
              {
                type: "caption.delete",
                sequence_id: sequence.id,
                cue_id: cue.id,
              },
            ])
            .catch((error) => controller.report(error))
        }
      >
        {t("Delete caption")}
      </Button>
    </fieldset>
  );
}
