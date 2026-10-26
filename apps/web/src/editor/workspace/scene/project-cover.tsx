import { useEffect, useRef, useState } from "react";
import { Select } from "../../shared/controls";
import { Button, Field, Modal } from "../../shared/ui";
import { ErrorNotice } from "../../shared/error-notice";
import { t, useLocale } from "../../shared/locale";
import { secondsForDisplay, timeFromDecimalSeconds } from "../../model/time";
import { useWorkspace } from "../context";
import { uploadMedia } from "../upload";
import { editRecoveryBlocked } from "./edit-recovery";
import type { Document, Operation } from "./types";

type Cover = NonNullable<Document["cover"]>;
type Mode = Cover["kind"] | "automatic";

/** Artwork selects immutable source media; it never reinterprets a timeline
 * frame, creates a render job, or writes browser-only project metadata. */
export function ProjectCoverEditor({ onClose }: { onClose(): void }) {
  useLocale();
  const { api, scene, controller, busy } = useWorkspace();
  const [scope] = useState(() => ({ api, project: scene!.head.project, revision: scene!.head.revision }));
  const original = scope.project.cover;
  const [mode, setMode] = useState<Mode>(original?.kind ?? (scope.project.assets.some(a => a.kind === "video") ? "video_frame" : "image"));
  const [assetId, setAssetId] = useState(original?.asset_id ?? scope.project.assets.find(a => a.kind === "video")?.id ?? scope.project.assets.find(a => a.kind === "image")?.id ?? "");
  const [time, setTime] = useState(original?.kind === "video_frame" ? secondsForDisplay(original.time).toFixed(6) : "0");
  const [timeEdited, setTimeEdited] = useState(false);
  const [file, setFile] = useState<File | null>(null);
  const [url, setUrl] = useState<string | null>(null);
  const [ready, setReady] = useState(false), [pending, setPending] = useState(false);
  const [progress, setProgress] = useState(""), [error, setError] = useState<unknown>(null);
  const video = useRef<HTMLVideoElement>(null), upload = useRef<AbortController | null>(null), flight = useRef(false);
  const asset = scope.project.assets.find(a => a.id === assetId && (mode === "image" ? a.kind === "image" : a.kind === "video"));
  const duration = asset?.duration ? secondsForDisplay(asset.duration) : 0;
  const selectedTime = Number(time);
  const validTime = mode !== "video_frame" || (!!time.trim() && Number.isFinite(selectedTime) && selectedTime >= 0 && selectedTime < duration);
  const stale = api !== scope.api || scene?.head.project.id !== scope.project.id || scene?.head.revision !== scope.revision || !!scene?.historical;
  const blocked = stale || busy || pending || editRecoveryBlocked(scene?.recovery) || controller.scene.recovery.blocked()
    || !!scene?.gainDraft || !!scene?.media.attempt || !!scene?.propertyDrafts?.length
    || ["loading", "playing", "stalled", "unconfirmed"].includes(scene?.audioPreview.phase ?? "");
  const fence = () => {
    const current = controller.getSnapshot();
    if (current.api !== scope.api || current.scene?.head.project.id !== scope.project.id || current.scene.head.revision !== scope.revision || current.scene.historical)
      throw Error(t("The project changed. Reopen the cover editor."));
  };
  useEffect(() => () => upload.current?.abort(), []);
  useEffect(() => {
    const abort = new AbortController();
    setUrl(null); setReady(false); setError(null);
    if (stale || mode === "automatic") return () => abort.abort();
    if (mode === "image" && file) {
      const local = URL.createObjectURL(file); setUrl(local);
      return () => { abort.abort(); URL.revokeObjectURL(local); };
    }
    if (asset && scope.api) void scope.api.coverSource(scope.project.id, scope.revision, asset, abort.signal).then(source => {
      if (abort.signal.aborted) return;
      if (!source) throw Error(t("Cover preview requires the local workspace."));
      setUrl(source);
    }).catch(reason => { if (!abort.signal.aborted) setError(reason); });
    return () => abort.abort();
  }, [scope, asset, file, mode, stale]);
  const seek = () => {
    if (!video.current || !validTime) return;
    setReady(false);
    try { video.current.currentTime = selectedTime; }
    catch (reason) { setError(reason); }
  };
  useEffect(() => { if (video.current?.readyState && validTime) seek(); else setReady(false); }, [time, url, validTime]);
  function chooseMode(next: Mode) {
    setMode(next); setFile(null); setTime("0");
    setTimeEdited(true);
    setAssetId(scope.project.assets.find(a => a.kind === (next === "image" ? "image" : "video"))?.id ?? "");
  }
  async function save() {
    if (blocked || flight.current || !scope.api || !validTime || (mode !== "automatic" && !ready)) return;
    flight.current = true; setPending(true); setError(null);
    const abort = new AbortController(); upload.current = abort;
    try {
      fence();
      const operations: Operation[] = [];
      let cover: Cover | null = null;
      if (mode === "image" && file) {
        const imported = await uploadMedia(scope.api.sdk, scope.project.id, file, setProgress, abort.signal);
        abort.signal.throwIfAborted(); fence();
        if (imported.media.project_id !== scope.project.id || imported.asset.kind !== "image" || !imported.version)
          throw Error(t("Choose an image for the project cover."));
        const id = crypto.randomUUID();
        operations.push({ type: "asset_bind", id, name: file.name, source: { kind: "project_media", media_id: imported.media_id, version: imported.version } });
        cover = { kind: "image", asset_id: id };
      } else if (mode !== "automatic") {
        if (!asset) throw Error(t("Choose source media for the cover."));
        cover = mode === "image" ? { kind: "image", asset_id: asset.id }
          : { kind: "video_frame", asset_id: asset.id, time: !timeEdited && original?.kind === "video_frame" && original.asset_id === asset.id ? original.time : timeFromDecimalSeconds(time) };
      }
      fence(); abort.signal.throwIfAborted();
      operations.push({ type: "cover_set", cover });
      await controller.scene.write("Set project cover", operations);
      onClose();
    } catch (reason) { if (!abort.signal.aborted) setError(reason); }
    finally { flight.current = false; setPending(false); upload.current = null; setProgress(""); }
  }
  return <Modal title={t("Project cover")} onClose={onClose} className="project-cover-dialog">
    <section className="project-cover-editor">
      <fieldset disabled={blocked}>
        <Field label={t("Cover source")}><Select value={mode} onChange={e => chooseMode(e.target.value as Mode)}>
          <option value="video_frame">{t("Choose a video frame")}</option>
          <option value="image">{t("Use an image")}</option>
          <option value="automatic">{t("Automatic cover")}</option>
        </Select></Field>
        {mode !== "automatic" && <Field label={t(mode === "image" ? "Image" : "Video")}><Select value={file ? "" : assetId} onChange={e => { setFile(null); setAssetId(e.target.value); setTime("0"); setTimeEdited(true); }}>
          <option value="">{t("Choose source media for the cover.")}</option>
          {scope.project.assets.filter(a => a.kind === (mode === "image" ? "image" : "video")).map(a => <option key={a.id} value={a.id}>{a.name}</option>)}
        </Select></Field>}
        {mode === "image" && <Field label={t("Upload cover image")}><input type="file" accept="image/*" onChange={e => { setFile(e.target.files?.[0] ?? null); setAssetId(""); }} /></Field>}
        {mode === "video_frame" && <>
          <Field label={t("Cover frame (seconds)")}><input type="number" min="0" max={Math.max(0, duration - 0.001)} step="0.001" value={time} onChange={e => { setTime(e.target.value); setTimeEdited(true); }} /></Field>
          <input type="range" aria-label={t("Select cover frame")} min="0" max={Math.max(0, duration - 0.001)} step="0.001" value={validTime ? selectedTime : 0} disabled={!duration} onChange={e => { setTime(e.target.value); setTimeEdited(true); }} />
        </>}
      </fieldset>
      {mode === "automatic" ? <p className="muted">{t("Use the first visual source in the selected scene.")}</p> : <>
        <div className="project-cover-preview">
          {url && (mode === "image" ? <img src={url} alt={t("Cover preview")} onLoad={() => setReady(true)} onError={() => { setReady(false); setError(t("Cover preview unavailable.")); }} />
            : <video key={url} ref={video} src={url} muted playsInline preload="auto" aria-label={t("Cover preview")} onLoadedData={() => { if (selectedTime === 0) setReady(true); else seek(); }} onSeeked={() => setReady(validTime)} onError={() => { setReady(false); setError(t("Cover preview unavailable.")); }} />)}
          {!url && <span className="muted">{t("Choose source media for the cover.")}</span>}
        </div>
        <p className="muted">{t("The cover uses source media. Timeline effects and titles are not included.")}</p>
      </>}
      {stale && <p role="alert">{t("The project changed. Reopen the cover editor.")}</p>}
      {!validTime && <p role="alert">{t("Choose a time within the source video.")}</p>}
      {error != null && <ErrorNotice error={error} />}
      {progress && <p role="status">{progress}</p>}
      <div className="toolbar"><Button className="primary" disabled={blocked || !validTime || (mode !== "automatic" && !ready)} onClick={() => void save()}>{t(pending ? "Saving cover…" : "Save cover")}</Button><Button onClick={onClose}>{t("Cancel")}</Button></div>
    </section>
  </Modal>;
}
