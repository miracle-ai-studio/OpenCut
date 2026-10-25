import { useEffect, useRef, useState } from "react";
import { useShortcutActions } from "../../../shortcuts";
import { Button, Modal } from "../../../shared/ui";
import { t, useLocale } from "../../../shared/locale";
import { formatTime } from "../../../model/time";
import { ignoresTimelineShortcut } from "./actions";
import type { TimelineScope } from "./types";
import type { ClipboardState, TimelineClipboard } from "./clipboard";
import { Copy, ClipboardPaste, CopyPlus, ClipboardCheck } from "lucide-react";

export type ClipboardControlsProps = {
  controller: TimelineClipboard;
  state: ClipboardState;
  scope: TimelineScope;
  canCopy: boolean;
  copyUnavailableReason?: string | null;
  canPaste: boolean;
  disabled: boolean;
  focusRoot?: Element | null;
  onFiles(files: File[], capturedScope: TimelineScope): void | Promise<void>;
  onReview?(): void;
};

/** Event ownership stays in the product timeline. Text/IME, gain controls and
 * dialogs retain their own browser clipboard and keyboard semantics. */
export function ClipboardControls(props: ClipboardControlsProps) {
  useLocale();
  const latest = useRef(props);
  latest.current = props;
  const wrapper = useRef<HTMLDivElement>(null);
  const [review, setReview] = useState(false),
    [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null),
    [info, setInfo] = useState<string | null>(null);
  const pendingRef = useRef(false);
  const composing = useRef(false);
  const attempt = props.state.attempt;
  const captureUi = () => ({
    controller: latest.current.controller,
    scope: { ...latest.current.scope },
    ...latest.current.controller.identity(),
  });
  const sameUi = (captured: ReturnType<typeof captureUi>) => {
    const current = latest.current,
      identity = current.controller.identity();
    return (
      current.controller === captured.controller &&
      identity.api === captured.api &&
      identity.generation === captured.generation &&
      identity.viewGeneration === captured.viewGeneration &&
      current.scope.projectId === captured.scope.projectId &&
      current.scope.revision === captured.scope.revision &&
      current.scope.sequenceId === captured.scope.sequenceId
    );
  };
  const run = async (action: () => Promise<unknown>) => {
    if (pendingRef.current) return;
    const captured = captureUi();
    pendingRef.current = true;
    setPending(true);
    setError(null);
    setInfo(null);
    try {
      await action();
    } catch (cause) {
      if (sameUi(captured))
        setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      pendingRef.current = false;
      setPending(false);
    }
  };
  const openReview = () => {
    setReview(true);
    latest.current.onReview?.();
  };
  const copy = () =>
    run(async () => {
      const captured = captureUi();
      const controller = latest.current.controller;
      await controller.copy();
      if (!sameUi(captured)) return;
      const text = controller.text();
      if (!text) return;
      try {
        if (!navigator.clipboard?.writeText) throw new Error("unavailable");
        await navigator.clipboard.writeText(text);
        if (sameUi(captured)) setInfo("Timeline elements copied.");
      } catch {
        if (sameUi(captured))
          setInfo(
            "Copied in the editor. The system clipboard is unavailable; use Paste copied elements here.",
          );
      }
    });
  const paste = () => {
    openReview();
    return run(() => latest.current.controller.planPaste());
  };
  const duplicate = () => {
    openReview();
    return run(() => latest.current.controller.duplicate());
  };
  useShortcutActions({
    "copy-selected": {
      run: copy,
      enabled:
        props.canCopy && !props.disabled && !props.state.attempt && !pending,
    },
    "paste-copied": {
      run: paste,
      nativePaste: true,
      enabled:
        props.canPaste && !props.disabled && !props.state.attempt && !pending,
    },
    "duplicate-selected": {
      run: duplicate,
      enabled:
        props.canCopy &&
        props.canPaste &&
        !props.disabled &&
        !props.state.attempt &&
        !pending,
    },
  });
  const handlers = useRef({ run, copy, paste, duplicate });
  handlers.current = { run, copy, paste, duplicate };
  useEffect(() => {
    const owned = (target: EventTarget | null) => {
      if (
        composing.current ||
        ignoresTimelineShortcut(target) ||
        !(target instanceof Element)
      )
        return false;
      const current = latest.current;
      return current.focusRoot
        ? current.focusRoot.contains(target)
        : !!target.closest(".editor-timeline-panel") ||
            !!wrapper.current?.contains(target);
    };
    const onCopy = (event: ClipboardEvent) => {
      const current = latest.current;
      if (
        !owned(event.target) ||
        !current.canCopy ||
        current.disabled ||
        current.state.attempt ||
        pendingRef.current
      )
        return;
      event.preventDefault();
      const text = current.controller.selectedText();
      if (text && event.clipboardData) {
        event.clipboardData.setData("text/plain", text);
        setInfo("Timeline elements copied.");
      } else void handlers.current.copy();
    };
    const onPaste = (event: ClipboardEvent) => {
      const current = latest.current;
      if (
        !owned(event.target) ||
        !current.canPaste ||
        current.disabled ||
        current.state.attempt ||
        pendingRef.current ||
        !event.clipboardData
      )
        return;
      const files = Array.from(event.clipboardData.files);
      if (files.length) {
        event.preventDefault();
        const scope = { ...current.scope };
        void handlers.current.run(async () => {
          await current.onFiles(files, scope);
        });
        return;
      }
      const text = event.clipboardData.getData("text/plain");
      try {
        if (!current.controller.importText(text)) return;
        event.preventDefault();
        void handlers.current.paste();
      } catch (cause) {
        event.preventDefault();
        setError(cause instanceof Error ? cause.message : String(cause));
      }
    };
    const startComposition = () => {
        composing.current = true;
      },
      endComposition = () => {
        composing.current = false;
      };
    document.addEventListener("copy", onCopy);
    document.addEventListener("paste", onPaste);
    document.addEventListener("compositionstart", startComposition);
    document.addEventListener("compositionend", endComposition);
    return () => {
      document.removeEventListener("copy", onCopy);
      document.removeEventListener("paste", onPaste);
      document.removeEventListener("compositionstart", startComposition);
      document.removeEventListener("compositionend", endComposition);
    };
  }, []);
  const busy = pending || props.disabled || props.state.phase === "copying";
  const cannotPaste =
    !props.canPaste ||
    !props.state.buffer ||
    props.state.buffer.source_project_id !== props.scope.projectId;
  const decision = attempt?.plan?.decision;
  const names = new Map<string, string>();
  for (const operation of attempt?.plan?.transaction.operations ?? []) {
    if (operation.type === "element_put")
      names.set(operation.element.id, operation.element.name);
    else if (operation.type === "track_put") {
      names.set(operation.track.id, operation.track.name);
      operation.track.elements.forEach((element) =>
        names.set(element.id, element.name),
      );
    }
  }
  const status =
    attempt?.phase === "planning"
      ? "Preparing paste…"
      : attempt?.phase === "applying"
        ? "Saving pasted elements…"
        : attempt?.phase === "saved"
          ? "Pasted elements saved."
          : attempt?.phase === "unconfirmed"
            ? "Paste is unconfirmed. Retry the original operation to recover its receipt."
            : attempt?.phase === "conflict"
              ? "The original paste was not saved. Discard it before making a new plan."
              : "Review copied elements before saving.";
  return (
    <div
      className="v4-clipboard-controls"
      ref={wrapper}
      tabIndex={0}
      aria-label={t("Timeline clipboard")}
    >
      <div className="toolbar">
        <Button
          title={t("Copy elements")}
          disabled={busy || !props.canCopy || !!attempt}
          onClick={() => void copy()}
        >
          <Copy size={15} /><span className="workbench-action-label">{t("Copy elements")}</span>
        </Button>
        <Button
          title={t("Paste copied elements")}
          disabled={busy || cannotPaste || !!attempt}
          onClick={() => void paste()}
        >
          <ClipboardPaste size={15} /><span className="workbench-action-label">{t("Paste copied elements")}</span>
        </Button>
        <Button
          title={t("Duplicate elements")}
          disabled={busy || !props.canCopy || !props.canPaste || !!attempt}
          onClick={() => void duplicate()}
        >
          <CopyPlus size={15} /><span className="workbench-action-label">{t("Duplicate elements")}</span>
        </Button>
        {attempt && <Button title={t("Review paste")} aria-label={t("Review paste")} icon={<ClipboardCheck size={15}/>} onClick={openReview} />}
        {props.state.buffer && (
          <span>
            {t("{count} elements copied", {
              count: props.state.buffer.items.length,
            })}
          </span>
        )}
      </div>
      {props.copyUnavailableReason && (
        <p role="status" className="muted">
          {t(props.copyUnavailableReason)}
        </p>
      )}
      {props.state.buffer &&
        props.state.buffer.source_project_id !== props.scope.projectId && (
          <p role="status">
            {t(
              "Timeline elements can only be pasted within their original project.",
            )}
          </p>
        )}
      {info && <p role="status">{t(info)}</p>}
      {(error || props.state.error) && (
        <p role="alert">{t(error ?? props.state.error!)}</p>
      )}
      {review && attempt && (
        <Modal title={t("Review paste")} onClose={() => setReview(false)}>
          <div className="padded scroll">
            <p role="status">{t(status)}</p>
            {decision && (
              <>
                <p>
                  {t("Created items")}: {decision.created_ids.length} ·{" "}
                  {t("New tracks")}: {decision.new_track_ids.length}
                </p>
                <p>
                  {t("Restored assets")}: {decision.restored_asset_ids.length} ·{" "}
                  {t("Restored fonts")}: {decision.restored_font_ids.length}
                </p>
                <ul>
                  {decision.actual_starts.map((value) => (
                    <li key={value.element_id}>
                      {names.get(value.element_id) ?? t("Copied element")} ·{" "}
                      {formatTime(value.start)}
                    </li>
                  ))}
                </ul>
              </>
            )}
            {(attempt.error || error) && (
              <p role="alert">{t(error ?? attempt.error!)}</p>
            )}
            {attempt.phase === "unconfirmed" && (
              <p>
                {t(
                  "The project head is not a receipt. Retry sends the original transaction and key; it does not create a new plan.",
                )}
              </p>
            )}
            <div className="toolbar">
              <Button onClick={() => setReview(false)}>{t("Close")}</Button>
              {attempt.phase === "planning" && (
                <Button
                  onClick={() => {
                    props.controller.cancel();
                    setReview(false);
                  }}
                >
                  {t("Cancel planning")}
                </Button>
              )}
              {!["planning", "applying", "unconfirmed"].includes(
                attempt.phase,
              ) && (
                <Button
                  disabled={pending}
                  onClick={() => {
                    props.controller.cancel();
                    setReview(false);
                  }}
                >
                  {t(attempt.phase === "saved" ? "Done" : "Discard paste")}
                </Button>
              )}
              {attempt.phase === "failed" && !attempt.plan && (
                <Button
                  disabled={busy || !props.canPaste}
                  onClick={() => void run(() => props.controller.retryPlan())}
                >
                  {t("Retry paste plan")}
                </Button>
              )}
              {attempt.phase === "unconfirmed" && (
                <Button
                  disabled={busy || !props.canPaste}
                  onClick={() => void run(() => props.controller.apply())}
                >
                  {t("Retry original paste operation")}
                </Button>
              )}
              {["planned", "failed"].includes(attempt.phase) &&
                attempt.plan && (
                  <Button
                    disabled={busy || !props.canPaste}
                    onClick={() => void run(() => props.controller.apply())}
                  >
                    {t("Apply copied elements")}
                  </Button>
                )}
              {attempt.phase === "saved" && attempt.error && (
                <Button
                  disabled={busy}
                  onClick={() =>
                    void run(() => props.controller.refreshSaved())
                  }
                >
                  {t("Refresh saved paste")}
                </Button>
              )}
            </div>
          </div>
        </Modal>
      )}
    </div>
  );
}
