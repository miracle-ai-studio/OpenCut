import { t } from "../../../shared/locale";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { Eye } from "lucide-react";
import type { EditorApi } from "../../api";
import { acquireSourceArt, artSelectionKey } from "./service";
import type { ArtSelection, ArtRequestSelection, DisplayArt } from "./service";
import { ClassicWaveform } from "./waveform";
import { sourceArtFailure, type SourceArtFailure } from "./failure";
import "./view.css";

export function useSourceArt(api: EditorApi | null, selection: ArtRequestSelection | null) {
  const key = selection ? artSelectionKey(selection) : "";
  const [epoch, setEpoch] = useState(0);
  const live = useRef({ api, key, epoch });
  live.current = { api, key, epoch };
  const retrying = useRef(false);
  const [state, setState] = useState<{ api: EditorApi; key: string; epoch: number; art?: DisplayArt; error?: SourceArtFailure } | null>(null);
  useEffect(() => {
    if (!api || !selection) return;
    retrying.current = false;
    let current = true, invalidated = false;
    let release: (() => void) | undefined;
    setState({ api, key, epoch });
    try {
      const lease = acquireSourceArt(api, selection, error => {
        invalidated = true;
        if (current) setState({ api, key, epoch, error: sourceArtFailure(error) });
      }); release = lease.release;
      void lease.promise.then(art => {
        if (current && !invalidated) setState({ api, key, epoch, art });
      }, error => {
        if (current && !invalidated) setState({ api, key, epoch, error: sourceArtFailure(error) });
      });
    } catch (error) { setState({ api, key, epoch, error: sourceArtFailure(error) }); }
    return () => { current = false; release?.(); };
  }, [api, key, epoch]);
  if (state?.api !== api || state?.key !== key || state?.epoch !== epoch) return null;
  return { ...state, retry: () => {
    // A queued click from a replaced API/revision/attempt must not restart its successor.
    if (!state.error?.retryable || retrying.current || live.current.api !== api || live.current.key !== key || live.current.epoch !== epoch) return;
    retrying.current = true;
    setEpoch(value => value + 1);
  } };
}

function ArtAction({ label, title, onClick, children }: { label: string; title: string; onClick(): void; children: ReactNode }) {
  return <button type="button" draggable={false} title={title} aria-label={label}
    onPointerDown={event => event.stopPropagation()} onDragStart={event => { event.preventDefault(); event.stopPropagation(); }}
    onKeyDown={event => event.stopPropagation()} onClick={event => { event.stopPropagation(); onClick(); }}>{children}</button>;
}

/** Rendering derived JPEG/source peaks never opens the authored source media. */
export function ArtDisplay({ art, error, loading = false, tiled = false, onRetry, retryName, retryHint, onPreview }: {
  art?: DisplayArt; error?: SourceArtFailure; loading?: boolean; tiled?: boolean; onRetry?: () => void; retryName?: string; retryHint?: string; onPreview?: () => void;
}) {
  if (error) {
    const message = [t(error.kind === "connection" ? "Reconnect to load source artwork."
      : error.kind === "authorization" ? "Source artwork access was rejected. Check your permissions."
      : error.kind === "budget" ? "Source artwork reached its resource limit. Retry later." : "Source artwork could not load. Retry."),
      error.retryable && retryHint && t(retryHint)].filter(Boolean).join(" ");
    const details = [error.code, error.message, error.requestId].filter(Boolean).join(" · ");
    return <div className="source-art-failure" data-timeline-no-gesture draggable={false}
      onDragStart={event => { event.preventDefault(); event.stopPropagation(); }}>
      <small title={message} aria-label="Source artwork unavailable">{message}</small>
      <div className="source-art-actions" onPointerDown={event => event.stopPropagation()} onClick={event => event.stopPropagation()}>
        {onRetry && error.retryable && <ArtAction title={message}
          label={t("Retry source artwork for {name}", { name: retryName ?? "" })} onClick={onRetry}>{t("Retry")}</ArtAction>}
        {onPreview && error.retryable && <ArtAction title={t("Preview source {name}", { name: retryName ?? "" })}
          label={t("Preview source {name}", { name: retryName ?? "" })} onClick={onPreview}><Eye size={14} /></ArtAction>}
        <span className="source-art-technical" tabIndex={0} title={`${message}\n${details}`}
          onKeyDown={event => event.stopPropagation()}
          aria-label={`${message} ${t("Source artwork technical details")}: ${details}`}>ⓘ</span>
      </div>
    </div>;
  }
  if (!art) return loading ? <small aria-label="Loading source artwork">{t("Loading artwork…")}</small> : null;
  if (art.kind === "no_frame") return <small aria-label="No source thumbnail at one second">{t("No frame at 1s")}</small>;
  if (art.kind === "classic_waveform") return <ClassicWaveform art={art} />;
  if (art.kind === "thumbnail" && tiled) return <div role="img" aria-label="Source thumbnail" style={{
    height: 32, width: "100%", backgroundImage: `url(${art.url})`, backgroundRepeat: "repeat-x", backgroundSize: "57px 32px", pointerEvents: "none" }} />;
  if (art.kind === "thumbnail") return <img src={art.url} alt="Source thumbnail" draggable={false}
    style={{ display: "block", width: "100%", height: "100%", maxHeight: 72, objectFit: "contain", pointerEvents: "none" }} />;
  const count = art.amplitudes.length;
  const path = Array.from(art.amplitudes, (peak, i) => peak > 0 ? `M${i * 2 + .5} 24V${24 * (1 - Math.min(1, peak))}` : "").join("");
  return <svg role="img" aria-label="Source peaks before element gain" width="100%" height="24" viewBox={`0 0 ${Math.max(1, count * 2)} 24`}
    preserveAspectRatio="none" style={{ display: "block", pointerEvents: "none" }}>
    <path d={path} stroke="currentColor" strokeWidth="1" fill="none" />
  </svg>;
}

export function SourceArtView({ api, selection, onPreview }: { api: EditorApi | null; selection: ArtSelection; onPreview?: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    const node = ref.current;
    if (!node || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver(entries => setVisible(entries.some(entry => entry.isIntersecting)),
      { root: node.closest(".editor-media-library"), rootMargin: "80px" });
    observer.observe(node); return () => observer.disconnect();
  }, []);
  const state = useSourceArt(api, visible ? selection : null);
  return <div ref={ref} className="v4-source-art" style={{ minHeight: 24, maxHeight: 72, overflow: "hidden" }}>
    <ArtDisplay art={state?.art} error={state?.error} loading={visible && !state?.art && !state?.error}
      onRetry={state?.retry} retryName={selection.asset.name} onPreview={onPreview} />
  </div>;
}
