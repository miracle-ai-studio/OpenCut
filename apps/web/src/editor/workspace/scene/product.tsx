import { Select } from "../../shared/controls";
import { SceneStackProvider } from "./scene-stack";
import { GainKeyframeLine } from "./gain-keyframes";
import { gainChannel } from "./gain-keyframes/model";
import { ProjectSettingsEditor } from "./property-edit/settings";
import { MaskAuthoringProvider, MaskPanel } from "./mask/workspace";
import { ExportProjectPackage, type PackageSource } from "./package";
import { CaptionWorkspace } from "./captions/workspace";
import { useCallback, useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { useEditingWorkspace } from "../context";
import { Button, Panel, Modal } from "../../shared/ui";
import {
  formatTime,
} from "../../model/time";
import type { Bookmark } from "./types";
import { BookmarkEditor } from "./bookmarks";
import { applyMediaPlan } from "./media-dialog";
import { Inspector } from "./inspector";
import { t, useLocale } from "../../shared/locale";
import { History } from "./history";
import "./product.css";
import { activeBookmarkNotes } from "./presentation";
import { AudioGainLine, AudioGainStatus } from "./audio-gain-line";
import { FrameCanvas } from "./frame-canvas";
import { PlaybackFooter } from "./playback-footer";
import { TimelinePanel, type TimelinePanelProps } from "./timeline-edit/panel";
import { TimelineEditReview } from "./timeline-edit/review";
import { ClipboardControls } from "./timeline-edit/clipboard-controls";
import { editorProjectId } from "../../../routes";
import { EditRecoveryPanel } from "./edit-recovery-panel";
import { MediaPlanDialog } from "./media-dialog";
import type { MediaFileTarget } from "./media";
import type { Element } from "./types";
import { GraphicAuthoringProvider, GraphicPanel } from "./graphic/workspace";
import { TextAuthoringProvider, TextPanel } from "./text/workspace";
import { canvasTextPlanner } from "./text/canvas-plan";
import { editRecoveryBlocked } from "./edit-recovery";
import { BookmarkPlus, BookmarkCheck, FolderOutput, Settings2 } from "lucide-react";
import { Workbench, TimelineHistoryActions } from "./workbench/layout";
import { WorkbenchResources } from "./workbench/resources";
import { ProjectSummary } from "./workbench/project-summary";

/** The workspace Session owns commands and project state; the workbench owns views. */
export function Product({ historyRequest = 0 }: { historyRequest?: number } = {}) {
  useLocale();
  const { controller, scene, busy, api } = useEditingWorkspace();
  useSyncExternalStore(controller.scene.recovery.subscribe, controller.scene.recovery.getSnapshot, controller.scene.recovery.getSnapshot);
  const [editing, setEditing] = useState<Bookmark | "new" | null>(null);
  const [timelineReview, setTimelineReview] = useState(false);
  const [mediaReview, setMediaReview] = useState(false);
  const [settingsVisible,setSettingsVisible] = useState(false);
  const [packageSource, setPackageSource] = useState<PackageSource | null>(null);
  const selectGain = useCallback((id: string) => { controller.scene.select(id); setEditing(null); }, [controller]);
  const renderAudioControls = useCallback((element: Element) => gainChannel(element) ? <GainKeyframeLine compact elementId={element.id} onSelect={selectGain} /> : <AudioGainLine elementId={element.id} onSelect={selectGain} />, [selectGain]);
  const [pending, setPending] = useState(false);
  const [playerTools,setPlayerTools]=useState<HTMLDivElement|null>(null);
  useEffect(() => {
    setEditing(null);
  }, [scene?.head.revision, scene?.historical?.revision, scene?.sequenceId]);
  useEffect(() => { setTimelineReview(false); }, [scene?.head.project.id, scene?.historical?.revision, scene?.sequenceId]);
  useEffect(() => { setMediaReview(false); }, [scene?.head.project.id, scene?.historical?.revision, scene?.sequenceId]);
  useEffect(() => {
    if (!scene?.sequenceId || editorProjectId(location.pathname) !== scene.head.project.id) return;
    const url = new URL(location.href);
    url.searchParams.set("scene", scene.sequenceId);
    history.replaceState(history.state, "", url.pathname + url.search + url.hash);
  }, [scene?.head.project.id, scene?.sequenceId]);
  const planCanvasText = useMemo(() => api ? canvasTextPlanner(api) : undefined, [api]);
  if (!scene) return null;
  const snapshot = scene.historical ?? scene.head,
    project = snapshot.project;
  const sequence =
    project.sequences.find((s) => s.id === scene.sequenceId) ??
    project.sequences.find((s) => s.id === project.default_sequence_id)!;
  const settings =
    project.settings.profile === "classic_global"
      ? project.settings.settings
      : project.settings.formats.find((f) => f.sequence_id === sequence.id)!;
  const selected = sequence.bookmarks.find((b) => b.id === scene.selectedId);
  const copyTargets = scene.timelineEdit.selection.length
    ? scene.timelineEdit.selection
    : sequence.tracks.flatMap((track) =>
        track.elements
          .filter((element) => element.id === scene.selectedId)
          .map((element) => ({ track_id: track.id, element_id: element.id })),
      );
  const canCopySelection =
    copyTargets.length > 0 &&
    copyTargets.every((target) =>
      sequence.tracks.some(
        (track) =>
          track.id === target.track_id &&
          track.elements.some((element) => element.id === target.element_id),
      ),
    );
  const copyUnavailableReason = copyTargets.some((target) =>
    sequence.captions.some(
      (caption) =>
        caption.id === target.element_id && caption.track_id === target.track_id,
    ),
  )
    ? "Caption markers support timing edits. Use editable subtitle text to copy or duplicate subtitles."
    : copyTargets.length > 0 && !canCopySelection
      ? "Select existing timeline elements to copy or duplicate."
      : null;
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
  const captionBlocked = controller.scene.recovery.blocked();
  const fontBlocked = editRecoveryBlocked(scene.recovery);
  const textAvailable = scene.frameCapability?.frame.available === true;
  const importFont = (request:Parameters<typeof controller.scene.importFont>[0]) => controller.scene.importFont(request);
  const fontImportBlockedReason = captionBlocked ? "Resolve the original caption edit before starting another operation." : fontBlocked ? "Resolve the original font import before starting another operation." : null;
  const readonly = !!scene.historical || busy || pending || fontBlocked || captionBlocked;
  const importTimelineFiles = async (files: File[], target: MediaFileTarget) => {
    const capturedApi = api;
    try {
      const plan = await controller.scene.media.importFiles(files, undefined, target);
      if (!capturedApi) return;
      await applyMediaPlan(controller, capturedApi, plan);
    }
    finally {
      const current = controller.getSnapshot();
      if (current.api === capturedApi && current.scene?.head.project.id === target.scope.projectId &&
        current.scene.sequenceId === target.scope.sequenceId && !current.scene.historical)
        setMediaReview(!!current.scene?.media.attempt);
    }
  };
  return (
    <SceneStackProvider>
    <MaskAuthoringProvider>
    <GraphicAuthoringProvider>
    <TextAuthoringProvider available={textAvailable} planCanvasText={planCanvasText} importFont={importFont} fontImportBlockedReason={fontImportBlockedReason}>
    <main className="workspace-layout editor-product" aria-label={t("OpenCut editor")}>
      <EditRecoveryPanel />
      {settingsVisible && <Modal title={t("Canvas and background")} onClose={()=>setSettingsVisible(false)}><ProjectSettingsEditor /></Modal>}
      {packageSource && <ExportProjectPackage source={packageSource} onClose={() => setPackageSource(null)} />}
      {scene.externalRevision && <p role="status" className="history-banner">
        {t("The project changed in another client. Finish or resolve the current edit to load the new revision.")}
      </p>}
      {scene.historical && (
        <div className="history-banner">
          {t("Viewing a saved revision · Read only")}{" "}
          <Button
            onClick={() => {
              controller.scene.returnToHead();
              setEditing(null);
            }}
          >
            {t("Return to current")}
          </Button>
        </div>
      )}
      <Workbench
        historyRequest={historyRequest}
        resources={<WorkbenchResources sequence={sequence} captions={<CaptionWorkspace recovery={controller.scene.recovery} mutateCaption={context => controller.scene.applyCaption(context)} />} />}
        player={<>
      <div className="editor-scene-bar workbench-player-heading toolbar">
        <strong>{t("Timeline")}</strong>
        <Button aria-label={t("Canvas and background")} title={t("Canvas and background")} icon={<Settings2 size={15} />} disabled={busy || !!scene.historical} onClick={()=>setSettingsVisible(true)}>{t("Canvas and background")}</Button>
        <Button aria-label={t("Export project package")} title={t("Export project package")} icon={<FolderOutput size={15} />} disabled={!api || busy} onClick={() => setPackageSource({project_id:project.id, revision:snapshot.revision, name:project.name})}>{t("Export project package")}</Button>
        <Select
          aria-label={t("Scene")}
          value={sequence.id}
          onChange={(e) => {
            controller.scene.chooseSequence(e.target.value);
            setEditing(null);
          }}
        >
          {project.sequences.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </Select>
        <span>
          {settings.canvas_size.width} × {settings.canvas_size.height} ·{" "}
          {settings.fps.numerator / settings.fps.denominator} fps
        </span>
        <span className="muted">
          OpenCut ·{" "}
          {t(
            project.settings.profile === "classic_global"
              ? "Global settings"
              : "Per-scene settings",
          )}
        </span>
      </div>
          <div className="workbench-player-surface">
          <PlaybackCanvas
            controlsTarget={playerTools}
            width={settings.canvas_size.width}
            height={settings.canvas_size.height}
          />
          </div>
          <PlaybackFooter toolsRef={setPlayerTools}/>
        </>}
        inspector={<>
            <TextPanel />
            <GraphicPanel />
            <MaskPanel />
            {editing ? (
              <BookmarkEditor
                key={editing === "new" ? `new:${sequence.id}` : editing.id}
                bookmark={editing === "new" ? null : editing}
                sequenceId={sequence.id}
                playhead={controller.scene.currentPlayhead()}
                disabled={readonly}
                save={(op) => controller.scene.bookmark(op)}
                close={() => setEditing(null)}
              />
            ) : selected ? (
              <>
                <h3>
                  {t("Bookmark")} · {formatTime(selected.time)}
                </h3>
                <p className="editor-note-text">
                  {selected.note ?? t("No canvas note")}
                </p>
                <div className="toolbar">
                  <Button
                    disabled={readonly}
                    onClick={() => setEditing(selected)}
                  >
                    {t("Edit bookmark")}
                  </Button>
                  <Button
                    disabled={readonly}
                    onClick={() =>
                      void run(() =>
                        controller.scene.bookmark({
                          kind: "move",
                          sequence_id: sequence.id,
                          bookmark_id: selected.id,
                          time: controller.scene.currentPlayhead(),
                        }),
                      )
                    }
                  >
                    {t("Move to playhead")}
                  </Button>
                  <Button
                    disabled={readonly}
                    onClick={() =>
                      void run(() =>
                        controller.scene.bookmark({
                          kind: "delete",
                          sequence_id: sequence.id,
                          bookmark_id: selected.id,
                        }),
                      )
                    }
                  >
                    {t("Delete bookmark")}
                  </Button>
                </div>
              </>
            ) : !scene.selectedId ? (
              <ProjectSummary project={project} sequence={sequence} onSettings={() => setSettingsVisible(true)} />
            ) : null}
            {scene.selectedId && <Inspector
              project={project}
              sequence={sequence}
              selectedId={scene.selectedId}
            />}
        </>}
        project={<ProjectSummary project={project} sequence={sequence} onSettings={() => setSettingsVisible(true)} />}
        history={<History />}
        timeline={
      <Panel
        title={t("Timeline")}
        className="editor-timeline"
        actions={
          <>
            <Button
              aria-label={t("Add bookmark")}
              title={t("Add bookmark")}
              icon={<BookmarkPlus size={15} />}
              disabled={readonly}
              onClick={() => {
                controller.scene.select(null);
                setEditing("new");
              }}
            >
              {t("Add bookmark")}
            </Button>
            <Button
              className="editor-quick-marker"
              aria-label={t("Toggle bookmark at playhead")}
              title={`${t("Toggle bookmark at playhead")} (M)`}
              icon={<BookmarkCheck size={15} />}
              disabled={readonly}
              onClick={() =>
                void run(() =>
                  controller.scene.bookmark({
                    kind: "toggle",
                    sequence_id: sequence.id,
                    time: controller.scene.currentPlayhead(),
                    bookmark_id: crypto.randomUUID(),
                  }),
                )
              }
            >
              {t("Toggle bookmark at playhead")}
            </Button>
          </>
        }
      >
        <div className="scroll padded">

          {scene.timelineEdit.recoveryError && <div role="alert">
            <p>{t(scene.timelineEdit.recoveryError)}</p>
            <Button disabled={busy || pending} onClick={() => void run(() => controller.scene.recoverTimeline())}>{t("Retry timeline recovery")}</Button>
            <Button disabled={busy || pending} onClick={() => void run(() => controller.scene.recoverTimeline(true))}>{t("Repair recovery pointer")}</Button>
          </div>}
          {scene.timelineEdit.attempt && (!scene.timelineEdit.attempt.automatic || scene.timelineEdit.attempt.error) && <div className="editor-timeline-status" role="status">
            {t(scene.timelineEdit.attempt.phase === "saved" ? "Timeline changes saved."
              : scene.timelineEdit.attempt.phase === "unconfirmed" ? "Timeline save is unconfirmed. Retry the original operation to recover its receipt."
                : "Timeline changes await review.")}
            <Button onClick={() => setTimelineReview(true)}>{t("Review timeline changes")}</Button>
          </div>}
          <PlaybackTimeline
            toolbar={<><TimelineHistoryActions />          <ClipboardControls
            controller={controller.scene.clipboard}
            state={scene.clipboard}
            scope={{ projectId: project.id, revision: snapshot.revision, sequenceId: sequence.id }}
            canCopy={canCopySelection}
            copyUnavailableReason={copyUnavailableReason}
            canPaste={!scene.historical && !scene.timelineEdit.attempt && !scene.timelineEdit.recoveryError && !scene.gainDraft &&
              !scene.media.attempt && !scene.historyPlan && !scene.audioGain && !scene.sourceAudio &&
              (!scene.clipboard.attempt || (scene.clipboard.attempt.request.project_id === project.id &&
                scene.clipboard.attempt.request.sequence_id === sequence.id))}
            disabled={busy || pending || !!scene.gainDraft}
            onFiles={async (files, captured) => {
              const current = controller.getSnapshot().scene;
              if (!current || current.historical || current.head.project.id !== captured.projectId ||
                current.head.revision !== captured.revision || current.sequenceId !== captured.sequenceId)
                throw new Error("The file paste target changed. Paste into the current scene again.");
              const scene = current.head.project.sequences.find(s => s.id === captured.sequenceId)!;
              const track = scene.tracks.find(t => t.id === current.selectedId || t.elements.some(e => e.id === current.selectedId)) ??
                scene.tracks.find(t => t.id === current.timelineEdit.selection[0]?.track_id) ??
                scene.tracks.find(t => t.id === scene.main_track_id)!;
              await importTimelineFiles(files, { scope: captured, trackId: track.id, at: current.playhead });
            }}
          /></>}
            key={`${project.id}/${sequence.id}/${scene.historical?.revision ?? "head"}`}
            controller={controller.scene.timelineEdit}
            sequence={sequence}
            sourceArt={api ? { api, assets: project.assets, gainDraft: scene.gainDraft } : undefined}
            scope={{ projectId: project.id, revision: snapshot.revision, sequenceId: sequence.id }}
            selection={scene.timelineEdit.selection}
            selectedId={scene.selectedId}
            playhead={scene.playhead}
            frameStep={{ticks:String(settings.fps.denominator),timebase:settings.fps.numerator}}
            onScrubStart={()=>{if(["playing","loading","stalled"].includes(controller.getSnapshot().scene?.audioPreview.phase??""))void controller.scene.audioPreview.pause().catch(error=>controller.report(error));}}
            attempt={scene.timelineEdit.attempt}
            disabled={readonly || !!scene.timelineEdit.attempt || !!scene.timelineEdit.recoveryError || !!scene.clipboard.attempt ||
              !!scene.media.attempt || !!scene.historyPlan || !!scene.audioGain || !!scene.sourceAudio || !!scene.recovery?.record || !!scene.recovery?.error}
            gainDraftActive={!!scene.gainDraft}
            onSelect={(target, mode) => { controller.scene.timelineEdit.select(target, mode); setEditing(null); }}
            onSelectMany={(targets, primary) => controller.scene.timelineEdit.selectMany(targets, primary)}
            onSelectOther={id => { controller.scene.select(id); setEditing(null); }}
            onSeek={time => controller.scene.seek(time)}
            onReview={() => setTimelineReview(true)}
            onFiles={importTimelineFiles}
            renderAudioControls={renderAudioControls}
          />
        </div>
      </Panel>
        }
      />
      {mediaReview && scene.media.attempt && <MediaPlanDialog onClose={() => setMediaReview(false)} />}
      {timelineReview && scene.timelineEdit.attempt && <TimelineEditReview
        attempt={scene.timelineEdit.attempt}
        controller={controller.scene.timelineEdit}
        sequence={sequence}
        unavailableReason={scene.historical ? "Return to the current revision before editing."
          : scene.timelineEdit.attempt.request.sequence_id !== sequence.id ? "Return to the original scene before retrying this timeline operation."
            : ["planned", "failed"].includes(scene.timelineEdit.attempt.phase) &&
              scene.timelineEdit.attempt.request.expected_revision !== scene.head.revision
              ? "The project changed. Discard this plan and review a new timeline operation." : null}
        disabled={readonly || scene.timelineEdit.attempt.request.project_id !== project.id ||
          scene.timelineEdit.attempt.request.sequence_id !== sequence.id ||
          (["planned", "failed"].includes(scene.timelineEdit.attempt.phase) &&
            scene.timelineEdit.attempt.request.expected_revision !== scene.head.revision)}
        onClose={() => setTimelineReview(false)}
      />}
      <AudioGainStatus />
    </main>
    </TextAuthoringProvider>
    </GraphicAuthoringProvider>
    </MaskAuthoringProvider>
    </SceneStackProvider>
  );
}

function PlaybackTimeline(props:TimelinePanelProps) {
  const {controller}=useEditingWorkspace();
  const presentation=useMemo(()=>({time:()=>controller.scene.currentPlayhead(),subscribe:controller.scene.audioPreview.subscribePresentation}),[controller]);
  return <TimelinePanel {...props} presentation={presentation}/>;
}
function PlaybackCanvas(props:{width:number;height:number;controlsTarget?:HTMLElement|null}) {
  const {controller,scene}=useEditingWorkspace();
  const notes=useMemo(()=>{
    let value:Bookmark[]=[];
    return ()=>{
      const sequence=scene&&(scene.historical??scene.head).project.sequences.find(q=>q.id===scene.sequenceId);
      const next=sequence?activeBookmarkNotes(sequence.bookmarks,controller.scene.currentPlayhead()):[];
      if(value.length!==next.length||value.some((note,i)=>note!==next[i]))value=next;
      return value;
    };
  },[controller,scene?.head,scene?.historical,scene?.sequenceId]);
  const active=useSyncExternalStore(controller.scene.audioPreview.subscribePresentation,notes);
  return <FrameCanvas {...props} notes={active}/>;
}
