import { useEffect, useId, useMemo, useRef, useState } from "react";
import { LoaderCircle, Pause, Play, RotateCcw } from "lucide-react";
import { Button } from "../../shared/ui";
import { ErrorNotice, localSessionExpired } from "../../shared/error-notice";
import { useResourceCopy } from "./workbench/resources-copy";
import { useWorkspace } from "../context";
import { usePlayerContext } from "./workbench/player-context";
import type { CatalogEntry } from "./catalog_resources";

/** Fixed source overview, independent from the actual HTML audition clock. */
function waveform(entry: CatalogEntry): string | null {
  const data = entry.audio;
  if (!data || data.source_sha256 !== entry.source_sha256 || data.bucket_size !== 128 ||
      !data.amplitudes.length || data.amplitudes.some(p => !Number.isFinite(p) || p < 0)) return null;
  const scale = Math.max(1, ...data.amplitudes);
  return data.amplitudes.map((peak, i) => {
    const amplitude = peak / scale * 12;
    return `M${i + .5},${14 - amplitude}v${2 * amplitude}`;
  }).join("");
}

/** A single read-only audition owner. No project import or browser PCM decode. */
export function CatalogSoundPreview({ entry, name = entry.name, active = true, disabled = false, onActivate, onClose }: {
  entry: CatalogEntry; name?: string; active?: boolean; disabled?: boolean;
  onActivate?(): void; onClose(): void;
}) {
  const { api, controller } = useWorkspace(), c = useResourceCopy(), context = usePlayerContext();
  const owner = useRef(Symbol("catalog-audition")), player = useRef<HTMLAudioElement>(null);
  const latestContext = useRef(context); latestContext.current = context;
  const generation = useRef(0), autoPlay = useRef(false), errorId = useId();
  const [url, setUrl] = useState<string | null>(null), [error, setError] = useState<unknown>(null);
  const [playing, setPlaying] = useState(false), [position, setPosition] = useState(0);
  const [observedDuration, setDuration] = useState<number | null>(null), [reload, setReload] = useState(0);
  const analysis = entry.audio;
  const duration = observedDuration ?? (analysis && analysis.source_sha256 === entry.source_sha256 ? analysis.audition_duration_seconds : null);
  const peaks = useMemo(() => waveform(entry), [analysis, entry.source_sha256]), length = analysis?.amplitudes.length ?? 1;

  useEffect(() => {
    const run = ++generation.current, cancel = new AbortController();
    setUrl(null); setError(null); setPlaying(false); setPosition(0); setDuration(null);
    autoPlay.current = active && !disabled;
    if (!active || disabled) return () => { ++generation.current; };
    let objectUrl: string | undefined;
    const audio = player.current, identity = api?.connectionIdentity;
    void (async () => {
      if (!api) throw Error("OpenCut is not ready.");
      const response = await api.sdk.call("opencut_catalog_resources_preview", { catalog_id: entry.id }, { signal: cancel.signal });
      cancel.signal.throwIfAborted();
      if (response.catalog_id !== entry.id || response.mime_type !== "audio/ogg" || !entry.source_sha256 ||
          response.version.content_hash !== entry.source_sha256 || response.version.id !== `av_${response.version.content_hash}` ||
          !Number.isSafeInteger(response.version.size) || response.version.size <= 0 || response.version.size > 262144 ||
          response.bytes_hex.length !== response.version.size * 2 || !/^[0-9a-f]+$/i.test(response.bytes_hex) ||
          analysis && (analysis.source_sha256 !== response.version.content_hash || analysis.source_size !== response.version.size))
        throw Error("Sound identity changed.");
      const bytes = Uint8Array.from(response.bytes_hex.match(/../g)!, byte => Number.parseInt(byte, 16));
      const hash = Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)), byte => byte.toString(16).padStart(2, "0")).join("");
      cancel.signal.throwIfAborted();
      if (hash !== response.version.content_hash) throw Error("Sound bytes changed.");
      objectUrl = URL.createObjectURL(new Blob([bytes], { type: response.mime_type }));
      setUrl(objectUrl);
    })().catch(failure => {
      if (!cancel.signal.aborted && generation.current === run) {
        setError(failure);
        if (localSessionExpired(failure) && controller?.getSnapshot().api === api && api?.connectionIdentity === identity)
          controller.report(failure);
      }
    });
    return () => {
      ++generation.current; cancel.abort(); audio?.pause(); latestContext.current?.release(owner.current);
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [api, api?.connectionIdentity, entry.id, entry.source_sha256, analysis, active, disabled, reload]);

  useEffect(() => {
    if (!playing) return;
    let frame = 0;
    const tick = () => { setPosition(player.current?.currentTime ?? 0); frame = requestAnimationFrame(tick); };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [playing]);

  const play = async () => {
    const run = generation.current, audio = player.current;
    if (!audio || !active || disabled || !url) return;
    try {
      setError(null);
      if (context && !await context.begin(owner.current, () => audio.pause())) return;
      if (run !== generation.current) { latestContext.current?.release(owner.current); return; }
      await audio.play();
      if (run !== generation.current) audio.pause();
    } catch (failure) { if (run === generation.current) setError(failure); }
  };
  const loading = active && !url && !error && !disabled;
  const action = error ? c("retryPreview") : loading ? c("cancelPreview") : c(playing ? "pause" : "listen");
  const progress = duration && Number.isFinite(duration) && duration > 0 ? Math.max(0, Math.min(1, position / duration)) : 0;
  return <div className="resource-audio-preview" data-audition-active={active || undefined}>
    <audio ref={player} src={url ?? undefined} preload={active ? "auto" : "none"}
      onCanPlay={() => { if (autoPlay.current) { autoPlay.current = false; void play(); } }}
      onLoadedMetadata={() => { const value = player.current?.duration; if (value && Number.isFinite(value)) setDuration(value); }}
      onTimeUpdate={() => setPosition(player.current?.currentTime ?? 0)}
      onPlay={() => setPlaying(true)}
      onPause={() => { setPlaying(false); latestContext.current?.release(owner.current); }}
      onEnded={() => { setPlaying(false); setPosition(player.current?.currentTime ?? 0); latestContext.current?.release(owner.current); }}
      onError={() => { if (url) { setPlaying(false); setError(Error(c("previewFailed"))); } }} />
    <Button className="resource-catalog-listen" aria-label={`${action} · ${name}`} title={action}
      aria-pressed={playing} aria-busy={loading} aria-describedby={error ? errorId : undefined} disabled={disabled}
      icon={error ? <RotateCcw size={14} /> : loading ? <LoaderCircle size={14} /> : playing ? <Pause size={14} /> : <Play size={14} />}
      onClick={() => {
        if (loading) { onClose(); return; }
        if (!active) { onActivate?.(); return; }
        if (playing) player.current?.pause();
        else if (!url) setReload(value => value + 1);
        else void play();
      }} />
    <div className="resource-audio-overview" title={c("waveformOverview")}>
      {peaks !== null ? <svg role="img" aria-label={`${c("waveformOverview")} · ${name}`} viewBox={`0 0 ${length} 28`} preserveAspectRatio="none"><path d={peaks} /></svg>
        : <span className="muted" aria-label={c("waveformUnavailable")} title={c("waveformUnavailable")}>—</span>}
      <div className="resource-audio-progress" role="progressbar" aria-label={`${c("auditionProgress")} · ${name}`}
        aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(progress * 100)}
        aria-valuetext={`${position.toFixed(2)} / ${(duration ?? 0).toFixed(2)} s`}><span style={{ width: `${progress * 100}%` }} /></div>
    </div>
    <span className="resource-audio-duration" data-duration-source={observedDuration === null ? "catalog-observation" : "media-clock"}
      title={duration ? `${duration.toFixed(6)} s` : c("durationUnavailable")}>
      {duration && Number.isFinite(duration) ? `${duration.toFixed(2)}s` : "—"}
    </span>
    {loading && <span className="resource-audio-loading" role="status">{c("loading")}</span>}
    {error != null && <div className="resource-audio-error" id={errorId}><ErrorNotice error={error} localSession={controller?.canStartLocalSession ?? true} /></div>}
  </div>;
}
