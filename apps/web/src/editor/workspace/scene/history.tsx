import { t, useLocale, hasMessage } from "../../shared/locale";
import { useEffect, useId, useRef, useState } from "react";
import { History as HistoryIcon, MoreHorizontal, RotateCcw, RotateCw } from "lucide-react";
import { Button, Panel, Modal } from "../../shared/ui";
import { useEditingWorkspace as useWorkspace } from "../context";
import type { EditorApi } from "../api";
import type { WorkspaceController } from "../controller";
import type { Language } from "../../shared/locale";
import { actorLabel, changeSummary, historyChanges, readHistoryPair, type RevisionActor } from "./history-changes";
import { formatHistoryDate, historyText } from "./history-copy";
import "./history.css";

const connections = new WeakMap<EditorApi, number>();
let nextConnection = 0;
function connectionKey(api: EditorApi | null) {
  if (!api) return 0;
  if (!connections.has(api)) connections.set(api, ++nextConnection);
  return connections.get(api)!;
}

export function History() {
  const { api, scene } = useWorkspace();
  return <HistoryBody key={`${connectionKey(api)}:${scene?.head.project.id ?? ""}`} />;
}

function HistoryBody() {
  const { language } = useLocale();
  const { api, controller, scene, busy } = useWorkspace();
  const h = (key: string) => historyText(key, language);
  const [pending, setPending] = useState(false);
  const [restore, setRestore] = useState<{ revision: string; expected: string; label: string; createdAt: string } | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [actions, setActions] = useState<string | null>(null);
  const panelId = useId();
  const [summaries, setSummaries] = useState<Record<string, { en: string; zh: string }>>({});
  const [actors, setActors] = useState<Record<string, RevisionActor | null>>({});
  const row = scene?.history.revisions.find(item => item.revision === expanded);
  const detail = useHistoryComparison(api, controller, scene?.head.project.id, row?.parent_revision ?? null, row?.revision);
  const restoring = useHistoryComparison(api, controller, scene?.head.project.id, restore?.expected ?? null, restore?.revision);
  useEffect(() => {
    if (!row || !detail.pair) return;
    setSummaries(current => ({ ...current, [row.revision]: {
      en: changeSummary(historyChanges(detail.pair!.before, detail.pair!.after, "en"), "en"),
      zh: changeSummary(historyChanges(detail.pair!.before, detail.pair!.after, "zh"), "zh"),
    } }));
  }, [row?.revision, detail.pair]);
  useEffect(() => {
    if (row && detail.actor !== undefined) setActors(current => ({ ...current, [row.revision]: detail.actor! }));
  }, [row?.revision, detail.actor]);
  useEffect(() => {
    if (restore && restoring.actor !== undefined) setActors(current => ({ ...current, [restore.revision]: restoring.actor! }));
  }, [restore?.revision, restoring.actor]);
  useEffect(() => {
    if (!actions) return;
    const outside = (event: PointerEvent) => {
      if ((event.target as Element)?.closest?.("[data-history-revision]")?.getAttribute("data-history-revision") !== actions) setActions(null);
    };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [actions]);
  if (!scene) return null;
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
  const blocked = busy || pending || !!scene.historical;
  const proposal = scene.historyPlan;
  const area = (path: string) => {
    const parts = path
      .split("/")
      .filter(Boolean)
      .map((p) => p.replaceAll("~1", "/").replaceAll("~0", "~"));
    const sequence =
      parts[0] === "sequences"
        ? scene.head.project.sequences.find((s) => s.id === parts[1])
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
    return t(name) + (sequence ? ` · ${sequence.name}` : "");
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
      className="editor-history"
      actions={
        <>
          <Button
            disabled={blocked || !scene.navigation.undo}
            onClick={() =>
              void run(() =>
                controller.scene.planUndo(scene.navigation.undo!.transaction_id, true),
              )
            }
          >
            {t("Undo")}
          </Button>
          <Button
            disabled={blocked || !scene.navigation.redo}
            onClick={() =>
              void run(() =>
                controller.scene.planRedo(scene.navigation.redo!.undo_transaction_id, true),
              )
            }
          >
            {t("Redo")}
          </Button>
        </>
      }
    >
      {restore && <Modal title={t("Restore this revision")} onClose={() => { if (!pending) setRestore(null); }}>
        <p><strong>{restoring.pair ? changeSummary(historyChanges(restoring.pair.before, restoring.pair.after, language), language) : restore.label}</strong> · {formatHistoryDate(restore.createdAt, language)}</p>
        <p>{actorLabel(restoring.actor !== undefined ? restoring.actor : Object.hasOwn(actors, restore.revision) ? actors[restore.revision] : scene.revisionActors?.[restore.revision], language)}</p>
        <p>{t("This replaces the current project contents with the selected saved version. Your newer versions stay in History, so you can restore them later.")}</p>
        <h4>{h("restoreCompare")}</h4>
        <ChangeDetails value={restoring} language={language} />
        {restore.expected !== scene.head.revision && <p role="alert">{h("staleRestore")}</p>}
        <div className="modal-footer"><Button disabled={pending} onClick={() => setRestore(null)}>{t("Cancel")}</Button><Button className="primary" disabled={pending || busy || !restoring.pair || !!restoring.error || restore.expected !== scene.head.revision} onClick={() => void run(async () => { await controller.scene.restoreRevision(restore.revision, restore.expected); setRestore(null); })}>{t("Restore project")}</Button></div>
      </Modal>}
      <div className="scroll history-scroll">
        <p className="muted history-intro">{h("rowHint")}</p>
        {proposal && (
          <section aria-label={t("History plan")} className="editor-history-plan">
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
              onClick={() => void run(() => controller.scene.applyHistory())}
            >
              {t(proposal.direction === "undo" ? "Apply undo" : "Apply redo")}
            </Button>
          </section>
        )}
        <ol className="editor-revisions">
          {scene.history.revisions.map((r) => {
            const label = historyLabel(r.label, r.parent_revision, language);
            const summary = summaries[r.revision]?.[language];
            const causal = r.label === "Undo change" || r.label === "Redo change";
            const heading = summary ? causal ? `${label} · ${summary}` : summary : label;
            const actor = actorLabel(r.revision === row?.revision && detail.actor !== undefined ? detail.actor : Object.hasOwn(actors, r.revision) ? actors[r.revision] : scene.revisionActors?.[r.revision], language);
            const Icon = r.label === "Undo change" ? RotateCcw : r.label === "Redo change" ? RotateCw : HistoryIcon;
            const detailId = `${panelId}-${r.revision}-changes`, actionId = `${panelId}-${r.revision}-actions`;
            return <li key={r.revision} data-history-revision={r.revision} onKeyDown={event => {
              if (event.key !== "Escape" || event.defaultPrevented || actions !== r.revision && expanded !== r.revision) return;
              event.preventDefault(); event.stopPropagation();
              if (actions === r.revision) { setActions(null); (event.currentTarget.querySelector(".history-entry-more") as HTMLElement)?.focus(); }
              else { setExpanded(null); (event.currentTarget.querySelector(".history-entry-summary") as HTMLElement)?.focus(); }
            }}>
              <div className="history-entry-row">
                <span className="history-entry-thumbnail" title={h("noThumbnail")} aria-hidden="true"><Icon size={17} /></span>
                <Button className="history-entry-summary" aria-label={`${h(expanded === r.revision ? "collapse" : "expand")} · ${heading}`} aria-expanded={expanded === r.revision} aria-controls={detailId}
                  onClick={() => { setActions(null); setExpanded(expanded === r.revision ? null : r.revision); }}>
                  <strong title={heading}>{heading}</strong>
                  <span className="history-entry-meta"><span title={actor}>{actor}</span><time title={formatHistoryDate(r.created_at, language)}>{formatHistoryDate(r.created_at, language, undefined, true)}</time></span>
                </Button>
                <Button className="history-entry-more" icon={<MoreHorizontal size={16} />} title={`${h("actions")} · ${heading}`} aria-label={`${h("actions")} · ${heading}`} aria-expanded={actions === r.revision} aria-controls={actionId} onClick={() => setActions(actions === r.revision ? null : r.revision)} />
              </div>
              {actions === r.revision && <div className="history-entry-actions" id={actionId} role="group" aria-label={h("actions")}>
                <Button
                  disabled={pending || busy}
                  onClick={() =>
                    { setActions(null); void run(() => controller.scene.inspectRevision(r.revision)); }
                  }
                >
                  {t("View revision")}
                </Button>
                <Button disabled={pending || busy || r.revision === scene.head.revision || !!scene.recovery?.record || !!scene.recovery?.error}
                  onClick={() => { setActions(null); setRestore({ revision: r.revision, expected: scene.head.revision, label: heading, createdAt: r.created_at }); }}>
                  {t("Restore this revision")}
                </Button>
                <Button
                  disabled={blocked || !r.transaction_id}
                  onClick={() =>
                    { setActions(null); void run(() => controller.scene.planUndo(r.transaction_id!)); }
                  }
                >
                  {t("Undo this change")}
                </Button>
              </div>}
              {expanded === r.revision && <div id={detailId}><ChangeDetails value={detail} language={language} compact /></div>}
            </li>;
          })}
        </ol>
      </div>
      <footer className="history-footer"><span className="muted">{historyText("loaded", language, { count: scene.history.revisions.length })}</span>
        {scene.history.next_cursor && (
          <Button
            disabled={pending}
            onClick={() => void run(() => controller.scene.olderHistory())}
          >
            {t("Older revisions")}
          </Button>
        )}
      </footer>
    </Panel>
  );
}

type Comparison = {
  api: EditorApi; connection: string | null; project: string; before: string | null; after: string;
  loading: boolean; pair?: Awaited<ReturnType<typeof readHistoryPair>>;
  actor?: RevisionActor | null; error?: unknown; actorError?: unknown;
};
function useHistoryComparison(api: EditorApi | null, controller: WorkspaceController, project: string | undefined, before: string | null, after: string | undefined) {
  const [result, setResult] = useState<Comparison | null>(null), [attempt, setAttempt] = useState(0);
  const latest = useRef({ api, project, before, after });
  latest.current = { api, project, before, after };
  useEffect(() => {
    if (!api || !project || !after) return;
    let live = true;
    const connection = api.connectionIdentity;
    const current = () => {
      const state = controller.getSnapshot(), wanted = latest.current;
      return live && state.api === api && api.connectionIdentity === connection && state.scene?.head.project.id === project &&
        wanted.api === api && wanted.project === project && wanted.before === before && wanted.after === after;
    };
    const scope = { api, connection, project, before, after };
    setResult({ ...scope, loading: true });
    void Promise.allSettled([
      readHistoryPair((id, revision) => api.scene.inspect(id, revision), project, before, after),
      api.scene.revisionActors(project, [after]),
    ]).then(([snapshots, attribution]) => {
      if (!current()) return;
      setResult({ ...scope, loading: false,
        ...(snapshots.status === "fulfilled" ? { pair: snapshots.value } : { error: snapshots.reason }),
        ...(attribution.status === "fulfilled" ? { actor: attribution.value[after] } : { actorError: attribution.reason }),
      });
    });
    return () => { live = false; };
  }, [api, controller, project, before, after, attempt]);
  const matches = result?.api === api && result?.connection === api?.connectionIdentity && result?.project === project && result?.before === before && result?.after === after;
  return { ...(matches ? result : null), retry: () => setAttempt(value => value + 1) };
}
function ChangeDetails({ value, language, compact = false }: { value: ReturnType<typeof useHistoryComparison>; language: Language; compact?: boolean }) {
  const h = (key: string) => historyText(key, language);
  if (value.loading || !value.pair && !value.error) return <p role="status">{h("loading")}</p>;
  const changes = value.pair ? historyChanges(value.pair.before, value.pair.after, language) : [];
  return <section className="history-change-details" aria-label={h("changes")}>
    {!!value.error && <p role="alert">{h("failed")}</p>}
    {!!value.actorError && <p role="alert">{h("actorFailed")}</p>}
    {!!(value.error || value.actorError) && <Button onClick={value.retry}>{h("retry")}</Button>}
    {value.pair && <><p className="history-change-summary">{changeSummary(changes, language)}</p>
      {!!changes.length && (compact ? <ul className="history-change-cards" aria-label={h("changes")}>
        {changes.map((change, index) => <li key={index}><strong>{change.object}</strong><span className="muted">{change.field}</span><dl>
          <dt>{h("before")}</dt><dd>{change.before}</dd><dt>{h("after")}</dt><dd>{change.after}</dd>
        </dl></li>)}
      </ul> : <div className="history-change-table"><table><thead><tr><th>{h("object")}</th><th>{h("property")}</th><th>{h("before")}</th><th>{h("after")}</th></tr></thead><tbody>
        {changes.map((change, index) => <tr key={index}><th scope="row">{change.object}</th><td>{change.field}</td><td>{change.before}</td><td>{change.after}</td></tr>)}
      </tbody></table></div>)}</>}
  </section>;
}

function historyLabel(label: string | null | undefined, parent: string | null | undefined, language: Language) {
  if (!label?.trim()) return historyText(parent === null ? "initial" : "edited", language);
  if (label === "Undo change" || label === "Redo change") return historyText(label === "Undo change" ? "undoSummary" : "redoSummary", language);
  const known: Record<string, string> = { "Place media": "Add media to timeline", "Import media": "Import media", "Create project": "Project created", "Edit project canvas and frame rate": "Change canvas settings", "Edit scene bookmark": "Edit bookmark", "Edit element properties": "Change clip properties" };
  if (known[label]) return t(known[label]);
  const translated = t(label);
  return hasMessage(label) ? translated : t("Project edited");
}
