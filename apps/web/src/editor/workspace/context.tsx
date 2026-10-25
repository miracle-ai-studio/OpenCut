import {
  createContext,
  useContext,
  useEffect,
  useState,
  useMemo,
  useSyncExternalStore,
} from "react";
import type { ReactNode } from "react";
import { WorkspaceController } from "./controller";
import type { WorkspaceState } from "./controller";
const Context = createContext<WorkspaceController | null>(null);
export function WorkspaceProvider({
  children,
  controller: provided,
}: {
  children: ReactNode;
  controller?: WorkspaceController;
}) {
  const [controller] = useState(() => provided ?? new WorkspaceController());
  useEffect(() => {
    if (!provided) void controller.start();
    const timer = window.setInterval(() => void controller.sync(), 2000);
    return () => window.clearInterval(timer);
  }, [controller, provided]);
  return <Context.Provider value={controller}>{children}</Context.Provider>;
}
export function useWorkspace() {
  const controller = useContext(Context);
  if (!controller) throw new Error("WorkspaceProvider missing");
  const state = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
  );
  return { controller, ...state };
}

/** Command owners do not subscribe to the playback cursor. */
export function useWorkspaceController() {
  const controller = useContext(Context);
  if (!controller) throw new Error("WorkspaceProvider missing");
  return controller;
}

const workspaceIdentity = (state:WorkspaceState) => state;
/** Editing views do not depend on continuous device counters. Paused seeks,
 * phase/owner/generation changes and every authored/selection fact still notify.
 * Event handlers that use the playing cursor read it from the controller. */
export function sameEditingWorkspace(a:WorkspaceState,b:WorkspaceState) {
  if(a===b)return true;
  for(const key of new Set([...Object.keys(a),...Object.keys(b)]) as Set<keyof WorkspaceState>)
    if(key!=="scene"&&!Object.is(a[key],b[key]))return false;
  const x=a.scene,y=b.scene;
  if(x===y)return true;
  if(!x||!y)return false;
  const moving=["playing","stalled"].includes(x.audioPreview.phase)&&x.audioPreview.phase===y.audioPreview.phase;
  for(const key of new Set([...Object.keys(x),...Object.keys(y)]) as Set<keyof typeof x>) {
    if(key==="playhead"&&moving)continue;
    if(key!=="audioPreview"&&!Object.is(x[key],y[key]))return false;
  }
  for(const key of new Set([...Object.keys(x.audioPreview),...Object.keys(y.audioPreview)]) as Set<keyof typeof x.audioPreview>) {
    if(moving&&["session","deviceUnderflowEvents","deviceUnderflowFrames"].includes(key))continue;
    if(!Object.is(x.audioPreview[key],y.audioPreview[key]))return false;
  }
  const s=x.audioPreview.session,t=y.audioPreview.session;
  return !moving||s===t||!!s&&!!t&&s.session_id===t.session_id&&s.generation===t.generation&&
    s.overlay_epoch===t.overlay_epoch&&s.owner_id===t.owner_id&&s.lease_id===t.lease_id;
}
export function useEditingWorkspace() {
  const {controller,selected}=useWorkspaceSelection(workspaceIdentity,sameEditingWorkspace);
  return {controller,...selected};
}

/** Subscribe to stable presentation facts without duplicating workspace state.
 * Controllers still inspect the current authoritative snapshot for every action. */
export function useWorkspaceSelection<T>(
  select: (state: ReturnType<WorkspaceController["getSnapshot"]>) => T,
  equal: (a: T, b: T) => boolean,
) {
  const controller = useContext(Context);
  if (!controller) throw new Error("WorkspaceProvider missing");
  const snapshot = useMemo(() => {
    let value = select(controller.getSnapshot());
    return () => {
      const next = select(controller.getSnapshot());
      if (!equal(value, next)) value = next;
      return value;
    };
  }, [controller, select, equal]);
  return { controller, selected: useSyncExternalStore(controller.subscribe, snapshot) };
}
