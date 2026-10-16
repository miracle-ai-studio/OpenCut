import { Field } from "../../../shared/ui";
import { moveAnchor, removePathPoints, splitPathSegment } from "./model";
import { useSceneText } from "./messages";
import type { Mask, Point } from "./types";
import type { V4TextFont } from "../text/types";
import { displayFontFamily, requestedFontStyle } from "../text/font-synthesis";

type NumberProps = {
  label: string;
  value: number;
  change(value: number): void;
  min?: number;
  max?: number;
  percent?: boolean;
};
export function NumberField({
  label,
  value,
  change,
  min,
  max,
  percent = false,
}: NumberProps) {
  const t = useSceneText();
  return (
    <Field label={t(label)}>
      <input
        type="number"
        required
        step="any"
        min={min}
        max={max}
        value={value * (percent ? 100 : 1)}
        onChange={(event) => {
          if (
            event.target.value !== "" &&
            Number.isFinite(event.target.valueAsNumber)
          )
            change(event.target.valueAsNumber / (percent ? 100 : 1));
        }}
      />
    </Field>
  );
}

export function MaskEditor({
  mask,
  fonts,
  fontLoading = false,
  fontError = null,
  change,
}: {
  mask: Mask;
  fonts: readonly V4TextFont[];
  fontLoading?: boolean;
  fontError?: string | null;
  change(mask: Mask): void;
}) {
  const t = useSceneText(),
    shape = mask.shape;
  const update = (action: (next: Mask) => void) => {
    const next = structuredClone(mask);
    action(next);
    change(next);
  };
  if (
    shape.kind.startsWith("legacy_") ||
    mask.feather.unit !== "classic_pixels"
  )
    return (
      <p>
        {t(
          "This mask uses a legacy profile. Its exact fields are preserved; only order, enabled and removal are editable here.",
        )}
      </p>
    );
  const geometry =
    "geometry" in shape
      ? shape.geometry
      : shape.kind === "split"
        ? shape
        : shape.kind === "text"
          ? shape.mask
          : shape.kind === "freeform"
            ? shape.path
            : null;
  const setGeometry = (key: string, value: number) =>
    update((next) => {
      const s = next.shape,
        g =
          "geometry" in s
            ? s.geometry
            : s.kind === "split"
              ? s
              : s.kind === "text"
                ? s.mask
                : s.kind === "freeform"
                  ? s.path
                  : null;
      if (g && key in g) (g as unknown as Record<string, number>)[key] = value;
    });
  const text = shape.kind === "text" ? shape.mask : null;
  const textUpdate = (action: (value: NonNullable<typeof text>) => void) =>
    update((next) => {
      if (next.shape.kind === "text") action(next.shape.mask);
    });
  return (
    <section aria-label={t("Mask properties")} className="scene-mask-fields">
      <label>
        <input
          type="checkbox"
          checked={mask.inverted}
          onChange={(event) =>
            change({ ...mask, inverted: event.target.checked })
          }
        />
        {t("Inverted")}
      </label>
      {geometry && (
        <div className="scene-stack-grid">
          {(
            [
              ["center_x", "X (%)", true],
              ["center_y", "Y (%)", true],
              ["width", "Width (%)", true],
              ["height", "Height (%)", true],
              ["rotation_degrees", "Rotation (degrees)", false],
              ["scale", "Scale (%)", true],
            ] as const
          ).map(([key, label, percent]) =>
            key in geometry ? (
              <NumberField
                key={key}
                label={label}
                percent={percent}
                min={
                  key === "width" || key === "height"
                    ? 0.000001
                    : key === "scale"
                      ? 0
                      : undefined
                }
                value={(geometry as unknown as Record<string, number>)[key]}
                change={(value) => setGeometry(key, value)}
              />
            ) : null,
          )}
        </div>
      )}
      <NumberField
        label="Feather (pixels)"
        min={0}
        value={mask.feather.value}
        change={(value) =>
          change({ ...mask, feather: { unit: "classic_pixels", value } })
        }
      />
      <NumberField
        label="Stroke width (pixels)"
        min={0}
        value={mask.stroke.width_px}
        change={(value) =>
          update((next) => {
            next.stroke.width_px = value;
          })
        }
      />
      <Field label={t("Stroke alignment")}>
        <select
          value={mask.stroke.align}
          onChange={(event) =>
            update((next) => {
              next.stroke.align = event.target.value as Mask["stroke"]["align"];
            })
          }
        >
          {(
            [
              ["inside", "Inside"],
              ["center", "Center"],
              ["outside", "Outside"],
            ] as const
          ).map(([value, label]) => (
            <option key={value} value={value}>
              {t(label)}
            </option>
          ))}
        </select>
      </Field>
      <div className="scene-stack-grid">
        {(
          [
            ["red", "Red"],
            ["green", "Green"],
            ["blue", "Blue"],
            ["alpha", "Alpha"],
          ] as const
        ).map(([key, label]) => (
          <NumberField
            key={key}
            label={label}
            min={0}
            max={1}
            value={mask.stroke.color[key]}
            change={(value) =>
              update((next) => {
                next.stroke.color[key] = value;
              })
            }
          />
        ))}
      </div>
      {text && (
        <>
          <Field label={t("Content")}>
            <textarea
              value={text.content}
              onChange={(event) =>
                textUpdate((value) => {
                  value.content = event.target.value;
                })
              }
            />
          </Field>
          <Field label={t("Imported font")}>
            <select
              disabled={fontLoading || !!fontError}
              value={text.font.kind === "locked" ? text.font.font_id : ""}
              onChange={(event) => {
                const font = fonts.find((f) => f.id === event.target.value);
                if (font)
                  textUpdate((value) => {
                    value.font = { kind: "locked", font_id: font.id };
                    value.font_weight = font.weight === 700 ? "bold" : "normal";
                    value.font_style = requestedFontStyle(font);
                  });
              }}
            >
              <option value="" disabled>
                {t("Choose a font")}
              </option>
              {fonts.map((font) => (
                <option key={font.id} value={font.id} title={font.family}>
                  {displayFontFamily(font)} · {font.weight} · {requestedFontStyle(font)}{font.synthesis_binding ? ` · ${t("Classic Canvas italic synthesis")}` : ""}
                </option>
              ))}
            </select>
            {fontLoading && (
              <p role="status">{t("Reading font synthesis proof…")}</p>
            )}
            {fontError && <p role="alert">{fontError}</p>}
          </Field>
          <Field label={t("Font size unit")}>
            <select
              value={text.font_size.unit}
              onChange={(event) =>
                textUpdate((value) => {
                  value.font_size = {
                    unit: event.target.value as typeof value.font_size.unit,
                    value: value.font_size.value,
                  };
                })
              }
            >
              <option value="classic_height90">
                {t("Classic height / 90")}
              </option>
              <option value="canvas_pixels">{t("Canvas pixels")}</option>
            </select>
          </Field>
          <NumberField
            label="Font size"
            min={0.000001}
            value={text.font_size.value}
            change={(number) =>
              textUpdate((value) => {
                value.font_size.value = number;
              })
            }
          />
          <Field label={t("Decoration")}>
            <select
              value={text.decoration}
              onChange={(event) =>
                textUpdate((value) => {
                  value.decoration = event.target
                    .value as typeof value.decoration;
                })
              }
            >
              {(
                [
                  ["none", "None"],
                  ["underline", "Underline"],
                  ["line_through", "Line through"],
                ] as const
              ).map(([value, label]) => (
                <option key={value} value={value}>
                  {t(label)}
                </option>
              ))}
            </select>
          </Field>
          <NumberField
            label="Letter spacing (pixels)"
            value={text.letter_spacing_px}
            change={(number) =>
              textUpdate((value) => {
                value.letter_spacing_px = number;
              })
            }
          />
          <NumberField
            label="Line height"
            min={0.000001}
            value={text.line_height_multiplier}
            change={(number) =>
              textUpdate((value) => {
                value.line_height_multiplier = number;
              })
            }
          />
        </>
      )}
      {shape.kind === "freeform" && (
        <>
          <label>
            <input
              type="checkbox"
              checked={shape.path.closed}
              disabled={!shape.path.closed && shape.path.points.length < 3}
              onChange={(event) =>
                update((next) => {
                  if (next.shape.kind === "freeform")
                    next.shape.path.closed = event.target.checked;
                })
              }
            />
            {t("Closed path")}
          </label>
          {!shape.path.closed && (
            <p>
              {t(
                "An open path is editable but does not mask the image until closed.",
              )}
            </p>
          )}
          <section aria-label={t("Path points")}>
            {shape.path.points.map((point, index) => (
              <fieldset key={point.id}>
                <legend>{t("Point {index}", { index: index + 1 })}</legend>
                <div className="scene-stack-grid">
                  {(
                    [
                      ["x", "Anchor X (%)"],
                      ["y", "Anchor Y (%)"],
                      ["in_x", "In X (%)"],
                      ["in_y", "In Y (%)"],
                      ["out_x", "Out X (%)"],
                      ["out_y", "Out Y (%)"],
                    ] as const
                  ).map(([key, label]) => (
                    <NumberField
                      key={key}
                      label={label}
                      percent
                      value={point[key]}
                      change={(value) =>
                        update((next) => {
                          if (next.shape.kind !== "freeform") return;
                          const points = next.shape.path.points,
                            p = points[index];
                          points[index] =
                            key === "x" || key === "y"
                              ? moveAnchor(
                                  p,
                                  key === "x" ? value : p.x,
                                  key === "y" ? value : p.y,
                                )
                              : { ...p, [key]: value };
                        })
                      }
                    />
                  ))}
                </div>
                <div className="scene-stack-actions">
                  <button
                    type="button"
                    disabled={
                      shape.path.points.length >= 4096 ||
                      (!shape.path.closed &&
                        index === shape.path.points.length - 1)
                    }
                    onClick={() =>
                      update((next) => {
                        if (next.shape.kind === "freeform")
                          next.shape.path.points = splitPathSegment(
                            next.shape.path.points,
                            index,
                            next.shape.path.closed,
                          );
                      })
                    }
                  >
                    {t("Insert after")}
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      update((next) => {
                        if (next.shape.kind === "freeform")
                          next.shape.path = removePathPoints(next.shape.path, [point.id]);
                      })
                    }
                  >
                    {t("Delete point")}
                  </button>
                </div>
              </fieldset>
            ))}
          </section>
          <button
            type="button"
            disabled={shape.path.closed || shape.path.points.length >= 4096}
            onClick={() =>
              update((next) => {
                if (next.shape.kind !== "freeform") return;
                const p = next.shape.path.points.at(-1),
                  x = p ? p.x + 0.1 : 0,
                  y = p ? p.y + 0.1 : 0;
                const point: Point = {
                  id: crypto.randomUUID(),
                  x,
                  y,
                  in_x: x,
                  in_y: y,
                  out_x: x,
                  out_y: y,
                };
                next.shape.path.points.push(point);
              })
            }
          >
            {t("Append point")}
          </button>
        </>
      )}
    </section>
  );
}
