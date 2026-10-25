import { Select } from "../../shared/controls";
import { useEffect, useRef, useState } from "react";
import { useWorkspace } from "../context";
import { Field } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { editRecoveryBlocked } from "./edit-recovery";
import type { Element } from "./types";

type BlendMode = NonNullable<Element["visual"]>["blend_mode"];
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
export function BlendEditor({
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
      scene = state.scene;
    const snapshot = scene && (scene.historical ?? scene.head);
    const track = snapshot?.project.sequences
      .find((s) => s.id === scene?.sequenceId)
      ?.tracks.find((tr) => tr.id === trackId);
    const element = track?.elements.find((e) => e.id === elementId);
    return {
      api: state.api,
      connection: state.api?.connectionIdentity,
      projectId: scene?.head.project.id,
      revision: scene?.head.revision,
      sequenceId: scene?.sequenceId,
      selectedId: scene?.selectedId,
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
      scene = state.scene;
    if (
      !scene ||
      state.api !== captured.api ||
      state.api?.connectionIdentity !== captured.connection ||
      scene.head.project.id !== captured.projectId ||
      scene.head.revision !== captured.revision ||
      scene.sequenceId !== captured.sequenceId ||
      scene.selectedId !== captured.selectedId ||
      trackId !== captured.trackId ||
      elementId !== captured.elementId
    )
      return "The element changed. Reopen its blend controls before editing.";
    if (scene.historical) return "Return to the current revision before editing.";
    const track = scene.head.project.sequences
      .find((s) => s.id === captured.sequenceId)
      ?.tracks.find((tr) => tr.id === captured.trackId);
    const element = track?.elements.find((e) => e.id === captured.elementId);
    if (
      !element?.visual ||
      JSON.stringify(element) !== JSON.stringify(captured.element)
    )
      return "The element changed. Reopen its blend controls before editing.";
    if (track?.locked) return "Unlock the track before editing its blend mode.";
    if (state.busy || scene.gainDraft)
      return "Wait for the current edit to finish.";
    if (
      editRecoveryBlocked(scene.recovery) ||
      controller.scene.recovery.blocked() ||
      scene.recovery?.record ||
      scene.recovery?.error ||
      ["reading", "staging", "applying"].includes(scene.recovery?.phase ?? "") ||
      ["applying", "unconfirmed"].includes(
        scene.timelineEdit.attempt?.phase ?? "",
      ) ||
      ["applying", "unconfirmed"].includes(scene.clipboard.attempt?.phase ?? "")
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
      await controller.scene.write("Edit blend mode", [
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
        <Select
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
        </Select>
      </Field>
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
