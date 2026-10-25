import { t, useLocale } from "../shared/locale";
import { Eye } from "lucide-react";
import { useWorkspace } from "../workspace/context";
import { Button, Empty, Panel } from "../shared/ui";

/** Historical revisions are inspected without authoring or replaying edits. */
export function HistoryPanel() {
  useLocale();
  const { controller, snapshot, historical, history, busy } = useWorkspace();
  return <Panel title={t("History")}>
    <div className="toolbar padded">
      {historical && <Button disabled={busy} onClick={() => controller.returnToHead()}>{t("Return to current")}</Button>}
    </div>
    {!history?.revisions.length ? <Empty title={t("No history yet")} /> : <ol className="history-list">
      {history.revisions.map(revision => <li key={revision.revision}>
        <span>{revision.label}</span><span className="muted">{revision.revision}</span>
        <Button icon={<Eye size={14} />} disabled={busy || revision.revision === (historical ?? snapshot)?.revision}
          onClick={() => void controller.inspectRevision(revision.revision).catch(error => controller.report(error))}>{t("View")}</Button>
      </li>)}
    </ol>}
    {history?.next_cursor && <Button disabled={busy} onClick={() => void controller.olderHistory().catch(error => controller.report(error))}>{t("Load more")}</Button>}
  </Panel>;
}
