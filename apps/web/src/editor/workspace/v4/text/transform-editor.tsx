import { useEffect, useRef, useState } from "react";
import { useWorkspace } from "../../context";
import { Button, Field } from "../../../shared/ui";
import { t, useLocale } from "../../../shared/locale";
import { fontRecoveryBlocked } from "./font-recovery";

const fields = [
  ["position_x_px", "Horizontal center offset (pixels)"],
  ["position_y_px", "Vertical center offset (pixels)"],
  ["scale_x", "Horizontal scale (multiplier)"],
  ["scale_y", "Vertical scale (multiplier)"],
] as const;
type Key = (typeof fields)[number][0];
export type V4TextTransformEditorProps = {
  trackId: string;
  elementId: string;
  onClose(): void;
};

/** Edit authored transform values through the existing atomic write use case.
 * A stale form never rebases its captured element onto a newer project head. */
export function V4TextTransformEditor(props: V4TextTransformEditorProps) {
  useLocale();
  const { controller } = useWorkspace();
  const [captured] = useState(() => {
    const state = controller.getSnapshot();
    const v4 = state.v4;
    const track = v4?.head.project.sequences
      .find((s) => s.id === v4.sequenceId)
      ?.tracks.find((tr) => tr.id === props.trackId);
    const element = track?.elements.find((e) => e.id === props.elementId);
    return {
      api: state.api,
      projectId: v4?.head.project.id,
      revision: v4?.head.revision,
      sequenceId: v4?.sequenceId,
      trackId: props.trackId,
      elementId: props.elementId,
      element: element ? structuredClone(element) : null,
    };
  });
  const original = captured.element?.visual?.transform;
  const [draft, setDraft] = useState(
    () =>
      Object.fromEntries(
        fields.map(([key]) => [key, String(original?.[key] ?? "")]),
      ) as Record<Key, string>,
  );
  const [pending, setPending] = useState(false),
    [error, setError] = useState<string | null>(null);
  const active = useRef(true),
    inFlight = useRef(false),
    composing = useRef(false);
  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);
  const reason = () => {
    const state = controller.getSnapshot(),
      v4 = state.v4;
    if (
      !v4 ||
      state.api !== captured.api ||
      v4.head.project.id !== captured.projectId ||
      v4.head.revision !== captured.revision ||
      v4.sequenceId !== captured.sequenceId ||
      props.trackId !== captured.trackId ||
      props.elementId !== captured.elementId
    )
      return "The text target changed. Close this form and edit the current revision.";
    if (v4.historical) return "Return to the current revision before editing.";
    const track = v4.head.project.sequences
      .find((s) => s.id === captured.sequenceId)
      ?.tracks.find((tr) => tr.id === captured.trackId);
    const element = track?.elements.find((e) => e.id === captured.elementId);
    if (
      !element ||
      element.content.kind !== "text" ||
      !element.visual ||
      JSON.stringify(element) !== JSON.stringify(captured.element)
    )
      return "The text target changed. Close this form and edit the current revision.";
    if (track?.locked)
      return "Unlock the text track before editing its position.";
    if (state.busy || v4.gainDraft)
      return "Wait for the current edit to finish.";
    if (
      fontRecoveryBlocked(v4.fontRecovery) ||
      controller.v4.captionRecovery.blocked() ||
      v4.recovery?.record ||
      v4.recovery?.error ||
      ["reading", "staging", "applying"].includes(v4.recovery?.phase ?? "") ||
      ["applying", "unconfirmed"].includes(
        v4.timelineEdit.attempt?.phase ?? "",
      ) ||
      ["applying", "unconfirmed"].includes(v4.clipboard.attempt?.phase ?? "")
    )
      return "Resolve the original edit before starting another operation.";
    return null;
  };
  const blocked = reason();
  const valid = fields.every(
    ([key]) => draft[key].trim() !== "" && Number.isFinite(Number(draft[key])),
  );
  const changed =
    !!original && fields.some(([key]) => Number(draft[key]) !== original[key]);
  const animated =
    captured.element?.channels.filter((channel) => {
      if (
        channel.kind !== "scalar" ||
        channel.keys.length === 0 ||
        channel.target.kind !== "element" ||
        channel.target.element_id !== captured.elementId ||
        channel.binding.kind !== "property"
      )
        return false;
      const parameter = channel.binding.parameter;
      return fields.some(([key]) => key === parameter);
    }) ?? [];
  const save = async () => {
    if (inFlight.current || composing.current || !valid || !changed) return;
    const problem = reason();
    if (problem) {
      setError(problem);
      return;
    }
    if (!captured.element?.visual) return;
    inFlight.current = true;
    setPending(true);
    setError(null);
    const element = structuredClone(captured.element);
    for (const [key] of fields)
      if (Number(draft[key]) !== original![key])
        element.visual!.transform[key] = Number(draft[key]);
    try {
      await controller.v4.write("Edit text position and scale", [
        {
          type: "element_put",
          sequence_id: captured.sequenceId!,
          track_id: captured.trackId,
          element,
        },
      ]);
      if (active.current) props.onClose();
    } catch (cause) {
      if (active.current)
        setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      inFlight.current = false;
      if (active.current) setPending(false);
    }
  };
  return (
    <form
      aria-label={t("Edit text position and scale")}
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
      onCompositionStart={() => {
        composing.current = true;
      }}
      onCompositionEnd={() => {
        composing.current = false;
      }}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (
          event.key === "Escape" &&
          !event.nativeEvent.isComposing &&
          !composing.current &&
          !inFlight.current
        ) {
          event.preventDefault();
          props.onClose();
        }
      }}
    >
      <p>
        {t(
          "Position is measured in pixels from the canvas center. Scale is a multiplier.",
        )}
      </p>
      {animated.length > 0 && (
        <p role="status">
          {t(
            "Animation controls these transform fields. Editing base values preserves the channels; animation may override the result.",
          )}
        </p>
      )}
      {blocked && <p role="status">{t(blocked)}</p>}
      <fieldset disabled={pending || !!blocked}>
        {fields.map(([key, label]) => (
          <Field key={key} label={t(label)}>
            <input
              type="number"
              step="any"
              value={draft[key]}
              onChange={(event) => {
                setDraft((current) => ({
                  ...current,
                  [key]: event.target.value,
                }));
                setError(null);
              }}
            />
          </Field>
        ))}
      </fieldset>
      {!valid && (
        <p role="alert">{t("Enter finite numbers for position and scale.")}</p>
      )}
      {error && <p role="alert">{t(error)}</p>}
      <div className="toolbar">
        <Button disabled={pending} onClick={props.onClose}>
          {t("Cancel")}
        </Button>
        <button
          type="submit"
          disabled={pending || !!blocked || !valid || !changed}
        >
          {t("Save text position and scale")}
        </button>
      </div>
    </form>
  );
}
