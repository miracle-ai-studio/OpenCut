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
import { setupV4 } from "../fixture";
import { V4ElementPropertiesEditor } from "./editor";
import type { ToolInput } from "../../../../lib/editor-sdk";
const context = vi.hoisted(() => ({ value: {} as unknown }));
vi.mock("../../context", () => ({ useWorkspace: () => context.value }));
beforeEach(() => preferences({ language: "en" }));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
async function setup() {
  const f = await setupV4(),
    state = {
      ...f.controller.getSnapshot(),
      v4: structuredClone(f.controller.getSnapshot().v4),
    };
  state.v4!.selectedId = "image";
  const element = state.v4!.head.project.sequences[0].tracks[0].elements[0];
  element.visual!.transform.position_y_px = 1.0000000000000002;
  vi.mocked(f.api.sdk.hostCapabilities).mockResolvedValue({
    tools: ["opencut_element_properties_v1_plan"],
  } as never);
  const original = f.call.getMockImplementation()!,
    read = vi.fn(
      async (request: ToolInput<"opencut_element_properties_v1_plan">) => ({
        project_id: request.project_id,
        expected_revision: request.expected_revision,
        idempotency_key: request.idempotency_key,
        label: request.label,
        operations: [
          {
            type: "element_put" as const,
            sequence_id: request.sequence_id,
            track_id: state.v4!.head.project.sequences[0].tracks[0].id,
            element: {
              ...element,
              visual: {
                ...element.visual!,
                transform: request.transform ?? element.visual!.transform,
              },
            },
          },
        ],
      }),
    );
  f.call.mockImplementation(async (name, ...args) =>
    name === "opencut_element_properties_v1_plan"
      ? ((await read(
          args[0] as ToolInput<"opencut_element_properties_v1_plan">,
        )) as never)
      : original(name, ...args),
  );
  vi.spyOn(f.controller, "getSnapshot").mockImplementation(() => state);
  const apply = vi
    .spyOn(f.controller.v4, "applyPlanned")
    .mockResolvedValue({} as never);
  context.value = { ...state, controller: f.controller };
  const view = render(<V4ElementPropertiesEditor elementId="image" />);
  await screen.findByRole("button", { name: "Save element properties" });
  return { f, state, element, read, apply, view };
}
it("changes one visual input through the Rust property intent while retaining untouched precision", async () => {
  const s = await setup();
  try {
    fireEvent.change(screen.getByLabelText("Horizontal position (pixels)"), {
      target: { value: "25.5" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Save element properties" }),
    );
    await waitFor(() => expect(s.apply).toHaveBeenCalledTimes(1));
    expect(s.read.mock.calls[0][0]).toMatchObject({
      element_id: "image",
      transform: { position_x_px: 25.5, position_y_px: 1.0000000000000002 },
      opacity: null,
      audio_gain: null,
      retime: null,
    });
  } finally {
    s.f.controller.dispose();
  }
});
it("a rejected plan retries the original immutable property request instead of reading changed form values", async () => {
  const s = await setup();
  try {
    s.read.mockRejectedValueOnce(new Error("Try later"));
    fireEvent.change(screen.getByLabelText("Horizontal scale"), {
      target: { value: "-0.5" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Save element properties" }),
    );
    await screen.findByText("Try later");
    const original = s.read.mock.calls[0][0];
    fireEvent.change(screen.getByLabelText("Horizontal scale"), {
      target: { value: "2" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Retry original property request" }),
    );
    await waitFor(() => expect(s.read).toHaveBeenCalledTimes(2));
    expect(s.read.mock.calls[1][0]).toBe(original);
    expect(s.apply).toHaveBeenCalledTimes(1);
  } finally {
    s.f.controller.dispose();
  }
});
