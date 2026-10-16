import {
  ACTIONS,
  actionDefinition,
  type ShortcutActionId,
  type ShortcutBindings,
  type ShortcutScope,
} from "./catalog";
import {
  chordFromEvent,
  modalOpen,
  textOrControlTarget,
  type ShortcutPlatform,
} from "./bindings";
import type { ShortcutRegistry } from "./registry";

const scopeOf = (target: Element): ShortcutScope =>
  target.closest(
    '[data-shortcut-scope="timeline"],.v4-timeline,.v4-timeline-panel,.timeline,.timeline-panel,.timeline-area',
  )
    ? "timeline"
    : target.closest(
          '[data-shortcut-scope="assets"],.v4-media-panel,.assets-panel',
        )
      ? "assets"
      : target.closest('[data-shortcut-scope="canvas"],.preview-stage')
        ? "canvas"
        : "workspace";
const rootOf = (target: EventTarget | null) =>
  target instanceof Element
    ? target.closest("[data-shortcuts-root],.editor-app")
    : null;

/** One event owner, no polling, synthetic clicks or project mutation. Repeatable
 * navigation is bounded; all edits/toggles require a real release before reuse. */
export function attachShortcutDispatcher(options: {
  registry: ShortcutRegistry;
  bindings(): ShortcutBindings;
  platform: ShortcutPlatform;
  onError(error: unknown): void;
  target?: Window;
  now?: () => number;
}) {
  const host = options.target ?? window,
    now = options.now ?? (() => performance.now());
  const held = new Set<string>(),
    repeatedAt = new Map<ShortcutActionId, number>();
  const releases = new Map<string, () => void>();
  let composing = false;
  const releaseAll = () => {
    const pending = [...releases.values()];
    releases.clear();
    pending.forEach((release) => release());
  };
  const clear = () => {
    releaseAll();
    held.clear();
    repeatedAt.clear();
    composing = false;
  };
  const keyup = (event: KeyboardEvent) => {
    const physical = event.code || event.key;
    held.delete(physical);
    const release = releases.get(physical);
    releases.delete(physical);
    release?.();
    if (["Meta", "Control", "Alt", "Shift"].includes(event.key)) releaseAll();
  };
  const startComposition = () => {
    releaseAll();
    composing = true;
    held.clear();
  };
  const endComposition = () => {
    composing = false;
    held.clear();
  };
  const keydown = (event: KeyboardEvent) => {
    const physical = event.code || event.key;
    const alreadyHeld = held.has(physical);
    held.add(physical);
    if (
      composing ||
      event.isComposing ||
      event.keyCode === 229 ||
      event.defaultPrevented ||
      !rootOf(event.target) ||
      textOrControlTarget(event.target) ||
      modalOpen()
    )
      return;
    const chord = chordFromEvent(event, options.platform);
    if (!chord) return;
    const bindings = options.bindings();
    const action = ACTIONS.find((item) => bindings[item.id].includes(chord));
    const scope = scopeOf(event.target as Element);
    if (!action || (action.scope && action.scope !== scope)) {
      // A cleared/reassigned native paste key must not fall through to the old
      // timeline paste listener. Inputs and menus were excluded above.
      if (chord === "mod+v" && scope === "timeline") event.preventDefault();
      return;
    }
    const availability = options.registry.availability(action.id);
    if (
      action.id === "paste-copied" &&
      chord === "mod+v" &&
      options.registry.handler(action.id)?.nativePaste &&
      availability.enabled &&
      !alreadyHeld &&
      !event.repeat
    )
      return;
    event.preventDefault();
    // Registered actions replace the old element/window shortcuts. Capture is
    // necessary so a disabled binding cannot invoke a legacy local handler.
    event.stopImmediatePropagation();
    if (
      !availability.enabled ||
      ((alreadyHeld || event.repeat) && !actionDefinition(action.id).repeat)
    )
      return;
    const time = now();
    if (
      (alreadyHeld || event.repeat) &&
      time - (repeatedAt.get(action.id) ?? -Infinity) < 32
    )
      return;
    repeatedAt.set(action.id, time);
    const release = options.registry.handler(action.id)?.release;
    if (action.hold && release) releases.set(physical, release);
    void options.registry.invoke(action.id).catch((error) => {
      releases.get(physical)?.();
      releases.delete(physical);
      options.onError(error);
    });
  };
  // Existing canvas/timeline gestures get first refusal on Escape. They already
  // prevent the event when cancelling a draft; global deselection is a fallback.
  const captureKeydown = (event: KeyboardEvent) => {
    if (event.key !== "Escape") keydown(event);
  };
  const bubbleEscape = (event: KeyboardEvent) => {
    if (event.key === "Escape") keydown(event);
  };
  const hidden = () => {
    if (document.visibilityState !== "visible") clear();
  };
  host.addEventListener("keydown", captureKeydown, true);
  host.addEventListener("keydown", bubbleEscape);
  host.addEventListener("keyup", keyup, true);
  host.addEventListener("blur", clear);
  host.addEventListener("pagehide", clear);
  document.addEventListener("visibilitychange", hidden);
  document.addEventListener("focusin", releaseAll, true);
  document.addEventListener("compositionstart", startComposition, true);
  document.addEventListener("compositionend", endComposition, true);
  return () => {
    clear();
    host.removeEventListener("keydown", captureKeydown, true);
    host.removeEventListener("keydown", bubbleEscape);
    host.removeEventListener("keyup", keyup, true);
    host.removeEventListener("blur", clear);
    host.removeEventListener("pagehide", clear);
    document.removeEventListener("visibilitychange", hidden);
    document.removeEventListener("focusin", releaseAll, true);
    document.removeEventListener("compositionstart", startComposition, true);
    document.removeEventListener("compositionend", endComposition, true);
  };
}
