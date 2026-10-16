import { createContext, useContext, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { useWorkspace } from "../../context";
import { Button, Field } from "../../../shared/ui";
import { t, useLocale } from "../../../shared/locale";
import { compareTime } from "../../../model/time";
import { secondsForDisplay } from "../../../model/time";
import type { UnifiedFontCatalog, FontImportProtocol } from "./font-catalog";
import type { DerivedFontCatalog } from "./font-derived";
import { fontRecoveryBlocked } from "./font-recovery";
import { useSynthesisFonts } from "./font-synthesis-hook";
import { requestedFontStyle, fontSynthesisText } from "./font-synthesis";
import { editableTextFonts } from "./draft";
import { V4TextEditor } from "./editor";
import { V4FontImport } from "./font-import";
import { checkTextPlan, textPlanValues, textTimeInput } from "./planning";
import type {
  FontCatalog,
  FontImportRequest,
  FontImportResult,
  TextPlanRequest,
  TextPlanResult,
} from "./planning";
import type {
  V4TextEditorValue,
  V4TextInitial,
  V4TextScope,
  V4TextFont,
} from "./types";
const NO_FONTS: readonly V4TextFont[] = [];
import type { CanvasTextPlan, CanvasTextPlanning } from "./canvas-types";

export type TextWorkspaceOptions = {
  /** Supplied only by the executable versioned text-frame capability. */
  available: boolean;
  planCanvasText?: CanvasTextPlanning;
  importFont?: (
    request: FontImportRequest,
    protocol?: FontImportProtocol,
  ) => Promise<FontImportResult>;
  fontImportBlockedReason?: string | null;
};
type Draft = {
  mode: "create" | "edit";
  scope: V4TextScope;
  initial: V4TextInitial;
  elementId: string;
  trackId: string | null;
};
type Context = {
  available: boolean;
  disabled: boolean;
  canImport: boolean;
  beginCreate(): void;
  beginEdit(elementId: string): void;
  beginImport(): void;
  beginCanvasEdit(elementId: string): void;
  canvas: CanvasTextPlan | null;
  planCanvasText?: CanvasTextPlanning;
  commitCanvas(content: string): Promise<void>;
  cancelCanvas(): void;
  panel: ReactNode;
};
const TextContext = createContext<Context | null>(null);
const message = (error: unknown) =>
  error instanceof Error ? error.message : String(error);

export function V4TextAuthoringProvider({
  children,
  ...options
}: TextWorkspaceOptions & { children: ReactNode }) {
  const { language } = useLocale();
  const { controller, api, v4, busy } = useWorkspace();
  const [canvas, setCanvas] = useState<CanvasTextPlan | null>(null);
  const canvasInitial = useRef<V4TextInitial | null>(null);
  const canvasRead = useRef(false);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [review, setReview] = useState<TextPlanResult | null>(null);
  const [catalog, setCatalog] = useState<FontCatalog | null>(null);
  const [importing, setImporting] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const epoch = useRef(0),
    reading = useRef<AbortController | null>(null),
    writing = useRef(false),
    reviewScope = useRef<V4TextScope | null>(null);
  const snapshot = v4?.historical ?? v4?.head;
  const project = snapshot?.project;
  const resolvedFonts = useSynthesisFonts(
    api,
    project && snapshot
      ? { projectId: project.id, revision: snapshot.revision }
      : null,
    project?.fonts ?? NO_FONTS,
  );
  const sequence = project?.sequences.find(
    (scene) => scene.id === v4?.sequenceId,
  );
  const fontBlocked = fontRecoveryBlocked(v4?.fontRecovery);
  const recoveryBlocked =
    !!v4?.recovery?.record ||
    !!v4?.recovery?.error ||
    ["reading", "staging", "applying"].includes(v4?.recovery?.phase ?? "");
  const disabledReason = v4?.historical
    ? "Return to the current revision before editing."
    : fontBlocked
      ? "Resolve the original font import before starting another operation."
      : recoveryBlocked
        ? "Resolve the original edit before starting another operation."
        : (resolvedFonts.error ??
          (resolvedFonts.loading
            ? fontSynthesisText("Reading font synthesis proof…", language)
            : (options.fontImportBlockedReason ?? null)));
  const disabled = busy || pending || !!disabledReason;
  const currentScope = () => {
    const state = controller.getSnapshot();
    if (
      !api ||
      state.api !== api ||
      !state.v4 ||
      state.v4.historical ||
      !options.available
    )
      throw new Error("The edit target changed.");
    return {
      projectId: state.v4.head.project.id,
      revision: state.v4.head.revision,
      sequenceId: state.v4.sequenceId,
    };
  };
  const checkScope = (scope: V4TextScope, serial: number) => {
    const current = currentScope();
    if (
      serial !== epoch.current ||
      current.projectId !== scope.projectId ||
      current.revision !== scope.revision ||
      current.sequenceId !== scope.sequenceId
    )
      throw new Error("The edit target changed.");
  };
  useEffect(() => {
    epoch.current++;
    reading.current?.abort();
    reading.current = null;
    setDraft(null);
    setCanvas(null);
    canvasInitial.current = null;
    setReview(null);
    reviewScope.current = null;
    setImporting(false);
    setCatalog(null);
    setError(null);
    setNotice(null);
    setPending(false);
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
    options.available,
  ]);
  const read = async (
    task: (
      scope: V4TextScope,
      serial: number,
      signal: AbortSignal,
    ) => Promise<void>,
  ) => {
    if (disabled || !options.available || !api || !v4) return;
    reading.current?.abort();
    const abort = new AbortController();
    reading.current = abort;
    const scope = currentScope(),
      serial = ++epoch.current;
    setPending(true);
    setError(null);
    setNotice(null);
    try {
      await task(scope, serial, abort.signal);
    } catch (value) {
      if (serial === epoch.current && !abort.signal.aborted)
        setError(message(value));
    } finally {
      if (serial === epoch.current) {
        canvasRead.current = false;
        setPending(false);
        reading.current = null;
      }
    }
  };
  const [unifiedCatalog, setUnifiedCatalog] =
    useState<UnifiedFontCatalog | null>(null);
  const [derivedCatalog, setDerivedCatalog] =
    useState<DerivedFontCatalog | null>(null);
  const beginImport = () => {
    void read(async (scope, serial, signal) => {
      if (!options.importFont) return;
      const result = await api!.sdk.call(
        "opencut_font_v2_catalog",
        {},
        { signal },
      );
      checkScope(scope, serial);
      if (result.schema_version !== 2)
        throw new Error("The service returned an invalid font catalog.");
      const capabilities = await api!.sdk.hostCapabilities();
      checkScope(scope, serial);
      const unified = capabilities.tools.includes("opencut_font_catalog_v3")
        ? await api!.sdk.call("opencut_font_catalog_v3", {}, { signal })
        : null;
      checkScope(scope, serial);
      if (unified && unified.schema_version !== 3)
        throw new Error("The service returned an invalid font catalog.");
      const derived =
        !unified &&
        capabilities.tools.includes("opencut_font_derived_v1_catalog")
          ? await api!.sdk.call(
              "opencut_font_derived_v1_catalog",
              {},
              { signal },
            )
          : null;
      checkScope(scope, serial);
      if (derived && derived.schema_version !== 1)
        throw new Error(
          "The service returned an invalid derived font catalog.",
        );
      setUnifiedCatalog(unified);
      setDerivedCatalog(derived);
      setCatalog(result);
      setDraft(null);
      setReview(null);
      setImporting(true);
    });
  };
  const beginCreate = () => {
    if (!project || !sequence) return;
    const font = editableTextFonts(resolvedFonts.fonts)[0];
    if (!font) {
      if (options.importFont) beginImport();
      return;
    }
    void read(async (scope, serial, signal) => {
      const request: TextPlanRequest = {
        project_id: scope.projectId,
        expected_revision: scope.revision,
        sequence_id: scope.sequenceId,
        idempotency_key: crypto.randomUUID(),
        command: {
          kind: "defaults",
          font_id: font.id,
          start: textTimeInput(v4!.playhead),
        },
      };
      const result = await api!.sdk.call("opencut_text_v1_plan", request, {
        signal,
      });
      checkScope(scope, serial);
      checkTextPlan(request, result);
      const selectedTrack = sequence.tracks.find(
        (track) =>
          track.id === v4?.selectedId ||
          track.elements.some((element) => element.id === v4?.selectedId),
      );
      const initial = structuredClone(result.initial!);
      initial.text.font_style = requestedFontStyle(font);
      setDraft({
        mode: "create",
        scope,
        initial,
        elementId: crypto.randomUUID(),
        trackId:
          selectedTrack?.kind === "text" && !selectedTrack.locked
            ? selectedTrack.id
            : null,
      });
      setReview(null);
      setImporting(false);
    });
  };
  const beginEdit = (elementId: string) => {
    if (disabled || !options.available || !sequence) return;
    const track = sequence.tracks.find((track) =>
      track.elements.some((element) => element.id === elementId),
    );
    const element = track?.elements.find((element) => element.id === elementId);
    if (!track || track.locked || !element || element.content.kind !== "text")
      return;
    reading.current?.abort();
    epoch.current++;
    setDraft({
      mode: "edit",
      scope: { ...currentScope(), elementId },
      elementId,
      trackId: track.id,
      initial: {
        name: element.name,
        text: element.content.text,
        timeline_range: element.timeline_range,
      },
    });
    setReview(null);
    setImporting(false);
    setError(null);
    setNotice(null);
  };
  const canvasClock = `${v4?.playhead.ticks}/${v4?.playhead.timebase}:${v4?.audioPreview.phase}`;
  useEffect(() => {
    if (!canvas && !canvasRead.current) return;
    epoch.current++;
    reading.current?.abort();
    reading.current = null;
    canvasRead.current = false;
    setCanvas(null);
    canvasInitial.current = null;
    setPending(false);
  }, [canvasClock]);
  const beginCanvasEdit = (elementId: string) => {
    if (
      !options.planCanvasText ||
      !sequence ||
      !v4 ||
      canvas ||
      writing.current ||
      ["loading", "playing", "stalled", "unconfirmed"].includes(
        v4.audioPreview.phase,
      )
    )
      return;
    const track = sequence.tracks.find((track) =>
      track.elements.some((element) => element.id === elementId),
    );
    const element = track?.elements.find((element) => element.id === elementId);
    if (!track || track.locked || !element || element.content.kind !== "text")
      return;
    const initial: V4TextInitial = {
      name: element.name,
      text: element.content.text,
      timeline_range: element.timeline_range,
    };
    canvasRead.current = true;
    const capturedTime = structuredClone(v4.playhead);
    void read(async (scope, serial, signal) => {
      const target = { ...scope, elementId };
      const plan = await options.planCanvasText!(target, capturedTime, signal);
      checkScope(target, serial);
      const current = controller.getSnapshot().v4;
      if (
        !current ||
        compareTime(current.playhead, capturedTime) !== 0 ||
        ["loading", "playing", "stalled", "unconfirmed"].includes(
          current.audioPreview.phase,
        )
      )
        throw new Error("The canvas edit time changed.");
      canvasRead.current = false;
      if (
        plan.scope.projectId !== target.projectId ||
        plan.scope.revision !== target.revision ||
        plan.scope.sequenceId !== target.sequenceId ||
        plan.scope.elementId !== elementId ||
        plan.layer.elementId !== elementId
      )
        throw new Error("The edit target changed.");
      canvasInitial.current = initial;
      setDraft(null);
      setReview(null);
      setImporting(false);
      setCanvas(plan);
    });
  };
  const cancelCanvas = () => {
    if (!writing.current) {
      setCanvas(null);
      canvasInitial.current = null;
    }
  };
  const commitCanvas = async (content: string) => {
    const initial = canvasInitial.current;
    if (!canvas || !initial || !api || disabled || writing.current)
      throw new Error("Wait for the current edit to finish.");
    const scope = canvas.scope,
      serial = epoch.current;
    checkScope(scope, serial);
    if (content === initial.text.content) {
      cancelCanvas();
      return;
    }
    const track = sequence?.tracks.find((track) =>
      track.elements.some((element) => element.id === scope.elementId),
    );
    if (!track || track.locked) throw new Error("The edit target changed.");
    const request: TextPlanRequest = {
      project_id: scope.projectId,
      expected_revision: scope.revision,
      sequence_id: scope.sequenceId,
      idempotency_key: crypto.randomUUID(),
      command: {
        kind: "edit",
        target: { track_id: track.id, element_id: scope.elementId! },
        values: textPlanValues({
          ...initial,
          text: { ...initial.text, content },
        }),
      },
    };
    const abort = new AbortController();
    reading.current = abort;
    writing.current = true;
    setPending(true);
    try {
      const result = await api.sdk.call("opencut_text_v1_plan", request, {
        signal: abort.signal,
      });
      checkScope(scope, serial);
      checkTextPlan(request, result);
      if (result.transaction)
        await controller.v4.applyPlanned(result.transaction, scope.sequenceId);
      if (serial === epoch.current) {
        setCanvas(null);
        canvasInitial.current = null;
      }
    } finally {
      writing.current = false;
      if (serial === epoch.current) {
        canvasRead.current = false;
        setPending(false);
        reading.current = null;
      }
    }
  };
  const planText = async (value: V4TextEditorValue) => {
    if (!draft || !api || disabled)
      throw new Error("Wait for the current edit to finish.");
    const serial = epoch.current;
    checkScope(draft.scope, serial);
    const request: TextPlanRequest = {
      project_id: draft.scope.projectId,
      expected_revision: draft.scope.revision,
      sequence_id: draft.scope.sequenceId,
      idempotency_key: crypto.randomUUID(),
      command:
        draft.mode === "create"
          ? {
              kind: "create",
              element_id: draft.elementId,
              track: draft.trackId
                ? { kind: "existing", track_id: draft.trackId }
                : { kind: "new", track_id: crypto.randomUUID() },
              values: textPlanValues(value),
            }
          : {
              kind: "edit",
              target: { track_id: draft.trackId!, element_id: draft.elementId },
              values: textPlanValues(value),
            },
    };
    const abort = new AbortController();
    reading.current = abort;
    setPending(true);
    try {
      const result = await api.sdk.call("opencut_text_v1_plan", request, {
        signal: abort.signal,
      });
      checkScope(draft.scope, serial);
      checkTextPlan(request, result);
      if (result.transaction) {
        reviewScope.current = draft.scope;
        setReview(result);
      } else setNotice("No text changes to save.");
    } finally {
      if (serial === epoch.current) {
        canvasRead.current = false;
        setPending(false);
        reading.current = null;
      }
    }
  };
  const apply = async () => {
    const tx = review?.transaction;
    if (!tx || disabled || writing.current || !v4 || !api) return;
    const scope = reviewScope.current;
    if (!scope) return;
    const serial = epoch.current;
    writing.current = true;
    setPending(true);
    setError(null);
    try {
      checkScope(scope, serial);
      const receipt = await controller.v4.applyPlanned(tx, scope.sequenceId);
      const current = controller.getSnapshot();
      if (
        current.api === api &&
        current.v4?.head.project.id === scope.projectId &&
        current.v4.sequenceId === scope.sequenceId &&
        current.v4.head.revision === receipt.revision &&
        review.selection
      )
        controller.v4.select(review.selection.element_id);
      if (serial === epoch.current) {
        setReview(null);
        setNotice("Text changes saved.");
      }
    } catch (value) {
      if (serial === epoch.current) setError(message(value));
    } finally {
      writing.current = false;
      if (serial === epoch.current) setPending(false);
    }
  };
  const importFont = async (
    choiceId: string,
    requiredText: string,
    protocol?: FontImportProtocol,
  ) => {
    if (!api || !options.importFont || disabled)
      throw new Error("Font import is unavailable.");
    const scope = currentScope();
    const request: FontImportRequest = {
      project_id: scope.projectId,
      expected_revision: scope.revision,
      idempotency_key: crypto.randomUUID(),
      catalog_id: choiceId,
      font_id: crypto.randomUUID(),
      required_text: requiredText,
    };
    // The injected host action stages and recovers this exact typed font intent.
    // It is deliberately never translated into a public-apply transaction here.
    await options.importFont(request, protocol);
  };
  const panel =
    options.available && v4 && project && sequence ? (
      <div className="v4-text-authoring">
        {error && <p role="alert">{t(error)}</p>}
        {notice && <p role="status">{t(notice)}</p>}
        {draft && !review && (
          <>
            {draft.mode === "create" && (
              <Field label={t("Text track")}>
                <select
                  disabled={disabled}
                  value={draft.trackId ?? ""}
                  onChange={(event) =>
                    setDraft({ ...draft, trackId: event.target.value || null })
                  }
                >
                  <option value="">{t("New text track")}</option>
                  {sequence.tracks
                    .filter((track) => track.kind === "text" && !track.locked)
                    .map((track) => (
                      <option key={track.id} value={track.id}>
                        {track.name}
                      </option>
                    ))}
                </select>
              </Field>
            )}
            <V4TextEditor
              mode={draft.mode}
              scope={draft.scope}
              initial={draft.initial}
              fonts={resolvedFonts.fonts}
              available
              disabled={busy || pending}
              disabledReason={disabledReason}
              save={planText}
              close={() => setDraft(null)}
            />
          </>
        )}
        {review?.initial && review.transaction && (
          <section aria-label={t("Review text changes")}>
            <h3>{t("Review text changes")}</h3>
            <p className="v4-note-text">{review.initial.text.content}</p>
            <p>{t("Review the actual frame-aligned times before saving.")}</p>
            <dl className="v4-properties">
              <div>
                <dt>{t("Start (seconds)")}</dt>
                <dd>
                  {secondsForDisplay(
                    review.initial.timeline_range.start,
                  ).toFixed(9)}
                </dd>
              </div>
              <div>
                <dt>{t("Duration (seconds)")}</dt>
                <dd>
                  {secondsForDisplay(
                    review.initial.timeline_range.duration,
                  ).toFixed(9)}
                </dd>
              </div>
            </dl>
            <div className="toolbar">
              <Button
                className="primary"
                disabled={disabled}
                onClick={() => void apply()}
              >
                {t("Apply text changes")}
              </Button>
              <Button
                disabled={pending}
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
        {importing && catalog && (
          <V4FontImport
            scope={{ projectId: project.id, revision: v4.head.revision }}
            available={true}
            choices={[]}
            browserCatalog={catalog}
            unifiedCatalog={unifiedCatalog}
            readUnifiedSource={(request, signal) =>
              api!.sdk.call("opencut_font_source_v3", request, { signal })
            }
            derivedCatalog={derivedCatalog}
            readDerivedSource={(catalog_id, signal) =>
              api!.sdk.call(
                "opencut_font_derived_v1_source",
                { catalog_id },
                { signal },
              )
            }
            readSource={(catalog_id, signal) =>
              api!.sdk.call(
                "opencut_font_v2_source",
                { catalog_id },
                { signal },
              )
            }
            requiredText={draft?.initial.text.content ?? "OpenCut 你好"}
            disabled={busy || pending}
            disabledReason={disabledReason}
            importFont={importFont}
            close={() => setImporting(false)}
          />
        )}
      </div>
    ) : null;
  return (
    <TextContext.Provider
      value={{
        available: options.available,
        disabled,
        canImport: !!options.importFont,
        beginCreate,
        beginEdit,
        beginImport,
        beginCanvasEdit,
        canvas,
        planCanvasText: options.planCanvasText,
        commitCanvas,
        cancelCanvas,
        panel,
      }}
    >
      {children}
    </TextContext.Provider>
  );
}

export function V4TextActions() {
  useLocale();
  const context = useContext(TextContext);
  if (!context?.available) return null;
  return (
    <>
      <Button disabled={context.disabled} onClick={context.beginCreate}>
        {t("Add text at playhead")}
      </Button>
      {context.canImport && (
        <Button disabled={context.disabled} onClick={context.beginImport}>
          {t("Import font")}
        </Button>
      )}
    </>
  );
}
export function V4TextEditButton({
  elementId,
  locked,
}: {
  elementId: string;
  locked: boolean;
}) {
  useLocale();
  const context = useContext(TextContext);
  if (!context?.available) return null;
  return (
    <Button
      disabled={context.disabled || locked}
      onClick={() => context.beginEdit(elementId)}
    >
      {t("Edit text")}
    </Button>
  );
}
export function V4TextPanel() {
  return useContext(TextContext)?.panel ?? null;
}

export function useCanvasTextAuthoring() {
  return useContext(TextContext);
}
