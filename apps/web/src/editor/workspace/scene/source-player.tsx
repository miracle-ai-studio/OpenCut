import { useEffect, useRef, useState } from "react";
import { MediaPlayer, Select, type MediaPlayerHandle } from "../../shared/controls";
import { Button, Field } from "../../shared/ui";
import { compareTime, formatTime, fromSeconds, secondsForDisplay, subtractTime, timeFromDecimalSeconds, zero } from "../../model/time";
import type { Time } from "../../model/types";
import { useWorkspace } from "../context";
import { useShortcutActions } from "../../shortcuts/provider";
import { editRecoveryBlocked } from "./edit-recovery";
import { mediaPlacementTracks, placeMediaDirect, MediaPlanDialog } from "./media-dialog";
import type { Asset } from "./types";
import { useResourceCopy } from "./workbench/resources-copy";
import { usePlayerContext } from "./workbench/player-context";
import { SourceArtView } from "./source-art/view";

const inputTime = (time: Time) => secondsForDisplay(time).toFixed(6);

/** Source-domain cursor and markers only. Placement stays in the shared media plan. */
export function SourcePlayer({ asset, projectId, revision, url, onPlaced }: {
  asset: Asset; projectId: string; revision: string; url: string; onPlaced(): void;
}) {
  const { controller, scene, busy, api } = useWorkspace(), player = usePlayerContext(), c = useResourceCopy();
  const media = useRef<MediaPlayerHandle>(null), owner = useRef(Symbol("source-player"));
  const root = useRef<HTMLDivElement>(null);
  const [sequenceId] = useState(scene?.sequenceId);
  const at = controller.scene.currentPlayhead();
  const [initialSelection] = useState(scene?.selectedId);
  const sequence = scene?.head.project.sequences.find(s => s.id === sequenceId);
  const tracks = mediaPlacementTracks(sequence, asset.kind);
  const [target, setTarget] = useState(() => tracks.find(t => t.id === scene?.selectedId || t.elements.some(e => e.id === scene?.selectedId))?.id ?? "new");
  const duration = asset.duration ?? { ticks: "3", timebase: 1 };
  const unavailable = ["audio", "video"].includes(asset.kind) && (!asset.duration || compareTime(asset.duration, zero) <= 0);
  const ranged = ["audio", "video"].includes(asset.kind) && !unavailable;
  const [markers, setMarkers] = useState({ start: zero, end: duration });
  const [draft, setDraft] = useState({ start: inputTime(zero), end: inputTime(duration) });
  const [error, setError] = useState<string | null>(null), [pending, setPending] = useState(false), [review, setReview] = useState(false);
  const writing = useRef(false);
  const dirty = draft.start !== inputTime(markers.start) || draft.end !== inputTime(markers.end);
  const changed = !scene || scene.head.project.id !== projectId || scene.head.revision !== revision || scene.sequenceId !== sequenceId;
  const blocked = unavailable || changed || busy || pending || !!scene?.historical || !!scene?.media.attempt || editRecoveryBlocked(scene?.recovery) ||
    ["loading", "playing", "stalled", "unconfirmed"].includes(scene?.audioPreview.phase ?? "") ||
    !!(scene?.historyPlan || scene?.gainDraft || scene?.audioGain || scene?.sourceAudio || scene?.timelineEdit.attempt || scene?.timelineEdit.recoveryError || scene?.clipboard.attempt);
  const validTarget = target === "new" || tracks.some(t => t.id === target);
  const span = ranged ? subtractTime(markers.end, markers.start) : duration;
  const confirm = (which: "start" | "end", value: Time) => {
    const next = { ...markers, [which]: value };
    if (compareTime(next.start, zero) < 0 || compareTime(next.end, next.start) <= 0 || compareTime(next.end, duration) > 0) {
      setError(c("sourceRangeInvalid")); return;
    }
    setMarkers(next); setDraft(values => ({...values, [which]: inputTime(value)})); setError(null);
  };
  const mark = (which: "start" | "end") => { if (ranged) confirm(which, fromSeconds(media.current?.position() ?? 0)); };
  const commit = (which: "start" | "end") => {
    try { confirm(which, timeFromDecimalSeconds(draft[which])); }
    catch { setError(c("sourceRangeInvalid")); }
  };
  const add = async () => {
    if (blocked || dirty || error || writing.current || !validTarget || !sequenceId) return;
    writing.current = true; setPending(true);
    try {
      const result = await placeMediaDirect(controller, asset, { projectId, revision, sequenceId, at, trackId: target === "new" ? null : target },
        ranged ? { start: markers.start, duration: span } : undefined);
      const live = controller.getSnapshot().scene;
      if (live?.head.project.id === projectId && live.sequenceId === sequenceId) {
        if (live.selectedId === initialSelection) controller.scene.select(result.elementId);
        if (!player) onPlaced();
      }
    } catch (error) { controller.report(error); setReview(!!controller.getSnapshot().scene?.media.attempt); }
    finally { writing.current = false; setPending(false); }
  };
  useEffect(() => () => player?.release(owner.current), [player?.release]);
  useEffect(() => { root.current?.focus({ preventScroll: true }); }, []);
  useShortcutActions({
    "toggle-play": { enabled: ["audio", "video"].includes(asset.kind), run: () => media.current?.toggle() },
    "stop-playback": { enabled: ["audio", "video"].includes(asset.kind), run: () => { media.current?.pause(); media.current?.seek(secondsForDisplay(markers.start)); } },
    "seek-forward": { run: () => media.current?.seek(Math.min(secondsForDisplay(duration), (media.current?.position() ?? 0) + 1)) },
    "seek-backward": { run: () => media.current?.seek(Math.max(0, (media.current?.position() ?? 0) - 1)) },
  }, { priority: 10 });
  return <div className={`source-player${asset.kind === "audio" ? " source-player-audio" : ""}`} ref={root} tabIndex={0} onKeyDown={event => {
    if (pending || event.defaultPrevented || event.nativeEvent.isComposing || event.ctrlKey || event.metaKey || event.altKey || (event.target as Element).closest("input,textarea,select,button,[contenteditable=true]")) return;
    if (event.key.toLowerCase() === "i" || event.key.toLowerCase() === "o") { event.preventDefault(); event.stopPropagation(); mark(event.key.toLowerCase() === "i" ? "start" : "end"); }
    if (event.key === "Enter" && event.shiftKey) { event.preventDefault(); event.stopPropagation(); if (!event.repeat) void add(); }
  }}>
    {asset.kind === "audio" && <div className="source-player-audio-art"><SourceArtView api={api} selection={{asset,projectId,revision,query:{kind:"waveform",sample_rate:48000,projection:{kind:"source",range:null,bins:128}}}} /></div>}
    {asset.kind === "image" || asset.kind === "graphic" ? <div className="source-player-image"><img src={url} alt={asset.name} /></div> :
      <MediaPlayer src={url} audioOnly={asset.kind === "audio"} controlRef={media}
        beforePlay={() => player?.begin(owner.current, () => media.current?.pause())} />}
    <div className="source-range-controls">
      {ranged && <div className="source-markers">
        {(["start", "end"] as const).map(which => <Field key={which} label={c(which === "start" ? "sourceIn" : "sourceOut")}>
          <input inputMode="decimal" disabled={pending} value={draft[which]} onChange={event => { const value = event.target.value; setDraft(draft => ({...draft, [which]: value})); }}
            onKeyDown={event => {
              if (event.key === "Enter") { event.preventDefault(); event.stopPropagation(); commit(which); }
              if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); setDraft(values => ({...values, [which]: inputTime(markers[which])})); setError(null); }
            }} />
        </Field>)}
        <Button disabled={pending} onClick={() => mark("start")}>{c("markIn")}</Button><Button disabled={pending} onClick={() => mark("end")}>{c("markOut")}</Button>
      </div>}
      <div className="source-placement">
        <span>{unavailable ? c("sourceDurationUnavailable") : <>{c("sourceSelected")} <strong>{formatTime(span)}</strong> · {c("dropStart")} {formatTime(at)}</>}</span>
        <Field label={c("sourceTarget")}><Select disabled={blocked} value={target} onChange={event => setTarget(event.target.value)}>
          <option value="new">{c("sourceNewTrack")}</option>{tracks.map(track => <option key={track.id} value={track.id}>{track.name}</option>)}
        </Select></Field>
        <Button className="primary" disabled={blocked || dirty || !!error || !validTarget} onClick={() => void add()}>{c(ranged ? "sourceAdd" : "add")}</Button>
      </div>
      {target === "new" && <p className="muted">{c("sourceNewTrack")} · {asset.kind === "audio" ? c("tabAudio") : c("tabMedia")}</p>}
      {dirty && <p role="status">{c("sourceRangeConfirm")}</p>}{error && <p role="alert">{error}</p>}
    </div>
    {review && <MediaPlanDialog onClose={() => setReview(false)} />}
  </div>;
}
