import { useState } from "react";
import { Button } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { useWorkspace } from "../context";
import { sourceAudioAvailability } from "./source-audio";

export function SourceAudioControls() {
  useLocale();
  const { controller, v4, busy } = useWorkspace();
  const [pending, setPending] = useState(false);
  if (!v4) return null;
  const available = sourceAudioAvailability(v4),
    attempt = v4.sourceAudio;
  if (!available && !attempt) return null;
  const unresolved =
    !!attempt && (attempt.phase !== "saved" || !attempt.headChecked);
  const locked = busy || pending || !!v4.historical;
  const sameTarget =
    attempt?.request.element_id === v4.selectedId &&
    attempt?.request.sequence_id === v4.sequenceId;
  const run = async (action: () => Promise<unknown>) => {
    setPending(true);
    try {
      await action();
    } catch (error) {
      controller.report(error);
    } finally {
      setPending(false);
    }
  };
  const returnToVideo = () => {
    if (!attempt) return;
    if (v4.historical) controller.v4.returnToHead();
    controller.v4.chooseSequence(attempt.request.sequence_id);
    controller.v4.select(attempt.request.element_id);
  };
  return (
    <section
      aria-label={t("Source audio controls")}
      className="v4-source-audio"
    >
      <h3>{t("Source audio")}</h3>
      {available && (
        <>
          {available.reason && <p className="muted">{t(available.reason)}</p>}
          {!available.sourceEnabled && (
            <p>
              {t(
                "Restoring source audio keeps separated audio in the scene. Both may play together.",
              )}
            </p>
          )}
          <div className="toolbar">
            <Button
              disabled={
                locked ||
                unresolved ||
                !!available.reason ||
                !available.sourceEnabled
              }
              onClick={() =>
                void run(() => controller.v4.sourceAudio.start("separate"))
              }
            >
              {t("Separate source audio")}
            </Button>
            <Button
              disabled={
                locked ||
                unresolved ||
                !!available.reason ||
                available.sourceEnabled
              }
              onClick={() =>
                void run(() => controller.v4.sourceAudio.start("restore"))
              }
            >
              {t("Restore source audio")}
            </Button>
          </div>
        </>
      )}
      {attempt && (
        <div role={attempt.error ? "alert" : "status"}>
          <p>
            {t(
              attempt.phase === "unconfirmed"
                ? "The saved outcome is not confirmed. Check and retry the original operation before making another change."
                : attempt.phase === "saved"
                  ? "Source audio changes saved."
                  : attempt.phase === "planning" || attempt.phase === "applying"
                    ? "Saving source audio changes…"
                    : "Source audio changes were not saved.",
            )}
          </p>
          {attempt.error && <p>{t(attempt.error)}</p>}
          <div className="toolbar">
            {!sameTarget && (
              <Button disabled={busy || pending} onClick={returnToVideo}>
                {t("Return to source video")}
              </Button>
            )}
            {["unconfirmed", "retryable", "blocked"].includes(
              attempt.phase,
            ) && (
              <Button
                disabled={locked || !sameTarget}
                onClick={() =>
                  void run(() => controller.v4.sourceAudio.retry())
                }
              >
                {t(
                  attempt.phase === "unconfirmed"
                    ? "Check and retry operation"
                    : "Retry operation",
                )}
              </Button>
            )}
            {attempt.phase === "saved" && !attempt.headChecked && (
              <Button
                disabled={pending}
                onClick={() =>
                  void run(() => controller.v4.sourceAudio.refreshSaved())
                }
              >
                {t("Refresh saved changes")}
              </Button>
            )}
            {["saved", "retryable", "blocked"].includes(attempt.phase) && (
              <Button
                disabled={busy || pending}
                onClick={() => controller.v4.sourceAudio.dismiss()}
              >
                {t("Close")}
              </Button>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
