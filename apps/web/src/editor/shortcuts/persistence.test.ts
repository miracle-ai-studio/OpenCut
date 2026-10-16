import { expect, it, vi } from "vitest";
import { defaultBindings } from "./catalog";
import {
  MAX_SHORTCUT_CONFIG_BYTES,
  parseShortcutConfig,
  readShortcutPreferences,
  serializeShortcutConfig,
} from "./persistence";

it("round-trips clears and customized keys without project or history fields", () => {
  const bindings = defaultBindings();
  bindings.split = ["alt+s"];
  bindings["split-left"] = [];
  const json = serializeShortcutConfig(bindings);
  expect(parseShortcutConfig(json)).toEqual(bindings);
  expect(Object.keys(JSON.parse(json))).toEqual(["schema_version", "bindings"]);
  const write = vi.fn();
  expect(readShortcutPreferences({ read: () => json, write })).toEqual({
    bindings,
    notice: null,
  });
  expect(write).not.toHaveBeenCalled();
});

it.each([
  "syntax",
  "version",
  "missing",
  "unknown",
  "conflict",
  "reserved",
  "oversize",
  "duplicates",
  "wrong-shape",
  "unknown-field",
])(
  "refuses %s and falls back without overwriting saved preferences",
  (kind) => {
    const config: Record<string, unknown> = {
      schema_version: 1,
      bindings: defaultBindings(),
    };
    const bindings = config.bindings as Record<string, unknown>;
    if (kind === "version") config.schema_version = 8;
    if (kind === "missing") delete bindings.split;
    if (kind === "unknown") {
      delete bindings.split;
      bindings["secret-operation"] = ["s"];
    }
    if (kind === "conflict") bindings.split = ["q"];
    if (kind === "reserved") bindings.split = ["mod+w"];
    if (kind === "duplicates") bindings.split = ["s", "s"];
    if (kind === "wrong-shape") bindings.split = {};
    if (kind === "unknown-field")
      config.project = { id: "must-never-be-saved" };
    const text =
      kind === "syntax"
        ? "{"
        : kind === "oversize"
          ? " ".repeat(MAX_SHORTCUT_CONFIG_BYTES + 1)
          : JSON.stringify(config);
    expect(() => parseShortcutConfig(text)).toThrow();
    const write = vi.fn();
    const result = readShortcutPreferences({ read: () => text, write });
    expect(result.bindings).toEqual(defaultBindings());
    expect(result.notice).toMatch(/Default keys/);
    expect(write).not.toHaveBeenCalled();
  },
);

it("storage denial is recoverable and does not hide that defaults are active", () => {
  const result = readShortcutPreferences({
    read: () => {
      throw new DOMException("denied", "SecurityError");
    },
    write: vi.fn(),
  });
  expect(result.bindings).toEqual(defaultBindings());
  expect(result.notice).toBeTruthy();
});
