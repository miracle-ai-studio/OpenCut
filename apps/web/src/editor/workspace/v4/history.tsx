import { t, useLocale } from "../../shared/locale";
import { useState } from "react";
import { Button, Panel } from "../../shared/ui";
import { useWorkspace } from "../context";

export function V4History() {
  useLocale();
  const { controller, v4, busy } = useWorkspace();
  const [pending, setPending] = useState(false);
  if (!v4) return null;
  const run = async (action: () => Promise<unknown>) => {
    setPending(true);
    try {
      await action();
    } catch (e) {
      controller.report(e);
    } finally {
      setPending(false);
    }
  };
  const blocked = busy || pending || !!v4.historical;
  const proposal = v4.historyPlan;
  const area = (path: string) => {
    const parts = path
      .split("/")
      .filter(Boolean)
      .map((p) => p.replaceAll("~1", "/").replaceAll("~0", "~"));
    const scene =
      parts[0] === "sequences"
        ? v4.head.project.sequences.find((s) => s.id === parts[1])
        : null;
    const name = parts.includes("bookmarks")
      ? "Scene bookmarks"
      : parts.includes("captions")
        ? "Captions"
        : parts.includes("transitions")
          ? "Scene transitions"
          : parts.includes("tracks")
            ? "Tracks"
            : parts[0] === "settings"
              ? "Project settings"
              : parts[0] === "assets"
                ? "Media"
                : parts[0] === "fonts"
                  ? "Fonts"
                  : "Project contents";
    return t(name) + (scene ? ` · ${scene.name}` : "");
  };
  const reasons = {
    dependent_change: "Changed after the selected edit",
    project_constraint: "Restoring this change would invalidate the project",
    locked_track: "A track is locked",
    no_effect: "Nothing to change",
  } as const;
  return (
    <Panel
      title={t("History")}
      className="v4-history"
      actions={
        <>
          <Button
            disabled={blocked || !v4.navigation.undo}
            onClick={() =>
              void run(() =>
                controller.v4.planUndo(v4.navigation.undo!.transaction_id),
              )
            }
          >
            {t("Undo")}
          </Button>
          <Button
            disabled={blocked || !v4.navigation.redo}
            onClick={() =>
              void run(() =>
                controller.v4.planRedo(v4.navigation.redo!.undo_transaction_id),
              )
            }
          >
            {t("Redo")}
          </Button>
        </>
      }
    >
      <div className="scroll padded">
        <p className="muted">
          {t(
            "Select a change to undo. A plan checks conflicts against the current revision before saving.",
          )}
        </p>
        {proposal && (
          <section aria-label={t("History plan")} className="v4-history-plan">
            <strong>
              {t(proposal.direction === "undo" ? "Undo plan" : "Redo plan")}
            </strong>
            <ul>
              {proposal.plan.changes.map((c) => (
                <li key={c.path}>{area(c.path)}</li>
              ))}
            </ul>
            {proposal.plan.conflicts.map((c, i) => (
              <p role="alert" key={i}>
                {area(c.path)}: {t(reasons[c.reason])}
              </p>
            ))}
            <Button
              className="primary"
              disabled={blocked || !!proposal.plan.conflicts.length}
              onClick={() => void run(() => controller.v4.applyHistory())}
            >
              {t(proposal.direction === "undo" ? "Apply undo" : "Apply redo")}
            </Button>
          </section>
        )}
        <ol className="v4-revisions">
          {v4.history.revisions.map((r) => (
            <li key={r.revision}>
              <strong>{r.label ?? t("Project revision")}</strong>
              <time>{new Date(r.created_at).toLocaleString()}</time>
              <div className="toolbar">
                <Button
                  disabled={pending || busy}
                  onClick={() =>
                    void run(() => controller.v4.inspectRevision(r.revision))
                  }
                >
                  {t("View revision")}
                </Button>
                <Button disabled={pending || busy || !!v4.recovery?.record || !!v4.recovery?.error}
                  onClick={() => void run(() => controller.v4.restoreRevision(r.revision))}>
                  {t("Restore this revision")}
                </Button>
                <Button
                  disabled={blocked || !r.transaction_id}
                  onClick={() =>
                    void run(() => controller.v4.planUndo(r.transaction_id!))
                  }
                >
                  {t("Undo this change")}
                </Button>
              </div>
            </li>
          ))}
        </ol>
        {v4.history.next_cursor && (
          <Button
            disabled={pending}
            onClick={() => void run(() => controller.v4.olderHistory())}
          >
            {t("Older revisions")}
          </Button>
        )}
      </div>
    </Panel>
  );
}
