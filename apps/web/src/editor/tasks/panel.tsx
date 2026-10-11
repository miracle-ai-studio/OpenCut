import { transcriptionFailure } from "../shared/transcription";
import { t, useLocale } from "../shared/locale";
import { publicJobResult } from "../../lib/editor-sdk";
import { Download, RefreshCw, X } from "lucide-react";
import { useWorkspace } from "../workspace/context";
import { Button, Empty, Panel } from "../shared/ui";
import { saveResponse, extension } from "../shared/download";
export function TasksPanel() {
  useLocale();
  const { controller, api, jobs, snapshot, historical, busy } = useWorkspace();
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
        {!jobs.length && (
          <Empty title={t("All clear")}>
            {t("Imports, previews and exports appear here.")}
          </Empty>
        )}
        {jobs.map((job) => (
          <article className="task-card" key={job.job_id}>
            <div className="task-title">
              <strong>
                {job.kind.startsWith("render.")
                  ? t("Export")
                  : job.kind.startsWith("preview.")
                    ? t("Preview")
                    : job.kind === "media.import"
                      ? t("Import")
                      : job.kind === "analysis.sequence_transcribe"
                        ? t("Transcription")
                        : job.kind.replaceAll(".", " ")}
              </strong>
              <span className={`state ${job.state}`}>{t(job.state)}</span>
            </div>
            {job.progress && (
              <>
                <progress max="1" value={job.progress.fraction ?? undefined} />
                <small>{t(job.progress.stage.replaceAll("_", " "))}</small>
              </>
            )}
            {job.failure && (
              <p className="error">
                {t(
                  job.kind === "analysis.sequence_transcribe"
                    ? transcriptionFailure(job.failure.code)
                    : job.failure.message,
                )}
              </p>
            )}
            {job.result_unavailable && (
              <p className="error">{job.result_unavailable.message}</p>
            )}
            <div className="task-actions">
              {job.kind === "media.import" &&
                job.state === "succeeded" &&
                !historical && (
                  <Button
                    disabled={busy}
                    onClick={async () => {
                      if (!api || !snapshot) return;
                      try {
                        const result = publicJobResult(job, "media.import");
                        if (
                          snapshot.project.assets.some(
                            (asset) => asset.id === result.asset.id,
                          )
                        ) {
                          controller.notify(
                            "This media is already in the library.",
                          );
                          return;
                        }
                        const media = await api.sdk.call(
                          "opencut_project_media_inspect",
                          {
                            project_id: snapshot.project.id,
                            media_id: result.media_id,
                          },
                        );
                        if (
                          controller.getSnapshot().snapshot?.project.id !==
                          snapshot.project.id
                        )
                          return;
                        await controller.mutate("Recover imported media", [
                          { type: "asset.bind", asset: media.asset },
                        ]);
                        controller.notify("Media added to the library.");
                      } catch (error) {
                        controller.report(error);
                      }
                    }}
                  >
                    {t("Add to library")}
                  </Button>
                )}
              {["queued", "running"].includes(job.state) && (
                <Button
                  disabled={job.cancel_requested}
                  icon={<X size={13} />}
                  onClick={() => {
                    if (api)
                      void api.sdk
                        .cancelPublicJob(job.job_id)
                        .then(() => controller.sync())
                        .catch((error) => controller.report(error));
                  }}
                >
                  {job.cancel_requested ? t("Cancelling…") : t("Cancel")}
                </Button>
              )}
              {job.artifacts.map((artifact) => (
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
