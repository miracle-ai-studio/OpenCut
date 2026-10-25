// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import type { ProjectRef } from "../model/types";
import { ProjectBrowser } from "./browser";

const state = vi.hoisted(() => ({ workspace: {} as Record<string, unknown> }));
vi.mock("../workspace/context", () => ({
  useWorkspace: () => state.workspace,
  useEditingWorkspace: () => state.workspace,
}));
vi.mock("../shared/locale", () => ({
  useLocale: () => ({ language: "en" }),
  t: (key: string, values: Record<string, unknown> = {}) =>
    key.replace(/\{([^}]+)\}/g, (_, name) => String(values[name] ?? name)),
}));
vi.mock("../shared/motion", () => ({ useEntrance: () => ({ current: null }) }));
vi.mock("./cover", () => ({
  ProjectCover: () => <div className="project-cover" />,
}));

const project: ProjectRef = {
  id: "hainan",
  name: "海南·海岸慢行｜多轨走查",
  revision: "saved-revision",
  format: "editor",
  library: {
    project_id: "hainan",
    name: "海南·海岸慢行｜多轨走查",
    revision: "saved-revision",
    format: "editor",
    width: 1920,
    height: 1080,
    fps: { numerator: 25, denominator: 1 },
    duration: { ticks: "42", timebase: 1 },
    scene_count: 1,
    asset_count: 14,
    created_at: "2026-10-21T12:00:00Z",
    modified_at: "2026-10-25T12:00:00Z",
  },
};
function setup() {
  const controller = {
    open: vi.fn(async () => {}),
    report: vi.fn(),
    refreshProjects: vi.fn(async () => {}),
  };
  state.workspace = {
    controller,
    projects: [project],
    connection: "connected",
    api: {},
  };
  const onOpen = vi.fn();
  render(<ProjectBrowser onConnect={vi.fn()} onOpen={onOpen} />);
  return { controller, onOpen };
}
afterEach(() => {
  cleanup();
  localStorage.clear();
  sessionStorage.clear();
  vi.clearAllMocks();
});

it("opens the retained project from its saved metadata and keeps normal package import", async () => {
  const { controller, onOpen } = setup();
  expect(screen.getByText(/1920 × 1080/).textContent).toContain(
    "14 media items",
  );
  expect(screen.getByText("Showing all 1 projects")).toBeTruthy();
  expect(screen.queryByText(/Recover Classic/)).toBeNull();
  expect(
    screen.getByRole("button", { name: "Import project package" }),
  ).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: /Continue editing/ }));
  await waitFor(() => expect(onOpen).toHaveBeenCalledOnce());
  expect(controller.open).toHaveBeenCalledExactlyOnceWith(project);
});

it("preserves searching, layout switching and selection after the redesign", () => {
  setup();
  fireEvent.change(screen.getByLabelText("Search projects"), {
    target: { value: "absent" },
  });
  expect(screen.getByText("No results found")).toBeTruthy();
  expect(screen.queryByRole("button", { name: /Continue editing/ })).toBeNull();
  fireEvent.change(screen.getByLabelText("Search projects"), {
    target: { value: "海南" },
  });
  fireEvent.click(screen.getByLabelText("List view"));
  expect(
    screen
      .getByRole("button", { name: "List view" })
      .getAttribute("aria-pressed"),
  ).toBe("true");
  fireEvent.click(
    screen.getByRole("checkbox", { name: `Select project ${project.name}` }),
  );
  expect(screen.getByRole("button", { name: "Delete selected" })).toBeTruthy();
  fireEvent.click(screen.getByLabelText("Grid view"));
  expect(
    screen
      .getByRole("button", { name: "Grid view" })
      .getAttribute("aria-pressed"),
  ).toBe("true");
});
