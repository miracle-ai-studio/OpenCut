// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { setupV4 } from "../fixture";
import { textInitial } from "./fixture";
import { V4TextTransformEditor } from "./transform-editor";
const context = vi.hoisted(() => ({ value: {} as unknown }));
vi.mock("../../context", () => ({ useWorkspace: () => context.value }));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
const labels = [
  "Horizontal center offset (pixels)",
  "Vertical center offset (pixels)",
  "Horizontal scale (multiplier)",
  "Vertical scale (multiplier)",
];
async function setup(animated = false) {
  const f = await setupV4();
  let snapshot = {
    ...f.controller.getSnapshot(),
    v4: structuredClone(f.controller.getSnapshot().v4),
  };
  const element = snapshot.v4!.head.project.sequences[0].tracks[0].elements[0];
  element.content = { kind: "text", text: textInitial().text };
  element.source_span = { profile: "classic_trimmed", trim_start: { ticks: "0", timebase: 1 }, trim_end: { ticks: "0", timebase: 1 }, source_duration: null };
  element.visual!.transform.position_x_px = 17.1234567890123;
  if (animated)
    element.channels.push({
      kind: "scalar",
      id: "position-animation",
      target: { kind: "element", element_id: "image" },
      binding: { kind: "property", parameter: "position_x_px" },
      keys: [
        {
          id: "position-key",
          time: { ticks: "0", timebase: 1 },
          value: 10,
          left_handle: null,
          right_handle: null,
          segment_to_next: { kind: "linear" },
          tangent_mode: "auto",
        },
      ],
      extrapolation: { before: "hold", after: "hold" },
    });
  const write = vi
    .spyOn(f.controller.v4, "write")
    .mockResolvedValue({} as never);
  vi.spyOn(f.controller, "getSnapshot").mockImplementation(() => snapshot);
  context.value = { ...snapshot, controller: f.controller };
  const close = vi.fn();
  const view = render(
    <V4TextTransformEditor trackId="track" elementId="image" onClose={close} />,
  );
  return {
    f,
    view,
    write,
    close,
    element,
    state: () => snapshot,
    set: (patch: Partial<typeof snapshot>) => {
      snapshot = { ...snapshot, ...patch };
      context.value = { ...snapshot, controller: f.controller };
    },
  };
}
it("retains original numeric precision and writes nothing for unchanged or cancelled drafts", async () => {
  const s = await setup();
  try {
    expect((screen.getByLabelText(labels[0]) as HTMLInputElement).value).toBe(
      "17.1234567890123",
    );
    fireEvent.submit(screen.getByRole("form"));
    expect(s.write).not.toHaveBeenCalled();
    fireEvent.change(screen.getByLabelText(labels[0]), {
      target: { value: "32" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(s.close).toHaveBeenCalledOnce();
    expect(s.write).not.toHaveBeenCalled();
  } finally {
    s.f.controller.dispose();
  }
});
it("one write changes only the four authored fields and preserves all original nested content", async () => {
  const s = await setup();
  try {
    const original = structuredClone(s.element);
    for (const [index, value] of ["32", "-12", "1.25", "0.75"].entries())
      fireEvent.change(screen.getByLabelText(labels[index]), {
        target: { value },
      });
    const form = screen.getByRole("form");
    fireEvent.submit(form);
    fireEvent.submit(form);
    await waitFor(() => expect(s.close).toHaveBeenCalledOnce());
    expect(s.write).toHaveBeenCalledOnce();
    const expected = structuredClone(original);
    Object.assign(expected.visual!.transform, {
      position_x_px: 32,
      position_y_px: -12,
      scale_x: 1.25,
      scale_y: 0.75,
    });
    expect(s.write).toHaveBeenCalledWith("Edit text position and scale", [
      {
        type: "element_put",
        sequence_id: "scene",
        track_id: "track",
        element: expected,
      },
    ]);
  } finally {
    s.f.controller.dispose();
  }
});
it("save rechecks stale head, locked track and unresolved recovery even without a UI rerender", async () => {
  for (const condition of ["head", "locked", "recovery"]) {
    const s = await setup();
    try {
      fireEvent.change(screen.getByLabelText(labels[0]), {
        target: { value: "32" },
      });
      const next = structuredClone(s.state().v4!);
      if (condition === "head") next.head.revision = "new-head";
      if (condition === "locked")
        next.head.project.sequences[0].tracks[0].locked = true;
      if (condition === "recovery")
        next.recovery = {
          ...next.recovery!,
          phase: "unconfirmed",
          error: "Original write is unknown",
        };
      s.set({ v4: next });
      fireEvent.submit(screen.getByRole("form"));
      expect(s.write).not.toHaveBeenCalled();
      expect(screen.getByRole("alert").textContent).toMatch(
        /changed|Unlock|Resolve/,
      );
    } finally {
      cleanup();
      s.f.controller.dispose();
    }
  }
});
it("changing one field retains untouched precision and warns about preserved animation", async () => {
  const s = await setup(true);
  try {
    const original = structuredClone(s.element);
    expect(
      screen.getByText(/Animation controls these transform fields/),
    ).toBeTruthy();
    fireEvent.change(screen.getByLabelText(labels[3]), {
      target: { value: "0.75" },
    });
    fireEvent.submit(screen.getByRole("form"));
    await waitFor(() => expect(s.write).toHaveBeenCalledOnce());
    const saved = s.write.mock.calls[0][1][0];
    expect(saved.type).toBe("element_put");
    if (saved.type === "element_put") {
      expect(saved.element.visual!.transform.position_x_px).toBe(
        original.visual!.transform.position_x_px,
      );
      expect(saved.element.channels).toEqual(original.channels);
    }
  } finally {
    s.f.controller.dispose();
  }
});

it("refuses historical, busy and missing-visual targets and rejects nonfinite input", async () => {
  for (const condition of ["historical", "busy", "visual", "input"]) {
    const s = await setup();
    try {
      fireEvent.change(screen.getByLabelText(labels[0]), {
        target: { value: "32" },
      });
      if (condition === "busy") s.set({ busy: true });
      else if (condition === "input")
        fireEvent.change(screen.getByLabelText(labels[0]), {
          target: { value: "" },
        });
      else {
        const next = structuredClone(s.state().v4!);
        if (condition === "historical")
          next.historical = structuredClone(next.head);
        else next.head.project.sequences[0].tracks[0].elements[0].visual = null;
        s.set({ v4: next });
      }
      fireEvent.submit(screen.getByRole("form"));
      expect(s.write).not.toHaveBeenCalled();
    } finally {
      cleanup();
      s.f.controller.dispose();
    }
  }
});
