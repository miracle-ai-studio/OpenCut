import { useEffect, useId, useRef, useState, useSyncExternalStore } from "react";
import type { WorkspaceController } from "../../controller";
import { usePlayerContext } from "../workbench/player-context";
import { addTime, compareTime } from "../../../model/time";
import { t } from "../../../shared/locale";
import type { VisualTarget } from "./visual-properties";
import type { VisualPropertyPlan } from "./visual-property-plan";

type Props = {
  controller: WorkspaceController; targets: readonly VisualTarget[]; value: string;
  disabledReason: string | null;
  prepare(value: number, signal: AbortSignal): Promise<VisualPropertyPlan>;
  commit(plan: VisualPropertyPlan): Promise<boolean>;
  own(active: boolean): void; change(value: string): void; restore(): void; report(error: unknown): void;
};
type Gesture = {
  id: string; scope: string; pointer: number | null; value: number; version: number;
  shown: number; plan: VisualPropertyPlan | null; abort: AbortController | null;
  work: Promise<void> | null; releasing: boolean; committing: boolean; cancelled: boolean;
};
const keys = ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End", "PageUp", "PageDown"];

/** One gesture owns one latest draft and the form's original atomic commit.
 * Thumb movement never implies that the native picture has finished rendering. */
export function OpacitySlider(props: Props) {
  const {controller} = props, player = usePlayerContext(), latest = useRef(props); latest.current = props;
  const playerRef = useRef(player); playerRef.current = player;
  const visual = controller.scene.visualPreview;
  const picture = useSyncExternalStore(visual.subscribe, visual.snapshot);
  const [phase, setPhase] = useState<"idle"|"previewing"|"ready"|"saving"|"restoring">("idle");
  const gesture = useRef<Gesture | null>(null), mounted = useRef(true), input = useRef<HTMLInputElement>(null);
  const description = useId();
  const scope = () => {
    const s = controller.getSnapshot(), v = s.scene, session = visual.pausedEditSession();
    return JSON.stringify([s.api?.connectionIdentity, v?.head.project.id, v?.head.revision, v?.sequenceId,
      v?.selectedId, v?.timelineEdit.selection, v?.playhead, controller.scene.timelineEdit.identity(),
      session && [session.session_id, session.generation, session.overlay_epoch, session.owner_id, session.lease_id], playerRef.current?.preview ?? null]);
  };
  const liveScope = scope();
  const activeReason = () => {
    const p = latest.current, v = controller.getSnapshot().scene;
    if (p.disabledReason) return p.disabledReason;
    if (playerRef.current?.preview) return "Return to the timeline picture to preview opacity.";
    if (!v || !visual.pausedEditSession())
      return "Pause the timeline picture to preview opacity.";
    const sequence = v.head.project.sequences.find(s => s.id === v.sequenceId);
    if (p.targets.some(({trackId, element}) => {
      const track = sequence?.tracks.find(t => t.id === trackId), visual = element.visual;
      return !track?.enabled || !element.enabled || !visual ||
        compareTime(v.playhead, element.timeline_range.start) < 0 ||
        compareTime(v.playhead, addTime(element.timeline_range.start, element.timeline_range.duration)) >= 0 ||
        !["video","image","graphic","sticker","text"].includes(element.content.kind) ||
        !(visual.profile === "classic_canvas_v31" || visual.profile === "legacy_composition_v3" && ["video","image"].includes(element.content.kind));
    })) return "All selected clips must be visible at the paused time to preview opacity.";
    return null;
  };
  const status = (next: typeof phase) => { if (mounted.current) setPhase(next); };
  const valid = (g: Gesture) => !g.cancelled && gesture.current === g && g.scope === scope() && !activeReason();
  const finish = (g: Gesture) => {
    if (gesture.current !== g) return;
    gesture.current = null;
    if (input.current && g.pointer !== null && input.current.hasPointerCapture?.(g.pointer)) input.current.releasePointerCapture(g.pointer);
    if (mounted.current) { latest.current.own(false); status("idle"); }
  };
  const cancel = async () => {
    const g = gesture.current; if (!g || g.committing || g.cancelled) return;
    g.cancelled = true;
    // Retire publication before aborting a possibly blocking HTTP phase.
    visual.supersedeEditDraft(g.id); g.abort?.abort();
    status("restoring");
    const restored = await visual.clearEditDraft(g.id);
    if (gesture.current !== g) return;
    if (mounted.current) {
      latest.current.restore();
      if (!restored && g.scope === scope() && visual.snapshot().editDraftId === g.id)
        latest.current.report(Error("The original picture could not be restored. Retry the preview before dragging again."));
    }
    finish(g);
  };
  const pump = (g: Gesture): Promise<void> => {
    if (g.work) return g.work;
    const work = (async () => {
      while (valid(g) && g.shown !== g.version) {
        const version = g.version, value = g.value, abort = new AbortController(); g.abort = abort;
        try {
          const plan = await latest.current.prepare(value / 100, abort.signal);
          if (!valid(g) || version !== g.version) continue;
          if (plan.transaction.operations.length) {
            const scene = controller.getSnapshot().scene!;
            await visual.previewEditDraft({draftId:g.id, intent:{kind:"property",property:"opacity"},
              transaction:plan.transaction, sequenceId:scene.sequenceId, time:structuredClone(scene.playhead)}, abort.signal);
          } else {
            // Returning to the exact saved value is a real baseline restore,
            // never a no-op transaction or a CSS approximation.
            await visual.clearEditDraft(g.id);
          }
          if (!valid(g) || version !== g.version) continue;
          g.shown = version; g.plan = plan; status(g.releasing ? "saving" : "ready");
        } catch (error) {
          if (!valid(g) || abort.signal.aborted || version !== g.version) continue;
          latest.current.report(error); await cancel(); return;
        }
      }
    })();
    g.work = work;
    void work.finally(() => { if (g.work === work) g.work = null; });
    return work;
  };
  const begin = (pointer: number | null) => {
    if (gesture.current || activeReason() || picture.phase !== "ready") return;
    const g: Gesture = {id:crypto.randomUUID(), scope:scope(), pointer, value:Number(latest.current.value || 50),
      version:0, shown:0, plan:null, abort:null, work:null, releasing:false, committing:false, cancelled:false};
    gesture.current = g; latest.current.own(true); playerRef.current?.stopExternal();
    if (pointer !== null) input.current?.setPointerCapture?.(pointer);
  };
  const change = (value: string) => {
    const g = gesture.current;
    if (!g || g.releasing || !valid(g)) return;
    g.value = Number(value); g.version++; g.plan = null;
    visual.supersedeEditDraft(g.id); g.abort?.abort();
    latest.current.change(value); status("previewing"); void pump(g);
  };
  const release = async () => {
    const g = gesture.current; if (!g || g.releasing) return;
    g.releasing = true; status("saving");
    await pump(g);
    if (!valid(g) || !g.plan || g.shown !== g.version) { await cancel(); return; }
    if (!g.plan.transaction.operations.length) { await cancel(); return; }
    // The exact final candidate has passed Native admission and rendering.
    // Its original planned request/key enters the existing recovery pipeline.
    g.committing = true;
    await latest.current.commit(g.plan);
    await visual.clearEditDraft(g.id);
    finish(g);
  };
  useEffect(() => {
    mounted.current = true;
    const blur = () => { void cancel(); }, hidden = () => { if (document.hidden) void cancel(); };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || !gesture.current || gesture.current.committing) return;
      event.preventDefault(); event.stopPropagation(); void cancel();
    };
    window.addEventListener("blur", blur); document.addEventListener("visibilitychange", hidden);
    document.addEventListener("keydown", escape, true);
    return () => { mounted.current = false; window.removeEventListener("blur",blur); document.removeEventListener("visibilitychange",hidden); document.removeEventListener("keydown",escape,true); void cancel(); };
  }, []);
  useEffect(() => { const g=gesture.current; if (g && !g.committing && (g.scope !== liveScope || activeReason())) void cancel(); }, [liveScope, props.disabledReason]);
  const reason = activeReason() ?? (!gesture.current && picture.phase !== "ready" ? "Wait for the timeline picture before dragging." : null);
  const text = phase === "previewing" ? "Preparing opacity preview…" : phase === "ready" ? "Opacity preview ready; release to save." :
    phase === "saving" ? "Confirming the final opacity…" : phase === "restoring" ? "Restoring the original picture…" : reason ?? "Drag to preview; release to save once. Esc restores the picture.";
  return <div className="workbench-opacity-slider" data-opacity-phase={phase}>
    <label htmlFor={`${description}-input`}>{t("Preview opacity")}</label>
    <input id={`${description}-input`} ref={input} type="range" min="0" max="100" step="1" value={props.value || "50"}
      aria-label={t("Preview opacity")} aria-valuetext={props.value ? `${props.value}%` : t("Mixed opacity values")}
      aria-describedby={description} disabled={!!reason || phase === "saving" || phase === "restoring"}
      onPointerDown={event => { if (event.button === 0) begin(event.pointerId); }}
      onChange={event => change(event.target.value)}
      onPointerUp={() => { void release(); }}
      onPointerCancel={() => { void cancel(); }}
      onLostPointerCapture={() => { if (!gesture.current?.releasing) void cancel(); }}
      onBlur={() => { if (!gesture.current?.releasing) void cancel(); }}
      onKeyDown={event => {
        event.stopPropagation();
        if (event.nativeEvent.isComposing || event.keyCode === 229) return;
        if (event.key === "Escape") { event.preventDefault(); void cancel(); }
        else if (event.key === "Enter") { event.preventDefault(); void release(); }
        else if (keys.includes(event.key)) begin(null);
      }}
      onKeyUp={event => { if (keys.includes(event.key)) { event.stopPropagation(); void release(); } }} />
    <p id={description} role={phase === "idle" ? undefined : "status"} className="muted">{t(text)}</p>
  </div>;
}
