// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { setupV4 } from "./fixture";
import { V4BlendEditor } from "./blend-editor";
import { OpenCutApiError } from "../../../lib/editor-sdk";
import type { V4Element } from "./types";

const context = vi.hoisted(() => ({ value: {} as unknown }));
vi.mock("../context", () => ({ useWorkspace: () => context.value }));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

async function setup(
  options: {
    mode?: NonNullable<V4Element["visual"]>["blend_mode"];
    historicalMode?: NonNullable<V4Element["visual"]>["blend_mode"];
  } = {},
) {
  const f = await setupV4();
  let snapshot = {
    ...f.controller.getSnapshot(),
    v4: structuredClone(f.controller.getSnapshot().v4),
  };
  const element = snapshot.v4!.head.project.sequences[0].tracks[0].elements[0];
  element.visual!.blend_mode = options.mode ?? "normal";
  element.visual!.effects = [
    {
      id: "effect",
      enabled: true,
      effect: { kind: "brightness", value: 0.123456789 },
    },
  ] as never;
  element.visual!.masks = [
    { id: "mask", enabled: true, shape: { kind: "ellipse", feather: 7.25 } },
  ] as never;
  element.channels = [
    {
      id: "original-channel",
      kind: "scalar",
      keys: [{ time: { ticks: "1001", timebase: 30000 }, value: 0.123456789 }],
    },
  ] as never;
  if (options.historicalMode) {
    snapshot.v4!.historical = structuredClone(snapshot.v4!.head);
    snapshot.v4!.historical.project.sequences[0].tracks[0].elements[0].visual!.blend_mode =
      options.historicalMode;
  }
  const write = vi
    .spyOn(f.controller.v4, "write")
    .mockResolvedValue({} as never);
  vi.spyOn(f.controller, "getSnapshot").mockImplementation(() => snapshot);
  context.value = { ...snapshot, controller: f.controller };
  const view = render(<V4BlendEditor trackId="track" elementId="image" />);
  return {
    f,
    view,
    write,
    element,
    state: () => snapshot,
    set: (patch: Partial<typeof snapshot>) => {
      snapshot = { ...snapshot, ...patch };
      context.value = { ...snapshot, controller: f.controller };
    },
  };
}

it("offers the exact 17 Classic modes and commits only the selected field once", async () => {
  const s = await setup();
  try {
    const select = screen.getByRole<HTMLSelectElement>("combobox", {
      name: "Blend mode",
    });
    expect(Array.from(select.options, (option) => option.value)).toEqual([
      "normal",
      "darken",
      "multiply",
      "color-burn",
      "lighten",
      "screen",
      "plus-lighter",
      "color-dodge",
      "overlay",
      "soft-light",
      "hard-light",
      "difference",
      "exclusion",
      "hue",
      "saturation",
      "color",
      "luminosity",
    ]);
    const expected = structuredClone(s.element);
    expected.visual!.blend_mode = "color-dodge";
    fireEvent.change(select, { target: { value: "color-dodge" } });
    fireEvent.submit(screen.getByRole("form"));
    fireEvent.submit(screen.getByRole("form"));
    await waitFor(() => expect(s.write).toHaveBeenCalledOnce());
    expect(s.write).toHaveBeenCalledWith("Edit blend mode", [
      {
        type: "element_put",
        sequence_id: "scene",
        track_id: "track",
        element: expected,
      },
    ]);
    expect(s.element.visual!.blend_mode).toBe("normal");
    expect(
      screen.getByText(/effects or masks that are not supported/),
    ).toBeTruthy();
  } finally {
    s.f.controller.dispose();
  }
});

it("does not write an unchanged mode or discard a failed permission error", async () => {
  const s = await setup();
  try {
    fireEvent.submit(screen.getByRole("form"));
    expect(s.write).not.toHaveBeenCalled();
    s.write.mockRejectedValueOnce(
      new OpenCutApiError(
        "PERMISSION_DENIED",
        "Blend edit permission revoked",
        null,
        403,
        "request",
        [],
      ),
    );
    fireEvent.change(screen.getByRole("combobox"), {
      target: { value: "multiply" },
    });
    fireEvent.submit(screen.getByRole("form"));
    expect((await screen.findByRole("alert")).textContent).toBe(
      "Blend edit permission revoked",
    );
    expect(s.element.visual!.blend_mode).toBe("normal");
  } finally {
    s.f.controller.dispose();
  }
});

it("shows a historical blend mode read-only instead of the current head mode", async () => {
  const s = await setup({ mode: "normal", historicalMode: "screen" });
  try {
    const select = screen.getByRole<HTMLSelectElement>("combobox", {
      name: "Blend mode",
    });
    expect(select.value).toBe("screen");
    expect(select.disabled).toBe(true);
    fireEvent.submit(screen.getByRole("form"));
    expect(s.write).not.toHaveBeenCalled();
  } finally {
    s.f.controller.dispose();
  }
});

it("can return a saved non-normal element to normal blending", async () => {
  const s = await setup({ mode: "multiply" });
  try {
    const expected = structuredClone(s.element);
    expected.visual!.blend_mode = "normal";
    fireEvent.change(screen.getByRole("combobox"), {
      target: { value: "normal" },
    });
    fireEvent.submit(screen.getByRole("form"));
    await waitFor(() =>
      expect(s.write).toHaveBeenCalledWith("Edit blend mode", [
        {
          type: "element_put",
          sequence_id: "scene",
          track_id: "track",
          element: expected,
        },
      ]),
    );
  } finally {
    s.f.controller.dispose();
  }
});

it.each([
  "head",
  "locked",
  "recovery",
  "historical",
  "busy",
  "visual",
  "connection",
  "selection",
])(
  "rechecks %s before committing even without a form rerender",
  async (condition) => {
    const s = await setup();
    try {
      fireEvent.change(screen.getByRole("combobox"), {
        target: { value: "screen" },
      });
      const next = structuredClone(s.state().v4!);
      if (condition === "head") next.head.revision = "new-head";
      if (condition === "locked")
        next.head.project.sequences[0].tracks[0].locked = true;
      if (condition === "recovery")
        next.recovery = {
          ...next.recovery!,
          phase: "unconfirmed",
          error: "Original edit is unknown",
        };
      if (condition === "historical")
        next.historical = structuredClone(next.head);
      if (condition === "visual")
        next.head.project.sequences[0].tracks[0].elements[0].visual = null;
      if (condition === "selection") next.selectedId = "other-element";
      s.set({
        v4: next,
        ...(condition === "busy" ? { busy: true } : {}),
        ...(condition === "connection" ? { api: null } : {}),
      });
      fireEvent.submit(screen.getByRole("form"));
      expect(s.write).not.toHaveBeenCalled();
      expect(screen.getByRole("alert").textContent).toMatch(
        /changed|Unlock|Resolve|current revision|Wait/,
      );
    } finally {
      s.f.controller.dispose();
    }
  },
);
