import { transcriptionFailure } from "../shared/transcription";
import { useEffect, useRef, useState } from "react";
import { Captions, LoaderCircle, X } from "lucide-react";
import { useWorkspace } from "../workspace/context";
import { activeSequence } from "../model/types";
import { Button } from "../shared/ui";
import { t, useLocale } from "../shared/locale";
const languages = [
  ["en", "English"],
  ["es", "Spanish"],
  ["it", "Italian"],
  ["fr", "French"],
  ["de", "German"],
  ["pt", "Portuguese"],
  ["ru", "Russian"],
  ["ja", "Japanese"],
  ["zh", "Chinese"],
] as const;
export function AutomaticCaptions() {
  useLocale();
  const { controller, api, snapshot, historical, sequenceId, jobs, busy } =
    useWorkspace();
  const [language, setLanguage] = useState("auto");
  const [words, setWords] = useState(3);
  const [waiting, setWaiting] = useState(false);
  const pending = useRef<AbortController | null>(null);
  const submitted = useRef<string | null>(null);
  const scope = `${snapshot?.project.id}/${sequenceId}`;
  useEffect(() => {
    setWaiting(false);
    return () => {
      pending.current?.abort();
      pending.current = null;
    };
  }, [scope]);
  const scene = activeSequence(snapshot, sequenceId);
  const hasAudio = scene?.tracks.some(
    (track) =>
      track.enabled &&
      !track.muted &&
      (track.kind === "video" || track.kind === "audio") &&
      track.clips.length > 0,
  );
  const task = jobs.find(
    (job) =>
      job.kind === "analysis.sequence_transcribe" &&
      job.target?.revision === snapshot?.revision &&
      job.target?.sequence_id === sequenceId,
  );
  const runningTask = jobs.find(
    (job) =>
      job.kind === "analysis.sequence_transcribe" &&
      job.target?.sequence_id === sequenceId &&
      (job.state === "queued" || job.state === "running"),
  );
  const running = !!runningTask;
  const ready = task?.state === "succeeded" && !task.result_unavailable;
  const disabled =
    !api || !snapshot || !!historical || busy || snapshot.format !== "visual";
  async function insert(
    jobId: string,
    revision: string,
    projectId: string,
    sceneId: string,
  ) {
    const state = controller.getSnapshot();
    if (
      state.api !== api ||
      state.snapshot?.project.id !== projectId ||
      state.snapshot.revision !== revision ||
      state.sequenceId !== sceneId ||
      state.historical
    ) {
      controller.notify(
        "The edit changed. Generate captions again for the current scene.",
      );
      return;
    }
    const result = await controller.write(() =>
      api!.sdk.call("opencut_subtitle_generate", {
        project_id: projectId,
        expected_revision: revision,
        sequence_id: sceneId,
        analysis_job_id: jobId,
        idempotency_key: crypto.randomUUID(),
        words_per_caption: words,
        style: null,
      }),
    );
    if (result)
      controller.notify(
        "Captions added. Review the words and timing before exporting.",
      );
  }
  async function generate() {
    if (
      disabled ||
      !snapshot ||
      !sequenceId ||
      !api ||
      waiting ||
      pending.current
    )
      return;
    const cancellation = new AbortController();
    pending.current = cancellation;
    setWaiting(true);
    const projectId = snapshot.project.id,
      revision = snapshot.revision,
      sceneId = sequenceId;
    try {
      const job = await api.sdk.call("opencut_analysis_sequence_transcribe", {
        project_id: projectId,
        revision,
        sequence_id: sceneId,
        language,
        idempotency_key: crypto.randomUUID(),
      });
      submitted.current = job.job_id;
      void controller.sync();
      await api.sdk.waitForPublicJob(job.job_id, {
        signal: cancellation.signal,
        timeoutMs: 1_800_000,
        pollIntervalMs: 1000,
      });
      if (!cancellation.signal.aborted)
        await insert(job.job_id, revision, projectId, sceneId);
    } catch (error) {
      if (!cancellation.signal.aborted) {
        const latest = submitted.current
          ? await api.sdk
              .call("opencut_job_view", { job_id: submitted.current })
              .catch(() => null)
          : null;
        controller.report(
          latest?.state === "cancelled"
            ? new Error("Transcription cancelled.")
            : latest?.failure
              ? new Error(transcriptionFailure(latest.failure.code))
              : error,
        );
      }
    } finally {
      if (pending.current === cancellation) {
        pending.current = null;
        submitted.current = null;
        setWaiting(false);
      }
      void controller.sync();
    }
  }
  return (
    <section
      className="automatic-captions"
      aria-label={t("Automatic captions")}
    >
      <div className="caption-section-heading">
        <Captions size={16} />
        <strong>{t("Automatic captions")}</strong>
      </div>
      <p className="muted">
        {t(
          "Turn the speech in this scene into editable captions. Audio is processed on your computer.",
        )}
      </p>
      <label>
        {t("Language")}
        <select
          aria-label={t("Language")}
          value={language}
          disabled={!!running || waiting}
          onChange={(event) => setLanguage(event.target.value)}
        >
          <option value="auto">{t("Auto detect")}</option>
          {languages.map(([code, name]) => (
            <option key={code} value={code}>
              {t(name)}
            </option>
          ))}
        </select>
      </label>
      <label>
        {t("Words per caption")}
        <select
          aria-label={t("Words per caption")}
          value={words}
          disabled={waiting}
          onChange={(event) => setWords(Number(event.target.value))}
        >
          {[3, 5, 10, 20].map((value) => (
            <option key={value} value={value}>
              {value}
            </option>
          ))}
        </select>
      </label>
      <div className="caption-generation-actions">
        <Button
          className="primary"
          disabled={disabled || !hasAudio || !!running || waiting}
          icon={
            running || waiting ? (
              <LoaderCircle className="spin" size={15} />
            ) : (
              <Captions size={15} />
            )
          }
          onClick={() => void generate()}
        >
          {t(running || waiting ? "Transcribing…" : "Generate transcript")}
        </Button>
        {running && (
          <Button
            aria-label={t("Cancel transcription")}
            disabled={runningTask!.cancel_requested}
            icon={<X size={14} />}
            onClick={() =>
              void api!.sdk
                .cancelPublicJob(runningTask!.job_id)
                .then(() => controller.sync())
                .catch((error) => controller.report(error))
            }
          >
            {t("Cancel")}
          </Button>
        )}
      </div>
      {!hasAudio && (
        <p className="muted">
          {t("Add speech to the timeline to generate captions.")}
        </p>
      )}
      {ready && !waiting && (
        <Button
          disabled={disabled}
          onClick={() =>
            void insert(
              task.job_id,
              snapshot!.revision,
              snapshot!.project.id,
              sequenceId!,
            ).catch((error) => controller.report(error))
          }
        >
          {t("Add generated captions")}
        </Button>
      )}
      {task?.failure && (
        <p className="error" role="status">
          {t(transcriptionFailure(task.failure.code))}
        </p>
      )}
    </section>
  );
}
