import { Select } from "../../../shared/controls";
import { useWorkbenchView } from "../workbench/view";
import { createContext, useContext, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { useEditingWorkspace as useWorkspace } from "../../context";
import { Button, Field } from "../../../shared/ui";
import { Plus, Type as TypeIcon } from "lucide-react";
import { t, useLocale } from "../../../shared/locale";
import { compareTime } from "../../../model/time";
import { secondsForDisplay } from "../../../model/time";

import { editRecoveryBlocked } from "../edit-recovery";
import { useSynthesisFonts } from "./font-synthesis-hook";
import { requestedFontStyle, fontSynthesisText } from "./font-synthesis";
import { editableTextFonts } from "./draft";
import { TextEditor } from "./editor";
import { FontImport } from "./font-import";
import { checkTextPlan, textPlanValues, textTimeInput } from "./planning";
import type {
  FontCatalog,
  FontImportRequest,
  FontImportResult,
  TextPlanRequest,
  TextPlanResult,
} from "./planning";
import type {
  TextEditorValue,
  TextInitial,
  TextScope,
  TextFont,
} from "./types";
const NO_FONTS: readonly TextFont[] = [];
import type { CanvasTextPlan, CanvasTextPlanning } from "./canvas-types";

export type TextWorkspaceOptions = {
  /** Supplied only by the executable versioned text-frame capability. */
  available: boolean;
  planCanvasText?: CanvasTextPlanning;
  importFont?: (request: FontImportRequest) => Promise<FontImportResult>;
  fontImportBlockedReason?: string | null;
};
type Draft = {
  mode: "create" | "edit";
  scope: TextScope;
  initial: TextInitial;
  elementId: string;
  trackId: string | null;
};
type Context = {
  available: boolean;
  disabled: boolean;
  canImport: boolean;
  beginCreate(fontId?: string, insert?: boolean): void;
  beginEdit(elementId: string): void;
  beginImport(): void;
  fontApplication(fontId: string): FontApplication;
  beginCanvasEdit(elementId: string): void;
  canvas: CanvasTextPlan | null;
  planCanvasText?: CanvasTextPlanning;
  commitCanvas(content: string): Promise<void>;
  cancelCanvas(): void;
  panel: ReactNode;
};
type FontApplication = {
  font: TextFont | null;
  targetName: string | null;
  disabledReason: string | null;
  apply(): void;
};
const TextContext = createContext<Context | null>(null);
const message = (error: unknown) =>
  error instanceof Error ? error.message : String(error);

export function TextAuthoringProvider({
  children,
  ...options
}: TextWorkspaceOptions & { children: ReactNode }) {
  const { language } = useLocale();
  const { controller, api, scene, busy } = useWorkspace();
  const [canvas, setCanvas] = useState<CanvasTextPlan | null>(null);
  const [canvasRequest, setCanvasRequest] = useState<{
    api: NonNullable<typeof api>;
    identity: string | null;
    project: string;
    revision: string;
    sequence: string;
    selection: string | null;
    time: string;
    elementId: string;
  } | null>(null);
  const canvasInitial = useRef<TextInitial | null>(null);
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
    reviewScope = useRef<TextScope | null>(null);
  const snapshot = scene?.historical ?? scene?.head;
  const project = snapshot?.project;
  const resolvedFonts = useSynthesisFonts(
    api,
    project && snapshot
      ? { projectId: project.id, revision: snapshot.revision }
      : null,
    project?.fonts ?? NO_FONTS,
  );
  const sequence = project?.sequences.find(
    (sequence) => sequence.id === scene?.sequenceId,
  );
  const recoveryBlocked = editRecoveryBlocked(scene?.recovery);
  const disabledReason = scene?.historical
    ? "Return to the current revision before editing."
    : recoveryBlocked
      ? ["reading", "staging", "applying"].includes(
          scene?.recovery?.phase ?? "",
        )
        ? "Wait for the current edit to finish."
        : scene?.recovery?.record?.context.intent.kind === "font_import"
          ? "Resolve the original font import before starting another operation."
          : "Resolve the original edit before starting another operation."
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
      !state.scene ||
      state.scene.historical ||
      !options.available
    )
      throw new Error("The edit target changed.");
    return {
      projectId: state.scene.head.project.id,
      revision: state.scene.head.revision,
      sequenceId: state.scene.sequenceId,
    };
  };
  const checkScope = (scope: TextScope, serial: number) => {
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
    scene?.head.project.id,
    scene?.head.revision,
    scene?.historical?.revision,
    scene?.sequenceId,
    scene?.selectedId,
    JSON.stringify(scene?.timelineEdit.selection),
    options.available,
  ]);
  const read = async (
    task: (
      scope: TextScope,
      serial: number,
      signal: AbortSignal,
    ) => Promise<void>,
  ) => {
    if (disabled || !options.available || !api || !scene) return;
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
  const beginImport = () => {
    void read(async (scope, serial, signal) => {
      if (!options.importFont) return;
      const result = await api!.sdk.call(
        "opencut_font_catalog",
        {},
        { signal },
      );
      checkScope(scope, serial);
      setCatalog(result);
      setDraft(null);
      setReview(null);
      setImporting(true);
    });
  };
  const beginCreate = (fontId?: string, insert = false) => {
    if (!project || !sequence) return;
    const at = structuredClone(controller.scene.currentPlayhead());
    const fonts = editableTextFonts(resolvedFonts.fonts);
    const font = fontId ? fonts.find((font) => font.id === fontId) : fonts[0];
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
          start: textTimeInput(at),
        },
      };
      const result = await api!.sdk.call("opencut_text_plan", request, {
        signal,
      });
      checkScope(scope, serial);
      checkTextPlan(request, result);
      const selectedTrack = sequence.tracks.find(
        (track) =>
          track.id === scene?.selectedId ||
          track.elements.some((element) => element.id === scene?.selectedId),
      );
      const initial = structuredClone(result.initial!);
      initial.text.font_style = requestedFontStyle(font);
      if (insert) {
        const create: TextPlanRequest = {
          ...request,
          idempotency_key: crypto.randomUUID(),
          command: {
            kind: "create",
            element_id: crypto.randomUUID(),
            track: { kind: "new", track_id: crypto.randomUUID() },
            values: textPlanValues(initial),
          },
        };
        const planned = await api!.sdk.call("opencut_text_plan", create, {
          signal,
        });
        checkScope(scope, serial);
        checkTextPlan(create, planned);
        if (!planned.transaction) throw Error("No text changes to save.");
        writing.current = true;
        try {
          await controller.scene.applyPlanned(
            planned.transaction,
            scope.sequenceId,
          );
        } finally {
          writing.current = false;
        }
        const current = controller.getSnapshot();
        if (
          current.api === api &&
          current.scene?.head.project.id === scope.projectId &&
          current.scene.sequenceId === scope.sequenceId &&
          !current.scene.historical &&
          planned.selection
        )
          controller.scene.select(planned.selection.element_id);
        return;
      }
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
  const fontApplication = (fontId: string): FontApplication => {
    const track = sequence?.tracks.find((track) =>
      track.elements.some((element) => element.id === scene?.selectedId),
    );
    const element = track?.elements.find(
      (element) => element.id === scene?.selectedId,
    );
    const font = editableTextFonts(resolvedFonts.fonts).find(
      (font) => font.id === fontId && font.face_index === 0,
    );
    const reason = !options.available
      ? "Font application is unavailable."
      : (disabledReason ??
        (busy || pending
          ? "Wait for the current edit to finish."
          : draft || canvas || review || importing
            ? "Finish or cancel the current text edit first."
            : (scene?.timelineEdit.selection.length ?? 0) > 1
              ? "Select one editable text or subtitle clip."
              : !element || element.content.kind !== "text"
                ? "Select one editable text or subtitle clip."
                : track?.locked
                  ? "Unlock the text track before applying a font."
                  : !font
                    ? "This font weight or style is unavailable for editing."
                    : null));
    return {
      font: font ?? null,
      targetName: element?.content.kind === "text" ? element.name : null,
      disabledReason: reason,
      apply() {
        if (
          reason ||
          !font ||
          !track ||
          !element ||
          element.content.kind !== "text" ||
          writing.current
        )
          return;
        const targetId = element.id;
        const initial = {
          name: element.name,
          timeline_range: element.timeline_range,
          text: {
            ...element.content.text,
            font: { kind: "locked" as const, font_id: font.id },
            font_weight:
              font.weight === 700 ? ("bold" as const) : ("normal" as const),
            font_style: requestedFontStyle(font),
          },
        };
        writing.current = true;
        void read(async (scope, serial, signal) => {
          const checkTarget = () => {
            checkScope(scope, serial);
            const current = controller.getSnapshot().scene;
            if (
              current?.selectedId !== targetId ||
              current.timelineEdit.selection.length > 1
            )
              throw new Error("The edit target changed.");
          };
          checkTarget();
          const request: TextPlanRequest = {
            project_id: scope.projectId,
            expected_revision: scope.revision,
            sequence_id: scope.sequenceId,
            idempotency_key: crypto.randomUUID(),
            command: {
              kind: "edit",
              target: { track_id: track.id, element_id: targetId },
              values: textPlanValues(initial),
            },
          };
          const result = await api!.sdk.call("opencut_text_plan", request, {
            signal,
          });
          checkTarget();
          checkTextPlan(request, result);
          if (result.transaction)
            await controller.scene.applyPlanned(
              result.transaction,
              scope.sequenceId,
            );
          else setNotice("The selected text already uses this font.");
        }).finally(() => {
          writing.current = false;
        });
      },
    };
  };
  const canvasClock = `${scene?.playhead.ticks}/${scene?.playhead.timebase}:${scene?.audioPreview.phase}`;
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
  const beginCanvasEditNow = (elementId: string) => {
    if (
      disabled ||
      !options.available ||
      !options.planCanvasText ||
      !sequence ||
      !scene ||
      canvas ||
      writing.current ||
      ["loading", "playing", "stalled", "unconfirmed"].includes(
        scene.audioPreview.phase,
      )
    )
      return;
    const track = sequence.tracks.find((track) =>
      track.elements.some((element) => element.id === elementId),
    );
    const element = track?.elements.find((element) => element.id === elementId);
    if (!track || track.locked || !element || element.content.kind !== "text")
      return;
    const initial: TextInitial = {
      name: element.name,
      text: element.content.text,
      timeline_range: element.timeline_range,
    };
    canvasRead.current = true;
    const capturedTime = structuredClone(scene.playhead);
    void read(async (scope, serial, signal) => {
      const target = { ...scope, elementId };
      const plan = await options.planCanvasText!(target, capturedTime, signal);
      checkScope(target, serial);
      const current = controller.getSnapshot().scene;
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
  const beginCanvasEdit = (elementId: string) => {
    const latest = controller.getSnapshot(),
      state = latest.scene;
    if (!latest.api || !state || state.historical) return;
    setCanvasRequest({
      api: latest.api,
      identity: latest.api.connectionIdentity,
      project: state.head.project.id,
      revision: state.head.revision,
      sequence: state.sequenceId,
      selection: state.selectedId,
      time: JSON.stringify(state.playhead),
      elementId,
    });
  };
  // Actual-alpha hit selection and its edit request arrive in one event. Start
  // only after the selection retirement effect above has processed that render.
  useEffect(() => {
    if (!canvasRequest) return;
    setCanvasRequest(null);
    const latest = controller.getSnapshot(),
      state = latest.scene;
    if (
      !latest.api ||
      latest.api !== canvasRequest.api ||
      latest.api.connectionIdentity !== canvasRequest.identity ||
      !state ||
      state.historical ||
      state.head.project.id !== canvasRequest.project ||
      state.head.revision !== canvasRequest.revision ||
      state.sequenceId !== canvasRequest.sequence ||
      state.selectedId !== canvasRequest.selection ||
      JSON.stringify(state.playhead) !== canvasRequest.time
    )
      return;
    beginCanvasEditNow(canvasRequest.elementId);
  }, [
    canvasRequest,
    api,
    api?.connectionIdentity,
    scene?.head.project.id,
    scene?.head.revision,
    scene?.historical?.revision,
    scene?.sequenceId,
    scene?.selectedId,
    canvasClock,
    disabled,
    options.available,
  ]);
  const cancelCanvas = () => {
    if (!writing.current) {
      setCanvasRequest(null);
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
      const result = await api.sdk.call("opencut_text_plan", request, {
        signal: abort.signal,
      });
      checkScope(scope, serial);
      checkTextPlan(request, result);
      if (result.transaction)
        await controller.scene.applyPlanned(
          result.transaction,
          scope.sequenceId,
        );
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
  const planText = async (value: TextEditorValue) => {
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
      const result = await api.sdk.call("opencut_text_plan", request, {
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
    if (!tx || disabled || writing.current || !scene || !api) return;
    const scope = reviewScope.current;
    if (!scope) return;
    const serial = epoch.current;
    writing.current = true;
    setPending(true);
    setError(null);
    try {
      checkScope(scope, serial);
      const receipt = await controller.scene.applyPlanned(tx, scope.sequenceId);
      const current = controller.getSnapshot();
      if (
        current.api === api &&
        current.scene?.head.project.id === scope.projectId &&
        current.scene.sequenceId === scope.sequenceId &&
        current.scene.head.revision === receipt.revision &&
        review.selection
      )
        controller.scene.select(review.selection.element_id);
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
  const importFont = async (choiceId: string, requiredText: string) => {
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
    await options.importFont(request);
  };
  const panel =
    options.available && scene && project && sequence ? (
      <div className="editor-text-authoring">
        {error && <p role="alert">{t(error)}</p>}
        {notice && <p role="status">{t(notice)}</p>}
        {draft && !review && (
          <>
            {draft.mode === "create" && (
              <Field label={t("Text track")}>
                <Select
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
                </Select>
              </Field>
            )}
            <TextEditor
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
            <p className="editor-note-text">{review.initial.text.content}</p>
            <p>{t("Review the actual frame-aligned times before saving.")}</p>
            <dl className="editor-properties">
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
          <FontImport
            scope={{ projectId: project.id, revision: scene.head.revision }}
            catalog={catalog}
            readSource={(request, signal) =>
              api!.sdk.call("opencut_font_source", request, { signal })
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
        fontApplication,
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

export function TextActions({ card = false }: { card?: boolean } = {}) {
  useLocale();
  const { showProperties } = useWorkbenchView();
  const context = useContext(TextContext);
  if (!context?.available) return null;
  return (
    <>
      <Button
        className={card ? "resource-text-card" : undefined}
        aria-label={t("Add text at playhead")}
        disabled={context.disabled}
        onClick={() => {
          showProperties();
          context.beginCreate(undefined, card);
        }}
      >
        {card ? (
          <>
            <span className="resource-text-swatch">
              Aa<span>{t("Text")}</span>
            </span>
            <strong>{t("Default text")}</strong>
            <Plus size={16} className="resource-text-add" />
          </>
        ) : (
          t("Add text at playhead")
        )}
      </Button>
      {context.canImport && (
        <Button
          icon={<TypeIcon size={14} />}
          disabled={context.disabled}
          onClick={() => {
            showProperties();
            context.beginImport();
          }}
        >
          {t("Import font")}
        </Button>
      )}
    </>
  );
}
export function useFontApplication(fontId: string) {
  return useContext(TextContext)?.fontApplication(fontId) ?? null;
}
export function TextEditButton({
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
export function TextPanel() {
  return useContext(TextContext)?.panel ?? null;
}

export function useCanvasTextAuthoring() {
  return useContext(TextContext);
}
