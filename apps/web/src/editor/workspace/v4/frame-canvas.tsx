import { useEffect, useRef, useState } from "react";
import { useWorkspace } from "../context";
import { Button } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { formatTime } from "../../model/time";
import { frameAvailability } from "./frame";
import type { V4Bookmark } from "./types";

/** Scene pixels and notes are separate layers. Blob URLs live only in the view;
 * authorized task artifacts remain downloadable unchanged through Tasks. */
export function V4FrameCanvas({
  width,
  height,
  notes,
}: {
  width: number;
  height: number;
  notes: V4Bookmark[];
}) {
  useLocale();
  const { controller, api, v4, jobs } = useWorkspace();
  const [image, setImage] = useState<{ url: string; stamp: string } | null>(
    null,
  );
  const [pending, setPending] = useState(false);
  const epoch = useRef(0);
  const revision = v4 && (v4.historical ?? v4.head).revision;
  const stamp = JSON.stringify([
    api?.sdk.baseUrl,
    v4?.head.project.id,
    revision,
    v4?.sequenceId,
    v4?.playhead.ticks,
    v4?.playhead.timebase,
  ]);
  useEffect(() => {
    epoch.current++;
    setImage(null);
    return () => {
      epoch.current++;
      controller.v4.frame.invalidate();
    };
  }, [controller, api, stamp]);
  useEffect(
    () => () => {
      if (image) URL.revokeObjectURL(image.url);
    },
    [image],
  );
  const attempt = v4?.frame;
  const job = jobs.find((row) => row.job_id === attempt?.jobId);
  // Polling replaces row objects; a stable publication stamp prevents those
  // replacements from discarding an in-progress authorized download.
  const jobStamp = JSON.stringify([
    job?.job_id,
    job?.state,
    job?.result_unavailable?.code,
    job?.artifacts.map((a) => a.content_hash),
  ]);
  useEffect(() => {
    if (
      job &&
      (job.result_unavailable ||
        job.state === "failed" ||
        job.state === "cancelled")
    ) {
      epoch.current++;
      setImage(null);
      controller.v4.frame.rejectTask(job);
      return;
    }
    if (!v4 || !attempt?.jobId || attempt.phase !== "accepted") return;
    const requested = epoch.current;
    void controller.v4.frame
      .observe(jobs)
      .then((value) => {
        if (!value || requested !== epoch.current) return;
        const url = URL.createObjectURL(value.blob);
        setImage({ url, stamp });
      })
      .catch((error) => {
        if (requested === epoch.current) controller.report(error);
      });
  }, [
    controller,
    api,
    stamp,
    attempt?.request.idempotency_key,
    attempt?.jobId,
    jobStamp,
  ]);
  if (!v4) return null;
  const reason = frameAvailability(v4);
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
  const working =
    !!attempt &&
    ["submitting", "accepted", "downloading"].includes(attempt.phase);
  return (
    <>
      <div className="toolbar padded">
        <Button
          disabled={
            pending || !!reason || working || attempt?.phase === "unconfirmed"
          }
          onClick={() => {
            epoch.current++;
            setImage(null);
            void run(() => controller.v4.frame.start());
          }}
        >
          {t("Render scene frame")}
        </Button>
        {attempt && ["unconfirmed", "retryable"].includes(attempt.phase) && (
          <Button
            disabled={pending}
            onClick={() => void run(() => controller.v4.frame.retry())}
          >
            {t("Retry original frame request")}
          </Button>
        )}
        {attempt?.phase === "failed" &&
          job?.state === "succeeded" &&
          !job.result_unavailable && (
            <Button
              disabled={pending}
              onClick={() => {
                const requested = epoch.current;
                void run(async () => {
                  const value = await controller.v4.frame.retryDownload(jobs);
                  if (value && requested === epoch.current)
                    setImage({ url: URL.createObjectURL(value.blob), stamp });
                });
              }}
            >
              {t("Retry frame download")}
            </Button>
          )}
        {attempt?.jobId &&
          (job?.state === "queued" ||
            job?.state === "running" ||
            attempt.phase === "downloading") && (
            <Button
              disabled={pending || job?.cancel_requested}
              onClick={() => {
                epoch.current++;
                setImage(null);
                void run(() => controller.v4.frame.cancel());
              }}
            >
              {t("Cancel scene frame")}
            </Button>
          )}
      </div>
      {reason && (
        <div className="padded">
          <p className="muted" role="status">
            {t(reason)}
          </p>
          <Button
            disabled={pending}
            onClick={() =>
              void run(() => controller.v4.frame.refreshCapability())
            }
          >
            {t("Refresh frame availability")}
          </Button>
        </div>
      )}
      {attempt?.error && (
        <p className="padded error" role="alert">
          {t(attempt.error)}
        </p>
      )}
      {attempt?.phase === "unconfirmed" && (
        <p className="padded" role="status">
          {t(
            "The frame submission is not confirmed. Retry the original request to find its task.",
          )}
        </p>
      )}
      {attempt && (
        <small className="padded">
          {t("Saved revision")}: {attempt.request.revision} · {t("Scene time")}:{" "}
          {formatTime(attempt.request.time)}
        </small>
      )}
      <div
        className="v4-canvas"
        style={{ aspectRatio: `${width} / ${height}` }}
        role="region"
        aria-label={t("Canvas notes")}
      >
        {image?.stamp === stamp ? (
          <img
            className="v4-scene-frame"
            src={image.url}
            alt={t("Rendered scene frame")}
            onError={() => {
              setImage(null);
              controller.v4.frame.rejectImage();
            }}
          />
        ) : (
          <span className="v4-preview-status" role="status">
            {t(working ? "Preparing scene frame…" : "Preview unavailable")}
          </span>
        )}
        <div className="v4-note-hud" aria-live="polite">
          {notes.map((b) => (
            <div
              className="v4-canvas-note"
              key={b.id}
              style={{ borderColor: b.color ?? undefined }}
            >
              {b.note}
            </div>
          ))}
        </div>
      </div>
      <p className="padded muted">
        {t(
          "This is a still scene frame. Notes are shown only in the editor; playback and video export are not available yet.",
        )}
      </p>
    </>
  );
}
