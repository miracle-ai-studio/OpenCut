import { createContext, useContext, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { useWorkspace } from "../../context";
import { Button, Field } from "../../../shared/ui";
import { t, useLocale } from "../../../shared/locale";
import { secondsForDisplay } from "../../../model/time";
import { fontRecoveryBlocked } from "../text/font-recovery";
import { V4GraphicEditor } from "./editor";
import { checkGraphicPlan } from "./planning";
import { graphicKinds } from "./types";
import type {
  GraphicInitial,
  GraphicKind,
  GraphicPlanRequest,
  GraphicPlanResult,
  GraphicScope,
  GraphicValues,
} from "./types";
type Draft = {
  mode: "create" | "edit";
  scope: GraphicScope;
  initial: GraphicInitial;
  elementId: string;
  trackId: string | null;
};
type Context = {
  available: boolean;
  disabled: boolean;
  beginCreate(kind: GraphicKind): void;
  beginEdit(id: string): void;
  panel: ReactNode;
};
const GraphicContext = createContext<Context | null>(null);
const message = (error: unknown) =>
  error instanceof Error ? error.message : String(error);
const shapeLabel = (kind: string) =>
  t(
    kind === "rectangle"
      ? "Rectangle"
      : kind === "ellipse"
        ? "Ellipse"
        : kind === "polygon"
          ? "Polygon"
          : "Star",
  );
export function V4GraphicAuthoringProvider({
  children,
}: {
  children: ReactNode;
}) {
  useLocale();
  const { api, v4, busy, controller } = useWorkspace();
  const [available, setAvailable] = useState(false),
    [draft, setDraft] = useState<Draft | null>(null),
    [review, setReview] = useState<GraphicPlanResult | null>(null),
    [pending, setPending] = useState(false),
    [error, setError] = useState<string | null>(null);
  const epoch = useRef(0),
    reading = useRef<AbortController | null>(null),
    writing = useRef(false);
  const sequence = (v4?.historical ?? v4?.head)?.project.sequences.find(
    (s) => s.id === v4?.sequenceId,
  );
  const disabled =
    busy ||
    pending ||
    !!v4?.historical ||
    fontRecoveryBlocked(v4?.fontRecovery) ||
    !!v4?.recovery?.record ||
    !!v4?.recovery?.error ||
    ["reading", "staging", "applying"].includes(v4?.recovery?.phase ?? "") ||
    controller.v4.captionRecovery.blocked();
  useEffect(() => {
    let live = true;
    setAvailable(false);
    if (api)
      void api.sdk
        .hostCapabilities()
        .then((value) => {
          if (live)
            setAvailable(value.tools.includes("opencut_graphic_v1_plan"));
        })
        .catch((value) => {
          if (live) setError(message(value));
        });
    return () => {
      live = false;
    };
  }, [api, api?.connectionIdentity]);
  useEffect(() => {
    epoch.current++;
    reading.current?.abort();
    setDraft(null);
    setReview(null);
    setPending(false);
    setError(null);
    return () => {
      epoch.current++;
      reading.current?.abort();
    };
  }, [
    api,
    api?.connectionIdentity,
    v4?.head.project.id,
    v4?.head.revision,
    v4?.historical?.revision,
    v4?.sequenceId,
    v4?.selectedId,
  ]);
  const currentScope = (): GraphicScope => {
    const state = controller.getSnapshot();
    if (
      !api ||
      state.api !== api ||
      !state.v4 ||
      state.v4.historical ||
      !available
    )
      throw new Error("The edit target changed.");
    return {
      projectId: state.v4.head.project.id,
      revision: state.v4.head.revision,
      sequenceId: state.v4.sequenceId,
    };
  };
  const checkScope = (scope: GraphicScope, serial: number) => {
    const current = currentScope();
    if (
      serial !== epoch.current ||
      current.projectId !== scope.projectId ||
      current.revision !== scope.revision ||
      current.sequenceId !== scope.sequenceId
    )
      throw new Error("The edit target changed.");
  };
  const beginCreate = (geometry: GraphicKind) => {
    if (disabled || !available || !api || !v4) return;
    reading.current?.abort();
    const abort = new AbortController(),
      serial = ++epoch.current,
      scope = currentScope();
    reading.current = abort;
    setPending(true);
    setError(null);
    const request: GraphicPlanRequest = {
      project_id: scope.projectId,
      expected_revision: scope.revision,
      sequence_id: scope.sequenceId,
      idempotency_key: crypto.randomUUID(),
      command: {
        kind: "defaults",
        geometry,
        start: { kind: "exact", time: structuredClone(v4.playhead) },
      },
    };
    void (async () => {
      try {
        const result = await api.sdk.call("opencut_graphic_v1_plan", request, {
          signal: abort.signal,
        });
        checkScope(scope, serial);
        checkGraphicPlan(request, result);
        setReview(null);
        setDraft({
          mode: "create",
          scope,
          initial: result.initial!,
          elementId: crypto.randomUUID(),
          trackId: null,
        });
      } catch (value) {
        if (serial === epoch.current && !abort.signal.aborted)
          setError(message(value));
      } finally {
        if (serial === epoch.current) setPending(false);
      }
    })();
  };
  const beginEdit = (id: string) => {
    if (disabled || !available || !sequence) return;
    const track = sequence.tracks.find((t) =>
        t.elements.some((e) => e.id === id),
      ),
      element = track?.elements.find((e) => e.id === id);
    if (
      !track ||
      track.locked ||
      !element ||
      element.content.kind !== "graphic" ||
      !element.visual ||
      !["rectangle", "ellipse", "polygon", "star"].includes(
        element.content.graphic.geometry.kind,
      )
    )
      return;
    reading.current?.abort();
    epoch.current++;
    setError(null);
    setReview(null);
    setDraft({
      mode: "edit",
      scope: currentScope(),
      elementId: id,
      trackId: track.id,
      initial: structuredClone({
        name: element.name,
        graphic: element.content.graphic,
        transform: element.visual.transform,
        opacity: element.visual.opacity,
        timeline_range: element.timeline_range,
      }),
    });
  };
  const plan = async (values: GraphicValues) => {
    if (!draft || !api || disabled || writing.current)
      throw new Error("Wait for the current edit to finish.");
    const serial = epoch.current;
    checkScope(draft.scope, serial);
    const abort = new AbortController();
    reading.current = abort;
    setPending(true);
    setError(null);
    const request: GraphicPlanRequest = {
      project_id: draft.scope.projectId,
      expected_revision: draft.scope.revision,
      sequence_id: draft.scope.sequenceId,
      idempotency_key: crypto.randomUUID(),
      command:
        draft.mode === "edit"
          ? {
              kind: "edit",
              target: { track_id: draft.trackId!, element_id: draft.elementId },
              values,
            }
          : {
              kind: "create",
              element_id: draft.elementId,
              track: draft.trackId
                ? { kind: "existing", track_id: draft.trackId }
                : { kind: "new", track_id: crypto.randomUUID() },
              values,
            },
    };
    try {
      const result = await api.sdk.call("opencut_graphic_v1_plan", request, {
        signal: abort.signal,
      });
      checkScope(draft.scope, serial);
      checkGraphicPlan(request, result);
      setDraft({ ...draft, initial: result.initial! });
      setReview(result);
      if (!result.transaction) setError("No graphic changes to save.");
    } finally {
      if (serial === epoch.current) setPending(false);
    }
  };
  const apply = async () => {
    if (!draft || !review?.transaction || disabled || writing.current) return;
    const captured = review,
      scope = draft.scope,
      serial = epoch.current;
    writing.current = true;
    setPending(true);
    setError(null);
    try {
      checkScope(scope, serial);
      await controller.v4.applyPlanned(captured.transaction!, scope.sequenceId);
      const state = controller.getSnapshot();
      if (
        state.api === api &&
        state.v4?.head.project.id === scope.projectId &&
        state.v4.sequenceId === scope.sequenceId &&
        !state.v4.historical &&
        captured.selection
      )
        controller.v4.select(captured.selection.element_id);
      if (serial === epoch.current) {
        setDraft(null);
        setReview(null);
      }
    } catch (value) {
      if (serial === epoch.current) setError(message(value));
      else controller.report(value);
    } finally {
      writing.current = false;
      if (serial === epoch.current) setPending(false);
    }
  };
  const panel =
    available && draft ? (
      <section className="v4-text-authoring" aria-label={t("Graphic editing")}>
        {error && <p role="alert">{t(error)}</p>}
        {draft.mode === "create" && (
          <Field label={t("Graphic track")}>
            <select
              disabled={disabled}
              value={draft.trackId ?? ""}
              onChange={(event) => {
                setDraft({ ...draft, trackId: event.target.value || null });
                setReview(null);
              }}
            >
              <option value="">{t("New graphic track")}</option>
              {sequence?.tracks
                .filter((track) => track.kind === "graphic" && !track.locked)
                .map((track) => (
                  <option key={track.id} value={track.id}>
                    {track.name}
                  </option>
                ))}
            </select>
          </Field>
        )}
        {!review?.transaction ? (
          <V4GraphicEditor
            key={draft.elementId}
            initial={draft.initial}
            disabled={disabled}
            save={plan}
            close={() => {
              if (!writing.current) {
                setDraft(null);
                setReview(null);
              }
            }}
          />
        ) : (
          <section aria-label={t("Review graphic changes")}>
            <h3>{t("Review graphic changes")}</h3>
            <p>
              {review.initial!.name} ·{" "}
              {shapeLabel(review.initial!.graphic.geometry.kind)} ·{" "}
              {secondsForDisplay(review.initial!.timeline_range.start)} s ·{" "}
              {secondsForDisplay(review.initial!.timeline_range.duration)} s
            </p>
            <p>{t("The project changes only after you apply this plan.")}</p>
            <div className="toolbar">
              <Button disabled={disabled} onClick={() => void apply()}>
                {t("Apply graphic changes")}
              </Button>
              <Button disabled={disabled} onClick={() => setReview(null)}>
                {t("Back to graphic edit")}
              </Button>
              <Button
                disabled={disabled}
                onClick={() => {
                  setReview(null);
                  setDraft(null);
                }}
              >
                {t("Cancel")}
              </Button>
            </div>
          </section>
        )}
      </section>
    ) : error ? (
      <p role="alert">{t(error)}</p>
    ) : null;
  return (
    <GraphicContext.Provider
      value={{ available, disabled, beginCreate, beginEdit, panel }}
    >
      {children}
    </GraphicContext.Provider>
  );
}
export function V4GraphicActions({
  allowedKinds = graphicKinds,
}: { allowedKinds?: readonly GraphicKind[] } = {}) {
  useLocale();
  const context = useContext(GraphicContext),
    [kind, setKind] = useState<GraphicKind>("rectangle");
  if (!context?.available) return null;
  const kinds = graphicKinds.filter(value => allowedKinds.includes(value)),
    selected = kinds.includes(kind) ? kind : kinds[0];
  if (!selected) return <p role="status">{t("No matching shapes.")}</p>;
  return (
    <>
      <Field label={t("Graphic shape")}>
        <select
          disabled={context.disabled}
          value={selected}
          onChange={(event) => {
            const value = kinds.find(kind => kind === event.target.value);
            if (value) setKind(value);
          }}
        >
          {kinds.map((value) => (
            <option key={value} value={value}>
              {shapeLabel(value)}
            </option>
          ))}
        </select>
      </Field>
      <Button
        disabled={context.disabled}
        onClick={() => context.beginCreate(selected)}
      >
        {t("Add graphic")}
      </Button>
    </>
  );
}
export function V4GraphicEditButton({
  elementId,
  locked,
}: {
  elementId: string;
  locked: boolean;
}) {
  useLocale();
  const context = useContext(GraphicContext);
  return context?.available ? (
    <Button
      disabled={locked || context.disabled}
      onClick={() => context.beginEdit(elementId)}
    >
      {t("Edit graphic")}
    </Button>
  ) : null;
}
export function V4GraphicPanel() {
  return useContext(GraphicContext)?.panel ?? null;
}
