import { useEffect, useState } from "react";
import { useWorkspace } from "../context";
import { Button } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { ErrorNotice } from "../../shared/error-notice";
import { extension } from "../../shared/download";
import { ArtifactDownload } from "./artifact-download";
import { publishedArtifacts } from "./export-record";
import type { Snapshot } from "./types";
import { addTime, compareTime } from "../../model/time";

export function exportSummary(view: Snapshot, sequenceId: string) {
  const sequence = view.project.sequences.find(s => s.id === sequenceId);
  const settings = view.project.settings;
  const format = settings.profile === "classic_global" ? settings.settings : settings.formats.find(f => f.sequence_id === sequenceId);
  let end = { ticks: "0", timebase: 1 };
  for (const track of sequence?.tracks ?? []) for (const item of track.elements) {
    const candidate = addTime(item.timeline_range.start, item.timeline_range.duration);
    if (compareTime(candidate, end) > 0) end = candidate;
  }
  for (const caption of sequence?.captions ?? []) {
    const candidate = addTime(caption.timeline_range.start, caption.timeline_range.duration);
    if (compareTime(candidate, end) > 0) end = candidate;
  }
  return { size: format ? `${format.canvas_size.width} × ${format.canvas_size.height}` : "—", fps: format ? Number((format.fps.numerator / format.fps.denominator).toFixed(3)) : "—", seconds: Number(end.ticks) / end.timebase };
}
export function ExportSummary({ view, sequenceId, revision, duration, audioOnly = false }: { view: Snapshot; sequenceId: string; revision?: string; duration?: number; audioOnly?: boolean }) {
  const { api } = useWorkspace();
  const [captured, setCaptured] = useState<Snapshot | null>(null);
  const [readError, setReadError] = useState<{ revision: string; project: string; error: unknown } | null>(null), [retry, setRetry] = useState(0);
  useEffect(() => {
    let current = true;
    if (revision && revision !== view.revision && api) {
      setReadError(null);
      void api.scene.inspect(view.project.id, revision).then(result => {
        if (!current) return;
        if (result.revision !== revision || result.project.id !== view.project.id) throw Error("The saved export settings changed. Retry reading this version.");
        setCaptured(result);
      }).catch(error => { if (current) setReadError({revision, project:view.project.id, error}); });
    }
    return () => { current = false; };
  }, [api, revision, view.project.id, view.revision, retry]);
  const source = !revision || revision === view.revision ? view : captured?.revision === revision && captured.project.id === view.project.id ? captured : null;
  if (!source) return readError && readError.revision === revision && readError.project === view.project.id ? <div><ErrorNotice error={readError.error}/><Button onClick={() => setRetry(value => value + 1)}>{t("Retry")}</Button></div> : <p role="status">{t("Reading export settings…")}</p>;
  const summary = exportSummary(source, sequenceId);
  return <dl className="export-summary">{!audioOnly && <><div><dt>{t("Resolution")}</dt><dd>{summary.size}</dd></div><div><dt>{t("Frame rate")}</dt><dd>{summary.fps} fps</dd></div></>}<div><dt>{t(duration === undefined ? "Duration" : "Selected duration")}</dt><dd>{Number.isFinite(duration ?? summary.seconds) ? (duration ?? summary.seconds).toFixed(2) : "—"} {t("seconds")}</dd></div></dl>;
}
/** Only display a captured accepted job. The task feed remains the owner of its state. */
export function ExportStatus({ jobId, name }: { jobId: string; name: string }) {
  useLocale(); const { api, controller, jobs } = useWorkspace();
  const [pending, setPending] = useState(false), [error, setError] = useState<unknown>(null);
  const job = jobs.find(item => item.job_id === jobId);
  useEffect(() => { void controller.sync(); }, [controller, jobId]);
  const run = async (action: () => Promise<unknown>) => { setPending(true); setError(null); try { await action(); } catch(e) { if (!(e instanceof DOMException && e.name === "AbortError")) setError(e); } finally { setPending(false); } };
  return <section className="export-progress" aria-label={t("Export progress")}>
    <p role="status">{t(!job ? "Loading export progress…" : job.cancel_requested && ["queued","running"].includes(job.state) ? "Cancelling…" : ({queued:"Waiting to export…",running:"Exporting…",succeeded:"Your export is ready",failed:"Export failed",cancelled:"Export cancelled"} as Record<string,string>)[job.state] ?? "Exporting…")}</p>
    {job && ["queued","running"].includes(job.state) && <><progress max="1" value={job.progress?.fraction ?? undefined}/><Button disabled={pending || job.cancel_requested} onClick={() => void run(async () => { await api!.sdk.cancelJob(jobId); await controller.sync(); })}>{t("Cancel export")}</Button></>}
    {job && publishedArtifacts(job).map(artifact => <ArtifactDownload key={artifact.artifact_id} artifact={artifact} name={`${name.replace(/[\\/:*?"<>|]/g,"-")}.${extension(artifact.mime_type)}`} label="Save file…"/>)}
    {(error || job?.failure || job?.result_unavailable) && <ErrorNotice error={error ?? job?.failure?.message ?? job?.result_unavailable?.message}/>}
  </section>;
}
/** Round entered times once to the nearest sample, without floating-point multiplication. */
export function secondsRange(start: string, end: string, rate: number) {
  const sample = (value: string) => { if (!/^\d+(\.\d{1,6})?$/.test(value)) throw Error("Enter start and end times in seconds."); const [whole, fraction=""] = value.split("."); const scale=10n ** BigInt(fraction.length); return (BigInt(whole+fraction)*BigInt(rate)*2n+scale)/(2n*scale); };
  const first=sample(start), last=sample(end); if(last<=first) throw Error("The end time must be after the start time.");
  return {start_sample:String(first),frame_count:String(last-first)};
}
