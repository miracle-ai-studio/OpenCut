import { useEffect, useRef, useState } from "react";
import type { EditorApi } from "../../api";
import { acquireSourceArt, artSelectionKey } from "./service";
import type { ArtSelection, ArtRequestSelection, DisplayArt } from "./service";
import { ClassicWaveform } from "./waveform";

export function useSourceArt(api: EditorApi | null, selection: ArtRequestSelection | null) {
  const key = selection ? artSelectionKey(selection) : "";
  const [state, setState] = useState<{ api: EditorApi; key: string; art?: DisplayArt; error?: string } | null>(null);
  useEffect(() => {
    if (!api || !selection) return;
    let current = true, invalidated = false;
    let release: (() => void) | undefined;
    setState({ api, key });
    try {
      const lease = acquireSourceArt(api, selection, error => {
        invalidated = true;
        if (current) setState({ api, key, error });
      }); release = lease.release;
      void lease.promise.then(art => {
        if (current && !invalidated) setState({ api, key, art });
      }, error => {
        if (current) setState({ api, key, error: error instanceof Error ? error.message : "Source artwork unavailable." });
      });
    } catch (error) { setState({ api, key, error: error instanceof Error ? error.message : "Source artwork unavailable." }); }
    return () => { current = false; release?.(); };
  }, [api, key]);
  return state?.api === api && state.key === key ? state : null;
}

/** Rendering derived JPEG/source peaks never opens the authored source media. */
export function ArtDisplay({ art, error, loading = false, tiled = false }: { art?: DisplayArt; error?: string; loading?: boolean; tiled?: boolean }) {
  if (error) return <small title={error} aria-label="Source artwork unavailable">Artwork unavailable</small>;
  if (!art) return loading ? <small aria-label="Loading source artwork">Loading artwork…</small> : null;
  if (art.kind === "no_frame") return <small aria-label="No source thumbnail at one second">No frame at 1s</small>;
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

export function SourceArtView({ api, selection }: { api: EditorApi | null; selection: ArtSelection }) {
  const ref = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    const node = ref.current;
    if (!node || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver(entries => setVisible(entries.some(entry => entry.isIntersecting)),
      { root: node.closest(".v4-media-library"), rootMargin: "80px" });
    observer.observe(node); return () => observer.disconnect();
  }, []);
  const state = useSourceArt(api, visible ? selection : null);
  return <div ref={ref} className="v4-source-art" style={{ minHeight: 24, maxHeight: 72, overflow: "hidden" }}>
    <ArtDisplay art={state?.art} error={state?.error} loading={visible && !state?.art && !state?.error} />
  </div>;
}
