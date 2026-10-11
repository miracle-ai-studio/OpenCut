import { GuideControls, GuideOverlay } from "./guides";
import {
  readPreviewGuides,
  savePreviewGuides,
} from "../shared/view-preferences";
import { usePreviewViewport, ViewportControls } from "./viewport";
import { SnapshotActions } from "./snapshot-actions";
import { TransformOverlay } from "./transform-overlay";
import { t, useLocale } from "../shared/locale";
import { useEffect, useRef, useState } from "react";
import {
  Image,
  Maximize2,
  Pause,
  Play,
  SkipBack,
  Volume2,
  VolumeX,
} from "lucide-react";
import type { OpenCutClient } from "../../lib/editor-sdk";
import { useWorkspace } from "../workspace/context";
import { activeSequence } from "../model/types";
import { formatTime, zero } from "../model/time";
import { Button, Empty, Panel } from "../shared/ui";
import { Playback } from "./playback";
export function PreviewPanel() {
  useLocale();
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
    generation = useRef(0),
    frameAbort = useRef<AbortController | null>(null);
  const frameJob = useRef<{ sdk: OpenCutClient; id: string } | null>(null);
  const cancelFrame = () => {
    frameAbort.current?.abort();
    frameAbort.current = null;
    const job = frameJob.current;
    frameJob.current = null;
    if (job) void job.sdk.cancelPublicJob(job.id).catch(() => {});
  };
  const [playing, setPlaying] = useState(false),
    [loading, setLoading] = useState(false),
    [frame, setFrame] = useState<string | null>(null),
    [muted, setMuted] = useState(false),
    [snapshotMenu, setSnapshotMenu] = useState<{ x: number; y: number } | null>(
      null,
    );
  const stop = () => {
    generation.current++;
    cancelFrame();
    playback.current?.stop();
    playback.current = null;
    setPlaying(false);
    setLoading(false);
  };
  useEffect(() => {
    stop();
    setFrame(null);
    setSnapshotMenu(null);
  }, [api, view?.project.id, view?.revision, sequence?.id, seekId]);
  useEffect(
    () => () => {
      generation.current++;
      cancelFrame();
      playback.current?.stop();
    },
    [],
  );
  useEffect(
    () => () => {
      if (frame) URL.revokeObjectURL(frame);
    },
    [frame],
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
    cancelFrame();
    playback.current?.stop();
    const requested = ++generation.current;
    setFrame(null);
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
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (
        !event.defaultPrevented &&
        !(event.target as HTMLElement).closest(".preview-stage") &&
        event.code === "Space" &&
        !(event.target as HTMLElement).closest(
          "input,textarea,select,dialog,button,[contenteditable=true]",
        )
      ) {
        event.preventDefault();
        play();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  });
  const previewFrame = async (): Promise<Blob | null> => {
    if (!api || !view || !sequence) return null;
    stop();
    const requested = ++generation.current,
      request = new AbortController();
    frameAbort.current = request;
    setLoading(true);
    try {
      const input = {
        project_id: view.project.id,
        revision: view.revision,
        sequence_id: sequence.id,
        requested_time: playhead,
        alignment: "nearest" as const,
      };
      const accepted =
        view.format === "visual"
          ? await api.sdk.call("opencut_job_v3_frame", {
              ...input,
              idempotency_key: crypto.randomUUID(),
            })
          : await api.sdk.submitJob({
              kind: "preview.composited_frame",
              input,
              idempotency_key: crypto.randomUUID(),
            });
      if (requested !== generation.current) {
        void api.sdk.cancelPublicJob(accepted.job_id).catch(() => {});
        return null;
      }
      frameJob.current = { sdk: api.sdk, id: accepted.job_id };
      const job = await api.sdk.waitForPublicJob(accepted.job_id, {
        signal: request.signal,
        timeoutMs: 180000,
        pollIntervalMs: 200,
      });
      if (frameJob.current?.id === accepted.job_id) frameJob.current = null;
      const artifact = job.artifacts[0];
      if (!artifact) throw new Error("Frame job returned no artifact.");
      const blob = await (
        await api.sdk.downloadArtifact(artifact, { signal: request.signal })
      ).blob();
      if (requested !== generation.current) return null;
      setFrame(URL.createObjectURL(blob));
      return blob;
    } catch (error) {
      if (requested === generation.current) controller.report(error);
      return null;
    } finally {
      if (requested === generation.current) {
        frameAbort.current = null;
        setLoading(false);
      }
    }
  };
  useEffect(() => {
    if (
      !sequence ||
      (!sequence.tracks.some((track) => track.clips.length) &&
        !sequence.caption_cues?.length &&
        !view?.visual.some(
          (graph) => graph.sequence_id === sequence.id && graph.shapes.length,
        ))
    )
      return;
    const timer = setTimeout(() => {
      if (!playback.current && !frameAbort.current) void previewFrame();
    }, 450);
    return () => clearTimeout(timer);
  }, [api, view?.project.id, view?.revision, sequence?.id, seekId]);
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
        {...viewport.events}
        onContextMenu={(event) => {
          if (!sequence) return;
          event.preventDefault();
          setSnapshotMenu({ x: event.clientX, y: event.clientY });
        }}
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
          {frame && <img src={frame} alt="Rendered project frame" />}
          {!frame && !playback.current && !loading && (
            <Empty title={t("See your edit")}>
              <span>{t("Your project preview appears here.")}</span>
            </Empty>
          )}
          <GuideOverlay value={guides} />
          {frame && !playing && (
            <TransformOverlay
              key={`${view?.revision}-${sequenceId}-${controller.getSnapshot().selection?.id}`}
            />
          )}
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
        <span className="muted">{t("Middle drag or Space + drag to pan")}</span>
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
            disabled={!sequence || loading}
            onClick={play}
            icon={playing ? <Pause size={18} /> : <Play size={18} />}
          />
          <output>{formatTime(playhead)}</output>
        </div>
        <div className="toolbar">
          <Button
            aria-label={t("Render frame")}
            disabled={!sequence || loading}
            icon={<Image size={15} />}
            onClick={() => void previewFrame()}
          />
          <SnapshotActions
            menu={snapshotMenu}
            closeMenu={() => setSnapshotMenu(null)}
            capture={previewFrame}
            name={view?.project.name ?? "OpenCut"}
            disabled={!sequence || loading}
            report={(error) => controller.report(error)}
          />
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
