import { ErrorNotice, localSessionExpired } from "./shared/error-notice";
import { HistoricalProject } from "./historical-project";
import { PreservedProject } from "./projects/preserved";
import { LocalSessionButton } from "./workspace/local-session";
import { editorProjectId, editorProjectPath } from "../routes";


import { t, useLocale } from "./shared/locale";
import { useEffect, useRef, useState } from "react";

import { ArrowLeft, ChevronDown, Download, History as HistoryIcon, ListTodo, MoreHorizontal, X } from "lucide-react";
import { WorkspaceProvider, useEditingWorkspace, useWorkspaceController } from "./workspace/context";
import { Preferences } from "./shared/preferences";

import { RenameProject } from "./projects/rename";
import { ProjectBrowser } from "./projects/browser";
import { EditorShortcutsProvider, ShortcutsButton, WorkspaceShortcutActions } from "./shortcuts";








import { TasksPanel } from "./tasks/panel";
import { ExportRecords } from "./workspace/scene/export-records";
import { isUserExport } from "./workspace/scene/export-record";


import { Button, Modal } from "./shared/ui";
import { ExportDialog } from "./workspace/scene/export-dialog";
import symbol from "./shared/brand/symbol.svg";
import "./editor.css";
import { Product } from "./workspace/scene/product";
export function EditorWorkspace() {
  useLocale();
  return (
    <WorkspaceProvider>
      <ShortcutWorkspace />
    </WorkspaceProvider>
  );
}
function ShortcutWorkspace() {
  const controller = useWorkspaceController();
  return <EditorShortcutsProvider onError={error => controller.report(error)}><Workspace /></EditorShortcutsProvider>;
}
function Workspace() {
  useLocale();
  const {
    controller,
    connection,
    snapshot,
    scene,
    preserved,
    projects,
    historical,
    busy,
    message,
    error,
    jobs,
  } = useEditingWorkspace();
  const [exporting, setExporting] = useState(false), [exportRecords, setExportRecords] = useState(false), [historyRequest, setHistoryRequest] = useState(0);
  const [sceneTasks, setTasks] = useState(false);
  const [browsing, setBrowsing] = useState(() => editorProjectId(location.pathname) === null);
  const initialPath = useRef(location.pathname);
  useEffect(() => {
    if (connection !== "connected") {
      // A fresh local grant creates a new connection. Reopen the route's saved
      // project after authentication; never replay the action that was denied.
      initialPath.current = location.pathname;
      return;
    }
    if (!initialPath.current.startsWith("/editor/")) return;
    const id = editorProjectId(initialPath.current);
    const project = projects.find((project) => project.id === id);
    initialPath.current = "";
    setBrowsing(!project);
    if (project) {
      void controller.open(
        project,
        new URLSearchParams(location.search).get("scene"),
      );
    }
  }, [connection, projects, controller]);
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
    const state = controller.getSnapshot();
    const id = state.snapshot?.project.id ?? state.scene?.head.project.id ?? state.preserved?.project_id;
    if (id) {
      history.pushState(null, "", editorProjectPath(id));
      setBrowsing(false);
    }
  };
  const activeHistorical = !!snapshot && !browsing;
  const activeEditor = !!scene && !browsing;
  const activePreserved = !!preserved && !browsing;
  useEffect(() => {
    setExporting(false); setExportRecords(false); setHistoryRequest(0);
    setTasks(false);
  }, [activeEditor, scene?.head.project.id]);
  const activeJobs = jobs.filter(
    (job) => job.purpose !== "playback_optimization" && (job.state === "running" || job.state === "queued"),
  ).length;
  const activeExports = jobs.filter(job => isUserExport(job) && ["queued","running"].includes(job.state)).length;
  return (
    <div className={`editor-app ${activeEditor ? "editor-workbench-app" : ""}`}>
      <WorkspaceShortcutActions active={activeEditor} />
      <header className="editor-header">
        <div className="header-start">
          <Button
            className="brand"
            aria-label={t("Projects")}
            onClick={showProjects}
            icon={<img className="brand-symbol" src={symbol} alt="OpenCut" />}
          />
          {activeHistorical || activeEditor || activePreserved ? (
            <>
              <Button
                aria-label={t("Back to projects")}
                onClick={showProjects}
                icon={<ArrowLeft size={14} />}
              />
              {snapshot ? (
                <RenameProject
                  project={{
                    id: snapshot.project.id,
                    name: snapshot.project.name,
                    revision: snapshot.revision,
                    format: snapshot.format,
                  }}
                />
              ) : activeEditor && scene ? (
                <RenameProject project={{ id: scene.head.project.id, name: scene.head.project.name,
                  revision: scene.head.revision, format: "editor" }} />
              ) : (
                <strong>{activePreserved ? `${preserved?.name} · ${t("Read-only")}` : scene?.head.project.name}</strong>
              )}
              <ChevronDown size={13} />
              <span className="save-status">
                {activePreserved ? t("Read-only") : historical || scene?.historical
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
        <div className="toolbar editor-header-actions">
          <details className="workbench-header-menu"><summary aria-label={t("Workspace menu")} title={t("Workspace menu")}><MoreHorizontal size={19} /></summary><div className="workbench-popover">
            <LocalSessionButton />
            
            <ShortcutsButton />
            <Preferences />
            {activeEditor && <Button onClick={event => {setTasks(true);const menu=event.currentTarget.closest("details");if(menu)menu.open=false;}} icon={<ListTodo size={15}/>}>{t("Tasks")}{activeJobs ? ` · ${activeJobs}` : ""}</Button>}
          </div></details>
          {activeHistorical && !activeEditor && (
            <Button
              onClick={() => setTasks(true)}
              icon={<ListTodo size={15} />}
            >
              {t("Tasks")}
              {activeJobs ? ` · ${activeJobs}` : ""}
            </Button>
          )}
          {activeEditor && <>
            <Button aria-controls="inspector-history" onClick={() => setHistoryRequest(n=>n+1)} icon={<HistoryIcon size={15}/>}>{t("History")}</Button>
            <Button onClick={() => setExportRecords(true)} icon={<ListTodo size={15}/>}>{t("Export records")}{activeExports ? ` · ${activeExports}` : ""}</Button>
            <Button className="primary workbench-export-button" disabled={busy} onClick={() => setExporting(true)} icon={<Download size={15}/>}>{t("Export")}</Button>
          </>}

        </div>
      </header>
      {activePreserved ? (
        <PreservedProject />
      ) : activeEditor ? (
        <Product historyRequest={historyRequest}/>
      ) : activeHistorical ? (
        <HistoricalProject />
      ) : !browsing ? (
        <main>
          <p role="status">{t(error ? "Could not open this project" : "Opening your workspace")}</p>
          {error && <Button disabled={busy} onClick={() => {
            const project = projects.find(item => item.id === editorProjectId(location.pathname));
            if (project) void controller.open(project, new URLSearchParams(location.search).get("scene"));
          }}>{t("Retry")}</Button>}
        </main>
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
          {error ? <ErrorNotice error={error} localSession={controller.canStartLocalSession} /> : <span>{t(message ?? "")}</span>}
          {error && localSessionExpired(error) && controller.canStartLocalSession && <Button disabled={busy || connection === "connecting"} onClick={() => void controller.start()}>{t("Reconnect")}</Button>}
          <Button
            aria-label={t("Dismiss notification")}
            onClick={() => controller.clearMessage()}
            icon={<X size={14} />}
          />
        </div>
      )}
      {activeEditor && exporting && <ExportDialog onClose={()=>setExporting(false)} showRecords={()=>setExportRecords(true)}/>}
      {activeEditor && exportRecords && <Modal title={t("Export records")} className="export-records-modal" onClose={()=>setExportRecords(false)}><ExportRecords onExport={()=>{setExportRecords(false);setExporting(true);}}/></Modal>}
      {(activeEditor || activeHistorical) && sceneTasks && (
        <Modal title={t("Tasks")} onClose={() => setTasks(false)}>
          <TasksPanel />
        </Modal>
      )}

    </div>
  );
}
