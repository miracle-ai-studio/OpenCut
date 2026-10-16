import { V4SceneStackProvider, V4SceneEffectsPanel } from "./scene-stack";
import { V4GainKeyframeLine } from "./gain-keyframes";
import { gainChannel } from "./gain-keyframes/model";
import { V4ResourceCatalogPanel } from "./catalog_resources_v1-panel";
import { V4ProjectSettingsEditor } from "./property-edit/settings";
import { V4MaskAuthoringProvider, V4MaskPanel } from "./mask/workspace";
import { ExportProjectPackage, type PackageSource } from "./package";
import { V4CaptionWorkspace } from "./captions/workspace";
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { useWorkspace } from "../context";
import { Button, Field, Panel, Modal } from "../../shared/ui";
import {
  formatTime,
  secondsForDisplay,
  timeFromDecimalSeconds,
} from "../../model/time";
import type { V4Bookmark } from "./types";
import { BookmarkEditor } from "./bookmarks";
import { V4Inspector } from "./inspector";
import { V4MediaPanel } from "./media-panel";
import { t, useLocale } from "../../shared/locale";
import { V4History } from "./history";
import "./product.css";
import { audioWavAvailability } from "./audio-wav";
import { activeBookmarkNotes } from "./presentation";
import { V4AudioGainLine, V4AudioGainStatus } from "./audio-gain-line";
import { V4FrameCanvas } from "./frame-canvas";
import { V4AudioPreviewControls } from "./audio-preview-controls";
import { V4TimelinePanel } from "./timeline-edit/panel";
import { TimelineEditReview } from "./timeline-edit/review";
import { V4ClipboardControls } from "./timeline-edit/clipboard-controls";
import { editorProjectId } from "../../../routes";
import { EditRecoveryPanel } from "./edit-recovery-panel";
import { MediaPlanDialog } from "./media-dialog";
import type { MediaFileTarget } from "./media";
import type { V4Element } from "./types";
import { V4GraphicAuthoringProvider, V4GraphicActions, V4GraphicPanel } from "./graphic/workspace";
import { V4TextActions, V4TextAuthoringProvider, V4TextPanel } from "./text/workspace";
import { canvasTextPlanner } from "./text/canvas-plan";
import { fontRecoveryBlocked } from "./text/font-recovery";
import type { FontImportProtocol } from "./text/font-catalog";

/** Full V4 remains in the workspace controller. Read panels never manufacture
 * a legacy document. Changes use versioned public application operations. */
export function V4Product() {
  useLocale();
  const { controller, v4, busy, api } = useWorkspace();
  useSyncExternalStore(controller.v4.captionRecovery.subscribe, controller.v4.captionRecovery.getSnapshot, controller.v4.captionRecovery.getSnapshot);
  const [editing, setEditing] = useState<V4Bookmark | "new" | null>(null);
  const [timelineReview, setTimelineReview] = useState(false);
  const [mediaReview, setMediaReview] = useState(false);
  const [settingsVisible,setSettingsVisible] = useState(false);
  const [packageSource, setPackageSource] = useState<PackageSource | null>(null);
  const selectGain = useCallback((id: string) => { controller.v4.select(id); setEditing(null); }, [controller]);
  const renderAudioControls = useCallback((element: V4Element) => gainChannel(element) ? <V4GainKeyframeLine compact elementId={element.id} onSelect={selectGain} /> : <V4AudioGainLine elementId={element.id} onSelect={selectGain} />, [selectGain]);
  const [seek, setSeek] = useState("0"),
    [pending, setPending] = useState(false);
  const seekIntent = useRef({ dirty: false, focused: false });
  useEffect(() => {
    setEditing(null);
  }, [v4?.head.revision, v4?.historical?.revision, v4?.sequenceId]);
  useEffect(() => { setTimelineReview(false); }, [v4?.head.project.id, v4?.historical?.revision, v4?.sequenceId]);
  useEffect(() => { setMediaReview(false); }, [v4?.head.project.id, v4?.historical?.revision, v4?.sequenceId]);
  useEffect(() => {
    seekIntent.current = { dirty: false, focused: false };
    if (v4) setSeek(secondsForDisplay(v4.playhead).toFixed(6));
  }, [v4?.head.project.id, v4?.head.revision, v4?.historical?.revision, v4?.sequenceId]);
  useEffect(() => {
    if (v4 && !seekIntent.current.dirty && !seekIntent.current.focused)
      setSeek(secondsForDisplay(v4.playhead).toFixed(6));
  }, [v4?.playhead]);
  useEffect(() => {
    if (!v4?.sequenceId || editorProjectId(location.pathname) !== v4.head.project.id) return;
    const url = new URL(location.href);
    url.searchParams.set("scene", v4.sequenceId);
    history.replaceState(history.state, "", url.pathname + url.search + url.hash);
  }, [v4?.head.project.id, v4?.sequenceId]);
  const planCanvasText = useMemo(() => api ? canvasTextPlanner(api) : undefined, [api]);
  if (!v4) return null;
  const snapshot = v4.historical ?? v4.head,
    project = snapshot.project;
  const sequence =
    project.sequences.find((s) => s.id === v4.sequenceId) ??
    project.sequences.find((s) => s.id === project.default_sequence_id)!;
  const settings =
    project.settings.profile === "classic_global"
      ? project.settings.settings
      : project.settings.formats.find((f) => f.sequence_id === sequence.id)!;
  const selected = sequence.bookmarks.find((b) => b.id === v4.selectedId);
  const activeNotes = activeBookmarkNotes(sequence.bookmarks, v4.playhead);
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
  const captionBlocked = controller.v4.captionRecovery.blocked();
  const fontBlocked = fontRecoveryBlocked(v4.fontRecovery);
  const textAvailable = v4.textFrameCapability?.schema_version === 2 && v4.textFrameCapability.available && v4.textFrameCapability.submit_tool === "opencut_job_v4_text_frame_png";
  const importFont = v4.fontRecovery?.supported === true ? (request: Parameters<typeof controller.v4.importFont>[0], protocol?: FontImportProtocol) => controller.v4.importFont(request, protocol) : undefined;
  const fontImportBlockedReason = captionBlocked ? "Resolve the original caption edit before starting another operation." : fontBlocked ? "Resolve the original font import before starting another operation." : null;
  const readonly = !!v4.historical || busy || pending || fontBlocked || captionBlocked;
  const importTimelineFiles = async (files: File[], target: MediaFileTarget) => {
    const capturedApi = api;
    try { await controller.v4.media.importFiles(files, undefined, target); }
    finally {
      const current = controller.getSnapshot();
      if (current.api === capturedApi && current.v4?.head.project.id === target.scope.projectId &&
        current.v4.sequenceId === target.scope.sequenceId && !current.v4.historical)
        setMediaReview(true);
    }
  };
  const cancelSeek = () => {
    seekIntent.current = { dirty: false, focused: false };
    setSeek(secondsForDisplay(v4.playhead).toFixed(6));
  };
  const submitSeek = () => {
    try {
      controller.v4.seek(timeFromDecimalSeconds(seek));
      seekIntent.current = { dirty: false, focused: false };
      const current = controller.getSnapshot().v4;
      if (current) setSeek(secondsForDisplay(current.playhead).toFixed(6));
    } catch (error) { controller.report(error); }
  };
  return (
    <V4SceneStackProvider>
    <V4MaskAuthoringProvider>
    <V4GraphicAuthoringProvider>
    <V4TextAuthoringProvider available={textAvailable} planCanvasText={planCanvasText} importFont={importFont} fontImportBlockedReason={fontImportBlockedReason}>
    <main className="workspace-layout v4-product" aria-label={t("V4 editor")}>
      <EditRecoveryPanel />
      {settingsVisible && <Modal title={t("Canvas and background")} onClose={()=>setSettingsVisible(false)}><V4ProjectSettingsEditor /></Modal>}
      {packageSource && <ExportProjectPackage source={packageSource} onClose={() => setPackageSource(null)} />}
      <div className="v4-scene-bar toolbar">
        <strong>{t("Scenes")}</strong>
        <Button disabled={busy || !!v4.historical} onClick={()=>setSettingsVisible(true)}>{t("Canvas and background")}</Button>
        <Button disabled={!api || busy} onClick={() => setPackageSource({project_id:project.id, revision:snapshot.revision, name:project.name})}>{t("Export project package")}</Button>
        <select
          aria-label={t("V4 scene")}
          value={sequence.id}
          onChange={(e) => {
            controller.v4.chooseSequence(e.target.value);
            setEditing(null);
            setSeek("0");
          }}
        >
          {project.sequences.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </select>
        <span>
          {settings.canvas_size.width} × {settings.canvas_size.height} ·{" "}
          {settings.fps.numerator / settings.fps.denominator} fps
        </span>
        <span className="muted">
          V4 ·{" "}
          {t(
            project.settings.profile === "classic_global"
              ? "Global settings"
              : "Per-scene settings",
          )}
        </span>
      </div>
      {v4.historical && (
        <div className="history-banner">
          {t("Viewing a saved revision · Read only")}{" "}
          <Button
            onClick={() => {
              controller.v4.returnToHead();
              setEditing(null);
            }}
          >
            {t("Return to current")}
          </Button>
        </div>
      )}
      <V4MediaPanel />
      <div className="v4-stage">
        <Panel title={t("Scene")} className="v4-canvas-panel">
          <div className="v4-capability" role="status">
            {t(
              "Edit video, audio, images, text and graphics on the timeline. Preview and MP4 export support the available blend modes when this service provides rendering. Animation and timeline edits use the available planning operations.",
            )}
            <p>
              {t(
                audioWavAvailability(v4).reason ??
                  "Audio WAV export is available in the toolbar.",
              )}
            </p>
          </div>
          <V4FrameCanvas
            width={settings.canvas_size.width}
            height={settings.canvas_size.height}
            notes={activeNotes}
          />
          <V4AudioPreviewControls />
          <div className="toolbar padded">
            <Field label={t("Playhead (seconds)")}>
              <input
                inputMode="decimal"
                value={seek}
                onFocus={() => { seekIntent.current.focused = true; }}
                onBlur={() => {
                  seekIntent.current.focused = false;
                  // Keep a changed draft through blur so the adjacent submit
                  // button consumes that intention, not the live audio clock.
                  if (!seekIntent.current.dirty) setSeek(secondsForDisplay(v4.playhead).toFixed(6));
                }}
                onChange={(e) => { seekIntent.current.dirty = true; setSeek(e.target.value); }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") { e.preventDefault(); submitSeek(); }
                  if (e.key === "Escape") { e.preventDefault(); cancelSeek(); }
                }}
              />
            </Field>
            <Button
              onClick={submitSeek}
            >
              {t("Go to time")}
            </Button>
            <span>{formatTime(v4.playhead)}</span>
          </div>
        </Panel>
        <Panel title={t("Inspector")} className="v4-inspector">
          <div className="scroll padded">
            <V4ResourceCatalogPanel />
            <V4SceneEffectsPanel />
            <V4TextPanel />
            <V4GraphicPanel />
            <V4MaskPanel />
          <V4CaptionWorkspace recovery={controller.v4.captionRecovery} mutateCaption={context => controller.v4.applyCaption(context)} />
            {editing ? (
              <BookmarkEditor
                key={editing === "new" ? `new:${sequence.id}` : editing.id}
                bookmark={editing === "new" ? null : editing}
                sequenceId={sequence.id}
                playhead={v4.playhead}
                disabled={readonly}
                save={(op) => controller.v4.bookmark(op)}
                close={() => setEditing(null)}
              />
            ) : selected ? (
              <>
                <h3>
                  {t("Bookmark")} · {formatTime(selected.time)}
                </h3>
                <p className="v4-note-text">
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
                        controller.v4.bookmark({
                          kind: "move",
                          sequence_id: sequence.id,
                          bookmark_id: selected.id,
                          time: v4.playhead,
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
                        controller.v4.bookmark({
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
            ) : (
              <p className="muted">
                {t(
                  "Select an element, track, caption or bookmark to inspect it.",
                )}
              </p>
            )}
            <V4Inspector
              project={project}
              sequence={sequence}
              selectedId={v4.selectedId}
            />
          </div>
        </Panel>
        <V4History />
      </div>
      <Panel
        title={t("Timeline")}
        className="v4-timeline"
        actions={
          <>
            <V4TextActions />
            <V4GraphicActions />
            <Button
              disabled={readonly}
              onClick={() => {
                controller.v4.select(null);
                setEditing("new");
              }}
            >
              {t("Add bookmark")}
            </Button>
            <Button
              disabled={readonly}
              onClick={() =>
                void run(() =>
                  controller.v4.bookmark({
                    kind: "toggle",
                    sequence_id: sequence.id,
                    time: v4.playhead,
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
          <V4ClipboardControls
            controller={controller.v4.clipboard}
            state={v4.clipboard}
            scope={{ projectId: project.id, revision: snapshot.revision, sequenceId: sequence.id }}
            canCopy={v4.timelineEdit.selection.length > 0 || sequence.tracks.some(track => track.elements.some(element => element.id === v4.selectedId))}
            canPaste={!v4.historical && !v4.timelineEdit.attempt && !v4.timelineEdit.recoveryError && !v4.gainDraft &&
              !v4.media.attempt && !v4.historyPlan && !v4.audioGain && !v4.sourceAudio &&
              (!v4.clipboard.attempt || (v4.clipboard.attempt.request.project_id === project.id &&
                v4.clipboard.attempt.request.sequence_id === sequence.id))}
            disabled={busy || pending || !!v4.gainDraft}
            onFiles={async (files, captured) => {
              const current = controller.getSnapshot().v4;
              if (!current || current.historical || current.head.project.id !== captured.projectId ||
                current.head.revision !== captured.revision || current.sequenceId !== captured.sequenceId)
                throw new Error("The file paste target changed. Paste into the current scene again.");
              const scene = current.head.project.sequences.find(s => s.id === captured.sequenceId)!;
              const track = scene.tracks.find(t => t.id === current.selectedId || t.elements.some(e => e.id === current.selectedId)) ??
                scene.tracks.find(t => t.id === current.timelineEdit.selection[0]?.track_id) ??
                scene.tracks.find(t => t.id === scene.main_track_id)!;
              await importTimelineFiles(files, { scope: captured, trackId: track.id, at: current.playhead });
            }}
          />
          {v4.timelineEdit.recoveryError && <div role="alert">
            <p>{t(v4.timelineEdit.recoveryError)}</p>
            <Button disabled={busy || pending} onClick={() => void run(() => controller.v4.recoverTimeline())}>{t("Retry timeline recovery")}</Button>
            <Button disabled={busy || pending} onClick={() => void run(() => controller.v4.recoverTimeline(true))}>{t("Repair recovery pointer")}</Button>
          </div>}
          {v4.timelineEdit.attempt && <div className="v4-timeline-status" role="status">
            {t(v4.timelineEdit.attempt.phase === "saved" ? "Timeline changes saved."
              : v4.timelineEdit.attempt.phase === "unconfirmed" ? "Timeline save is unconfirmed. Retry the original operation to recover its receipt."
                : "Timeline changes await review.")}
            <Button onClick={() => setTimelineReview(true)}>{t("Review timeline changes")}</Button>
          </div>}
          <V4TimelinePanel
            key={`${project.id}/${sequence.id}/${v4.historical?.revision ?? "head"}`}
            controller={controller.v4.timelineEdit}
            sequence={sequence}
            sourceArt={api ? { api, assets: project.assets, gainDraft: v4.gainDraft } : undefined}
            scope={{ projectId: project.id, revision: snapshot.revision, sequenceId: sequence.id }}
            selection={v4.timelineEdit.selection}
            selectedId={v4.selectedId}
            playhead={v4.playhead}
            attempt={v4.timelineEdit.attempt}
            disabled={readonly || !!v4.timelineEdit.attempt || !!v4.timelineEdit.recoveryError || !!v4.clipboard.attempt ||
              !!v4.media.attempt || !!v4.historyPlan || !!v4.audioGain || !!v4.sourceAudio || !!v4.recovery?.record || !!v4.recovery?.error}
            gainDraftActive={!!v4.gainDraft}
            onSelect={(target, mode) => { controller.v4.timelineEdit.select(target, mode); setEditing(null); }}
            onSelectMany={(targets, primary) => controller.v4.timelineEdit.selectMany(targets, primary)}
            onSelectOther={id => { controller.v4.select(id); setEditing(null); }}
            onSeek={time => controller.v4.seek(time)}
            onReview={() => setTimelineReview(true)}
            onFiles={importTimelineFiles}
            renderAudioControls={renderAudioControls}
          />
        </div>
      </Panel>
      {mediaReview && v4.media.attempt && <MediaPlanDialog onClose={() => setMediaReview(false)} />}
      {timelineReview && v4.timelineEdit.attempt && <TimelineEditReview
        attempt={v4.timelineEdit.attempt}
        controller={controller.v4.timelineEdit}
        sequence={sequence}
        unavailableReason={v4.historical ? "Return to the current revision before editing."
          : v4.timelineEdit.attempt.request.sequence_id !== sequence.id ? "Return to the original scene before retrying this timeline operation."
            : ["planned", "failed"].includes(v4.timelineEdit.attempt.phase) &&
              v4.timelineEdit.attempt.request.expected_revision !== v4.head.revision
              ? "The project changed. Discard this plan and review a new timeline operation." : null}
        disabled={readonly || v4.timelineEdit.attempt.request.project_id !== project.id ||
          v4.timelineEdit.attempt.request.sequence_id !== sequence.id ||
          (["planned", "failed"].includes(v4.timelineEdit.attempt.phase) &&
            v4.timelineEdit.attempt.request.expected_revision !== v4.head.revision)}
        onClose={() => setTimelineReview(false)}
      />}
      <V4AudioGainStatus />
    </main>
    </V4TextAuthoringProvider>
    </V4GraphicAuthoringProvider>
    </V4MaskAuthoringProvider>
    </V4SceneStackProvider>
  );
}
