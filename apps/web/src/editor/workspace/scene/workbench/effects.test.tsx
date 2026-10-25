// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { initialSnapshot } from "../fixture";
import { preferences } from "../../../shared/locale";
import { EffectsResources } from "./effects";
const mocks = vi.hoisted(() => ({
  scene: {} as unknown,
  stack: {
    disabled: false,
    available: true,
    error: null,
    transact: vi.fn(),
    report: vi.fn(),
  },
}));
vi.mock("../../context", () => ({
  useWorkspace: () => ({ scene: mocks.scene }),
}));
vi.mock("../scene-stack", () => ({
  useSceneStack: () => mocks.stack,
  SceneEffectsPanel: () => null,
}));
afterEach(cleanup);
beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});
beforeEach(() => {
  preferences({ language: "en" });
  mocks.stack.transact.mockReset().mockResolvedValue(true);
  mocks.stack.report.mockReset();
  mocks.stack.disabled = false;
});
function fixture() {
  const sequence = initialSnapshot().project.sequences[0];
  mocks.scene = {
    head: initialSnapshot(),
    selectedId: "image",
    playhead: { ticks: "1", timebase: 3 },
  };
  return sequence;
}
it("adds the selected executable blur preset without changing the original clip", async () => {
  const sequence = fixture();
  render(<EffectsResources sequence={sequence} />);
  fireEvent.click(screen.getByRole("button", { name: "Preview · Strong" }));
  expect(mocks.stack.transact).not.toHaveBeenCalled();
  expect(screen.getByRole("dialog", { name: "Preview · Strong" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Close" }));
  fireEvent.click(
    screen.getByRole("button", { name: "Add to selected clip · Strong" }),
  );
  await screen.findByRole("status");
  expect(mocks.stack.transact).toHaveBeenCalledWith("Add blur", [
    expect.objectContaining({
      type: "element_put",
      sequence_id: "scene",
      track_id: "track",
      element: expect.objectContaining({
        id: "image",
        visual: expect.objectContaining({
          effects: [
            expect.objectContaining({
              enabled: true,
              effect: { kind: "classic_blur", intensity: 35 },
            }),
          ],
        }),
      }),
    }),
  ]);
  expect(sequence.tracks[0].elements[0].visual?.effects).toEqual([]);
});
it("adds an effect layer at exact playhead time and validates duration before dispatch", async () => {
  const sequence = fixture();
  render(<EffectsResources sequence={sequence} />);
  fireEvent.click(
    screen.getByText("Add effect layer", { selector: "summary" }),
  );
  fireEvent.change(screen.getByLabelText("Effect duration (seconds)"), {
    target: { value: "0" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Add effect layer" }));
  expect(mocks.stack.transact).not.toHaveBeenCalled();
  expect(mocks.stack.report).toHaveBeenCalledOnce();
  fireEvent.change(screen.getByLabelText("Effect duration (seconds)"), {
    target: { value: "0.125" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Preview · Soft" }));
  fireEvent.click(screen.getByRole("button", { name: "Close" }));
  fireEvent.click(screen.getByRole("button", { name: "Add effect layer" }));
  await waitFor(() => expect(mocks.stack.transact).toHaveBeenCalledOnce());
  expect(
    mocks.stack.transact.mock.calls[0][1][0].track.elements[0],
  ).toMatchObject({
    timeline_range: {
      start: { ticks: "1", timebase: 3 },
      duration: { ticks: "1", timebase: 8 },
    },
    content: {
      kind: "effect_layer",
      effect: { effect: { kind: "classic_blur", intensity: 5 } },
    },
  });
});
it("respects lock and existing write recovery guards", () => {
  const sequence = fixture();
  sequence.tracks[0].locked = true;
  mocks.stack.disabled = true;
  render(<EffectsResources sequence={sequence} />);
  expect(
    (
      screen.getByRole("button", {
        name: "Add to selected clip · Medium",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  fireEvent.click(
    screen.getByRole("button", { name: "Add to selected clip · Medium" }),
  );
  expect(mocks.stack.transact).not.toHaveBeenCalled();
  fireEvent.keyDown(screen.getByRole("button", { name: "Preview · Medium" }), {
    key: "Enter",
    shiftKey: true,
  });
  expect(mocks.stack.transact).not.toHaveBeenCalled();
});

it("Shift+Enter applies exactly the focused preset once, guards repeats, and ignores a late saved notice for another target", async () => {
  const sequence = fixture();
  let finish!: (saved: boolean) => void;
  mocks.stack.transact.mockImplementationOnce(
    () =>
      new Promise<boolean>((resolve) => {
        finish = resolve;
      }),
  );
  const view = render(<EffectsResources sequence={sequence} />);
  const preset = screen.getByRole("button", { name: "Preview · Strong" });
  fireEvent.keyDown(preset, { key: "Enter", shiftKey: true, repeat: true });
  expect(mocks.stack.transact).not.toHaveBeenCalled();
  fireEvent.keyDown(preset, { key: "Enter", shiftKey: true });
  fireEvent.keyDown(preset, { key: "Enter", shiftKey: true });
  expect(mocks.stack.transact).toHaveBeenCalledOnce();
  expect(
    mocks.stack.transact.mock.calls[0][1][0].element.visual.effects[0].effect
      .intensity,
  ).toBe(35);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(
    (
      screen.getByRole("button", {
        name: "Add to selected clip · Strong",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  mocks.scene = { ...(mocks.scene as object), selectedId: "another-clip" };
  view.rerender(<EffectsResources sequence={sequence} />);
  await act(async () => {
    finish(true);
  });
  expect(screen.queryByRole("status")).toBeNull();
});

it("keeps illustrative preview read-only for locked clips, and closes it on Escape or category change", () => {
  const sequence = fixture();
  sequence.tracks[0].locked = true;
  const view = render(<EffectsResources sequence={sequence} />);
  fireEvent.click(screen.getByRole("button", { name: "Preview · Soft" }));
  fireEvent(
    screen.getByRole("dialog"),
    new Event("cancel", { cancelable: true }),
  );
  expect(screen.queryByRole("dialog")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Preview · Medium" }));
  view.rerender(<EffectsResources sequence={sequence} active={false} />);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(mocks.stack.transact).not.toHaveBeenCalled();
});
