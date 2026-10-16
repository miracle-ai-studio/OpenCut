// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import {
  bindingConflict,
  chordFromEvent,
  displayChord,
  normalizeChord,
  reservedReason,
} from "./bindings";
import { defaultBindings } from "./catalog";

describe("Classic shortcut identity", () => {
  it("retains the Classic defaults and physical keys across layouts", () => {
    const bindings = defaultBindings();
    expect(bindings["toggle-play"]).toEqual(["space", "k"]);
    expect(bindings.redo).toEqual(["mod+shift+z", "mod+y"]);
    expect(bindings["toggle-ripple-editing"]).toEqual([]);
    expect(
      chordFromEvent(
        new KeyboardEvent("keydown", { key: "a", code: "KeyQ" }),
        "other",
      ),
    ).toBe("q");
    expect(
      chordFromEvent(
        new KeyboardEvent("keydown", { key: "¿", code: "Slash", altKey: true }),
        "apple",
      ),
    ).toBe("alt+/");
    expect(
      chordFromEvent(
        new KeyboardEvent("keydown", {
          key: "?",
          code: "Slash",
          shiftKey: true,
        }),
        "apple",
      ),
    ).toBe("shift+/");
  });
  it("uses only the platform's primary modifier and refuses IME or AltGraph", () => {
    expect(
      chordFromEvent(
        new KeyboardEvent("keydown", { key: "z", code: "KeyZ", metaKey: true }),
        "apple",
      ),
    ).toBe("mod+z");
    expect(
      chordFromEvent(
        new KeyboardEvent("keydown", { key: "z", code: "KeyZ", ctrlKey: true }),
        "apple",
      ),
    ).toBeNull();
    expect(
      chordFromEvent(
        new KeyboardEvent("keydown", { key: "z", code: "KeyZ", metaKey: true }),
        "other",
      ),
    ).toBeNull();
    expect(
      chordFromEvent(
        new KeyboardEvent("keydown", { key: "z", code: "KeyZ", ctrlKey: true }),
        "other",
      ),
    ).toBe("mod+z");
    expect(
      chordFromEvent(
        new KeyboardEvent("keydown", {
          key: "z",
          code: "KeyZ",
          isComposing: true,
        }),
        "apple",
      ),
    ).toBeNull();
    const altGraph = new KeyboardEvent("keydown", {
      key: "q",
      code: "KeyQ",
      ctrlKey: true,
      altKey: true,
    });
    Object.defineProperty(altGraph, "getModifierState", {
      value: (name: string) => name === "AltGraph",
    });
    expect(chordFromEvent(altGraph, "other")).toBeNull();
  });
  it("normalizes modifier order, rejects unknown combinations and reports reserved keys", () => {
    expect(normalizeChord("shift+mod+Z")).toBe("mod+shift+z");
    for (const value of [
      "mod+mod+z",
      "ctrl+z",
      "meta+z",
      "shift",
      "mod+",
      "x+y",
      "hyper+s",
    ])
      expect(normalizeChord(value)).toBeNull();
    for (const key of [
      "mod+w",
      "mod+space",
      "alt+f4",
      "tab",
      "mod+shift+t",
      "f12",
    ])
      expect(reservedReason(key, "split")).toBeTruthy();
    expect(reservedReason("escape", "split")).toBeTruthy();
    expect(reservedReason("escape", "cancel-interaction")).toBeNull();
    expect(reservedReason("mod+d", "duplicate-selected")).toBeNull();
    expect(bindingConflict(defaultBindings(), "split", "q")).toBe("split-left");
    expect(displayChord("mod+shift+z", "apple")).toBe("⌘ + Shift + Z");
    expect(displayChord("mod+shift+z", "other")).toBe("Ctrl + Shift + Z");
  });
});
