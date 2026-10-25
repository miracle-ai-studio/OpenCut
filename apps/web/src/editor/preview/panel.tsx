import { GuideControls, GuideOverlay } from "./guides";
import { readPreviewGuides, savePreviewGuides } from "../shared/view-preferences";
import { usePreviewViewport, ViewportControls } from "./viewport";
import { t, useLocale } from "../shared/locale";
import { useEffect, useRef, useState } from "react";
import { Maximize2, Pause, Play, SkipBack, Volume2, VolumeX } from "lucide-react";
import { useWorkspace } from "../workspace/context";
import { activeSequence } from "../model/types";
import { formatTime, zero } from "../model/time";
import { Button, Empty, Panel } from "../shared/ui";
import { Playback } from "./playback";
import { useShortcutActions, useShortcutLabels } from "../shortcuts";
import { useShortcutText } from "../shortcuts/messages";
export function PreviewPanel() {
  useLocale();
  const shortcut = useShortcutLabels(),
    st = useShortcutText();
  const {
    controller,
    api,
    snapshot,
    historical,
    sequenceId,
    playhead,
    seekId,
  } = useWorkspace();
  const view = historical ?? snapshot,
    sequence = activeSequence(view, sequenceId);
  const [guides, setGuides] = useState(readPreviewGuides);
  useEffect(() => savePreviewGuides(guides), [guides]);
  const viewport = usePreviewViewport(
    sequence?.width ?? 1920,
    sequence?.height ?? 1080,
    `${view?.project.id}/${sequence?.id}/${sequence?.width}/${sequence?.height}`,
  );
  const video = useRef<HTMLVideoElement>(null),
    playback = useRef<Playback | null>(null),
    generation = useRef(0);
  const [playing, setPlaying] = useState(false), [loading, setLoading] = useState(false), [muted, setMuted] = useState(false);
  const stop = () => {
    generation.current++;
    playback.current?.stop();
    playback.current = null;
    setPlaying(false);
    setLoading(false);
  };
  useEffect(() => {
    stop();
  }, [api, view?.project.id, view?.revision, sequence?.id, seekId]);
  useEffect(
    () => () => {
      generation.current++;
        playback.current?.stop();
    },
    [],
  );
  const play = () => {
    if (playing) {
      video.current?.pause();
      setPlaying(false);
      return;
    }
    if (playback.current && !video.current?.ended) {
      void video.current?.play().catch((error) => controller.report(error));
      setPlaying(true);
      return;
    }
    if (!api || !view || !sequence || !video.current) return;
    const startTime = video.current.ended ? zero : playhead;
    playback.current?.stop();
    const requested = ++generation.current;
    setLoading(true);
    const session = new Playback(api.sdk, video.current, (error) => {
      if (requested === generation.current) controller.report(error);
    });
    playback.current = session;
    void session.start(view, sequence.id, startTime).catch((error) => {
      if (requested === generation.current) {
        controller.report(error);
        stop();
      }
    });
  };
  useShortcutActions({
    "toggle-play": {
      run: play,
      enabled: !!sequence && !loading,
      reason: loading
        ? "Finish the current interaction first."
        : "Open a project first.",
    },
    "stop-playback": {
      run: () => {
        stop();
        controller.seek(zero);
      },
      enabled: !!sequence,
      reason: "Open a project first.",
    },
  });
  return (
    <Panel
      title={t("Preview")}
      className="preview-panel"
      actions={
        <>
          <span className="muted">
            {sequence ? `${sequence.width} × ${sequence.height}` : ""}
          </span>
          <Button
            aria-label={t("Fullscreen preview")}
            icon={<Maximize2 size={14} />}
            onClick={() =>
              void video.current?.parentElement
                ?.requestFullscreen()
                .catch((error) => controller.report(error))
            }
          />
        </>
      }
    >
      <div
        ref={viewport.ref}
        tabIndex={0}
        aria-label={t("Preview canvas")}
        className={`preview-stage${viewport.panning ? " panning" : ""}`}
        data-shortcut-scope="canvas"
        {...viewport.events}
        style={
          {
            "--canvas-ratio": sequence
              ? `${sequence.width}/${sequence.height}`
              : "16/9",
          } as React.CSSProperties
        }
      >
        <div
          className="preview-canvas"
          style={{
            width: viewport.geometry.width,
            height: viewport.geometry.height,
            transform: `translate(calc(-50% + ${viewport.geometry.pan.x}px), calc(-50% + ${viewport.geometry.pan.y}px))`,
          }}
        >
          <video
            ref={video}
            playsInline
            muted={muted}
            onCanPlay={() => setLoading(false)}
            onPlaying={() => {
              setPlaying(true);
              setLoading(false);
            }}
            onPause={() => setPlaying(false)}
            onEnded={() => {
              setPlaying(false);
              setLoading(false);
            }}
            onTimeUpdate={() => {
              const position = playback.current?.position();
              if (position) controller.present(position);
            }}
          />
          {!playback.current && !loading && (
            <Empty title={t("See your edit")}>
              <span>{t("Your project preview appears here.")}</span>
            </Empty>
          )}
          <GuideOverlay value={guides} />

        </div>
        {loading && (
          <div className="preview-loading" role="status">
            <span className="spinner" />
            {t("Preparing preview…")}
          </div>
        )}
      </div>
      <div className="preview-viewport-toolbar">
        <div className="toolbar">
          <ViewportControls viewport={viewport} />
          <GuideControls value={guides} onChange={setGuides} />
        </div>
        <span className="muted">
          {st("Middle drag to pan. Hold {key} and drag to use the hand tool.", {
            key: shortcut("pan-preview") || st("Unassigned"),
          })}
        </span>
      </div>
      <div className="preview-controls">
        <div className="toolbar">
          <Button
            aria-label={t("Back to beginning")}
            onClick={() => controller.seek(zero)}
            icon={<SkipBack size={15} />}
          />
          <Button
            className="play-button"
            aria-label={playing ? t("Pause preview") : t("Play preview")}
            title={shortcut("toggle-play")}
            disabled={!sequence || loading}
            onClick={play}
            icon={playing ? <Pause size={18} /> : <Play size={18} />}
          />
          <output>{formatTime(playhead)}</output>
        </div>
        <div className="toolbar">
          <Button
            aria-label={muted ? t("Unmute preview") : t("Mute preview")}
            onClick={() => setMuted(!muted)}
            icon={muted ? <VolumeX size={15} /> : <Volume2 size={15} />}
          />
        </div>
      </div>
    </Panel>
  );
}
