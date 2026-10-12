import { useEffect, useRef, useState } from "react";
import { useWorkspace } from "../context";
import { Button, Field, Panel } from "../../shared/ui";
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

/** Full V4 remains in the workspace controller. Read panels never manufacture
 * a legacy document. Changes use versioned public application operations. */
export function V4Product() {
  useLocale();
  const { controller, v4, busy, api } = useWorkspace();
  const [editing, setEditing] = useState<V4Bookmark | "new" | null>(null);
  const [timelineReview, setTimelineReview] = useState(false);
  const [seek, setSeek] = useState("0"),
    [pending, setPending] = useState(false);
  const seekIntent = useRef({ dirty: false, focused: false });
  useEffect(() => {
    setEditing(null);
  }, [v4?.head.revision, v4?.historical?.revision, v4?.sequenceId]);
  useEffect(() => { setTimelineReview(false); }, [v4?.head.project.id, v4?.historical?.revision, v4?.sequenceId]);
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
  const readonly = !!v4.historical || busy || pending;
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
    <main className="workspace-layout v4-product" aria-label={t("V4 editor")}>
      <EditRecoveryPanel />
      <div className="v4-scene-bar toolbar">
        <strong>{t("Scenes")}</strong>
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
              "Video, audio and image timeline editing, media import and placement, source media preview, scene notes, history, source audio separation/restoration, element volume and mute are available. Still scene frames and scene sound depend on this service’s rendering capability. Animated trim/split, other element editing, continuous picture playback and video export are not available yet.",
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
              await controller.v4.media.importFiles(files);
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
            disabled={readonly || !!v4.timelineEdit.attempt || !!v4.timelineEdit.recoveryError || !!v4.clipboard.attempt}
            gainDraftActive={!!v4.gainDraft}
            onSelect={(target, mode) => { controller.v4.timelineEdit.select(target, mode); setEditing(null); }}
            onSelectMany={(targets, primary) => controller.v4.timelineEdit.selectMany(targets, primary)}
            onSelectOther={id => { controller.v4.select(id); setEditing(null); }}
            onSeek={time => controller.v4.seek(time)}
            onReview={() => setTimelineReview(true)}
            renderAudioControls={element => <V4AudioGainLine elementId={element.id}
              onSelect={() => { controller.v4.select(element.id); setEditing(null); }} />}
          />
        </div>
      </Panel>
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
  );
}
