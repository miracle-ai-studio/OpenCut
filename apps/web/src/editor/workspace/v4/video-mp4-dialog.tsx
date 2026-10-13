import { useEffect, useState } from "react";
import { Button, Modal } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { useWorkspace } from "../context";
import { videoMp4Availability } from "./video-mp4";

export function VideoMp4Dialog({
  onClose,
  showTasks,
}: {
  onClose(): void;
  showTasks(): void;
}) {
  useLocale();
  const { controller, v4, busy } = useWorkspace();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    void controller.v4.videoMp4.refreshCapability();
  }, [controller]);
  if (!v4) return null;
  const attempt = v4.videoMp4,
    view = v4.historical ?? v4.head;
  const sequence = view.project.sequences.find((s) => s.id === v4.sequenceId);
  const availability = videoMp4Availability(v4);
  const run = async (action: () => Promise<unknown>) => {
    setPending(true);
    setError(null);
    try {
      await action();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      controller.report(e);
    } finally {
      setPending(false);
    }
  };
  return (
    <Modal title={t("Export video MP4")} onClose={onClose}>
      <p>
        {t("Scene")}: {attempt?.sceneName ?? sequence?.name}
      </p>
      <p className="muted">
        {t("Saved revision")}: {attempt?.request.revision ?? view.revision}
      </p>
      <p>
        {t(
          "Exports the saved scene as CFR MP4 with its canvas size and frame rate, fixed fonts and stereo 48 kHz sound. Scene notes are excluded.",
        )}
      </p>
      {availability.reason && <p role="status">{t(availability.reason)}</p>}
      {error && !attempt && <p role="alert">{t(error)}</p>}
      {attempt && (
        <div role={attempt.error ? "alert" : "status"}>
          <p>
            {t(
              attempt.phase === "unconfirmed"
                ? "The video export submission is not confirmed. Retry the original submission to find its task."
                : attempt.phase === "accepted"
                  ? "Video MP4 task accepted. Progress and downloads are in Tasks."
                  : attempt.phase === "submitting"
                    ? "Starting video MP4 export…"
                    : "Video MP4 export was not accepted.",
            )}
          </p>
          {attempt.error && <p>{t(attempt.error)}</p>}
          {attempt.phase === "accepted" && (
            <>
              <Button
                onClick={() => {
                  onClose();
                  showTasks();
                }}
              >
                {t("View tasks")}
              </Button>
              <Button
                disabled={busy || pending}
                onClick={() => controller.v4.videoMp4.dismiss()}
              >
                {t("New video export")}
              </Button>
            </>
          )}
          {["unconfirmed", "retryable"].includes(attempt.phase) && (
            <Button
              disabled={busy || pending}
              onClick={() => void run(() => controller.v4.videoMp4.retry())}
            >
              {t("Retry original export")}
            </Button>
          )}
          {attempt.phase === "retryable" && (
            <Button
              disabled={busy || pending}
              onClick={() => controller.v4.videoMp4.dismiss()}
            >
              {t("Reset export")}
            </Button>
          )}
        </div>
      )}
      <p className="muted">
        {t(
          "Closing this window does not cancel an accepted task. Use Tasks to request cancellation.",
        )}
      </p>
      <div className="modal-footer">
        <Button onClick={onClose}>{t("Close")}</Button>
        <Button
          className="primary"
          disabled={busy || pending || !!availability.reason || !!attempt}
          onClick={() => void run(() => controller.v4.videoMp4.start())}
        >
          {t("Start video export")}
        </Button>
      </div>
    </Modal>
  );
}
