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
