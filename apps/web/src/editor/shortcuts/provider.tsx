import {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import {
  actionDefinition,
  defaultBindings,
  type ShortcutActionId,
  type ShortcutBindings,
} from "./catalog";
import {
  bindingConflict,
  displayChord,
  normalizeChord,
  reservedReason,
  shortcutPlatform,
  type ShortcutPlatform,
} from "./bindings";
import { ShortcutRegistry, type ShortcutHandlers } from "./registry";
import {
  browserShortcutStorage,
  parseShortcutConfig,
  readShortcutPreferences,
  serializeShortcutConfig,
  shortcutStorageEventIsRelevant,
  type ShortcutStorage,
} from "./persistence";
import { attachShortcutDispatcher } from "./dispatcher";
import { ShortcutSettingsDialog } from "./dialog";

export type ShortcutContextValue = {
  registry: ShortcutRegistry;
  platform: ShortcutPlatform;
  bindings: ShortcutBindings;
  notice: string | null;
  setBinding(action: ShortcutActionId, key: string, replace?: string): void;
  clearBinding(action: ShortcutActionId, key?: string): void;
  reset(action?: ShortcutActionId): void;
  importConfig(text: string): void;
  exportConfig(): string;
  invoke(action: ShortcutActionId): Promise<boolean>;
  availability: ShortcutRegistry["availability"];
  label(action: ShortcutActionId): string;
  openHelp(): void;
};
const Context = createContext<ShortcutContextValue | null>(null);

export function EditorShortcutsProvider({
  children,
  onError,
  storage = browserShortcutStorage,
  platform = shortcutPlatform(),
}: {
  children: ReactNode;
  onError?(error: unknown): void;
  storage?: ShortcutStorage;
  platform?: ShortcutPlatform;
}) {
  const [registry] = useState(() => new ShortcutRegistry());
  const [preferences, setPreferences] = useState(() =>
    readShortcutPreferences(storage),
  );
  const [help, setHelp] = useState(false);
  const live = useRef({ preferences, onError });
  live.current = { preferences, onError };
  const report = (error: unknown) => {
    live.current.onError?.(error);
    setPreferences((previous) => ({
      ...previous,
      notice: error instanceof Error ? error.message : String(error),
    }));
  };
  useEffect(
    () =>
      attachShortcutDispatcher({
        registry,
        bindings: () => live.current.preferences.bindings,
        platform,
        onError: report,
      }),
    [registry, platform],
  );
  useEffect(
    () =>
      registry.register(Symbol("shortcut-help"), () => ({
        "show-shortcuts": { run: () => setHelp(true) },
      })),
    [registry],
  );
  useEffect(() => {
    const changed = (event: StorageEvent) => {
      if (
        storage !== browserShortcutStorage ||
        !shortcutStorageEventIsRelevant(event.key)
      )
        return;
      setPreferences(readShortcutPreferences(storage));
    };
    window.addEventListener("storage", changed);
    return () => window.removeEventListener("storage", changed);
  }, [storage]);
  const publish = (bindings: ShortcutBindings) => {
    // Validate before both runtime publication and persistence. A failed write
    // leaves valid session preferences active with an explicit unsaved notice.
    const text = serializeShortcutConfig(bindings),
      valid = parseShortcutConfig(text);
    let notice: string | null = null;
    try {
      storage.write(text);
    } catch {
      notice =
        "Shortcuts changed for this session only. Browser storage is unavailable.";
    }
    const next = { bindings: valid, notice };
    live.current = { ...live.current, preferences: next };
    setPreferences(next);
  };
  const value: ShortcutContextValue = {
    registry,
    platform,
    ...preferences,
    setBinding(action, raw, replace) {
      const chord = normalizeChord(raw);
      if (!chord)
        throw new Error(
          "Choose a key with optional Command / Control, Alt and Shift modifiers.",
        );
      const reason = reservedReason(chord, action);
      if (reason) throw new Error(reason);
      const current = live.current.preferences.bindings;
      const conflict = bindingConflict(current, action, chord);
      if (conflict)
        throw new Error(
          `Shortcut conflict: ${actionDefinition(conflict).label}`,
        );
      const next = current[action].filter((key) => key !== replace);
      if (!next.includes(chord)) next.push(chord);
      if (next.length > 4)
        throw new Error("Each action supports up to four shortcuts.");
      publish({ ...current, [action]: next });
    },
    clearBinding(action, key) {
      const current = live.current.preferences.bindings;
      publish({
        ...current,
        [action]: key ? current[action].filter((value) => value !== key) : [],
      });
    },
    reset(action) {
      if (!action) {
        publish(defaultBindings());
        return;
      }
      const current = live.current.preferences.bindings,
        defaults = defaultBindings();
      for (const key of defaults[action]) {
        const conflict = bindingConflict(current, action, key);
        if (conflict)
          throw new Error(
            `Shortcut conflict: ${actionDefinition(conflict).label}`,
          );
      }
      publish({ ...current, [action]: defaults[action] });
    },
    importConfig(text) {
      // A rejected import never replaces the currently working preferences.
      publish(parseShortcutConfig(text));
    },
    exportConfig: () =>
      serializeShortcutConfig(live.current.preferences.bindings),
    invoke: (action) =>
      registry.invoke(action).catch((error) => {
        report(error);
        throw error;
      }),
    availability: (action) => registry.availability(action),
    label: (action) =>
      preferences.bindings[action]
        .map((chord) => displayChord(chord, platform))
        .join(" / "),
    openHelp: () => setHelp(true),
  };
  return (
    <Context.Provider value={value}>
      {children}
      {help && <ShortcutSettingsDialog onClose={() => setHelp(false)} />}
    </Context.Provider>
  );
}

export function useShortcuts() {
  const context = useContext(Context);
  if (!context) throw new Error("EditorShortcutsProvider missing");
  useSyncExternalStore(
    context.registry.subscribe,
    context.registry.snapshot,
    context.registry.snapshot,
  );
  return context;
}

/** Button tooltips can use the configured key without requiring a provider in
 * isolated component previews. The mounted editor always supplies one. */
export function useShortcutLabels() {
  const context = useContext(Context);
  return (action: ShortcutActionId) =>
    context
      ? context.label(action)
      : defaultBindings()
          [action].map((key) => displayChord(key, shortcutPlatform()))
          .join(" / ");
}

/** This hook can coexist with standalone component tests. A provider activates
 * registrations; handlers always see the component's latest committed props. */
export function useShortcutActions(
  actions: ShortcutHandlers,
  options: { priority?: number } = {},
) {
  const context = useContext(Context);
  const live = useRef(actions);
  useLayoutEffect(() => {
    live.current = actions;
    context?.registry.refresh();
  });
  useEffect(
    () =>
      context?.registry.register(
        Symbol("shortcut-actions"),
        () => live.current,
        options.priority,
      ),
    [context?.registry, options.priority],
  );
}
