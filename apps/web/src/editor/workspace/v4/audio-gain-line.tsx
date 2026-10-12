import { memo, useEffect, useRef, useState } from "react";
import { useWorkspace, useWorkspaceSelection } from "../context";
import { t, useLocale } from "../../shared/locale";
import { audioGainAvailability } from "./audio-gain";
import { gainLinePercent, gainLineTenths } from "./audio-gain-display";

/** Only presentation and gestures. Saving uses the controller's fixed-head plan. */
const selectLine = (s: ReturnType<import("../controller").WorkspaceController["getSnapshot"]>) => ({ v4: s.v4, busy: s.busy, api: s.api });
const sameLine = (a: ReturnType<typeof selectLine>, b: ReturnType<typeof selectLine>) =>
  a.busy === b.busy && a.api === b.api && a.v4?.head === b.v4?.head && a.v4?.historical === b.v4?.historical &&
  a.v4?.sequenceId === b.v4?.sequenceId && a.v4?.gainDraft === b.v4?.gainDraft && a.v4?.audioGain === b.v4?.audioGain &&
  a.v4?.audioPreview.phase === b.v4?.audioPreview.phase;
export const V4AudioGainLine = memo(function V4AudioGainLine({
  elementId,
  onSelect,
}: {
  elementId: string;
  onSelect(elementId: string): void;
}) {
  useLocale();
  const { controller, selected: { v4, busy } } = useWorkspaceSelection(selectLine, sameLine);
  const [error, setError] = useState<string | null>(null);
  const gesture = useRef<{
    id: string;
    pointer: number | null;
    key: string | null;
  } | null>(null);
  const surface = useRef<HTMLDivElement>(null);
  const gain = controller.v4.audioGain;
  const identity = `${v4?.head.project.id}/${v4?.head.revision}/${v4?.historical?.revision}/${v4?.sequenceId}`;
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
  const available = v4 && audioGainAvailability(v4, elementId);
  if (!available || !v4) return null;
  const draft = v4.gainDraft?.elementId === elementId ? v4.gainDraft : null;
  const db = draft?.changed ? draft.tenths / 10 : available.audio.gain.value;
  const unresolved =
    v4.audioGain &&
    (v4.audioGain.phase !== "saved" ||
      v4.audioGain.receipt?.base_revision === v4.head.revision);
  const blocked = Boolean(busy || unresolved);
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
    <div className="v4-audio-controls" onClick={(e) => e.stopPropagation()}>
      <div className="v4-gain-surface" ref={surface}>
        {!available.animated && available.audio.gain.unit === "decibels" && (
          <div
            className="v4-volume-line"
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
                (v4.audioPreview.phase === "playing" ||
                v4.audioPreview.phase === "stalled" ||
                v4.audioPreview.phase === "loading"
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
                const current = controller.getSnapshot().v4!.gainDraft!;
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
            <span className="v4-volume-stroke" />
          </div>
        )}
        <span className="v4-volume-value">
          {available.audio.gain.unit === "decibels"
            ? `${db.toFixed(1)} dB`
            : t("Legacy gain")}
        </span>
      </div>
      <button
        className="v4-element-mute"
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
        <span className="v4-gain-reason" title={t(available.lineReason)}>
          {t(available.lineReason)}
        </span>
      )}
      {available.track.muted && (
        <span className="v4-gain-reason">{t("Track muted")}</span>
      )}
      {available.sourceDisabled && (
        <span className="v4-gain-reason">{t("Source audio disabled")}</span>
      )}
      {error && (
        <span role="alert" className="v4-gain-reason">
          {t(error)}
        </span>
      )}
    </div>
  );
});

export function V4AudioGainStatus() {
  const { controller, v4, busy } = useWorkspace();
  const [error, setError] = useState<string | null>(null);
  const attempt = v4?.audioGain;
  if (!attempt) return null;
  const execute = (action: () => Promise<unknown>) => {
    setError(null);
    void action().catch((e) =>
      setError(e instanceof Error ? e.message : String(e)),
    );
  };
  return (
    <div className="v4-gain-status" role="status">
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
          onClick={() => execute(() => controller.v4.audioGain.retry())}
        >
          {t("Retry original volume operation")}
        </button>
      )}
      {attempt.phase === "saved" &&
        attempt.receipt?.base_revision === v4!.head.revision && (
          <button
            disabled={busy}
            onClick={() =>
              execute(() => controller.v4.audioGain.refreshSaved())
            }
          >
            {t("Refresh saved audio changes")}
          </button>
        )}
      {["retryable", "saved"].includes(attempt.phase) && (
        <button
          disabled={
            busy || attempt.receipt?.base_revision === v4!.head.revision
          }
          onClick={() => controller.v4.audioGain.dismiss()}
        >
          {t("Close volume status")}
        </button>
      )}
    </div>
  );
}
