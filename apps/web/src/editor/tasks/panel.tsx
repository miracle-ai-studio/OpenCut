import { ErrorNotice } from "../shared/error-notice";
import { transcriptionFailure } from "../shared/transcription";
import { t, useLocale } from "../shared/locale";

import { Download, RefreshCw, X } from "lucide-react";
import { useWorkspace } from "../workspace/context";
import { Button, Empty, Panel } from "../shared/ui";
import { saveResponse, extension } from "../shared/download";
export function TasksPanel() {
  useLocale();
  const { controller, api, jobs } = useWorkspace();
  // Derived playback segments belong to the player's single progress display.
  // They are not user exports or individually actionable editing tasks.
  const visibleJobs=jobs.filter(job=>job.purpose!=="playback_optimization");
  return (
    <Panel
      title={t("Tasks")}
      actions={
        <Button
          aria-label={t("Refresh tasks")}
          icon={<RefreshCw size={14} />}
          onClick={() => void controller.sync()}
        />
      }
    >
      <div className="scroll task-list">
        {!visibleJobs.length && (
          <Empty title={t("All clear")}>
            {t("Imports, previews and exports appear here.")}
          </Empty>
        )}
        {visibleJobs.map((job) => (
          <article className="task-card" key={job.job_id}>
            <div className="task-title">
              <strong>
                {job.purpose === "playback_optimization" ? t("Playback optimization") : job.kind === "render.audio"
                  ? t("Audio WAV export")
                  : job.kind === "render.video_range"
                    ? t("Video range export")
                    : job.kind === "render.audio_range"
                      ? t("Audio range export")
                  : ["render.frame", "render.media_frame", "render.text_frame", "render.caption_frame", "render.graphic_frame", "render.mask_frame", "render.blend_frame"].includes(job.kind)
                    ? t("Scene frame")
                    : job.kind.startsWith("render.")
                      ? t("Export")
                      : job.kind.startsWith("preview.")
                        ? t("Preview")
                        : job.kind === "media.import"
                          ? t("Import")
                          : job.kind === "analysis.sequence_transcribe"
                            ? t("Transcription")
                            : t("Background task")}
              </strong>
              <span className={`state ${job.state}`}>{t(job.state)}</span>
            </div>

            {job.progress && ["queued", "running"].includes(job.state) && (
              <>
                <progress max="1" value={job.progress.fraction ?? undefined} />
                <small>{t(job.progress.stage === "encoding" ? "Saving file…" : job.progress.stage === "verifying" ? "Checking file…" : "Processing…")}</small>
              </>
            )}
            {job.failure && <ErrorNotice error={job.kind === "analysis.sequence_transcribe" ? transcriptionFailure(job.failure.code) : job.failure.message} />}
            {job.result_unavailable && <ErrorNotice error={job.result_unavailable.message} />}
            <div className="task-actions">
              {["queued", "running"].includes(job.state) && (
                <Button
                  disabled={job.cancel_requested}
                  icon={<X size={13} />}
                  onClick={() => {
                    if (api)
                      void api.sdk
                        .cancelJob(job.job_id)
                        .then(() => controller.sync())
                        .catch((error) => controller.report(error));
                  }}
                >
                  {job.cancel_requested ? t("Cancelling…") : t("Cancel")}
                </Button>
              )}
              {(job.purpose === "playback_optimization" ? [] : job.artifacts).map((artifact) => (
                <Button
                  key={artifact.artifact_id}
                  icon={<Download size={13} />}
                  onClick={async () => {
                    if (!api) return;
                    try {
                      await saveResponse(
                        `opencut-${job.job_id}.${extension(artifact.mime_type)}`,
                        () => api.sdk.downloadArtifact(artifact),
                      );
                    } catch (error) {
                      controller.report(error);
                    }
                  }}
                >
                  {t("Download")}
                </Button>
              ))}
            </div>
          </article>
        ))}
      </div>
    </Panel>
  );
}
