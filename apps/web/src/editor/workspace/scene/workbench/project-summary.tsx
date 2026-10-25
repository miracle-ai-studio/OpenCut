import { Film, Pencil, ScanLine } from "lucide-react";
import { Button } from "../../../shared/ui";
import { t } from "../../../shared/locale";
import { formatTime, fromSeconds } from "../../../model/time";
import { sequenceEnd } from "../timeline-edit/geometry";
import type { Document, Sequence } from "../types";

export function ProjectSummary({
  project,
  sequence,
  onSettings,
}: {
  project: Document;
  sequence: Sequence;
  onSettings(): void;
}) {
  const settings =
    project.settings.profile === "classic_global"
      ? project.settings.settings
      : project.settings.formats.find(
          (item) => item.sequence_id === sequence.id,
        )!;
  const fps = settings.fps.numerator / settings.fps.denominator;
  const { width, height } = settings.canvas_size;
  const gcd = (a: number, b: number): number => (b ? gcd(b, a % b) : a);
  const ratio = gcd(width, height);
  return (
    <section
      className="workbench-project-summary"
      aria-label={t("Draft parameters")}
    >
      <header>
        <ScanLine size={16} />
        <h3>{t("Draft parameters")}</h3>
      </header>
      <dl>
        <div>
          <dt>{t("Project name")}</dt>
          <dd>{project.name}</dd>
        </div>
        <div>
          <dt>{t("Save mode")}</dt>
          <dd>
            <span className="workbench-status-dot" />
            {t("Automatically saved")}
          </dd>
        </div>
        <div>
          <dt>{t("Canvas color space")}</dt>
          <dd>sRGB</dd>
        </div>
        <div>
          <dt>{t("Media")}</dt>
          <dd>
            {project.assets.length} {t("files")}
          </dd>
        </div>
      </dl>
      <dl>
        <div>
          <dt>{t("Aspect ratio")}</dt>
          <dd>
            {width / ratio}:{height / ratio}
          </dd>
        </div>
        <div>
          <dt>{t("Resolution")}</dt>
          <dd>
            {width} × {height}
          </dd>
        </div>
        <div>
          <dt>{t("Frame rate")}</dt>
          <dd>{Number.isInteger(fps) ? fps.toFixed(2) : fps.toFixed(3)} fps</dd>
        </div>
        <div>
          <dt>{t("Duration")}</dt>
          <dd>{formatTime(fromSeconds(sequenceEnd(sequence)))}</dd>
        </div>
      </dl>
      <footer>
        <span>
          <Film size={13} />
          {sequence.name}
        </span>
        <Button onClick={onSettings} icon={<Pencil size={13} />}>
          {t("Modify")}
        </Button>
      </footer>
    </section>
  );
}
