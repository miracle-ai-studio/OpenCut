// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { preferences } from "../../../shared/locale";
import { V4GraphicEditor } from "./editor";
import { graphicInitial } from "./fixture";
import { graphicDraft, graphicDraftValues } from "./draft";
import type { GraphicInitial, GraphicValues } from "./types";
beforeEach(() => preferences({ language: "en" }));
afterEach(() => cleanup());
it("keeps exact colors, transform and rational times when only a name is edited", async () => {
  const initial = graphicInitial(),
    save = vi.fn(async (_value: GraphicValues) => {});
  render(
    <V4GraphicEditor
      initial={initial}
      disabled={false}
      save={save}
      close={vi.fn()}
    />,
  );
  fireEvent.change(screen.getByLabelText("Graphic name"), {
    target: { value: "Renamed shape" },
  });
  fireEvent.submit(screen.getByRole("form"));
  await waitFor(() => expect(save).toHaveBeenCalledOnce());
  expect(save.mock.calls[0][0]).toEqual({
    name: "Renamed shape",
    graphic: initial.graphic,
    transform: initial.transform,
    opacity: initial.opacity,
    start: { kind: "exact", time: initial.timeline_range.start },
    duration: { kind: "exact", time: initial.timeline_range.duration },
  });
});
it.each(["rectangle", "ellipse", "polygon", "star"] as const)(
  "exposes the authored %s parameters without constructing another geometry",
  (kind) => {
    const initial = graphicInitial();
    initial.graphic.geometry =
      kind === "rectangle"
        ? { kind, corner_radius_percent: 10 }
        : kind === "ellipse"
          ? { kind }
          : kind === "polygon"
            ? { kind, sides: 5.25, corner_radius_percent: 13 }
            : { kind, points: 6.25, depth_percent: 45 };
    render(
      <V4GraphicEditor
        initial={initial}
        disabled={false}
        save={vi.fn()}
        close={vi.fn()}
      />,
    );
    expect(screen.queryByLabelText("Polygon sides") !== null).toBe(
      kind === "polygon",
    );
    expect(screen.queryByLabelText("Star points") !== null).toBe(
      kind === "star",
    );
    expect(screen.queryByLabelText("Corner radius (percent)") !== null).toBe(
      kind === "rectangle" || kind === "polygon",
    );
    expect(
      graphicDraftValues(initial, graphicDraft(initial)).graphic.geometry,
    ).toEqual(initial.graphic.geometry);
  },
);
it("patches the chosen stroke and alpha while retaining fill precision and source geometry", () => {
  const initial = graphicInitial(),
    draft = graphicDraft(initial);
  draft.strokeWidth = "9";
  draft.strokeAlign = "inside";
  draft.strokeAlpha = "0.25";
  const value = graphicDraftValues(initial, draft);
  expect(value.graphic.fill).toEqual(initial.graphic.fill);
  expect(value.graphic.stroke).toEqual({
    ...initial.graphic.stroke,
    width_px: 9,
    align: "inside",
    color: { ...initial.graphic.stroke.color, alpha: 0.25 },
  });
  expect(initial.graphic.stroke.width_px).toBe(2.25);
});
it("isolates composition and Escape from timeline shortcuts and writes nothing on cancellation", () => {
  const save = vi.fn(),
    close = vi.fn(),
    keys = vi.fn();
  render(
    <div onKeyDown={keys}>
      <V4GraphicEditor
        initial={graphicInitial()}
        disabled={false}
        save={save}
        close={close}
      />
    </div>,
  );
  const form = screen.getByRole("form");
  fireEvent.compositionStart(form);
  fireEvent.submit(form);
  fireEvent.keyDown(form, { key: "Escape", isComposing: true });
  expect(save).not.toHaveBeenCalled();
  expect(close).not.toHaveBeenCalled();
  fireEvent.compositionEnd(form);
  fireEvent.keyDown(form, { key: "Escape" });
  expect(close).toHaveBeenCalledOnce();
  expect(keys).not.toHaveBeenCalled();
});
it("rejects non-finite inputs without publishing a plan", () => {
  const initial: GraphicInitial = graphicInitial(),
    draft = graphicDraft(initial);
  draft.rotation = "Infinity";
  expect(() => graphicDraftValues(initial, draft)).toThrow("finite graphic");
});
