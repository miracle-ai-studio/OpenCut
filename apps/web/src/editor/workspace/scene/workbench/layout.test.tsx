// @vitest-environment jsdom
import type { ReactNode } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { preferences } from "../../../shared/locale";
import { Workbench } from "./layout";

const context = vi.hoisted(() => ({ value: {} as unknown }));
vi.mock("../../context", () => ({ useEditingWorkspace: () => context.value }));
vi.mock("react-resizable-panels", () => ({
  Group: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  Panel: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  Separator: () => <div />,
}));
vi.mock("./transition-workspace", () => ({
  TransitionProvider: ({ children }: { children: ReactNode }) => children,
  useTransitions: () => null,
}));
vi.mock("./resource-drag", () => ({
  ResourceDragProvider: ({ children }: { children: ReactNode }) => children,
}));
vi.mock("./player-preview", () => ({
  PlayerPreviewProvider: ({ children }: { children: ReactNode }) => children,
  WorkbenchPlayer: ({ children }: { children: ReactNode }) => children,
}));
vi.mock("./player-context", () => ({ usePlayerContext: () => null }));
beforeEach(() => preferences({ language: "en" }));
afterEach(() => cleanup());
const pane = () => (
  <Workbench
    resources="Resources"
    player="Player"
    inspector={<input aria-label="Fade duration" />}
    project="Project settings"
    history="Saved changes"
    timeline="Timeline"
  />
);
function setup() {
  const scene = {
    selectedId: "clip",
    timelineEdit: { selection: [] },
    historyPlan: null as null | {
      review: boolean;
      direction: "undo" | "redo";
      plan: { conflicts: unknown[] };
    },
  };
  context.value = { scene };
  const view = render(pane());
  return { scene, update: () => view.rerender(pane()) };
}
it.each(["undo", "redo"] as const)(
  "keeps visible properties during a direct %s proposal and after completion",
  (direction) => {
    const f = setup();
    const input = screen.getByRole("textbox", { name: "Fade duration" });
    f.scene.historyPlan = { direction, review: false, plan: { conflicts: [] } };
    f.update();
    expect(
      screen
        .getByRole("tab", { name: "Properties" })
        .getAttribute("aria-selected"),
    ).toBe("true");
    expect(screen.getByRole("textbox", { name: "Fade duration" })).toBe(input);
    f.scene.historyPlan = null;
    f.update();
    expect(screen.getByRole("textbox", { name: "Fade duration" })).toBe(input);
  },
);
it("shows explicit review even when its plan is conflict-free", () => {
  const f = setup();
  f.scene.historyPlan = {
    direction: "undo",
    review: true,
    plan: { conflicts: [] },
  };
  f.update();
  expect(
    screen.getByRole("tab", { name: "History" }).getAttribute("aria-selected"),
  ).toBe("true");
  expect(screen.queryByRole("textbox", { name: "Fade duration" })).toBeNull();
});
it("opens conflicts for review instead of hiding them behind properties", () => {
  const f = setup();
  f.scene.historyPlan = {
    direction: "undo",
    review: true,
    plan: { conflicts: [{ path: "clip" }] },
  };
  f.update();
  expect(
    screen.getByRole("tab", { name: "History" }).getAttribute("aria-selected"),
  ).toBe("true");
});
it("keeps the user's project panel during a direct proposal", () => {
  const f = setup();
  fireEvent.click(screen.getByRole("tab", { name: "Project" }));
  f.scene.historyPlan = {
    direction: "redo",
    review: false,
    plan: { conflicts: [] },
  };
  f.update();
  expect(
    screen.getByRole("tab", { name: "Project" }).getAttribute("aria-selected"),
  ).toBe("true");
});
