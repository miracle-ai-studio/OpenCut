import { memo, useEffect, useRef, useState } from "react";
import { useEditingWorkspace as useWorkspace, useWorkspaceSelection } from "../context";
import { t, useLocale } from "../../shared/locale";
import { audioGainAvailability } from "./audio-gain";
import { gainLinePercent, gainLineTenths } from "./audio-gain-display";

/** Only presentation and gestures. Saving uses the controller's fixed-head plan. */
const selectLine = (s: ReturnType<import("../controller").WorkspaceController["getSnapshot"]>) => ({ scene: s.scene, busy: s.busy, api: s.api });
const sameLine = (a: ReturnType<typeof selectLine>, b: ReturnType<typeof selectLine>) =>
  a.busy === b.busy && a.api === b.api && a.scene?.head === b.scene?.head && a.scene?.historical === b.scene?.historical &&
  a.scene?.sequenceId === b.scene?.sequenceId && a.scene?.gainDraft === b.scene?.gainDraft && a.scene?.propertyDrafts === b.scene?.propertyDrafts && a.scene?.audioGain === b.scene?.audioGain &&
  a.scene?.audioPreview.phase === b.scene?.audioPreview.phase;
export const AudioGainLine = memo(function AudioGainLine({
  elementId,
  onSelect,
}: {
  elementId: string;
  onSelect(elementId: string): void;
}) {
  useLocale();
  const { controller, selected: { scene, busy } } = useWorkspaceSelection(selectLine, sameLine);
  const [error, setError] = useState<string | null>(null);
  const gesture = useRef<{
    id: string;
    pointer: number | null;
    key: string | null;
  } | null>(null);
  const surface = useRef<HTMLDivElement>(null);
  const gain = controller.scene.audioGain;
  const identity = `${scene?.head.project.id}/${scene?.head.revision}/${scene?.historical?.revision}/${scene?.sequenceId}`;
  useEffect(
    () => () => {
      const g = gesture.current;
      gesture.current = null;
      if (g) gain.cancel(g.id);
    },
    [gain, identity],
  );
  useEffect(() => {
    const blur = () => {
      const g = gesture.current;
      gesture.current = null;
      if (g) gain.cancel(g.id);
    };
    window.addEventListener("blur", blur);
    return () => window.removeEventListener("blur", blur);
  }, [gain]);
  const available = scene && audioGainAvailability(scene, elementId);
  if (!available || !scene) return null;
  const draft = scene.gainDraft?.elementId === elementId ? scene.gainDraft : null;
  const db = draft?.changed ? draft.tenths / 10 : available.audio.gain.value;
  const unresolved =
    scene.audioGain &&
    (scene.audioGain.phase !== "saved" ||
      scene.audioGain.receipt?.base_revision === scene.head.revision);
  const blocked = Boolean(busy || unresolved || scene.propertyDrafts?.length);
  const cancel = () => {
    const g = gesture.current;
    gesture.current = null;
    if (g) gain.cancel(g.id);
  };
  const run = (action: () => Promise<unknown>) => {
    setError(null);
    void action().catch((e) =>
      setError(e instanceof Error ? e.message : String(e)),
    );
  };
  const updatePointer = (id: string, y: number) => {
    const rect = surface.current?.getBoundingClientRect();
    if (rect?.height)
      gain.update(id, gainLineTenths(((y - rect.top) / rect.height) * 100));
  };
  return (
    <div className="editor-audio-controls" onClick={(e) => e.stopPropagation()}>
      <div className="editor-gain-surface" ref={surface}>
        {!available.animated && available.audio.gain.unit === "decibels" && (
          <div
            className="editor-volume-line"
            role="slider"
            tabIndex={0}
            aria-label={`${t("Volume")} ${available.element.name}`}
            aria-valuemin={-60}
            aria-valuemax={20}
            aria-valuenow={Math.max(-60, Math.min(20, db))}
            aria-valuetext={`${db.toFixed(1)} dB`}
            aria-disabled={Boolean(blocked || available.lineReason)}
            title={t(
              available.lineReason ??
                (scene.audioPreview.phase === "playing" ||
                scene.audioPreview.phase === "stalled" ||
                scene.audioPreview.phase === "loading"
                  ? "Volume draft is previewed in the playing scene sound."
                  : "Play scene audio to hear volume drafts while dragging."),
            )}
            style={{ top: `${gainLinePercent(db)}%` }}
            onPointerDown={(e) => {
              e.stopPropagation();
              e.preventDefault();
              if (
                e.button !== 0 ||
                blocked ||
                available.lineReason ||
                gesture.current
              )
                return;
              try {
                e.currentTarget.focus();
                onSelect(elementId);
                const id = gain.begin(elementId);
                gesture.current = { id, pointer: e.pointerId, key: null };
                e.currentTarget.setPointerCapture(e.pointerId);
                updatePointer(id, e.clientY);
              } catch (e) {
                cancel();
                setError(e instanceof Error ? e.message : String(e));
              }
            }}
            onPointerMove={(e) => {
              if (gesture.current?.pointer === e.pointerId)
                updatePointer(gesture.current.id, e.clientY);
            }}
            onPointerUp={(e) => {
              const g = gesture.current;
              if (!g || g.pointer !== e.pointerId) return;
              e.stopPropagation();
              gesture.current = null;
              if (e.currentTarget.hasPointerCapture(e.pointerId))
                e.currentTarget.releasePointerCapture(e.pointerId);
              run(() => gain.commit(g.id));
            }}
            onPointerCancel={(e) => {
              if (gesture.current?.pointer === e.pointerId) cancel();
            }}
            onLostPointerCapture={(e) => {
              if (gesture.current?.pointer === e.pointerId) cancel();
            }}
            onBlur={cancel}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                cancel();
                return;
              }
              if (
                ![
                  "ArrowUp",
                  "ArrowDown",
                  "ArrowLeft",
                  "ArrowRight",
                  "Home",
                  "End",
                ].includes(e.key)
              )
                return;
              e.preventDefault();
              e.stopPropagation();
              if (
                blocked ||
                available.lineReason ||
                gesture.current?.pointer != null
              )
                return;
              try {
                if (!gesture.current) {
                  onSelect(elementId);
                  gesture.current = {
                    id: gain.begin(elementId),
                    pointer: null,
                    key: e.key,
                  };
                }
                const current = controller.getSnapshot().scene!.gainDraft!;
                const value =
                  e.key === "Home"
                    ? -600
                    : e.key === "End"
                      ? 200
                      : Math.max(
                          -600,
                          Math.min(
                            200,
                            current.tenths +
                              (["ArrowUp", "ArrowRight"].includes(e.key)
                                ? 1
                                : -1),
                          ),
                        );
                gain.update(gesture.current.id, value);
              } catch (e) {
                cancel();
                setError(e instanceof Error ? e.message : String(e));
              }
            }}
            onKeyUp={(e) => {
              const g = gesture.current;
              if (!g || g.key !== e.key || g.pointer !== null) return;
              e.preventDefault();
              e.stopPropagation();
              gesture.current = null;
              run(() => gain.commit(g.id));
            }}
          >
            <span className="editor-volume-stroke" />
          </div>
        )}
        <span className="editor-volume-value">
          {available.audio.gain.unit === "decibels"
            ? `${db.toFixed(1)} dB`
            : t("Legacy gain")}
        </span>
      </div>
      <button
        className="editor-element-mute"
        disabled={Boolean(blocked || available.reason)}
        title={available.reason ? t(available.reason) : undefined}
        aria-label={`${t(available.audio.muted ? "Unmute" : "Mute")} ${available.element.name}`}
        aria-pressed={available.audio.muted}
        onClick={(e) => {
          e.stopPropagation();
          onSelect(elementId);
          run(() => gain.mute(elementId, !available.audio.muted));
        }}
      >
        {t(available.audio.muted ? "Unmute" : "Mute")}
      </button>
      {available.lineReason && (
        <span className="editor-gain-reason" title={t(available.lineReason)}>
          {t(available.lineReason)}
        </span>
      )}
      {available.track.muted && (
        <span className="editor-gain-reason">{t("Track muted")}</span>
      )}
      {available.sourceDisabled && (
        <span className="editor-gain-reason">{t("Source audio disabled")}</span>
      )}
      {error && (
        <span role="alert" className="editor-gain-reason">
          {t(error)}
        </span>
      )}
    </div>
  );
});

export function AudioGainStatus() {
  const { controller, scene, busy } = useWorkspace();
  const [error, setError] = useState<string | null>(null);
  const attempt = scene?.audioGain;
  if (!attempt) return null;
  const execute = (action: () => Promise<unknown>) => {
    setError(null);
    void action().catch((e) =>
      setError(e instanceof Error ? e.message : String(e)),
    );
  };
  return (
    <div className="editor-gain-status" role="status">
      {t(
        attempt.phase === "saved"
          ? "Element audio changes saved."
          : attempt.phase === "unconfirmed"
            ? "Volume save is unconfirmed. Retry the original operation to recover its receipt."
            : "Saving element audio changes…",
      )}
      {(attempt.error || error) && (
        <span role="alert">{t(error ?? attempt.error!)}</span>
      )}
      {["retryable", "unconfirmed"].includes(attempt.phase) && (
        <button
          disabled={busy}
          onClick={() => execute(() => controller.scene.audioGain.retry())}
        >
          {t("Retry original volume operation")}
        </button>
      )}
      {attempt.phase === "saved" &&
        attempt.receipt?.base_revision === scene!.head.revision && (
          <button
            disabled={busy}
            onClick={() =>
              execute(() => controller.scene.audioGain.refreshSaved())
            }
          >
            {t("Refresh saved audio changes")}
          </button>
        )}
      {["retryable", "saved"].includes(attempt.phase) && (
        <button
          disabled={
            busy || attempt.receipt?.base_revision === scene!.head.revision
          }
          onClick={() => controller.scene.audioGain.dismiss()}
        >
          {t("Close volume status")}
        </button>
      )}
    </div>
  );
}
