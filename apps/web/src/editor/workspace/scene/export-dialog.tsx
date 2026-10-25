import { useState, useSyncExternalStore } from "react";
import { useWorkspace } from "../context";
import { Field, Modal } from "../../shared/ui";
import { Select } from "../../shared/controls";
import { t, useLocale } from "../../shared/locale";
import { AudioWavDialog } from "./audio-wav-dialog";
import { VideoExportDialog } from "./video-export-dialog";
import "./export-records.css";

/** One presentation entry; each existing exporter retains its original durable
 * request, capability checks, fixed revision and submission recovery. */
export function ExportDialog({ onClose, showRecords }: { onClose(): void; showRecords(): void }) {
  useLocale();
  const { controller, scene, busy } = useWorkspace();
  const exporter = controller.scene.exporter;
  const video = useSyncExternalStore(exporter.subscribe, exporter.getSnapshot);
  const [format, setFormat] = useState<"mp4" | "webm" | "wav">(() => video.attempt?.request.options.format ?? (scene?.audioWav ? "wav" : "mp4"));
  const captured = format === "wav" ? scene?.audioWav : video.attempt;
  if (!scene) return null;
  return <Modal title={t("Export")} onClose={onClose} className="unified-export-modal">
    <Field label={t("Format")}><Select value={format === "wav" ? format : video.attempt?.request.options.format ?? format} disabled={busy || !!captured || format !== "wav" && video.checking}
      onChange={event => {const next=event.target.value;if(next==="mp4" || next==="webm" || next==="wav")setFormat(next);}}>
      <option value="mp4">MP4 · H.264</option><option value="webm">WebM · VP9</option><option value="wav">WAV · {t("Audio only")}</option>
    </Select></Field>
    <div hidden={format !== "wav"} inert={format !== "wav"}><AudioWavDialog embedded onClose={onClose} showTasks={showRecords}/></div>
    <div hidden={format === "wav"} inert={format === "wav"}><VideoExportDialog embedded format={format === "wav" ? "mp4" : format} exporter={exporter} onClose={onClose} showTasks={showRecords}/></div>
  </Modal>;
}
