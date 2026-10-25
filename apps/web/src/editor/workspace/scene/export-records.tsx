import { Fragment, useState } from "react";
import { FileAudio, FileVideo, Info, RefreshCw, X } from "lucide-react";
import { useWorkspace } from "../context";
import { Button, Empty } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { ErrorNotice, friendlyError } from "../../shared/error-notice";
import { extension } from "../../shared/download";
import { ArtifactDownload } from "./artifact-download";
import { exportRecord, isUserExport, publishedArtifacts } from "./export-record";
import type { Job } from "../../../lib/editor-sdk";
import "./export-records.css";

export function ExportRecords({ onExport }: { onExport(): void }) {
  const { api, scene } = useWorkspace();
  return <ExportRecordsBody key={`${api?.connectionIdentity}:${scene?.head.project.id}`} onExport={onExport} />;
}
function ExportRecordsBody({ onExport }: { onExport(): void }) {
  useLocale();
  const { controller, api, jobs } = useWorkspace();
  const [filter, setFilter] = useState("all"), [search, setSearch] = useState(""), [limit, setLimit] = useState(50);
  const [expanded, setExpanded] = useState<string | null>(null), [pending, setPending] = useState(false), [error, setError] = useState<unknown>(null);
  const all = jobs.filter(isUserExport);
  const matching = all.filter(job => (filter === "all" || filter === "active" && ["queued", "running"].includes(job.state) || filter === job.state)
    && `${job.job_id} ${exportRecord(job).format ?? ""} ${t(job.kind.includes("audio") ? "Audio WAV export" : "Video export")}`.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()));
  const run = async (action: () => Promise<unknown>) => { if (pending) return; setPending(true); setError(null); try { await action(); } catch(e) { setError(e); } finally { setPending(false); } };
  return <section className="export-records" aria-label={t("Export records")}>
    <div className="export-record-toolbar">
      <label className="export-search"><span>{t("Search exports")}</span><input type="search" placeholder={t("Search format or task ID")} value={search} onChange={e => {setSearch(e.target.value);setLimit(50);}} /></label>
      <div role="group" aria-label={t("Export status")} className="export-filters">{[["all","All exports"],["active","In progress"],["succeeded","Completed"],["failed","Failed"],["cancelled","Cancelled"]].map(([value,label]) => <Button key={value} aria-pressed={filter===value} onClick={() => {setFilter(value);setLimit(50);}}>{t(label)}</Button>)}</div>
      <Button disabled={!api || pending} aria-label={t("Refresh exports")} icon={<RefreshCw size={15}/>} onClick={() => void run(() => controller.sync())}/>
    </div>
    {error !== null && <ErrorNotice error={error}/>}
    <div className="export-record-scroll">
      {!matching.length ? <Empty title={t(all.length ? "No matching exports" : "No export records yet")}><p>{t("Only video and audio exports appear here. Previews and imports stay in Tasks.")}</p><Button onClick={onExport}>{t("Export")}</Button></Empty>
        : <table className="export-record-table"><thead><tr><th>{t("File")}</th><th>{t("Output specifications")}</th><th>{t("Status")}</th><th>{t("Actions")}</th></tr></thead><tbody>{matching.slice(0,limit).map(job => {
          const spec = exportRecord(job), artifacts = publishedArtifacts(job), Icon = job.kind.includes("audio") ? FileAudio : FileVideo;
          const active = ["queued","running"].includes(job.state), failure = job.failure ?? job.result_unavailable;
          const progress = job.progress?.fraction;
          return <Fragment key={job.job_id}><tr className="export-record-row" data-export-job={job.job_id}>
            <td><div className="export-record-file"><span className="export-type-icon" aria-hidden="true"><Icon size={18}/></span><div><strong title={job.job_id}>{t(job.kind.includes("audio") ? "Audio WAV export" : job.kind === "render.video_range" ? "Video range export" : "Video export")}</strong><small title={t("File name and creation time were not recorded by this task.")}>— · {job.job_id.slice(-8)}</small></div></div></td>
            <td><strong>{spec.format ?? "—"}</strong><small>{spec.size ? `${spec.size} · ${spec.fps} fps` : spec.rate ? `${spec.rate/1000} kHz · ${spec.channels} ${t("channels")}` : "—"}{spec.seconds !== null && Number.isFinite(spec.seconds) ? ` · ${spec.seconds.toFixed(2)} ${t("seconds")}` : ""}</small></td>
            <td><span className={`export-state ${job.state}`}>{t(job.cancel_requested && active ? "Cancelling…" : job.state)}</span>{active && <progress max={1} value={typeof progress === "number" && Number.isFinite(progress) && progress >= 0 && progress <= 1 ? progress : undefined}/>}</td>
            <td><div className="export-record-actions">{active && <Button disabled={!api || pending || job.cancel_requested} icon={<X size={14}/>} onClick={() => void run(async () => {await api!.sdk.cancelJob(job.job_id);await controller.sync();})}>{t(job.cancel_requested ? "Cancelling…" : "Cancel export")}</Button>}
              {artifacts.map(a => <ArtifactDownload key={`${a.artifact_id}:${a.content_hash}`} artifact={a} name={`opencut-${job.job_id}.${extension(a.mime_type)}`}/>)}
              <Button className="export-details-button" aria-label={`${t("Export details")} · ${job.job_id}`} title={t("Export details")} aria-expanded={expanded===job.job_id} onClick={() => setExpanded(expanded===job.job_id ? null : job.job_id)} icon={<Info size={15}/>} />
            </div></td>
          </tr>{failure && <tr className="export-record-failure"><td colSpan={4}><span>{friendlyError(failure.message)}</span></td></tr>}
          {expanded===job.job_id && <tr className="export-record-detail"><td colSpan={4}><ExportDetails job={job}/></td></tr>}</Fragment>;
        })}</tbody></table>}
    </div>
    <footer className="export-record-footer"><span>{t("{count} export records",{count:matching.length})}</span>{matching.length>limit && <Button onClick={() => setLimit(n=>n+50)}>{t("Show more exports")}</Button>}<Button onClick={onExport}>{t("Export")}</Button></footer>
  </section>;
}
function ExportDetails({job}: {job:Job}) {
  return <div className="export-record-detail-body"><dl>{[["Task",job.job_id],["Saved revision",job.target?.revision],["Scene",job.target?.sequence_id],["File name",null],["Created",null]].map(([key,value])=><div key={key}><dt>{t(key!)}</dt><dd>{value??"—"}</dd></div>)}</dl>
    {(job.failure || job.result_unavailable) && <ErrorNotice error={job.failure?.message ?? job.result_unavailable?.message}/>}
    {!job.failure && !job.result_unavailable && !publishedArtifacts(job).length && job.state === "succeeded" && <p>{t("This task has no published downloadable file.")}</p>}
  </div>;
}
