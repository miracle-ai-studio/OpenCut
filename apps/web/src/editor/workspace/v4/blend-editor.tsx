import { useEffect, useRef, useState } from "react";
import { useWorkspace } from "../context";
import { Field } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { fontRecoveryBlocked } from "./text/font-recovery";
import type { V4Element } from "./types";

type BlendMode = NonNullable<V4Element["visual"]>["blend_mode"];
// Classic params/registry.ts order and wire values; labels are presentation only.
export const blendModeLabels: Record<BlendMode, string> = {
  normal: "Normal",
  darken: "Darken",
  multiply: "Multiply",
  "color-burn": "Color Burn",
  lighten: "Lighten",
  screen: "Screen",
  "plus-lighter": "Plus Lighter",
  "color-dodge": "Color Dodge",
  overlay: "Overlay",
  "soft-light": "Soft Light",
  "hard-light": "Hard Light",
  difference: "Difference",
  exclusion: "Exclusion",
  hue: "Hue",
  saturation: "Saturation",
  color: "Color",
  luminosity: "Luminosity",
};
const isBlendMode = (value: string): value is BlendMode =>
  Object.hasOwn(blendModeLabels, value);

/** A base-property edit through the shared fixed-head transaction and recovery.
 * Capture the entire element so this form cannot erase concurrent changes. */
export function V4BlendEditor({
  trackId,
  elementId,
}: {
  trackId: string;
  elementId: string;
}) {
  useLocale();
  const { controller } = useWorkspace();
  const [captured] = useState(() => {
    const state = controller.getSnapshot(),
      v4 = state.v4;
    const snapshot = v4 && (v4.historical ?? v4.head);
    const track = snapshot?.project.sequences
      .find((s) => s.id === v4?.sequenceId)
      ?.tracks.find((tr) => tr.id === trackId);
    const element = track?.elements.find((e) => e.id === elementId);
    return {
      api: state.api,
      connection: state.api?.connectionIdentity,
      projectId: v4?.head.project.id,
      revision: v4?.head.revision,
      sequenceId: v4?.sequenceId,
      selectedId: v4?.selectedId,
      trackId,
      elementId,
      element: element ? structuredClone(element) : null,
    };
  });
  const [draft, setDraft] = useState<BlendMode | "">(
    () => captured.element?.visual?.blend_mode ?? "",
  );
  const [pending, setPending] = useState(false),
    [error, setError] = useState<string | null>(null);
  const active = useRef(true),
    inFlight = useRef(false);
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
      state.api?.connectionIdentity !== captured.connection ||
      v4.head.project.id !== captured.projectId ||
      v4.head.revision !== captured.revision ||
      v4.sequenceId !== captured.sequenceId ||
      v4.selectedId !== captured.selectedId ||
      trackId !== captured.trackId ||
      elementId !== captured.elementId
    )
      return "The element changed. Reopen its blend controls before editing.";
    if (v4.historical) return "Return to the current revision before editing.";
    const track = v4.head.project.sequences
      .find((s) => s.id === captured.sequenceId)
      ?.tracks.find((tr) => tr.id === captured.trackId);
    const element = track?.elements.find((e) => e.id === captured.elementId);
    if (
      !element?.visual ||
      JSON.stringify(element) !== JSON.stringify(captured.element)
    )
      return "The element changed. Reopen its blend controls before editing.";
    if (track?.locked) return "Unlock the track before editing its blend mode.";
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
  const blocked = reason(),
    changed = draft !== captured.element?.visual?.blend_mode;
  const save = async () => {
    if (inFlight.current || !changed || !isBlendMode(draft)) return;
    const problem = reason();
    if (problem) {
      setError(problem);
      return;
    }
    if (!captured.element?.visual) return;
    const element = structuredClone(captured.element);
    element.visual!.blend_mode = draft;
    inFlight.current = true;
    setPending(true);
    setError(null);
    try {
      await controller.v4.write("Edit blend mode", [
        {
          type: "element_put",
          sequence_id: captured.sequenceId!,
          track_id: captured.trackId,
          element,
        },
      ]);
    } catch (cause) {
      if (active.current)
        setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      inFlight.current = false;
      if (active.current) setPending(false);
    }
  };
  const unsupported =
    captured.element?.visual &&
    (captured.element.visual.effects.some((effect) => effect.enabled) ||
      captured.element.visual.masks.some((mask) => mask.enabled && (mask.inverted || mask.stroke?.width_px !== 0 || mask.feather?.unit !== "classic_pixels" || !["rectangle", "ellipse"].includes(mask.shape.kind))));
  return (
    <form
      aria-label={t("Edit blend mode")}
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
      onKeyDown={(event) => event.stopPropagation()}
    >
      <Field label={t("Blend mode")}>
        <select
          value={draft}
          disabled={pending || !!blocked}
          onChange={(event) => {
            if (isBlendMode(event.target.value)) {
              setDraft(event.target.value);
              setError(null);
            }
          }}
        >
          {Object.entries(blendModeLabels).map(([value, label]) => (
            <option key={value} value={value}>
              {t(label)}
            </option>
          ))}
        </select>
      </Field>
      {unsupported && (
        <p role="status">
          {t(
            "This element contains effects or masks that are not supported by scene rendering yet.",
          )}
        </p>
      )}
      {blocked && <p role="status">{t(blocked)}</p>}
      {error && <p role="alert">{t(error)}</p>}
      <button
        type="submit"
        disabled={pending || !!blocked || !changed || !isBlendMode(draft)}
      >
        {t(pending ? "Saving blend mode…" : "Save blend mode")}
      </button>
    </form>
  );
}
