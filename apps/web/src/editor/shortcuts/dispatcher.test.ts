// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { attachShortcutDispatcher } from "./dispatcher";
import { defaultBindings } from "./catalog";
import { ShortcutRegistry } from "./registry";

const cleanups: (() => void)[] = [];
afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup());
  document.body.innerHTML = "";
});
const key = (
  target: Element,
  value: string,
  code: string,
  extra: KeyboardEventInit = {},
) => {
  const event = new KeyboardEvent("keydown", {
    key: value,
    code,
    bubbles: true,
    cancelable: true,
    ...extra,
  });
  target.dispatchEvent(event);
  return event;
};
const release = (target: Element, value: string, code: string) =>
  target.dispatchEvent(
    new KeyboardEvent("keyup", { key: value, code, bubbles: true }),
  );
function setup() {
  document.body.innerHTML =
    '<div class="editor-app"><div class="v4-timeline-panel" tabindex="0"><input><button>Native</button><div contenteditable="true"><span>Text</span></div></div><div class="canvas" tabindex="0"></div></div>';
  const timeline = document.querySelector(".v4-timeline-panel")!;
  const registry = new ShortcutRegistry(),
    bindings = defaultBindings(),
    errors = vi.fn();
  let now = 0;
  cleanups.push(
    attachShortcutDispatcher({
      registry,
      bindings: () => bindings,
      platform: "apple",
      onError: errors,
      now: () => now,
    }),
  );
  return {
    timeline,
    registry,
    bindings,
    errors,
    advance: (ms: number) => {
      now += ms;
    },
  };
}

it("executes the registered controller closure once per physical press and swallows disabled legacy fallthrough", async () => {
  const f = setup(),
    split = vi.fn(),
    legacy = vi.fn();
  f.registry.register(Symbol(), () => ({ split: { run: split } }));
  f.timeline.addEventListener("keydown", legacy);
  expect(key(f.timeline, "s", "KeyS").defaultPrevented).toBe(true);
  await Promise.resolve();
  key(f.timeline, "s", "KeyS", { repeat: true });
  key(f.timeline, "s", "KeyS");
  expect(split).toHaveBeenCalledTimes(1);
  expect(legacy).not.toHaveBeenCalled();
  release(f.timeline, "s", "KeyS");
  await Promise.resolve();
  key(f.timeline, "s", "KeyS");
  expect(split).toHaveBeenCalledTimes(2);
  release(f.timeline, "s", "KeyS");
  f.registry.register(Symbol(), () => ({
    split: { run: split, enabled: false },
  }));
  key(f.timeline, "s", "KeyS");
  expect(split).toHaveBeenCalledTimes(2);
  expect(legacy).not.toHaveBeenCalled();
});

it("lets inputs, controls, IME and native modals own their keys, and requires the declared panel scope", () => {
  const f = setup(),
    split = vi.fn();
  f.registry.register(Symbol(), () => ({ split: { run: split } }));
  for (const target of [
    f.timeline.querySelector("input")!,
    f.timeline.querySelector("button")!,
    f.timeline.querySelector("span")!,
    document.querySelector(".canvas")!,
  ]) {
    expect(key(target, "s", "KeyS").defaultPrevented).toBe(false);
    release(target, "s", "KeyS");
  }
  const dialog = document.createElement("dialog");
  dialog.setAttribute("open", "");
  document.body.append(dialog);
  key(f.timeline, "s", "KeyS");
  release(f.timeline, "s", "KeyS");
  dialog.remove();
  f.timeline.dispatchEvent(new Event("compositionstart", { bubbles: true }));
  key(f.timeline, "s", "KeyS");
  release(f.timeline, "s", "KeyS");
  f.timeline.dispatchEvent(new Event("compositionend", { bubbles: true }));
  key(f.timeline, "s", "KeyS", { isComposing: true });
  release(f.timeline, "s", "KeyS");
  expect(split).not.toHaveBeenCalled();
  key(f.timeline, "s", "KeyS");
  expect(split).toHaveBeenCalledTimes(1);
});

it("bounds repeat navigation, drops in-flight repeats and clears held state after window blur", async () => {
  const f = setup();
  let resolve!: () => void;
  const next = vi.fn(
    () =>
      new Promise<void>((done) => {
        resolve = done;
      }),
  );
  f.registry.register(Symbol(), () => ({
    "frame-step-forward": { run: next },
  }));
  key(f.timeline, "ArrowRight", "ArrowRight");
  f.advance(40);
  key(f.timeline, "ArrowRight", "ArrowRight", { repeat: true });
  expect(next).toHaveBeenCalledTimes(1);
  resolve();
  await Promise.resolve();
  await Promise.resolve();
  f.advance(40);
  key(f.timeline, "ArrowRight", "ArrowRight", { repeat: true });
  expect(next).toHaveBeenCalledTimes(2);
  resolve();
  await Promise.resolve();
  await Promise.resolve();
  f.advance(1);
  key(f.timeline, "ArrowRight", "ArrowRight", { repeat: true });
  expect(next).toHaveBeenCalledTimes(2);
  window.dispatchEvent(new Event("blur"));
  key(f.timeline, "ArrowRight", "ArrowRight");
  expect(next).toHaveBeenCalledTimes(3);
  resolve();
});

it("preserves real native paste only for its assigned default chord and calls the same port for custom keys", async () => {
  const f = setup(),
    paste = vi.fn();
  f.registry.register(Symbol(), () => ({
    "paste-copied": { run: paste, nativePaste: true },
  }));
  expect(key(f.timeline, "v", "KeyV", { metaKey: true }).defaultPrevented).toBe(
    false,
  );
  expect(paste).not.toHaveBeenCalled();
  expect(
    key(f.timeline, "v", "KeyV", { metaKey: true, repeat: true })
      .defaultPrevented,
  ).toBe(true);
  release(f.timeline, "v", "KeyV");
  f.bindings["paste-copied"] = ["alt+p"];
  expect(key(f.timeline, "v", "KeyV", { metaKey: true }).defaultPrevented).toBe(
    true,
  );
  key(f.timeline, "p", "KeyP", { altKey: true });
  expect(paste).toHaveBeenCalledTimes(1);
});

it("does not let global deselection steal Escape from an active canvas gesture", () => {
  const f = setup(),
    cancel = vi.fn();
  f.registry.register(Symbol(), () => ({
    "cancel-interaction": { run: cancel },
  }));
  const local = (event: Event) => {
    event.preventDefault();
    event.stopPropagation();
  };
  f.timeline.addEventListener("keydown", local);
  key(f.timeline, "Escape", "Escape");
  expect(cancel).not.toHaveBeenCalled();
  f.timeline.removeEventListener("keydown", local);
  release(f.timeline, "Escape", "Escape");
  key(f.timeline, "Escape", "Escape");
  expect(cancel).toHaveBeenCalledTimes(1);
});

it("customized keys replace defaults and rejected operations release the registry lock", async () => {
  const f = setup(),
    error = new Error("fixed revision changed");
  const split = vi
    .fn()
    .mockRejectedValueOnce(error)
    .mockResolvedValue(undefined);
  f.registry.register(Symbol(), () => ({ split: { run: split } }));
  f.bindings.split = ["alt+s"];
  key(f.timeline, "s", "KeyS");
  expect(split).not.toHaveBeenCalled();
  release(f.timeline, "s", "KeyS");
  key(f.timeline, "s", "KeyS", { altKey: true });
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
  expect(f.errors).toHaveBeenCalledWith(error);
  release(f.timeline, "s", "KeyS");
  key(f.timeline, "s", "KeyS", { altKey: true });
  expect(split).toHaveBeenCalledTimes(2);
});

it("held view tools release on keyup, modifier release, blur and teardown without repeated starts", async () => {
  const f = setup(),
    start = vi.fn(),
    end = vi.fn();
  const canvas = document.querySelector(".canvas")!;
  canvas.setAttribute("data-shortcut-scope", "canvas");
  f.registry.register(Symbol(), () => ({
    "pan-preview": { run: start, release: end },
  }));
  key(canvas, "h", "KeyH");
  await Promise.resolve();
  key(canvas, "h", "KeyH", { repeat: true });
  expect(start).toHaveBeenCalledTimes(1);
  release(canvas, "h", "KeyH");
  expect(end).toHaveBeenCalledTimes(1);
  await Promise.resolve();
  key(canvas, "h", "KeyH");
  window.dispatchEvent(new Event("blur"));
  expect(end).toHaveBeenCalledTimes(2);
  await Promise.resolve();
  f.bindings["pan-preview"] = ["alt+h"];
  key(canvas, "h", "KeyH", { altKey: true });
  release(canvas, "Alt", "AltLeft");
  expect(end).toHaveBeenCalledTimes(3);
  release(canvas, "h", "KeyH");
  await Promise.resolve();
  key(canvas, "h", "KeyH", { altKey: true });
  f.timeline
    .querySelector("input")!
    .dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
  expect(end).toHaveBeenCalledTimes(4);
  release(canvas, "h", "KeyH");
  await Promise.resolve();
  key(canvas, "h", "KeyH", { altKey: true });
  cleanups.splice(0).forEach((cleanup) => cleanup());
  expect(end).toHaveBeenCalledTimes(5);
});

it("local interactions override the broad fallback independently of mount order", async () => {
  const registry = new ShortcutRegistry(),
    local = vi.fn(),
    fallback = vi.fn();
  const leave = registry.register(Symbol(), () => ({
    "cancel-interaction": { run: local },
  }));
  registry.register(
    Symbol(),
    () => ({ "cancel-interaction": { run: fallback } }),
    -10,
  );
  await registry.invoke("cancel-interaction");
  expect(local).toHaveBeenCalledTimes(1);
  expect(fallback).not.toHaveBeenCalled();
  leave();
  await registry.invoke("cancel-interaction");
  expect(fallback).toHaveBeenCalledTimes(1);
});
