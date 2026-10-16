import { useState } from "react";
import type { V4Element } from "../types";
import { Field } from "../../../shared/ui";
import { useWorkspace } from "../../context";
import { timeFromDecimalSeconds } from "../../../model/time";
import { useSceneStack } from "./workspace";
import {
  acceptsMasks,
  createBlur,
  createMask,
  move,
  moveEffectTrack,
  newEffectTrack,
  removeEffect,
  removeMask,
  stackTargetProblem,
} from "./model";
import { MaskEditor, NumberField } from "./mask-editor";
import { MASK_KINDS, MASK_LABELS } from "./types";
import type { Effect, ShapeKind } from "./types";
import { useSceneText } from "./messages";
import { requestedFontStyle } from "../text/font-synthesis";

function EffectEditor({
  effect,
  change,
}: {
  effect: Effect;
  change(effect: Effect): void;
}) {
  const t = useSceneText();
  return (
    <div>
      <label>
        <input
          type="checkbox"
          checked={effect.enabled}
          onChange={(event) =>
            change({ ...effect, enabled: event.target.checked })
          }
        />
        {t("Enabled")}
      </label>
      {effect.effect.kind === "classic_blur" ? (
        <NumberField
          label="Blur intensity"
          min={0}
          value={effect.effect.intensity}
          change={(intensity) =>
            change({ ...effect, effect: { kind: "classic_blur", intensity } })
          }
        />
      ) : (
        <p>{effect.effect.kind}</p>
      )}
    </div>
  );
}
export function V4SceneStackInspector({
  elementId,
  locked = false,
}: {
  elementId: string;
  locked?: boolean;
}) {
  const { v4 } = useWorkspace(),
    stack = useSceneStack(),
    t = useSceneText(),
    [kind, setKind] = useState<ShapeKind>("rectangle"),
    [fontId, setFontId] = useState("");
  const document = (v4?.historical ?? v4?.head)?.project,
    sequence = document?.sequences.find((s) => s.id === v4?.sequenceId),
    track = sequence?.tracks.find((t) =>
      t.elements.some((e) => e.id === elementId),
    ),
    element = track?.elements.find((e) => e.id === elementId);
  if (!stack || !element || element.content.kind === "audio") return null;
  const draft = stack.draft?.element.id === elementId ? stack.draft : null,
    issue = stackTargetProblem(element),
    fonts = stack.fonts.filter(
      (f) => f.face_index === 0 && [400, 700].includes(f.weight),
    );
  const change = (update: (next: V4Element) => void) => {
    if (!draft) return;
    const next = structuredClone(draft.element);
    update(next);
    stack.change(next);
  };
  const run = (fn: () => void) => {
    try {
      fn();
    } catch (cause) {
      stack.report(cause);
    }
  };
  const disabled =
    locked || !!track?.locked || stack.disabled || !stack.available || !!issue;
  const active = draft?.element.visual?.masks.find(
    (m) => m.id === draft.activeMaskId,
  );
  return (
    <section className="scene-stack" aria-label={t("Visual stack")}>
      <h3>{t("Visual stack")}</h3>
      {!stack.available && (
        <p role="status">
          {t("Visual-stack authoring is unavailable on this host.")}
        </p>
      )}
      {issue && <p role="status">{t(issue)}</p>}
      {!draft ? (
        <button
          type="button"
          disabled={disabled}
          onClick={() => stack.open(elementId)}
        >
          {t("Edit visual stack")}
        </button>
      ) : (
        <form
          aria-label={t("Edit visual stack")}
          onSubmit={(event) => {
            event.preventDefault();
            void stack.save();
          }}
          onKeyDown={(event) => {
            event.stopPropagation();
            if (event.key === "Escape") {
              event.preventDefault();
              stack.cancel();
            }
          }}
        >
          <fieldset disabled={disabled}>
            {!!(draft.element.channels.length || track?.channels.length) && (
              <p>
                {t(
                  "Base values are preserved alongside keyframes. Static canvas handles are unavailable for animated targets.",
                )}
              </p>
            )}
            {draft.geometryError && (
              <p role="status">{t(draft.geometryError)}</p>
            )}
            {acceptsMasks(draft.element) && (
              <section aria-label={t("Masks")}>
                <h4>{t("Masks")}</h4>
                <ol className="scene-stack-list">
                  {draft.element.visual?.masks.map((mask, index) => (
                    <li key={mask.id}>
                      <button
                        type="button"
                        aria-pressed={mask.id === draft.activeMaskId}
                        onClick={() => stack.selectMask(mask.id)}
                      >
                        {MASK_LABELS[mask.shape.kind as ShapeKind]
                          ? t(MASK_LABELS[mask.shape.kind as ShapeKind])
                          : mask.shape.kind}{" "}
                        {index + 1}
                      </button>
                      <label>
                        <input
                          type="checkbox"
                          aria-label={`${t("Enabled")} ${index + 1}`}
                          checked={mask.enabled}
                          onChange={(event) =>
                            change((next) => {
                              next.visual!.masks[index].enabled =
                                event.target.checked;
                            })
                          }
                        />
                        {t("Enabled")}
                      </label>
                      <div className="scene-stack-actions">
                        <button
                          type="button"
                          disabled={index === 0}
                          aria-label={`${t("Earlier")} ${index + 1}`}
                          onClick={() =>
                            change((next) => {
                              next.visual!.masks = move(
                                next.visual!.masks,
                                index,
                                -1,
                              );
                            })
                          }
                        >
                          {t("Earlier")}
                        </button>
                        <button
                          type="button"
                          disabled={
                            index ===
                            (draft.element.visual?.masks.length ?? 0) - 1
                          }
                          aria-label={`${t("Later")} ${index + 1}`}
                          onClick={() =>
                            change((next) => {
                              next.visual!.masks = move(
                                next.visual!.masks,
                                index,
                                1,
                              );
                            })
                          }
                        >
                          {t("Later")}
                        </button>
                        <button
                          type="button"
                          aria-label={`${t("Remove mask")} ${index + 1}`}
                          onClick={() =>
                            run(() =>
                              stack.change(removeMask(draft.element, mask.id)),
                            )
                          }
                        >
                          {t("Remove mask")}
                        </button>
                      </div>
                    </li>
                  ))}
                </ol>
                {!draft.element.visual?.masks.length && <p>{t("No masks.")}</p>}
                <Field label={t("Mask shape")}>
                  <select
                    value={kind}
                    onChange={(event) =>
                      setKind(event.target.value as ShapeKind)
                    }
                  >
                    {MASK_KINDS.map((kind) => (
                      <option key={kind} value={kind}>
                        {t(MASK_LABELS[kind])}
                      </option>
                    ))}
                  </select>
                </Field>
                {kind === "text" && (
                  <Field label={t("Imported font")}>
                    <select
                      value={fontId || fonts[0]?.id || ""}
                      onChange={(event) => setFontId(event.target.value)}
                    >
                      <option value="" disabled>
                        {t("Choose a font")}
                      </option>
                      {fonts.map((font) => (
                        <option key={font.id} value={font.id}>
                          {font.family} · {font.weight} ·{" "}
                          {requestedFontStyle(font)}
                        </option>
                      ))}
                    </select>
                  </Field>
                )}
                {kind === "text" && !fonts.length && (
                  <p>
                    {t(
                      "No imported static fonts are available. Import a font before creating a text mask.",
                    )}
                  </p>
                )}
                {kind === "text" && stack.fontLoading && (
                  <p role="status">{t("Reading font synthesis proof…")}</p>
                )}
                {kind === "text" && stack.fontError && (
                  <p role="alert">{stack.fontError}</p>
                )}
                <button
                  type="button"
                  disabled={
                    (kind === "text" &&
                      (!fonts.length ||
                        stack.fontLoading ||
                        !!stack.fontError)) ||
                    (!draft.geometry &&
                      !["split", "text", "freeform"].includes(kind))
                  }
                  onClick={() =>
                    run(() => {
                      const mask = createMask(
                        kind,
                        draft.geometry,
                        fonts.find((f) => f.id === (fontId || fonts[0]?.id)),
                      );
                      change((next) => {
                        next.visual!.masks.push(mask);
                      });
                      stack.selectMask(mask.id);
                    })
                  }
                >
                  {t("Add mask")}
                </button>
                {active && (
                  <MaskEditor
                    key={active.id}
                    mask={active}
                    fonts={fonts}
                    fontLoading={stack.fontLoading}
                    fontError={stack.fontError}
                    change={(mask) =>
                      change((next) => {
                        const index = next.visual!.masks.findIndex(
                          (m) => m.id === mask.id,
                        );
                        if (index >= 0) next.visual!.masks[index] = mask;
                      })
                    }
                  />
                )}
              </section>
            )}
            <section aria-label={t("Effects")}>
              <h4>{t("Effects")}</h4>
              {draft.element.content.kind === "effect_layer" && (
                <EffectEditor
                  effect={draft.element.content.effect}
                  change={(effect) =>
                    change((next) => {
                      if (next.content.kind === "effect_layer")
                        next.content.effect = effect;
                    })
                  }
                />
              )}
              <ol className="scene-stack-list">
                {draft.element.visual?.effects.map((effect, index) => (
                  <li key={effect.id}>
                    <EffectEditor
                      effect={effect}
                      change={(effect) =>
                        change((next) => {
                          next.visual!.effects[index] = effect;
                        })
                      }
                    />
                    <div className="scene-stack-actions">
                      <button
                        type="button"
                        disabled={index === 0}
                        onClick={() =>
                          change((next) => {
                            next.visual!.effects = move(
                              next.visual!.effects,
                              index,
                              -1,
                            );
                          })
                        }
                      >
                        {t("Earlier")}
                      </button>
                      <button
                        type="button"
                        disabled={
                          index ===
                          (draft.element.visual?.effects.length ?? 0) - 1
                        }
                        onClick={() =>
                          change((next) => {
                            next.visual!.effects = move(
                              next.visual!.effects,
                              index,
                              1,
                            );
                          })
                        }
                      >
                        {t("Later")}
                      </button>
                      <button
                        type="button"
                        onClick={() =>
                          run(() =>
                            stack.change(
                              removeEffect(draft.element, effect.id),
                            ),
                          )
                        }
                      >
                        {t("Remove effect")}
                      </button>
                    </div>
                  </li>
                ))}
              </ol>
              {!draft.element.visual?.effects.length &&
                draft.element.content.kind !== "effect_layer" && (
                  <p>{t("No effects.")}</p>
                )}
              {draft.element.visual && (
                <button
                  type="button"
                  onClick={() =>
                    change((next) => {
                      next.visual!.effects.push(createBlur());
                    })
                  }
                >
                  {t("Add blur")}
                </button>
              )}
            </section>
          </fieldset>
          <div className="scene-stack-actions">
            <button type="submit" disabled={disabled}>
              {t(stack.pending ? "Saving stack…" : "Save stack")}
            </button>
            <button
              type="button"
              disabled={stack.pending}
              onClick={stack.cancel}
            >
              {t("Cancel")}
            </button>
          </div>
        </form>
      )}
      {stack.error && <p role="alert">{t(stack.error)}</p>}
    </section>
  );
}

export function V4SceneEffectsPanel() {
  const { v4, controller } = useWorkspace(),
    stack = useSceneStack(),
    t = useSceneText(),
    [duration, setDuration] = useState("5");
  const sequence = (v4?.historical ?? v4?.head)?.project.sequences.find(
    (s) => s.id === v4?.sequenceId,
  );
  if (!stack || !sequence || !v4) return null;
  const disabled = stack.disabled || !stack.available;
  const create = () => {
    try {
      const time = timeFromDecimalSeconds(duration);
      if (BigInt(time.ticks) <= 0n)
        throw new Error("Duration must be positive.");
      const track = newEffectTrack(v4.playhead, time);
      void stack.transact("Add effect layer", [
        { type: "track_put", sequence_id: sequence.id, track },
      ]);
    } catch (cause) {
      stack.report(cause);
    }
  };
  return (
    <section className="scene-stack" aria-label={t("Scene effect layers")}>
      <h3>{t("Scene effect layers")}</h3>
      <p>
        {t(
          "Order follows compositing order, from earlier to later. An effect layer changes the layers before it.",
        )}
      </p>
      <ol>
        {sequence.tracks.map((track, index) =>
          track.kind !== "effect" ? null : (
            <li key={track.id}>
              <strong>{track.name}</strong>
              <label>
                <input
                  type="checkbox"
                  checked={track.enabled}
                  disabled={disabled || track.locked}
                  onChange={(event) => {
                    const next = structuredClone(track);
                    next.enabled = event.target.checked;
                    void stack.transact("Enable effect layer", [
                      {
                        type: "track_put",
                        sequence_id: sequence.id,
                        track: next,
                      },
                    ]);
                  }}
                />
                {t("Enabled")}
              </label>
              <div className="scene-stack-actions">
                <button
                  type="button"
                  disabled={
                    disabled ||
                    track.locked ||
                    index === 0 ||
                    sequence.tracks[index - 1]?.locked
                  }
                  onClick={() => {
                    try {
                      void stack.transact("Reorder effect layer", [
                        moveEffectTrack(sequence, track.id, -1),
                      ]);
                    } catch (cause) {
                      stack.report(cause);
                    }
                  }}
                >
                  {t("Earlier")}
                </button>
                <button
                  type="button"
                  disabled={
                    disabled ||
                    track.locked ||
                    index === sequence.tracks.length - 1 ||
                    sequence.tracks[index + 1]?.locked
                  }
                  onClick={() => {
                    try {
                      void stack.transact("Reorder effect layer", [
                        moveEffectTrack(sequence, track.id, 1),
                      ]);
                    } catch (cause) {
                      stack.report(cause);
                    }
                  }}
                >
                  {t("Later")}
                </button>
                <button
                  type="button"
                  disabled={disabled || track.locked}
                  onClick={() =>
                    void stack.transact("Remove effect layer", [
                      {
                        type: "track_delete",
                        sequence_id: sequence.id,
                        track_id: track.id,
                      },
                    ])
                  }
                >
                  {t("Remove layer")}
                </button>
              </div>
              {track.elements.map((element) => (
                <div key={element.id}>
                  <span>{element.name}</span>
                  <button
                    type="button"
                    disabled={disabled || track.locked}
                    onClick={() => controller.v4.select(element.id)}
                  >
                    {t("Edit stack")}
                  </button>
                  <button
                    type="button"
                    disabled={disabled || track.locked}
                    onClick={() =>
                      void stack.transact("Remove effect", [
                        {
                          type: "element_delete",
                          sequence_id: sequence.id,
                          element_id: element.id,
                        },
                      ])
                    }
                  >
                    {t("Remove effect")}
                  </button>
                </div>
              ))}
            </li>
          ),
        )}
      </ol>
      {!sequence.tracks.some((track) => track.kind === "effect") && (
        <p>{t("No scene effect layers.")}</p>
      )}
      <fieldset disabled={disabled}>
        <Field label={t("Duration (seconds)")}>
          <input
            type="text"
            inputMode="decimal"
            value={duration}
            onChange={(event) => setDuration(event.target.value)}
          />
        </Field>
        <button type="button" onClick={create}>
          {t("Add effect layer")}
        </button>
      </fieldset>
      {stack.error && <p role="alert">{t(stack.error)}</p>}
    </section>
  );
}
