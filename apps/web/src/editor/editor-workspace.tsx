import { editorProjectId, editorProjectPath } from "../routes";
import { EditorGuide } from "./shared/editor-guide";
import {
  readWorkspaceLayouts,
  saveWorkspaceLayout,
} from "./shared/view-preferences";
import { t, useLocale } from "./shared/locale";
import { useEffect, useRef, useState } from "react";
import {
  Group,
  Panel as ResizablePanel,
  Separator,
} from "react-resizable-panels";
import {
  ArrowLeft,
  ChevronDown,
  Download,
  FolderOpen,
  History,
  ListTodo,
  Type,
  Music2,
  Smile,
  Sparkles,
  ArrowRightLeft,
  Captions,
  SlidersHorizontal,
  Settings2,
  Keyboard,
  CircleHelp,
  X,
} from "lucide-react";
import { WorkspaceProvider, useWorkspace } from "./workspace/context";
import { Preferences } from "./shared/preferences";
import { SceneSwitcher } from "./projects/scenes";
import { RenameProject } from "./projects/rename";
import { ProjectBrowser } from "./projects/browser";
import { ShortcutsDialog } from "./shared/shortcuts";
import { EffectsPanel } from "./effects/panel";
import { CaptionsPanel } from "./assets/captions";
import { SettingsPanel } from "./inspector/sequence-settings";
import { AssetsPanel } from "./assets/panel";
import { TitlesPanel } from "./assets/titles";
import { PreviewPanel } from "./preview/panel";
import { TimelinePanel } from "./timeline/panel";
import { InspectorPanel } from "./inspector/panel";
import { TasksPanel } from "./tasks/panel";
import { ExportDialog } from "./tasks/export-dialog";
import { HistoryPanel } from "./history/panel";
import { Button } from "./shared/ui";
import symbol from "./shared/brand/symbol.svg";
import "./editor.css";
const tabs = [
  { id: "media", name: "Media", icon: FolderOpen },
  { id: "sounds", name: "Sounds", icon: Music2 },
  { id: "titles", name: "Text", icon: Type },
  { id: "stickers", name: "Stickers", icon: Smile },
  { id: "effects", name: "Effects", icon: Sparkles },
  { id: "transitions", name: "Transitions", icon: ArrowRightLeft },
  { id: "captions", name: "Captions", icon: Captions },
  { id: "adjustment", name: "Adjustments", icon: SlidersHorizontal },
  { id: "settings", name: "Settings", icon: Settings2 },
  { id: "tasks", name: "Tasks", icon: ListTodo },
  { id: "history", name: "History", icon: History },
] as const;
export function EditorWorkspace() {
  useLocale();
  return (
    <WorkspaceProvider>
      <Workspace />
    </WorkspaceProvider>
  );
}
function Workspace() {
  useLocale();
  const {
    controller,
    snapshot,
    projects,
    historical,
    busy,
    message,
    error,
    jobs,
  } = useWorkspace();
  const [layouts] = useState(readWorkspaceLayouts);
  const [guide, setGuide] = useState(false);
  const [shortcuts, setShortcuts] = useState(false);
  const [exporting, setExporting] = useState(false),
    [tab, setTab] = useState("media"),
    [browsing, setBrowsing] = useState(true);
  const initialPath = useRef(location.pathname);
  useEffect(() => {
    if (!projects.length || !initialPath.current.startsWith("/editor/")) return;
    const id = editorProjectId(initialPath.current);
    const project = projects.find((project) => project.id === id);
    initialPath.current = "";
    if (project) {
      setBrowsing(false);
      void controller.open(
        project,
        new URLSearchParams(location.search).get("scene"),
      );
    }
  }, [projects, controller]);
  useEffect(() => {
    const pop = () => {
      const id = editorProjectId(location.pathname);
      const project = controller
        .getSnapshot()
        .projects.find((project) => project.id === id);
      setBrowsing(!project);
      if (project)
        void controller.open(
          project,
          new URLSearchParams(location.search).get("scene"),
        );
    };
    window.addEventListener("popstate", pop);
    return () => window.removeEventListener("popstate", pop);
  }, [controller]);
  const showProjects = () => {
    void controller
      .refreshProjects()
      .catch((error) => controller.report(error));
    history.pushState(null, "", "/projects");
    setBrowsing(true);
  };
  const showEditor = () => {
    const id = controller.getSnapshot().snapshot?.project.id;
    if (id) {
      history.pushState(null, "", editorProjectPath(id));
      setBrowsing(false);
    }
  };
  const active = !!snapshot && !browsing;
  const activeJobs = jobs.filter(
    (job) => job.state === "running" || job.state === "queued",
  ).length;
  return (
    <div className="editor-app">
      <header className="editor-header">
        <div className="header-start">
          <Button
            className="brand"
            aria-label={t("Projects")}
            onClick={showProjects}
            icon={<img className="brand-symbol" src={symbol} alt="OpenCut" />}
          />
          {active ? (
            <>
              <Button
                aria-label={t("Back to projects")}
                onClick={showProjects}
                icon={<ArrowLeft size={14} />}
              />
              <RenameProject
                project={{
                  id: snapshot.project.id,
                  name: snapshot.project.name,
                  revision: snapshot.revision,
                  format: snapshot.format,
                }}
              />
              <ChevronDown size={13} />
              <span className="save-status">
                {historical
                  ? t("Saved revision")
                  : busy
                    ? t("Saving…")
                    : t("Saved")}
              </span>
            </>
          ) : (
            <strong className="wordmark">{t("OpenCut")}</strong>
          )}
        </div>
        <div className="toolbar">
          <Button
            aria-label={t("Shortcuts")}
            title={t("Shortcuts")}
            icon={<Keyboard size={17} />}
            onClick={() => setShortcuts(true)}
          />
          {active && (
            <Button
              aria-label={t("Editor guide")}
              title={t("Editor guide")}
              icon={<CircleHelp size={17} />}
              onClick={() => setGuide(true)}
            />
          )}
          <Preferences />
          {active && (
            <Button
              onClick={() => setTab("tasks")}
              icon={<ListTodo size={15} />}
            >
              {t("Tasks")}
              {activeJobs ? ` · ${activeJobs}` : ""}
            </Button>
          )}
          {active && (
            <Button
              className="primary"
              icon={<Download size={15} />}
              onClick={() => setExporting(true)}
            >
              {t("Export")}
            </Button>
          )}
        </div>
      </header>
      {active ? (
        <>
          <EditorGuide forced={guide} onClose={() => setGuide(false)} />
          <div className="workspace-layout">
            <Group
              orientation="vertical"
              id="workspace"
              defaultLayout={layouts.workspace}
              onLayoutChanged={(layout, meta) => {
                if (meta.isUserInteraction)
                  saveWorkspaceLayout("workspace", layout);
              }}
              className="outer-panels"
            >
              <ResizablePanel id="stage" defaultSize="65%" minSize="35%">
                <Group
                  orientation="horizontal"
                  id="stage-panels"
                  defaultLayout={layouts.stage}
                  onLayoutChanged={(layout, meta) => {
                    if (meta.isUserInteraction)
                      saveWorkspaceLayout("stage", layout);
                  }}
                  className="stage-panels"
                >
                  <ResizablePanel
                    id="library"
                    defaultSize="25%"
                    minSize="240px"
                    maxSize="40%"
                  >
                    <div className="library">
                      <nav aria-label={t("Editor tools")} className="tool-rail">
                        {tabs.map((item) => (
                          <Button
                            key={item.id}
                            aria-label={t(item.name)}
                            title={t(item.name)}
                            aria-pressed={tab === item.id}
                            onClick={() => setTab(item.id)}
                            icon={<item.icon size={18} />}
                          />
                        ))}
                      </nav>
                      <div className="library-panel">
                        {tabs.map((item) => (
                          <div
                            key={item.id}
                            className="library-tab"
                            hidden={tab !== item.id}
                          >
                            {item.id === "media" ? (
                              <AssetsPanel active={tab === "media"} />
                            ) : item.id === "titles" ? (
                              <TitlesPanel />
                            ) : item.id === "sounds" ? (
                              <AssetsPanel
                                audioOnly
                                active={tab === "sounds"}
                              />
                            ) : item.id === "stickers" ? (
                              <TitlesPanel mode="stickers" />
                            ) : item.id === "effects" ||
                              item.id === "transitions" ||
                              item.id === "adjustment" ? (
                              <EffectsPanel mode={item.id} />
                            ) : item.id === "captions" ? (
                              <CaptionsPanel />
                            ) : item.id === "settings" ? (
                              <SettingsPanel />
                            ) : item.id === "tasks" ? (
                              <TasksPanel />
                            ) : (
                              <HistoryPanel />
                            )}
                          </div>
                        ))}
                      </div>
                    </div>
                  </ResizablePanel>
                  <Separator className="resize-handle" />
                  <ResizablePanel id="preview" defaultSize="50%" minSize="30%">
                    <PreviewPanel />
                  </ResizablePanel>
                  <Separator className="resize-handle" />
                  <ResizablePanel
                    id="properties"
                    defaultSize="25%"
                    minSize="240px"
                    maxSize="40%"
                  >
                    <InspectorPanel />
                  </ResizablePanel>
                </Group>
              </ResizablePanel>
              <Separator className="resize-handle horizontal" />
              <ResizablePanel id="timeline" defaultSize="35%" minSize="180px">
                <TimelinePanel />
              </ResizablePanel>
            </Group>
          </div>
          <footer className="editor-footer">
            <SceneSwitcher />
            <span>
              {historical
                ? t("Viewing history · Return to current in History")
                : t("All changes are saved on this computer")}
            </span>
            <span>{t("Space · Play / pause　S · Split")}</span>
          </footer>
        </>
      ) : (
        <ProjectBrowser
          onConnect={() => void controller.start()}
          onOpen={showEditor}
        />
      )}
      {(error || message) && (
        <div
          className={`notice ${error ? "error" : ""}`}
          role={error ? "alert" : "status"}
        >
          <span>{t(error ?? message ?? "")}</span>
          <Button
            aria-label={t("Dismiss notification")}
            onClick={() => controller.clearMessage()}
            icon={<X size={14} />}
          />
        </div>
      )}
      {shortcuts && <ShortcutsDialog onClose={() => setShortcuts(false)} />}
      {exporting && <ExportDialog onClose={() => setExporting(false)} />}
    </div>
  );
}
