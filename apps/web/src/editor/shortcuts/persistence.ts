import {
  ACTIONS,
  defaultBindings,
  isShortcutAction,
  type ShortcutBindings,
} from "./catalog";
import { normalizeChord, reservedReason } from "./bindings";

export const SHORTCUT_STORAGE_KEY = "opencut.shortcuts.v1";
export const MAX_SHORTCUT_CONFIG_BYTES = 48 * 1024;
export type ShortcutStorage = {
  read(): string | null;
  write(value: string): void;
};
// Only bounded action names and physical chords. No project, media, connection,
// credentials or edit transactions belong in this preference adapter.
export const browserShortcutStorage: ShortcutStorage = {
  read: () => window.localStorage.getItem("opencut.shortcuts.v1"),
  write: (value) => window.localStorage.setItem("opencut.shortcuts.v1", value),
};
const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === "object" && !Array.isArray(value);
export function parseShortcutConfig(text: string): ShortcutBindings {
  if (new TextEncoder().encode(text).byteLength > MAX_SHORTCUT_CONFIG_BYTES)
    throw new Error("The shortcut configuration is too large.");
  let config: unknown;
  try {
    config = JSON.parse(text);
  } catch {
    throw new Error(
      "The shortcut configuration is invalid or has an unsupported version.",
    );
  }
  if (
    !object(config) ||
    Object.keys(config).sort().join(",") !== "bindings,schema_version" ||
    config.schema_version !== 1 ||
    !object(config.bindings)
  )
    throw new Error(
      "The shortcut configuration is invalid or has an unsupported version.",
    );
  const bindings = defaultBindings();
  // Require the complete table. An incomplete or future table cannot silently
  // enable defaults that conflict with a user's existing binding.
  if (
    Object.keys(config.bindings).length !== ACTIONS.length ||
    Object.keys(config.bindings).some((key) => !isShortcutAction(key))
  )
    throw new Error(
      "The shortcut configuration has unknown or missing actions.",
    );
  const assigned = new Set<string>();
  for (const action of ACTIONS) {
    const values = config.bindings[action.id];
    if (!Array.isArray(values) || values.length > 4)
      throw new Error("Each action supports up to four shortcuts.");
    bindings[action.id] = values.map((value) => {
      if (typeof value !== "string" || normalizeChord(value) !== value)
        throw new Error("The shortcut configuration contains an invalid key.");
      const reserved = reservedReason(value, action.id);
      if (reserved) throw new Error(reserved);
      if (assigned.has(value))
        throw new Error(
          "The shortcut configuration contains conflicting keys.",
        );
      assigned.add(value);
      return value;
    });
  }
  return bindings;
}
export const serializeShortcutConfig = (bindings: ShortcutBindings) =>
  JSON.stringify({ schema_version: 1, bindings }, null, 2);
export function readShortcutPreferences(storage: ShortcutStorage): {
  bindings: ShortcutBindings;
  notice: string | null;
} {
  try {
    const saved = storage.read();
    return {
      bindings: saved === null ? defaultBindings() : parseShortcutConfig(saved),
      notice: null,
    };
  } catch {
    return {
      bindings: defaultBindings(),
      notice:
        "Shortcut preferences could not be read. Default keys are active; the saved configuration has not been overwritten.",
    };
  }
}
