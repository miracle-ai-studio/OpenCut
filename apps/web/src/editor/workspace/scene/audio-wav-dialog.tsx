import { ExportSummary, ExportStatus, secondsRange } from "./export-status";
import { ErrorNotice } from "../../shared/error-notice";
import { Select } from "../../shared/controls";
import { useEffect, useState } from "react";
import { Button, Field, Modal } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { useWorkspace } from "../context";
import { audioWavAvailability } from "./audio-wav";

export function AudioWavDialog({
  onClose,
  showTasks,
  embedded = false,
}: {
  onClose(): void;
  showTasks(): void;
  embedded?: boolean;
}) {
  useLocale();
  const { controller, scene, busy, api } = useWorkspace();
  const [scope, setScope] = useState<"whole" | "range">("whole");
  const [startSample, setStartSample] = useState("0");
  const [frameCount, setFrameCount] = useState("");
  const attempt = scene?.audioWav;
  const activeScope = attempt
    ? "range" in attempt.request
      ? "range"
      : "whole"
    : scope;
  const rates = scene ? audioWavAvailability(scene, activeScope).rates : [];
  const [sampleRate, setSampleRate] = useState<number>(() =>
    rates.includes(48000) ? 48000 : (rates[0] ?? 48000),
  );
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    const current = controller.getSnapshot();
    if (!api || !scene || current.api !== api || current.scene?.head.project.id !== scene.head.project.id) return;
    void controller.scene.audioWav.refreshCapability().catch(error => {
      const active = controller.getSnapshot();
      if (active.api === api && active.scene?.head.project.id === scene.head.project.id) controller.report(error);
    });
  }, [controller, api, scene?.head.project.id]);
  useEffect(() => {
    if (rates.length && !rates.some((rate) => rate === sampleRate))
      setSampleRate(rates[0]);
  }, [rates.join(","), sampleRate]);
  if (!scene) return null;
  const availability = audioWavAvailability(scene, activeScope);
  const capturedRange =
    attempt && "range" in attempt.request ? attempt.request.range : null;
  const view = scene.historical ?? scene.head;
  const sequence = view.project.sequences.find((s) => s.id === scene.sequenceId);
  const captured = !!attempt;
  let selectedDuration: number | undefined;
  if (activeScope === "range") {
    try { const range = capturedRange ?? secondsRange(startSample, frameCount, sampleRate); selectedDuration = Number(range.frame_count) / (attempt?.request.sample_rate ?? sampleRate); }
    catch { selectedDuration = NaN; }
  }
  const run = async (action: () => Promise<unknown>) => {
    setPending(true);
    setError(null);
    try {
      await action();
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
      controller.report(error);
    } finally {
      setPending(false);
    }
  };
  const body = (
    <>
      <p>
        {t("Scene")}: {captured ? attempt.sceneName : sequence?.name}
      </p>
      <Field label={t("Export scope")}>
        <Select
          value={activeScope}
          disabled={busy || pending || captured}
          onChange={(e) => setScope(e.target.value as "whole" | "range")}
        >
          <option value="whole">{t("Whole scene")}</option>
          <option value="range">{t("Time range")}</option>
        </Select>
      </Field>
      {activeScope === "range" && (
        <>
          <p className="muted">
            {t(
              "Enter start and end times in seconds. Times are aligned to the nearest audio sample.",
            )}
          </p>
          <Field label={t("Start time (seconds)")}>
            <input
              type="text"
              inputMode="decimal"
              value={capturedRange ? String(Number(capturedRange.start_sample) / attempt!.request.sample_rate) : startSample}
              disabled={busy || pending || captured}
              onChange={(e) => setStartSample(e.target.value)}
            />
          </Field>
          <Field label={t("End time (seconds)")}>
            <input
              type="text"
              inputMode="decimal"
              value={capturedRange ? String((Number(capturedRange.start_sample) + Number(capturedRange.frame_count)) / attempt!.request.sample_rate) : frameCount}
              disabled={busy || pending || captured}
              onChange={(e) => setFrameCount(e.target.value)}
            />
          </Field>
        </>
      )}
      <ExportSummary view={view} sequenceId={attempt?.request.sequence_id ?? scene.sequenceId} revision={attempt?.request.revision} audioOnly duration={selectedDuration} />
      <p>
        {t(
          "Save the mixed audio as a high-quality stereo WAV file.",
        )}
      </p>
      <Field label={t("Sample rate")}>
        <Select
          value={captured ? attempt.request.sample_rate : sampleRate}
          disabled={busy || pending || !!captured || !!availability.reason}
          onChange={(e) => setSampleRate(Number(e.target.value))}
        >
          {(captured ? [attempt.request.sample_rate] : rates).map((rate) => (
            <option key={rate} value={rate}>
              {rate === 44100 ? "44.1" : "48"} kHz
            </option>
          ))}
        </Select>
      </Field>
      {availability.reason && <p role="status">{t(availability.reason)}</p>}
      {error && !attempt && <ErrorNotice error={error} />}
      {attempt?.jobId && <ExportStatus jobId={attempt.jobId} name={attempt.sceneName} />}
      {attempt && (
        <div role={attempt.error ? "alert" : "status"}>
          <p>
            {t(
              attempt.phase === "unconfirmed"
                ? "The audio export submission is not confirmed. Retry the original submission to find its task."
                : attempt.phase === "accepted"
                  ? "Your export continues even if you close this window."
                  : attempt.phase === "submitting"
                    ? "Starting audio WAV export…"
                    : "Audio WAV export was not accepted.",
            )}
          </p>
          {attempt.error && <ErrorNotice error={attempt.error} />}
          {attempt.phase === "accepted" && (
            <>
              <Button
                onClick={() => {
                  onClose();
                  showTasks();
                }}
              >
                {t(embedded ? "View export records" : "View tasks")}
              </Button>
              <Button
                disabled={busy || pending}
                onClick={() => controller.scene.audioWav.dismiss()}
              >
                {t("New audio export")}
              </Button>
            </>
          )}
          {["unconfirmed", "retryable"].includes(attempt.phase) && (
            <Button
              disabled={busy || pending}
              onClick={() => void run(() => controller.scene.audioWav.retry())}
            >
              {t("Retry original export")}
            </Button>
          )}
          {attempt.phase === "retryable" && (
            <Button
              disabled={busy || pending}
              onClick={() => controller.scene.audioWav.dismiss()}
            >
              {t("Reset export")}
            </Button>
          )}
        </div>
      )}
      <p className="muted">
        {t(
          embedded ? "Find progress and published files in Export records." : "You can also find this export in Tasks.",
        )}
      </p>
      <div className="modal-footer">
        <Button onClick={onClose}>{t("Close")}</Button>
        <Button
          className="primary"
          disabled={
            busy ||
            pending ||
            !!availability.reason ||
            !!captured ||
            !rates.some((rate) => rate === sampleRate)
          }
          onClick={() =>
            void run(() =>
              controller.scene.audioWav.start(
                sampleRate,
                activeScope === "range"
                  ? secondsRange(startSample, frameCount, sampleRate)
                  : undefined,
              ),
            )
          }
        >
          {t("Start audio export")}
        </Button>
      </div>
    </>
  );
  return embedded ? body : <Modal title={t("Export audio WAV")} onClose={onClose}>{body}</Modal>;
}
