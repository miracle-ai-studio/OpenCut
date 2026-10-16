// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { preferences } from "../shared/locale";
import {
  EditorShortcutsProvider,
  useShortcutActions,
  useShortcuts,
  type ShortcutContextValue,
} from "./provider";
import { ShortcutsButton } from "./dialog";
import { defaultBindings } from "./catalog";
import { serializeShortcutConfig } from "./persistence";
import type { ReactNode } from "react";

vi.mock("../shared/ui", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../shared/ui")>()),
  Modal: ({ children, title }: { children: ReactNode; title: string }) => (
    <div role="dialog" aria-modal="true">
      <h2>{title}</h2>
      {children}
    </div>
  ),
}));
beforeEach(() => preferences({ language: "en" }));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
let current: ShortcutContextValue;
function Probe({
  run = () => {},
  disabled = false,
}: {
  run?: () => unknown;
  disabled?: boolean;
}) {
  useShortcutActions({
    split: {
      run,
      enabled: !disabled,
      reason: "Finish the current interaction first.",
    },
  });
  current = useShortcuts();
  return (
    <div className="editor-app">
      <div className="v4-timeline-panel" tabIndex={0}>
        Timeline
      </div>
      <ShortcutsButton />
    </div>
  );
}
function setup(saved: string | null = null, failWrites = false) {
  let value = saved;
  const write = vi.fn((text: string) => {
    if (failWrites) throw new Error("quota");
    value = text;
  });
  const storage = { read: () => value, write },
    run = vi.fn();
  const view = render(
    <EditorShortcutsProvider storage={storage} platform="apple">
      <Probe run={run} />
    </EditorShortcutsProvider>,
  );
  return { storage, write, run, view };
}

it("changes, clears, resets and persists preferences independently of action execution", () => {
  const f = setup();
  act(() => current.setBinding("split", "alt+s", "s"));
  expect(current.bindings.split).toEqual(["alt+s"]);
  expect(f.run).not.toHaveBeenCalled();
  f.view.unmount();
  render(
    <EditorShortcutsProvider storage={f.storage} platform="apple">
      <Probe run={f.run} />
    </EditorShortcutsProvider>,
  );
  expect(current.label("split")).toBe("⌥ + S");
  act(() => current.clearBinding("split"));
  expect(current.bindings.split).toEqual([]);
  act(() => current.reset("split"));
  expect(current.bindings.split).toEqual(["s"]);
  act(() => current.reset());
  expect(current.bindings).toEqual(defaultBindings());
  expect(f.run).not.toHaveBeenCalled();
});

it("never silently replaces another action or accepts a conflicting per-action reset", () => {
  const f = setup();
  expect(() => current.setBinding("split", "q", "s")).toThrow(/conflict/);
  expect(f.write).not.toHaveBeenCalled();
  expect(current.bindings.split).toEqual(["s"]);
  act(() => current.clearBinding("split"));
  act(() => current.setBinding("split-left", "s", "q"));
  expect(() => current.reset("split")).toThrow(/conflict/);
  expect(current.bindings.split).toEqual([]);
  expect(current.bindings["split-left"]).toEqual(["s"]);
});

it("keeps working session bindings on invalid import and explains unavailable persistent storage", () => {
  setup(null, true);
  act(() => current.setBinding("split", "alt+s", "s"));
  expect(current.notice).toMatch(/session only/);
  expect(() => current.importConfig("{}")).toThrow();
  expect(current.bindings.split).toEqual(["alt+s"]);
  const next = defaultBindings();
  next.split = [];
  act(() => current.importConfig(serializeShortcutConfig(next)));
  expect(current.bindings.split).toEqual([]);
});

it("shows the actual registry and records conflicts and system reservations without executing an edit", () => {
  const f = setup();
  fireEvent.click(
    screen.getByRole("button", { name: "Shortcuts and settings" }),
  );
  const row = document.querySelector(
    '[data-shortcut-action="split"]',
  )! as HTMLElement;
  fireEvent.click(
    within(row).getByRole("button", {
      name: "Split elements at playhead: Change S",
    }),
  );
  const recorder = screen.getByRole("textbox", { name: "Record a shortcut" });
  fireEvent.keyDown(recorder, { key: "q", code: "KeyQ" });
  expect(screen.getByRole("alert").textContent).toContain(
    "Split and remove left",
  );
  expect(
    (screen.getByRole("button", { name: "Save shortcut" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  fireEvent.keyDown(recorder, { key: "w", code: "KeyW", metaKey: true });
  expect(screen.getByRole("alert").textContent).toContain("reserved");
  fireEvent.keyDown(recorder, { key: "s", code: "KeyS", altKey: true });
  fireEvent.click(screen.getByRole("button", { name: "Save shortcut" }));
  expect(current.bindings.split).toEqual(["alt+s"]);
  expect(f.run).not.toHaveBeenCalled();
});

it("reports a damaged saved config and has Chinese help and real disabled action states", () => {
  preferences({ language: "zh" });
  const f = setup("{bad}");
  fireEvent.click(screen.getByRole("button", { name: "快捷键与设置" }));
  expect(screen.getByRole("status").textContent).toContain("默认按键");
  expect(screen.getByRole("heading", { name: "键盘快捷键" })).toBeTruthy();
  expect(
    (
      screen.getByRole("button", {
        name: "执行: 开关吸附",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  expect(f.write).not.toHaveBeenCalled();
});
