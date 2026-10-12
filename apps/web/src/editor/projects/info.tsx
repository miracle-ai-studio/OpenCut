import type { ProjectRef } from "../model/types";
import { formatTime } from "../model/time";
import { t, useLocale } from "../shared/locale";
import { Button, Modal } from "../shared/ui";
export function projectDate(value?: string, language = "en") {
  return value
    ? new Date(value).toLocaleString(language === "zh" ? "zh-CN" : "en-US", {
        dateStyle: "medium",
        timeStyle: "short",
      })
    : "—";
}
export function ProjectInfo({
  project,
  onClose,
}: {
  project: ProjectRef;
  onClose: () => void;
}) {
  const { language } = useLocale();
  const meta = project.library;
  const rows = [
    ["Duration", meta ? formatTime(meta.duration) : "—"],
    ["Created", projectDate(meta?.created_at, language)],
    ["Modified", projectDate(meta?.modified_at, language)],
    ["Canvas", meta ? `${meta.width} × ${meta.height}` : "—"],
    [
      "Frame rate",
      meta
        ? `${(meta.fps.numerator / meta.fps.denominator).toFixed(3)} ${t("fps")}`
        : "—",
    ],
    ["Scenes", meta ? String(meta.scene_count) : "—"],
    ["Media", meta ? String(meta.asset_count) : "—"],
    ["Schema", project.format === "v4" ? "4" : "Legacy"],
  ];
  return (
    <Modal title={t("Project information")} onClose={onClose}>
      <h3 className="project-info-name">{project.name}</h3>
      <dl className="project-info">
        {rows.map(([label, value]) => (
          <div key={label}>
            <dt>{t(label)}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
      <div className="modal-footer">
        <Button onClick={onClose}>{t("Done")}</Button>
      </div>
    </Modal>
  );
}
