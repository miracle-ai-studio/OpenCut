import { t, useLocale } from "../shared/locale";
import { Eye, Redo2, Undo2 } from "lucide-react";
import { useWorkspace } from "../workspace/context";
import { Button, Empty, Panel } from "../shared/ui";
export function HistoryPanel() {
  useLocale();
  const { controller, api, snapshot, historical, history, busy } =
    useWorkspace();
  const undo = () =>
    controller.undo().catch((error) => controller.report(error));
  const redoEdit = () =>
    controller.redo().catch((error) => controller.report(error));
  const restore = async () => {
    if (!api || !snapshot || !historical) return;
    try {
      await controller.write(() =>
        api.sdk.call("opencut_revision_restore", {
          project_id: snapshot.project.id,
          request: {
            expected_revision: snapshot.revision,
            target_revision: historical.revision,
            label: "Restore saved revision",
            idempotency_key: crypto.randomUUID(),
          },
        }),
      );
    } catch (error) {
      controller.report(error);
    }
  };
  return (
    <Panel title={t("History")}>
      <div className="toolbar padded">
        <Button
          aria-label={t("Undo latest action")}
          disabled={busy || !!historical || !controller.canUndo}
          icon={<Undo2 size={14} />}
          onClick={() => void undo()}
        >
          {t("Undo")}
        </Button>
        <Button
          aria-label={t("Redo action")}
          disabled={busy || !!historical || !controller.canRedo}
          icon={<Redo2 size={14} />}
          onClick={() => void redoEdit()}
        >
          {t("Redo")}
        </Button>
      </div>
      {historical && (
        <div className="history-banner">
          <strong>{t("Viewing a saved revision")}</strong>
          <Button onClick={() => controller.returnToHead()}>
            {t("Return to current")}
          </Button>
          {snapshot?.format === "base" && (
            <Button disabled={busy} onClick={() => void restore()}>
              {t("Restore this revision")}
            </Button>
          )}
        </div>
      )}
      <div className="scroll history-list">
        {!history && <Empty title={t("No history")} />}
        {history?.revisions.map((item, index) => (
          <button
            key={item.revision}
            className={`history-row ${historical?.revision === item.revision ? "active" : ""}`}
            onClick={() =>
              void controller
                .inspectRevision(item.revision)
                .catch((error) => controller.report(error))
            }
          >
            <Eye size={14} />
            <span>
              <strong>{item.label ?? "Project created"}</strong>
              <small>
                {index === 0 ? t("Current · ") : ""}
                {new Date(item.created_at).toLocaleString()}
              </small>
            </span>
          </button>
        ))}
        {history?.next_cursor && (
          <Button
            onClick={() =>
              void controller
                .olderHistory()
                .catch((error) => controller.report(error))
            }
          >
            {t("Load earlier history")}
          </Button>
        )}
      </div>
    </Panel>
  );
}
