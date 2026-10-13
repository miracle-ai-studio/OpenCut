// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, act } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { useWorkspace } from "../../context";
import type { useCanvasTextAuthoring } from "./workspace";
import { setupV4 } from "../fixture";
import { textInitial } from "./fixture";
import type { CanvasTextPlan, CanvasTextPlanning } from "./canvas-types";
let workspace: ReturnType<typeof useWorkspace>;
let authoring: NonNullable<ReturnType<typeof useCanvasTextAuthoring>>;
vi.mock("../../context", () => ({ useWorkspace: () => workspace }));
vi.mock("./workspace", () => ({ useCanvasTextAuthoring: () => authoring }));
import { CanvasTextOverlay } from "./canvas-overlay";
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
function plan(scope: CanvasTextPlan["scope"]): CanvasTextPlan {
  return {
    scope,
    width: 320,
    height: 180,
    block: { maxWidth: 80, height: 36 },
    layer: {
      elementId: scope.elementId!,
      text: {
        content: scope.elementId!,
        align: "center",
        fontSizePx: 30,
        transform: {
          centerX: 160,
          centerY: 90,
          scaleX: 1,
          scaleY: 1,
          rotationRadians: 0,
        },
      },
    },
  } as CanvasTextPlan;
}
async function setup(planner: CanvasTextPlanning) {
  const f = await setupV4();
  const track = f.current().project.sequences[0].tracks[0];
  track.kind = "text";
  track.elements[0].content = { kind: "text", text: textInitial().text };
  track.elements[0].id = "good";
  await f.controller.v4.refresh();
  workspace = { controller: f.controller, ...f.controller.getSnapshot() };
  authoring = {
    available: true,
    disabled: false,
    canImport: false,
    beginCreate: vi.fn(),
    beginEdit: vi.fn(),
    beginImport: vi.fn(),
    panel: null,
    beginCanvasEdit: vi.fn(),
    canvas: null,
    planCanvasText: planner,
    commitCanvas: vi.fn(),
    cancelCanvas: vi.fn(),
  };
  return f;
}
it("skips disabled tracks/elements and isolates a refused title so later text remains editable", async () => {
  const planner = vi.fn<CanvasTextPlanning>(async (scope) => {
    if (scope.elementId === "refused") throw new Error("Missing font");
    return plan(scope);
  });
  const f = await setup(planner);
  try {
    const track = workspace.v4!.head.project.sequences[0].tracks[0],
      original = track.elements[0];
    track.elements = [
      { ...original, id: "hidden", enabled: false },
      { ...original, id: "refused" },
      original,
    ];
    workspace.v4!.head.project.sequences[0].tracks.push({
      ...track,
      id: "disabled-track",
      enabled: false,
      elements: [{ ...original, id: "hidden-track-text" }],
    });
    render(<CanvasTextOverlay />);
    await screen.findByRole("button", { name: "Edit text on canvas: good" });
    expect(planner.mock.calls.map(([scope]) => scope.elementId)).toEqual([
      "refused",
      "good",
    ]);
  } finally {
    f.controller.dispose();
  }
});
it.each(["loading", "playing", "stalled", "unconfirmed"] as const)(
  "suspends extra plans during %s and resumes on pause",
  async (phase) => {
    const planner = vi.fn<CanvasTextPlanning>(async (scope) =>
      plan(scope),
    );
    const f = await setup(planner);
    try {
      workspace.v4!.audioPreview.phase = phase;
      const ui = render(<CanvasTextOverlay />);
      for (const ticks of ["1", "2", "3"]) {
        workspace.v4!.playhead = { ticks, timebase: 1 };
        ui.rerender(<CanvasTextOverlay />);
      }
      await act(async () => {});
      expect(planner).not.toHaveBeenCalled();
      workspace.v4!.audioPreview.phase = "paused";
      ui.rerender(<CanvasTextOverlay />);
      await screen.findByRole("button", { name: "Edit text on canvas: good" });
      expect(planner).toHaveBeenCalledTimes(1);
    } finally {
      f.controller.dispose();
    }
  },
);
it("callback identity alone does not repeat font planning on unchanged presentation renders", async () => {
  const planner = vi.fn<CanvasTextPlanning>(async (scope) => plan(scope));
  const f = await setup(planner);
  try {
    const ui = render(<CanvasTextOverlay />);
    await screen.findByRole("button", { name: "Edit text on canvas: good" });
    authoring = { ...authoring, planCanvasText: (...args) => planner(...args) };
    ui.rerender(<CanvasTextOverlay />);
    await act(async () => {});
    expect(planner).toHaveBeenCalledTimes(1);
  } finally {
    f.controller.dispose();
  }
});
it("play start aborts in-flight work and late completion cannot republish old hit targets", async () => {
  let resolve!: (plan: CanvasTextPlan) => void;
  let scope!: CanvasTextPlan["scope"];
  let signal!: AbortSignal;
  const planner = vi.fn<CanvasTextPlanning>(
    (target: CanvasTextPlan["scope"], _time: Parameters<CanvasTextPlanning>[1], abort: AbortSignal) => {
      scope = target;
      signal = abort;
      return new Promise<CanvasTextPlan>((done) => {
        resolve = done;
      });
    },
  );
  const f = await setup(planner);
  try {
    const ui = render(<CanvasTextOverlay />);
    await waitFor(() => expect(planner).toHaveBeenCalledTimes(1));
    workspace.v4!.audioPreview.phase = "playing";
    ui.rerender(<CanvasTextOverlay />);
    expect(signal.aborted).toBe(true);
    await act(async () => {
      resolve(plan(scope));
    });
    expect(
      screen.queryByRole("button", { name: "Edit text on canvas: good" }),
    ).toBeNull();
  } finally {
    f.controller.dispose();
  }
});
it("fixed-view changes retire a pending old plan without hiding the new authorized target", async () => {
  let resolve!: (value: CanvasTextPlan) => void;
  let oldScope!: CanvasTextPlan["scope"];
  const planner = vi.fn<CanvasTextPlanning>(
    (scope: CanvasTextPlan["scope"], _time: Parameters<CanvasTextPlanning>[1], _signal: AbortSignal) => {
      if (scope.revision === "r0") {
        oldScope = scope;
        return new Promise<CanvasTextPlan>((done) => {
          resolve = done;
        });
      }
      const value = plan(scope);
      value.layer.text.content = "new revision";
      return Promise.resolve(value);
    },
  );
  const f = await setup(planner);
  try {
    const ui = render(<CanvasTextOverlay />);
    await waitFor(() => expect(planner).toHaveBeenCalledTimes(1));
    workspace.v4!.head = { ...workspace.v4!.head, revision: "r1" };
    ui.rerender(<CanvasTextOverlay />);
    await screen.findByRole("button", {
      name: "Edit text on canvas: new revision",
    });
    await act(async () => {
      const old = plan(oldScope);
      old.layer.text.content = "old revision";
      resolve(old);
    });
    expect(
      screen.queryByRole("button", {
        name: "Edit text on canvas: old revision",
      }),
    ).toBeNull();
    expect(
      screen.getByRole("button", { name: "Edit text on canvas: new revision" }),
    ).toBeTruthy();
  } finally {
    f.controller.dispose();
  }
});
