import {
  ACTIONS,
  type ShortcutActionId,
  type ShortcutBindings,
} from "./catalog";

export type ShortcutPlatform = "apple" | "other";
export const shortcutPlatform = (): ShortcutPlatform =>
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad|iPod/.test(navigator.platform)
    ? "apple"
    : "other";
const keys = new Set([
  ..."abcdefghijklmnopqrstuvwxyz0123456789",
  "left",
  "right",
  "up",
  "down",
  "home",
  "end",
  "pageup",
  "pagedown",
  "space",
  "enter",
  "escape",
  "backspace",
  "delete",
  "tab",
  "/",
  ".",
  ",",
  ";",
  "'",
  "[",
  "]",
  "-",
  "=",
  "`",
  "\\",
  ...Array.from({ length: 24 }, (_, i) => `f${i + 1}`),
]);
const modifiers = ["mod", "alt", "shift"] as const;
const names: Record<string, string> = {
  " ": "space",
  spacebar: "space",
  arrowleft: "left",
  arrowright: "right",
  arrowup: "up",
  arrowdown: "down",
  esc: "escape",
};
const codes: Record<string, string> = {
  Space: "space",
  Slash: "/",
  Period: ".",
  Comma: ",",
  Semicolon: ";",
  Quote: "'",
  BracketLeft: "[",
  BracketRight: "]",
  Minus: "-",
  Equal: "=",
  Backquote: "`",
  Backslash: "\\",
  NumpadEnter: "enter",
};

export function normalizeChord(value: string): string | null {
  if (value.length > 48) return null;
  const parts = value.toLowerCase().split("+");
  const key = parts.pop();
  if (
    !key ||
    !keys.has(key) ||
    parts.some(
      (part) => !modifiers.includes(part as (typeof modifiers)[number]),
    ) ||
    new Set(parts).size !== parts.length
  )
    return null;
  return [
    ...modifiers.filter((modifier) => parts.includes(modifier)),
    key,
  ].join("+");
}

/** Physical codes match Classic on AZERTY/QWERTZ and avoid Option-generated
 * characters changing a binding. AltGraph, IME and the other platform modifier
 * are never treated as editor shortcuts. */
export function chordFromEvent(
  event: KeyboardEvent,
  platform: ShortcutPlatform,
): string | null {
  if (
    event.isComposing ||
    event.keyCode === 229 ||
    event.getModifierState?.("AltGraph")
  )
    return null;
  if (platform === "apple" ? event.ctrlKey : event.metaKey) return null;
  const raw = event.key.toLowerCase();
  const key = /^Key[A-Z]$/.test(event.code)
    ? event.code.slice(3).toLowerCase()
    : /^Digit[0-9]$/.test(event.code)
      ? event.code.slice(5)
      : (codes[event.code] ?? names[raw] ?? raw);
  if (!keys.has(key)) return null;
  return [
    (platform === "apple" ? event.metaKey : event.ctrlKey) ? "mod" : "",
    event.altKey ? "alt" : "",
    event.shiftKey ? "shift" : "",
    key,
  ]
    .filter(Boolean)
    .join("+");
}

export function reservedReason(
  chord: string,
  action?: ShortcutActionId,
): string | null {
  const parts = chord.split("+"),
    key = parts.at(-1)!;
  if (key === "tab") return "Tab is reserved for moving keyboard focus.";
  if (key === "escape" && action !== "cancel-interaction")
    return "Escape is reserved for cancelling interactions.";
  if (
    ["f1", "f3", "f5", "f6", "f10", "f11", "f12"].includes(key) ||
    (parts.includes("mod") &&
      ([
        "q",
        "w",
        "t",
        "n",
        "r",
        "l",
        "o",
        "p",
        "s",
        "h",
        "m",
        "f",
        "g",
        "e",
        "b",
        "u",
        "k",
        "space",
        "tab",
        "enter",
      ].includes(key) ||
        /^[0-9]$/.test(key) ||
        (parts.includes("shift") &&
          ["i", "j", "c", "delete", "escape"].includes(key)) ||
        (parts.includes("alt") && ["i", "j", "u", "escape"].includes(key)))) ||
    (parts.includes("alt") &&
      ["left", "right", "home", "f4", "space", "escape"].includes(key))
  )
    return "This shortcut is reserved by the browser or operating system.";
  return null;
}

export function bindingConflict(
  bindings: ShortcutBindings,
  action: ShortcutActionId,
  chord: string,
): ShortcutActionId | null {
  return (
    ACTIONS.find(
      (item) => item.id !== action && bindings[item.id].includes(chord),
    )?.id ?? null
  );
}

const display: Record<string, string> = {
  space: "Space",
  enter: "Enter",
  escape: "Esc",
  backspace: "Backspace",
  delete: "Delete",
  left: "←",
  right: "→",
  up: "↑",
  down: "↓",
  home: "Home",
  end: "End",
  pageup: "Page Up",
  pagedown: "Page Down",
};
export function displayChord(
  chord: string,
  platform: ShortcutPlatform,
): string {
  return chord
    .split("+")
    .map((part) =>
      part === "mod"
        ? platform === "apple"
          ? "⌘"
          : "Ctrl"
        : part === "alt"
          ? platform === "apple"
            ? "⌥"
            : "Alt"
          : part === "shift"
            ? "Shift"
            : (display[part] ?? part.toUpperCase()),
    )
    .join(" + ");
}

export const textOrControlTarget = (target: EventTarget | null): boolean =>
  target instanceof Element &&
  !!target.closest(
    "input,textarea,select,button,a,[contenteditable]:not([contenteditable=false]),[role=textbox],[role=combobox],[role=spinbutton],[role=slider],[role=button],[role=tab],[role=treeitem],[role=menu],[role=menubar],[data-shortcuts-ignore]",
  );
export const modalOpen = (): boolean =>
  !!document.querySelector(
    "dialog[open],[role=dialog][aria-modal=true],[role=alertdialog],[data-shortcuts-modal]",
  );
