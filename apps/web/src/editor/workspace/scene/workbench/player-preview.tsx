import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { ArrowLeft } from "lucide-react";
import { useEditingWorkspace } from "../../context";
import { useShortcutActions } from "../../../shortcuts/provider";
import { sceneShortcutActions } from "../../../shortcuts/scene-actions";
import { Button } from "../../../shared/ui";
import { PlayerContext, usePlayerContext, type PlayerPreview } from "./player-context";
import { useResourceCopy } from "./resources-copy";
import { ResourcePreview } from "../resource-preview";
import { EffectExample } from "./effects";
import { TransitionExample } from "./transitions";

export function PlayerPreviewProvider({ children }: { children: ReactNode }) {
  const { controller, api, scene } = useEditingWorkspace();
  const c = useResourceCopy();
  const snapshot = scene?.historical ?? scene?.head;
  const scope = JSON.stringify([api?.connectionIdentity, snapshot?.project.id, snapshot?.revision, scene?.sequenceId]);
  const liveScope = useRef(scope); liveScope.current = scope;
  const [state, setState] = useState<{ scope: string; value: PlayerPreview } | null>(null);
  const source = useRef<{ owner: symbol; pause(): void } | null>(null);
  const focus = useRef<HTMLElement | null>(null), openEpoch = useRef(0), audioEpoch = useRef(0);
  const stopExternal = useCallback(() => {
    audioEpoch.current++;
    const previous = source.current; source.current = null; previous?.pause();
  }, []);
  const release = useCallback((owner: symbol) => { if (source.current?.owner === owner) stopExternal(); }, [stopExternal]);
  const close = useCallback((restoreFocus = true) => {
    openEpoch.current++; stopExternal(); setState(null);
    const previous = focus.current; focus.current = null;
    if (restoreFocus) queueMicrotask(() => { if (previous?.isConnected) previous.focus({ preventScroll: true }); });
  }, [stopExternal]);
  const pauseTimeline = useCallback(async () => {
    const phase = controller.getSnapshot().scene?.audioPreview.phase;
    if (phase === "unconfirmed") throw Error(c("previewUnconfirmed"));
    if (!["loading", "playing", "stalled"].includes(phase ?? "")) return;
    await controller.scene.audioPreview.pause();
    const paused = controller.getSnapshot().scene?.audioPreview.phase;
    if (paused === "unconfirmed" || paused === "failed") throw Error(c("previewPauseFailed"));
    if (!["loading", "playing", "stalled"].includes(paused ?? "")) return;
    await new Promise<void>((resolve, reject) => {
      let unsubscribe = () => {};
      const timer = setTimeout(() => { unsubscribe(); reject(Error(c("previewPauseFailed"))); }, 10000);
      unsubscribe = controller.subscribe(() => {
        const phase = controller.getSnapshot().scene?.audioPreview.phase;
        if (["loading", "playing", "stalled"].includes(phase ?? "")) return;
        clearTimeout(timer); unsubscribe();
        phase === "unconfirmed" || phase === "failed" ? reject(Error(c("previewPauseFailed"))) : resolve();
      });
    });
  }, [controller, c("previewPauseFailed"), c("previewUnconfirmed")]);
  const open = useCallback(async (value: PlayerPreview) => {
    const attempt = ++openEpoch.current, originalScope = liveScope.current;
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const view = controller.getSnapshot().scene;
    const snapshot = view?.historical ?? view?.head;
    if (value.kind === "source" && (value.projectId !== snapshot?.project.id || value.revision !== snapshot?.revision)) return;
    stopExternal();
    focus.current = previousFocus;
    setState({ scope: originalScope, value });
    try {
      await pauseTimeline();
      if (attempt !== openEpoch.current || originalScope !== liveScope.current) return;
    } catch (error) { if (attempt === openEpoch.current && originalScope === liveScope.current) controller.report(error); }
  }, [controller, pauseTimeline, stopExternal]);
  const begin = useCallback(async (owner: symbol, pause: () => void) => {
    if (source.current?.owner !== owner) stopExternal();
    const attempt = ++audioEpoch.current, originalScope = liveScope.current;
    source.current = { owner, pause };
    try {
      await pauseTimeline();
      return attempt === audioEpoch.current && originalScope === liveScope.current && source.current?.owner === owner;
    } catch (error) { if (attempt === audioEpoch.current) { stopExternal(); controller.report(error); } return false; }
  }, [controller, pauseTimeline, stopExternal]);
  useLayoutEffect(() => { close(false); }, [scope, close]);
  useEffect(() => () => { openEpoch.current++; stopExternal(); }, [stopExternal]);
  const preview = state?.scope === scope ? state.value : null;
  const actions = sceneShortcutActions(controller);
  useShortcutActions({
    "toggle-play": preview ? { enabled: false, run: () => {} } : actions["toggle-play"] && { ...actions["toggle-play"], run: () => { stopExternal(); return actions["toggle-play"]!.run(); } },
    "stop-playback": preview ? { enabled: false, run: () => {} } : actions["stop-playback"] && { ...actions["stop-playback"], run: () => { stopExternal(); return actions["stop-playback"]!.run(); } },
    ...(preview ? { "cancel-interaction": { run: () => close() } } : {}),
  }, { priority: 0 });
  useEffect(() => {
    if (!preview) return;
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented || document.querySelector("dialog[open]") || (event.target as Element)?.closest?.("input,textarea,select,[contenteditable=true]")) return;
      event.preventDefault(); event.stopPropagation(); close();
    };
    document.addEventListener("keydown", escape);
    return () => document.removeEventListener("keydown", escape);
  }, [preview, close]);
  return <PlayerContext.Provider value={{ preview, open, close, begin, release, stopExternal }}>{children}</PlayerContext.Provider>;
}

export function WorkbenchPlayer({ children }: { children: ReactNode }) {
  const player = usePlayerContext(), c = useResourceCopy(), root = useRef<HTMLDivElement>(null);
  const preview = player?.preview;
  useEffect(() => { if (preview) root.current?.focus({ preventScroll: true }); }, [preview]);
  return <div className="workbench-player-context">
    <div className="workbench-timeline-player" aria-hidden={!!preview} inert={!!preview}>{children}</div>
    {preview && <div className="workbench-resource-player" ref={root} tabIndex={0} data-shortcut-scope="player" aria-label={c(preview.kind === "source" ? "sourceContext" : "effectContext")}>
      <div className="workbench-context-heading">
        <Button onClick={() => player.close()} icon={<ArrowLeft size={15} />}>{c("returnTimeline")}</Button>
        <strong title={preview.kind === "source" ? preview.asset.name : preview.name}>{c(preview.kind === "source" ? "sourceContext" : "effectContext")} · {preview.kind === "source" ? preview.asset.name : preview.name}</strong>
      </div>
      {preview.kind === "source" ? <ResourcePreview key={`${preview.projectId}:${preview.revision}:${preview.asset.id}`} {...preview} embedded onClose={() => player.close(false)} /> : <div className="workbench-effect-example">
        {preview.kind === "effect" ? <EffectExample intensity={preview.intensity} name={preview.name} /> : <TransitionExample seconds={preview.seconds} />}
        <p className="muted">{c("sample")}</p>
      </div>}
    </div>}
  </div>;
}
