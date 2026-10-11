import { t, useLocale, parameterLabel } from "../shared/locale";
import { useEffect, useState } from "react";
import { Trash2 } from "lucide-react";
import { useWorkspace } from "../workspace/context";
import type { VisualGraph } from "../model/types";
import { Button, Field } from "../shared/ui";
export function VisualProperties({
  targetId,
  sequenceId,
  readonly,
}: {
  targetId: string;
  sequenceId: string;
  readonly: boolean;
}) {
  useLocale();
  const { controller, snapshot, historical } = useWorkspace();
  const view = historical ?? snapshot,
    graph = view?.visual.find((graph) => graph.sequence_id === sequenceId),
    shape = graph?.shapes.find((shape) => shape.id === targetId);
  const [exposure, setExposure] = useState(0),
    [contrast, setContrast] = useState(1),
    [saturation, setSaturation] = useState(1),
    [feather, setFeather] = useState(0);
  const effect = graph?.effects.find(
      (effect) =>
        effect.target_id === targetId && effect.effect.kind === "color_adjust",
    ),
    mask = graph?.masks.find((mask) => mask.target_id === targetId);
  const color = effect?.effect.kind === "color_adjust" ? effect.effect : null;
  useEffect(() => {
    setExposure(color?.exposure_ev ?? 0);
    setContrast(color?.contrast ?? 1);
    setSaturation(color?.saturation ?? 1);
    setFeather(mask?.feather ?? 0);
  }, [
    targetId,
    color?.exposure_ev,
    color?.contrast,
    color?.saturation,
    mask?.feather,
  ]);
  if (view?.format !== "visual")
    return (
      <p className="muted">
        {t("Effects are available in projects created with this editor.")}
      </p>
    );
  const saveEffect = () =>
    void controller
      .mutate(
        "Adjust color",
        [],
        [
          {
            type: "effect.put",
            sequence_id: sequenceId,
            effect: {
              id: effect?.id ?? crypto.randomUUID(),
              target_id: targetId,
              effect: {
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
  const saveMask = (kind: "rectangle" | "ellipse") =>
    void controller
      .mutate(
        "Apply mask",
        [],
        [
          {
            type: "mask.put",
            sequence_id: sequenceId,
            mask: {
              id: mask?.id ?? crypto.randomUUID(),
              target_id: targetId,
              shape: { kind, x: 0.1, y: 0.1, width: 0.8, height: 0.8 },
              inverted: false,
              feather,
            },
          },
        ],
      )
      .catch((error) => controller.report(error));
  const shapeField = (
    property: "position_x" | "position_y" | "rotation_degrees" | "opacity",
    value: number,
  ) => {
    if (shape && shape[property] !== value)
      void controller
        .mutate(
          "Update shape",
          [],
          [
            {
              type: "shape.put",
              sequence_id: sequenceId,
              shape: { ...shape, [property]: value },
            },
          ],
        )
        .catch((error) => controller.report(error));
  };
  return (
    <fieldset
      disabled={
        readonly ||
        !!(
          shape &&
          view.project.sequences
            .find((sequence) => sequence.id === sequenceId)
            ?.tracks.find((track) => track.id === shape.track_id)?.locked
        )
      }
    >
      {shape && (
        <>
          <legend>{t("Shape")}</legend>
          {(
            ["position_x", "position_y", "rotation_degrees", "opacity"] as const
          ).map((name) => (
            <Field key={name} label={parameterLabel(name)}>
              <input
                key={`${shape.id}-${shape[name]}`}
                type="number"
                step=".05"
                defaultValue={shape[name]}
                onBlur={(event) => shapeField(name, Number(event.target.value))}
              />
            </Field>
          ))}
          <Field label={t("Fill")}>
            <input
              type="color"
              defaultValue={`#${[shape.fill.red, shape.fill.green, shape.fill.blue].map((value) => value.toString(16).padStart(2, "0")).join("")}`}
              onBlur={(event) => {
                const color = event.target.value;
                if (
                  color ===
                  `#${[shape.fill.red, shape.fill.green, shape.fill.blue].map((value) => value.toString(16).padStart(2, "0")).join("")}`
                )
                  return;
                void controller
                  .mutate(
                    "Shape fill",
                    [],
                    [
                      {
                        type: "shape.put",
                        sequence_id: sequenceId,
                        shape: {
                          ...shape,
                          fill: {
                            red: parseInt(color.slice(1, 3), 16),
                            green: parseInt(color.slice(3, 5), 16),
                            blue: parseInt(color.slice(5, 7), 16),
                            alpha: 255,
                          },
                        },
                      },
                    ],
                  )
                  .catch((error) => controller.report(error));
              }}
            />
          </Field>
          <Button
            icon={<Trash2 size={14} />}
            onClick={() =>
              void controller
                .mutate(
                  "Delete shape",
                  [],
                  [
                    {
                      type: "shape.delete",
                      sequence_id: sequenceId,
                      id: shape.id,
                    },
                  ],
                )
                .catch((error) => controller.report(error))
            }
          >
            {t("Delete shape")}
          </Button>
        </>
      )}
      <legend>{t("Color")}</legend>
      <Field label={t("Exposure")}>
        <input
          type="number"
          value={exposure}
          min="-1"
          max="1"
          step=".05"
          onChange={(event) => setExposure(Number(event.target.value))}
        />
      </Field>
      <Field label={t("Contrast")}>
        <input
          type="number"
          value={contrast}
          min="0"
          max="3"
          step=".05"
          onChange={(event) => setContrast(Number(event.target.value))}
        />
      </Field>
      <Field label={t("Saturation")}>
        <input
          type="number"
          value={saturation}
          min="0"
          max="3"
          step=".05"
          onChange={(event) => setSaturation(Number(event.target.value))}
        />
      </Field>
      <Button onClick={saveEffect}>{t("Apply color")}</Button>
      {effect && (
        <Button
          aria-label={t("Remove color effect")}
          icon={<Trash2 size={14} />}
          onClick={() =>
            void controller
              .mutate(
                "Remove color",
                [],
                [
                  {
                    type: "effect.delete",
                    sequence_id: sequenceId,
                    id: effect.id,
                  },
                ],
              )
              .catch((error) => controller.report(error))
          }
        />
      )}
      <legend>{t("Mask")}</legend>
      <Field label={t("Feather")}>
        <input
          type="number"
          min="0"
          max="0.5"
          step="0.01"
          value={feather}
          onChange={(event) => setFeather(Number(event.target.value))}
        />
      </Field>
      <div className="toolbar">
        <Button onClick={() => saveMask("rectangle")}>{t("Rectangle")}</Button>
        <Button onClick={() => saveMask("ellipse")}>{t("Ellipse")}</Button>
        {mask && (
          <Button
            aria-label={t("Remove mask")}
            icon={<Trash2 size={14} />}
            onClick={() =>
              void controller
                .mutate(
                  "Remove mask",
                  [],
                  [
                    {
                      type: "mask.delete",
                      sequence_id: sequenceId,
                      id: mask.id,
                    },
                  ],
                )
                .catch((error) => controller.report(error))
            }
          />
        )}
      </div>
      {graph && <Nodes graph={graph} targetId={targetId} />}
    </fieldset>
  );
}
function Nodes({ graph, targetId }: { graph: VisualGraph; targetId: string }) {
  useLocale();
  return (
    <div className="muted">
      {
        graph.effects.filter(
          (effect) =>
            effect.target_id === targetId &&
            effect.effect.kind === "color_adjust",
        ).length
      }{" "}
      {t("effect ·")}{" "}
      {graph.masks.filter((mask) => mask.target_id === targetId).length}{" "}
      {t("mask")}
    </div>
  );
}
