import { useEffect, useState, useSyncExternalStore } from "react";
import { Button, Field, Modal } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { useWorkspace } from "../context";
import { ExportVideoV2, type ExportVideoOptionsV2 } from "./export_video_v2";

export function ExportVideoV2Dialog({ exporter, onClose, showTasks }: { exporter: ExportVideoV2; onClose(): void; showTasks(): void }) {
  useLocale();
  const { controller, api, v4, busy } = useWorkspace(), state = useSyncExternalStore(exporter.subscribe, exporter.getSnapshot);
  const [options, setOptions] = useState<ExportVideoOptionsV2>({ format: "mp4", quality: "high", include_audio: true });
  const [pending, setPending] = useState(false), [error, setError] = useState<string | null>(null);
  const view = v4?.historical ?? v4?.head;
  useEffect(() => { void exporter.refreshCapability(); }, [exporter, api, v4?.head.project.id, v4?.sequenceId, view?.revision]);
  if (!v4 || !view) return null;
  const attempt = state.attempt, captured = attempt?.request.options ?? options, unavailable = exporter.unavailable(options);
  const run = async (action: () => Promise<unknown>) => {
    setPending(true); setError(null);
    try { await action(); } catch (e) { setError(e instanceof Error ? e.message : String(e)); controller.report(e); } finally { setPending(false); }
  };
  const reset = () => { try { exporter.dismiss(); setError(null); } catch (e) { setError(String(e)); } };
  return <Modal title={t("Export video")} onClose={onClose}>
    <p>{t("Scene")}: {attempt?.sceneName ?? view.project.sequences.find(s => s.id === v4.sequenceId)?.name}</p>
    <p className="muted">{t("Saved revision")}: {attempt?.request.revision ?? view.revision}</p>
    <p>{t("Exports the saved scene with its canvas size and frame rate. Scene notes are excluded.")}</p>
    <fieldset disabled={busy || pending || !!attempt}>
      <Field label={t("Format")}><select value={captured.format} onChange={e => {
        const format = e.target.value; if (format === "mp4" || format === "webm") setOptions(current => ({ ...current, format }));
      }}><option value="mp4">MP4 (H.264)</option><option value="webm">WebM (VP9)</option></select></Field>
      <Field label={t("Quality")}><select value={captured.quality} onChange={e => {
        const quality = e.target.value; if (quality === "low" || quality === "medium" || quality === "high" || quality === "very_high") setOptions(current => ({ ...current, quality }));
      }}><option value="low">{t("Low")}</option><option value="medium">{t("Medium")}</option><option value="high">{t("High")}</option><option value="very_high">{t("Very high")}</option></select></Field>
      <label><input type="checkbox" checked={captured.include_audio} onChange={e => setOptions(current => ({ ...current, include_audio: e.target.checked }))} />{t("Include audio")}</label>
    </fieldset>
    {unavailable && !attempt && <p role="status">{t(unavailable)}</p>}
    {(error || attempt?.error) && <p role="alert">{t(error ?? attempt?.error ?? "")}</p>}
    {attempt && <div role="status">
      <p>{t(attempt.phase === "accepted" ? "Export task accepted. Progress and downloads are in Tasks."
        : attempt.phase === "unconfirmed" ? "The submission is not confirmed. Retry the original export to find its task."
        : attempt.phase === "submitting" ? "Starting video export…" : "Export was not accepted.")}</p>
      {attempt.phase === "accepted" && <><Button onClick={() => { onClose(); showTasks(); }}>{t("View tasks")}</Button><Button disabled={busy || pending} onClick={reset}>{t("New video export")}</Button></>}
      {(attempt.phase === "unconfirmed" || attempt.phase === "retryable") && <Button disabled={busy || pending} onClick={() => void run(() => exporter.retry())}>{t("Retry original export")}</Button>}
      {attempt.phase === "retryable" && <Button disabled={busy || pending} onClick={reset}>{t("Reset export")}</Button>}
    </div>}
    <p className="muted">{t("Closing this window does not cancel an accepted task. Use Tasks to request cancellation.")}</p>
    <div className="modal-footer"><Button onClick={onClose}>{t("Close")}</Button>
      <Button className="primary" disabled={busy || pending || state.checking || !!unavailable || !!attempt} onClick={() => void run(() => exporter.start(options))}>{t("Start video export")}</Button></div>
  </Modal>;
}
