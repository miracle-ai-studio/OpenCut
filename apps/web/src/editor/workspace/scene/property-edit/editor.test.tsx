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
import { setup as createFixture } from "../fixture";
import { ElementPropertiesEditor } from "./editor";
import { setupAudioGain } from "../audio-gain-fixture";
import type { ToolInput } from "../../../../lib/editor-sdk";
const context = vi.hoisted(() => ({ value: {} as unknown }));
vi.mock("../../context", () => ({ useWorkspace: () => context.value }));
beforeEach(() => preferences({ language: "en" }));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
async function setup() {
  const f = await createFixture(),
    state = {
      ...f.controller.getSnapshot(),
      scene: structuredClone(f.controller.getSnapshot().scene),
    };
  state.scene!.selectedId = "image";
  const element = state.scene!.head.project.sequences[0].tracks[0].elements[0];
  element.visual!.transform.position_y_px = 1.0000000000000002;
  vi.mocked(f.api.sdk.hostCapabilities).mockResolvedValue({
    tools: ["opencut_element_properties_plan"],
  } as never);
  const original = f.call.getMockImplementation()!,
    read = vi.fn(
      async (request: ToolInput<"opencut_element_properties_plan">) => ({
        project_id: request.project_id,
        expected_revision: request.expected_revision,
        idempotency_key: request.idempotency_key,
        label: request.label,
        operations: [
          {
            type: "element_put" as const,
            sequence_id: request.sequence_id,
            track_id: state.scene!.head.project.sequences[0].tracks[0].id,
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
    name === "opencut_element_properties_plan"
      ? ((await read(
          args[0] as ToolInput<"opencut_element_properties_plan">,
        )) as never)
      : original(name, ...args),
  );
  vi.spyOn(f.controller, "getSnapshot").mockImplementation(() => state);
  const apply = vi
    .spyOn(f.controller.scene, "applyPlanned")
    .mockResolvedValue({} as never);
  context.value = { ...state, controller: f.controller };
  const view = render(<ElementPropertiesEditor elementId="image" />);
  await screen.findByRole("button", { name: "Save clip properties" });
  return { f, state, element, read, apply, view };
}
it("keeps audio-only speed and pitch editable when the compact inspector owns gain", async () => {
  const f = await setupAudioGain();
  try {
    const state = f.controller.getSnapshot(), element = state.scene!.head.project.sequences[0].tracks[0].elements[0];
    if (element.content.kind !== "video") throw Error();
    element.visual = null;
    element.content = { kind: "audio", asset_id: element.content.asset_id, audio: element.content.audio, retime: element.content.retime };
    context.value = { ...state, controller: f.controller };
    vi.mocked(f.api.sdk.hostCapabilities).mockResolvedValue({ tools: ["opencut_element_properties_plan"] } as never);
    render(<ElementPropertiesEditor elementId="video" showAudio={false} />);
    await waitFor(() => expect((screen.getByLabelText("Playback speed (×)") as HTMLInputElement).disabled).toBe(false));
    expect(screen.getByLabelText("Maintain audio pitch")).toBeTruthy();
    expect(screen.queryByLabelText("Gain unit")).toBeNull();
    fireEvent.change(screen.getByLabelText("Playback speed (×)"), { target: { value: "2" } });
    fireEvent.click(screen.getByRole("button", { name: "Cancel property draft" }));
    expect((screen.getByLabelText("Playback speed (×)") as HTMLInputElement).value).toBe("1");
  } finally { f.controller.dispose(); }
});
it("changes one visual input through the Rust property intent while retaining untouched precision", async () => {
  const s = await setup();
  try {
    fireEvent.change(screen.getByLabelText("Horizontal position (pixels)"), {
      target: { value: "25.5" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Save clip properties" }),
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
it("keeps the local form but refuses planning when refresh observes an external head", async () => {
  const s = await setup();
  try {
    fireEvent.change(screen.getByLabelText("Horizontal position (pixels)"), { target: { value: "25" } });
    s.state.scene!.externalRevision = "external";
    s.view.rerender(<ElementPropertiesEditor elementId="image" />);
    const save = screen.getByRole("button", { name: "Save clip properties" }) as HTMLButtonElement;
    expect(save.matches(":disabled")).toBe(true);
    fireEvent.click(save);
    expect(s.read).not.toHaveBeenCalled();
    expect(s.apply).not.toHaveBeenCalled();
    expect((screen.getByLabelText("Horizontal position (pixels)") as HTMLInputElement).value).toBe("25");
  } finally { s.f.controller.dispose(); }
});
it("a rejected plan retries the original immutable property request instead of reading changed form values", async () => {
  const s = await setup();
  try {
    s.read.mockRejectedValueOnce(new Error("Try later"));
    fireEvent.change(screen.getByLabelText("Horizontal scale"), {
      target: { value: "-0.5" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Save clip properties" }),
    );
    await screen.findByText("Try later");
    const original = s.read.mock.calls[0][0];
    fireEvent.change(screen.getByLabelText("Horizontal scale"), {
      target: { value: "2" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Retry saving changes" }),
    );
    await waitFor(() => expect(s.read).toHaveBeenCalledTimes(2));
    expect(s.read.mock.calls[1][0]).toBe(original);
    expect(s.apply).toHaveBeenCalledTimes(1);
  } finally {
    s.f.controller.dispose();
  }
});
