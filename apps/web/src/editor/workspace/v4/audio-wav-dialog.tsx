import { useEffect, useState } from "react";
import { Button, Field, Modal } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { useWorkspace } from "../context";
import { audioSampleRange, audioWavAvailability } from "./audio-wav";

export function AudioWavDialog({
  onClose,
  showTasks,
}: {
  onClose(): void;
  showTasks(): void;
}) {
  useLocale();
  const { controller, v4, busy } = useWorkspace();
  const [scope, setScope] = useState<"whole" | "range">("whole");
  const [startSample, setStartSample] = useState("0");
  const [frameCount, setFrameCount] = useState("");
  const attempt = v4?.audioWav;
  const activeScope = attempt
    ? "range" in attempt.request
      ? "range"
      : "whole"
    : scope;
  const rates = v4 ? audioWavAvailability(v4, activeScope).rates : [];
  const [sampleRate, setSampleRate] = useState<number>(() =>
    rates.includes(48000) ? 48000 : (rates[0] ?? 48000),
  );
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    void controller.v4.audioWav.refreshCapability();
    void controller.v4.audioWav.refreshCapability("range");
  }, [controller]);
  useEffect(() => {
    if (rates.length && !rates.some((rate) => rate === sampleRate))
      setSampleRate(rates[0]);
  }, [rates.join(","), sampleRate]);
  if (!v4) return null;
  const availability = audioWavAvailability(v4, activeScope);
  const capturedRange =
    attempt && "range" in attempt.request ? attempt.request.range : null;
  const view = v4.historical ?? v4.head;
  const sequence = view.project.sequences.find((s) => s.id === v4.sequenceId);
  const captured = !!attempt;
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
  return (
    <Modal title={t("Export audio WAV")} onClose={onClose}>
      <p>
        {t("Scene")}: {captured ? attempt.sceneName : sequence?.name}
      </p>
      <Field label={t("Export scope")}>
        <select
          value={activeScope}
          disabled={busy || pending || captured}
          onChange={(e) => setScope(e.target.value as "whole" | "range")}
        >
          <option value="whole">{t("Whole scene")}</option>
          <option value="range">{t("Sample range")}</option>
        </select>
      </Field>
      {activeScope === "range" && (
        <>
          <p className="muted">
            {t(
              "Sample indexes use the selected sample rate. The service validates the range against this saved scene; no seconds are rounded. Mastering uses the whole scene before extracting the range.",
            )}
          </p>
          <Field label={t("Starting sample (0-based)")}>
            <input
              type="text"
              inputMode="numeric"
              value={capturedRange?.start_sample ?? startSample}
              disabled={busy || pending || captured}
              onChange={(e) => setStartSample(e.target.value)}
            />
          </Field>
          <Field label={t("Frame count")}>
            <input
              type="text"
              inputMode="numeric"
              value={capturedRange?.frame_count ?? frameCount}
              disabled={busy || pending || captured}
              onChange={(e) => setFrameCount(e.target.value)}
            />
          </Field>
        </>
      )}
      <p className="muted">
        {t("Saved revision")}:{" "}
        {captured ? attempt.request.revision : view.revision}
      </p>
      <p>
        {t(
          "Exports the scene audio mix as a stereo Float32 WAV. This file contains audio only.",
        )}
      </p>
      <Field label={t("Sample rate")}>
        <select
          value={captured ? attempt.request.sample_rate : sampleRate}
          disabled={busy || pending || !!captured || !!availability.reason}
          onChange={(e) => setSampleRate(Number(e.target.value))}
        >
          {(captured ? [attempt.request.sample_rate] : rates).map((rate) => (
            <option key={rate} value={rate}>
              {rate === 44100 ? "44.1" : "48"} kHz
            </option>
          ))}
        </select>
      </Field>
      {availability.reason && <p role="status">{t(availability.reason)}</p>}
      {error && !attempt && <p role="alert">{t(error)}</p>}
      {attempt && (
        <div role={attempt.error ? "alert" : "status"}>
          <p>
            {t(
              attempt.phase === "unconfirmed"
                ? "The audio export submission is not confirmed. Retry the original submission to find its task."
                : attempt.phase === "accepted"
                  ? "Audio WAV task accepted. Progress and downloads are in Tasks."
                  : attempt.phase === "submitting"
                    ? "Starting audio WAV export…"
                    : "Audio WAV export was not accepted.",
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
                onClick={() => controller.v4.audioWav.dismiss()}
              >
                {t("New audio export")}
              </Button>
            </>
          )}
          {["unconfirmed", "retryable"].includes(attempt.phase) && (
            <Button
              disabled={busy || pending}
              onClick={() => void run(() => controller.v4.audioWav.retry())}
            >
              {t("Retry original export")}
            </Button>
          )}
          {attempt.phase === "retryable" && (
            <Button
              disabled={busy || pending}
              onClick={() => controller.v4.audioWav.dismiss()}
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
          disabled={
            busy ||
            pending ||
            !!availability.reason ||
            !!captured ||
            !rates.some((rate) => rate === sampleRate)
          }
          onClick={() =>
            void run(() =>
              controller.v4.audioWav.start(
                sampleRate,
                activeScope === "range"
                  ? audioSampleRange(startSample, frameCount)
                  : undefined,
              ),
            )
          }
        >
          {t("Start audio export")}
        </Button>
      </div>
    </Modal>
  );
}
