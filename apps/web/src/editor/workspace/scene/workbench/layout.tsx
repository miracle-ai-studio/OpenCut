import {
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { Group, Panel, Separator } from "react-resizable-panels";
import {
  History as HistoryIcon,
  Redo2,
  SlidersHorizontal,
  Undo2,
} from "lucide-react";
import { useEditingWorkspace as useWorkspace } from "../../context";
import { Button } from "../../../shared/ui";
import { t, useLocale } from "../../../shared/locale";
import "./workbench.css";
import { WorkbenchView } from "./view";
import { TransitionProvider, useTransitions } from "./transition-workspace";
import { ResourceDragProvider } from "./resource-drag";
import { TransitionInspector } from "./transitions";
import { PlayerPreviewProvider, WorkbenchPlayer } from "./player-preview";
import { usePlayerContext } from "./player-context";

/** View composition only. Project state and every command stay in Session. */
export function Workbench(props: {
  resources: ReactNode; player: ReactNode; inspector: ReactNode;
  project: ReactNode; history: ReactNode; timeline: ReactNode;
  historyRequest?: number;
}) {
  return <PlayerPreviewProvider><TransitionProvider><ResourceDragProvider><WorkbenchLayout {...props} /></ResourceDragProvider></TransitionProvider></PlayerPreviewProvider>;
}
function WorkbenchLayout({
  resources,
  player,
  inspector,
  project,
  history,
  timeline,
  historyRequest = 0,
}: {
  resources: ReactNode;
  player: ReactNode;
  inspector: ReactNode;
  project: ReactNode;
  history: ReactNode;
  timeline: ReactNode;
  historyRequest?: number;
}) {
  useLocale();
  const { scene } = useWorkspace();
  const transitions = useTransitions();
  const playerContext = usePlayerContext();
  const [tab, setTab] = useState<"properties" | "project" | "history">(
    "properties",
  );
  useEffect(() => {
    if (!historyRequest) return;
    setTab("history");
    document.getElementById("inspector-history-tab")?.focus();
  }, [historyRequest]);
  useEffect(() => {
    if (scene?.selectedId) setTab("properties");
  }, [scene?.selectedId]);
  useEffect(() => {
    if (scene?.timelineEdit.selection.length) setTab("properties");
  }, [scene?.timelineEdit.selection]);
  useEffect(() => {
    if (scene?.historyPlan?.review) setTab("history");
  }, [scene?.historyPlan]);
  return (
    <WorkbenchView.Provider value={{ showHistory: () => setTab("history"), showProperties: () => setTab("properties") }}>
      <Group
        orientation="vertical"
        className="workbench-split"
        id="editing-workbench"
      >
        <Panel id="editing-stage" defaultSize="56%" minSize="280px">
          <Group
            orientation="horizontal"
            className="workbench-stage"
            id="editing-panels"
          >
            <Panel id="editing-resources" defaultSize="32%" minSize="280px">
              <section
                className="workbench-resources workbench-pane"
                aria-label={t("Editing resources")}
              >
                {resources}
              </section>
            </Panel>
            <Separator
              className="workbench-separator"
              aria-label={t("Resize resource panel")}
            />
            <Panel id="editing-player" defaultSize="44%" minSize="280px">
              <section
                className="workbench-player workbench-pane"
                aria-label={t("Player")}
              >
                <WorkbenchPlayer>{player}</WorkbenchPlayer>
              </section>
            </Panel>
            <Separator
              className="workbench-separator"
              aria-label={t("Resize inspector panel")}
            />
            <Panel id="editing-inspector" defaultSize="24%" minSize="250px">
              <aside
                className="workbench-inspector workbench-pane"
                aria-label={t("Inspector")}
              >
                <div
                  className="workbench-panel-tabs"
                  role="tablist"
                  aria-label={t("Inspector view")}
                  onKeyDown={(event) => {
                    const views = ["properties", "project", "history"] as const;
                    const index = views.indexOf(tab);
                    const next =
                      event.key === "ArrowRight"
                        ? (index + 1) % views.length
                        : event.key === "ArrowLeft"
                          ? (index + views.length - 1) % views.length
                          : event.key === "Home"
                            ? 0
                            : event.key === "End"
                              ? views.length - 1
                              : null;
                    if (next !== null) {
                      event.preventDefault();
                      setTab(views[next]);
                      document
                        .getElementById(`inspector-${views[next]}-tab`)
                        ?.focus();
                    }
                  }}
                >
                  <button
                    role="tab"
                    id="inspector-properties-tab"
                    tabIndex={tab === "properties" ? 0 : -1}
                    aria-controls="inspector-properties"
                    aria-selected={tab === "properties"}
                    onClick={() => setTab("properties")}
                  >
                    <SlidersHorizontal size={14} />
                    {t("Properties")}
                  </button>
                  <button
                    role="tab"
                    id="inspector-project-tab"
                    tabIndex={tab === "project" ? 0 : -1}
                    aria-controls="inspector-project"
                    aria-selected={tab === "project"}
                    onClick={() => setTab("project")}
                  >
                    {t("Project")}
                  </button>
                  <button
                    role="tab"
                    id="inspector-history-tab"
                    tabIndex={tab === "history" ? 0 : -1}
                    aria-controls="inspector-history"
                    aria-selected={tab === "history"}
                    onClick={() => setTab("history")}
                  >
                    <HistoryIcon size={14} />
                    {t("History")}
                  </button>
                </div>
                <div
                  role="tabpanel"
                  id="inspector-properties"
                  aria-labelledby="inspector-properties-tab"
                  hidden={tab !== "properties"}
                  className="workbench-property-body"
                >
                  {transitions?.pair ? <TransitionInspector /> : inspector}
                </div>
                <div
                  role="tabpanel"
                  id="inspector-project"
                  aria-labelledby="inspector-project-tab"
                  hidden={tab !== "project"}
                  className="workbench-property-body"
                >
                  {project}
                </div>
                <div
                  role="tabpanel"
                  id="inspector-history"
                  aria-labelledby="inspector-history-tab"
                  hidden={tab !== "history"}
                  className="workbench-history-body"
                >
                  {history}
                </div>
              </aside>
            </Panel>
          </Group>
        </Panel>
        <Separator
          className="workbench-separator workbench-separator-horizontal"
          aria-label={t("Resize timeline panel")}
        />
        <Panel id="editing-timeline" defaultSize="44%" minSize="200px">
          <div className="workbench-timeline workbench-pane" onPointerDownCapture={() => { if (playerContext?.preview) playerContext.close(false); }}>{timeline}</div>
        </Panel>
      </Group>
    </WorkbenchView.Provider>
  );
}

export function TimelineHistoryActions() {
  const { controller, scene, busy } = useWorkspace();
  const { showHistory } = useContext(WorkbenchView);
  const run = (direction: "undo" | "redo") => {
    if (!scene) return;
    const work =
      direction === "undo" && scene.navigation.undo
        ? controller.scene.planUndo(scene.navigation.undo.transaction_id, true)
        : direction === "redo" && scene.navigation.redo
          ? controller.scene.planRedo(scene.navigation.redo.undo_transaction_id, true)
          : null;
    if (!work) return;
    void work.then(() => { if (controller.getSnapshot().scene?.historyPlan?.plan.conflicts.length) showHistory(); }).catch((error) => controller.report(error));
  };
  const blocked = busy || !!scene?.historical || !!scene?.historyPlan || !!scene?.propertyDrafts?.length;
  return (
    <div className="workbench-history-actions">
      <Button
        aria-label={t("Undo last edit")}
        title={t("Undo last edit")}
        disabled={blocked || !scene?.navigation.undo}
        onClick={() => run("undo")}
        icon={<Undo2 size={16} />}
      />
      <Button
        aria-label={t("Redo last edit")}
        title={t("Redo last edit")}
        disabled={blocked || !scene?.navigation.redo}
        onClick={() => run("redo")}
        icon={<Redo2 size={16} />}
      />
      <Button
        aria-label={t("Show edit history")}
        title={t("Show edit history")}
        onClick={showHistory}
        icon={<HistoryIcon size={15} />}
      />
    </div>
  );
}
