// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { preferences } from "../../../shared/locale";
import { setup as createFixture } from "../fixture";
import {
  MaskAuthoringProvider,
  MaskInspector,
  MaskPanel,
} from "./workspace";
import { CanvasMaskOverlay } from "./canvas-overlay";
import { boxMask, geometryPlan } from "./fixture";
import type { MaskGeometryPlan } from "./types";
import type { Element } from "../types";

const context = vi.hoisted(() => ({ value: {} as unknown }));
vi.mock("../../context", () => ({ useWorkspace: () => context.value, useEditingWorkspace: () => context.value }));
beforeEach(() => preferences({ language: "en" }));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
function Product() {
  return (
    <MaskAuthoringProvider>
      <MaskInspector elementId="image" />
      <MaskPanel />
      <CanvasMaskOverlay width={640} height={360} />
    </MaskAuthoringProvider>
  );
}
async function setup(
  options: {
    existing?: boolean;
    plan?: () => Promise<MaskGeometryPlan>;
    edit?: (element: Element) => void;
  } = {},
) {
  const f = await createFixture();
  const state = {
    ...f.controller.getSnapshot(),
    scene: structuredClone(f.controller.getSnapshot().scene),
  };
  state.scene!.selectedId = "image";
  const element = state.scene!.head.project.sequences[0].tracks[0].elements[0];
  if (options.existing) element.visual!.masks.push(boxMask());
  options.edit?.(element);
  const call = f.call.getMockImplementation()!;
  const read = vi.fn(options.plan ?? (async () => geometryPlan()));
  f.call.mockImplementation(async (name, ...args) =>
    name === "opencut_mask_geometry"
      ? ((await read()) as never)
      : call(name, ...args),
  );
  vi.mocked(f.api.sdk.hostCapabilities).mockResolvedValue({
    tools: ["opencut_mask_geometry"],
  } as never);
  const write = vi
    .spyOn(f.controller.scene, "write")
    .mockResolvedValue({} as never);
  vi.spyOn(f.controller, "getSnapshot").mockImplementation(() => state);
  context.value = { ...state, controller: f.controller };
  const view = render(<Product />);
  await waitFor(() =>
    expect(
      (
        screen.getByRole("button", {
          name: options.existing ? "Edit mask" : "Add rectangle mask",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false),
  );
  return {
    f,
    state,
    element,
    read,
    write,
    view,
    refresh: () => {
      context.value = { ...state, controller: f.controller };
      view.rerender(<Product />);
    },
  };
}
const begin = async (existing = false) => {
  fireEvent.click(
    screen.getByRole("button", {
      name: existing ? "Edit mask" : "Add rectangle mask",
    }),
  );
  await screen.findByRole("form", { name: "Edit mask" });
};

it("uses fixed Rust defaults, preserves the whole element and writes one normal transaction", async () => {
  const s = await setup({
    edit: (element) => {
      element.visual!.blend_mode = "multiply";
      element.visual!.masks = [
        { ...boxMask(), id: "disabled", enabled: false },
      ];
    },
  });
  try {
    await begin();
    expect(s.write).not.toHaveBeenCalled();
    const request = s.f.call.mock.calls.find(
      ([name]) => name === "opencut_mask_geometry",
    )![1];
    expect(request).toEqual({
      project_id: "v4-project",
      expected_revision: "r0",
      sequence_id: "scene",
      element_id: "image",
      time: s.state.scene!.playhead,
    });
    expect(
      (
        screen.getByRole("spinbutton", {
          name: "Mask width (%)",
        }) as HTMLInputElement
      ).value,
    ).toBe("33.75");
    fireEvent.change(screen.getByRole("spinbutton", { name: "Mask X (%)" }), {
      target: { value: "12.5" },
    });
    const form = screen.getByRole("form");
    fireEvent.submit(form);
    fireEvent.submit(form);
    await waitFor(() => expect(s.write).toHaveBeenCalledOnce());
    const [label, [operation]] = s.write.mock.calls[0];
    expect(label).toBe("Edit mask");
    expect(operation.type).toBe("element_put");
    if (operation.type !== "element_put") throw new Error("Wrong operation");
    const expected = structuredClone(s.element);
    const mask = operation.element.visual!.masks[1];
    expect(mask.shape.kind).toBe("rectangle");
    expect("geometry" in mask.shape && mask.shape.geometry).toEqual({
      ...geometryPlan().default_geometry,
      center_x: 0.125,
    });
    expected.visual!.masks.push(mask);
    expect(operation).toEqual({
      type: "element_put",
      sequence_id: "scene",
      track_id: "track",
      element: expected,
    });
    expect(s.element.visual!.masks).toHaveLength(1);
  } finally {
    s.f.controller.dispose();
  }
});

it("preserves existing scale, stroke and disabled nodes when editing feather, and cancellation writes nothing", async () => {
  const s = await setup({ existing: true });
  try {
    await begin(true);
    fireEvent.change(
      screen.getByRole("spinbutton", { name: "Mask feather (pixels)" }),
      { target: { value: "23.75" } },
    );
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(s.write).not.toHaveBeenCalled();
    await begin(true);
    fireEvent.change(
      screen.getByRole("spinbutton", { name: "Mask feather (pixels)" }),
      { target: { value: "23.75" } },
    );
    fireEvent.submit(screen.getByRole("form"));
    await waitFor(() => expect(s.write).toHaveBeenCalledOnce());
    const operation = s.write.mock.calls[0][1][0];
    if (operation.type !== "element_put") throw new Error("Wrong operation");
    const expected = structuredClone(s.element);
    expected.visual!.masks[0].feather.value = 23.75;
    expect(operation.element).toEqual(expected);
  } finally {
    s.f.controller.dispose();
  }
});

it.each([
  "revision",
  "locked",
  "selection",
  "historical",
  "recovery",
  "connection",
  "element",
  "playhead",
])("refuses a stale %s even before React rerenders", async (kind) => {
  const s = await setup({ existing: true });
  try {
    await begin(true);
    fireEvent.change(screen.getByRole("spinbutton", { name: "Mask X (%)" }), {
      target: { value: "20" },
    });
    const view = s.state.scene!;
    if (kind === "revision") view.head.revision = "changed";
    if (kind === "locked")
      view.head.project.sequences[0].tracks[0].locked = true;
    if (kind === "selection") view.selectedId = null;
    if (kind === "historical") view.historical = structuredClone(view.head);
    if (kind === "recovery")
      view.recovery = {
        ...view.recovery!,
        error: "Uncertain original request",
      };
    if (kind === "connection") s.state.api = null;
    if (kind === "element") s.element.name = "Another edit";
    if (kind === "playhead") view.playhead = { ticks: "1", timebase: 1 };
    fireEvent.submit(screen.getByRole("form"));
    if (["revision", "selection", "historical", "playhead"].includes(kind)) {
      // The immediate submit guard refuses the stale draft; its error render
      // then runs the scope effect, retiring the entire authoring session.
      await waitFor(() => expect(screen.queryByRole("form")).toBeNull());
    } else {
      await screen.findByRole("alert");
    }
    expect(s.write).not.toHaveBeenCalled();
  } finally {
    s.f.controller.dispose();
  }
});

it("drops a late geometry read after revision change and exposes permission failures without a write", async () => {
  let resolve!: (plan: MaskGeometryPlan) => void;
  const s = await setup({
    plan: () =>
      new Promise((done) => {
        resolve = done;
      }),
  });
  try {
    fireEvent.click(screen.getByRole("button", { name: "Add rectangle mask" }));
    await waitFor(() => expect(s.read).toHaveBeenCalledOnce());
    s.state.scene!.head.revision = "changed";
    s.refresh();
    await act(async () => resolve(geometryPlan()));
    expect(screen.queryByRole("form")).toBeNull();
    expect(s.write).not.toHaveBeenCalled();
    s.read.mockRejectedValueOnce(new Error("Permission revoked"));
    fireEvent.click(screen.getByRole("button", { name: "Add rectangle mask" }));
    await screen.findByText("Permission revoked");
    expect(s.write).not.toHaveBeenCalled();
  } finally {
    s.f.controller.dispose();
  }
});

it("rejects substituted geometry and unsupported masks instead of inventing fallback bounds", async () => {
  const s = await setup({
    plan: async () => ({ ...geometryPlan(), element_id: "other" }),
  });
  try {
    fireEvent.click(screen.getByRole("button", { name: "Add rectangle mask" }));
    await screen.findByText(
      "Mask geometry differs from the requested fixed revision.",
    );
    expect(screen.queryByRole("form")).toBeNull();
    expect(s.write).not.toHaveBeenCalled();
    s.element.visual!.masks = [{ ...boxMask(), inverted: true }];
    s.refresh();
    expect(
      (screen.getByRole("button", { name: "Edit mask" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  } finally {
    s.f.controller.dispose();
  }
});

function pointerSupport() {
  class Pointer extends MouseEvent {
    pointerId: number;
    constructor(type: string, init: PointerEventInit = {}) {
      super(type, init);
      this.pointerId = init.pointerId ?? 1;
    }
  }
  vi.stubGlobal("PointerEvent", Pointer);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    left: 0,
    top: 0,
    width: 640,
    height: 360,
  } as DOMRect);
  Object.defineProperty(Element.prototype, "setPointerCapture", {
    configurable: true,
    value: vi.fn(),
  });
  Object.defineProperty(Element.prototype, "releasePointerCapture", {
    configurable: true,
    value: vi.fn(),
  });
  Object.defineProperty(Element.prototype, "hasPointerCapture", {
    configurable: true,
    value: () => true,
  });
}

it("localizes mask actions, properties, canvas handles and validation in Chinese", async () => {
  pointerSupport();
  const s = await setup();
  try {
    act(() => preferences({ language: "zh" }));
    fireEvent.click(screen.getByRole("button", { name: "添加椭圆蒙版" }));
    await screen.findByRole("form", { name: "编辑蒙版" });
    expect(screen.getByRole("heading", { name: "椭圆蒙版" })).toBeTruthy();
    expect(
      screen.getByRole("spinbutton", { name: "蒙版旋转（度）" }),
    ).toBeTruthy();
    expect(screen.getByRole("group", { name: "画布蒙版控件" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "移动蒙版" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "旋转蒙版" })).toBeTruthy();
    fireEvent.change(
      screen.getByRole("spinbutton", { name: "蒙版羽化（像素）" }),
      { target: { value: "1001" } },
    );
    fireEvent.submit(screen.getByRole("form", { name: "编辑蒙版" }));
    expect(
      await screen.findByText(
        "请输入有效的蒙版尺寸，并将羽化设为 0 至 1000 像素。",
      ),
    ).toBeTruthy();
    expect(s.write).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(screen.queryByRole("form")).toBeNull();
  } finally {
    s.f.controller.dispose();
  }
});

it("captures a drag, changes only the local draft and commits once on pointerup", async () => {
  pointerSupport();
  const s = await setup({ existing: true });
  try {
    await begin(true);
    const hit = screen.getByRole("button", { name: "Move mask" });
    fireEvent.pointerDown(hit, {
      button: 0,
      pointerId: 4,
      clientX: 320,
      clientY: 180,
    });
    fireEvent.pointerMove(hit, { pointerId: 4, clientX: 352, clientY: 198 });
    expect(s.write).not.toHaveBeenCalled();
    expect(
      Number(
        (
          screen.getByRole("spinbutton", {
            name: "Mask X (%)",
          }) as HTMLInputElement
        ).value,
      ),
    ).toBeCloseTo(10);
    fireEvent.pointerUp(hit, { pointerId: 4, clientX: 352, clientY: 198 });
    fireEvent.pointerUp(hit, { pointerId: 4, clientX: 352, clientY: 198 });
    await waitFor(() => expect(s.write).toHaveBeenCalledOnce());
    const operation = s.write.mock.calls[0][1][0];
    if (operation.type !== "element_put") throw new Error("Wrong operation");
    const expected = boxMask();
    expected.shape.geometry.center_x = 0.1;
    expected.shape.geometry.center_y = 0.1;
    expect(operation.element.visual!.masks[0]).toEqual(expected);
  } finally {
    s.f.controller.dispose();
  }
});

it.each(["escape", "pointercancel", "revision"])(
  "abandons the captured gesture on %s without publishing",
  async (kind) => {
    pointerSupport();
    const s = await setup({ existing: true });
    try {
      await begin(true);
      const hit = screen.getByRole("button", { name: "Resize mask right" });
      fireEvent.pointerDown(hit, {
        button: 0,
        pointerId: 1,
        clientX: 416,
        clientY: 180,
      });
      fireEvent.pointerMove(hit, { pointerId: 1, clientX: 432, clientY: 180 });
      if (kind === "escape") fireEvent.keyDown(window, { key: "Escape" });
      if (kind === "pointercancel")
        fireEvent.pointerCancel(hit, { pointerId: 1 });
      if (kind === "revision") {
        s.state.scene!.head.revision = "advanced";
        s.refresh();
      }
      fireEvent.pointerUp(hit, { pointerId: 1, clientX: 432, clientY: 180 });
      expect(s.write).not.toHaveBeenCalled();
      if (kind === "pointercancel")
        expect(
          Number(
            (
              screen.getByRole("spinbutton", {
                name: "Mask width (%)",
              }) as HTMLInputElement
            ).value,
          ),
        ).toBe(60);
      else expect(screen.queryByRole("form")).toBeNull();
    } finally {
      s.f.controller.dispose();
    }
  },
);

it.each(["busy", "loading"])(
  "discards an interrupted gesture when %s clears without committing its intermediate draft",
  async (blocked) => {
    pointerSupport();
    const s = await setup({ existing: true });
    try {
      await begin(true);
      const hit = screen.getByRole("button", { name: "Move mask" });
      fireEvent.pointerDown(hit, {
        button: 0,
        pointerId: 1,
        clientX: 320,
        clientY: 180,
      });
      fireEvent.pointerMove(hit, {
        pointerId: 1,
        clientX: 352,
        clientY: 180,
      });
      expect(
        Number(
          (
            screen.getByRole("spinbutton", {
              name: "Mask X (%)",
            }) as HTMLInputElement
          ).value,
        ),
      ).toBeCloseTo(10);
      const phase = s.state.scene!.audioPreview.phase;
      if (blocked === "busy") s.state.busy = true;
      else s.state.scene!.audioPreview.phase = "loading";
      s.refresh();
      await waitFor(() => expect(screen.queryByRole("form")).toBeNull());
      if (blocked === "busy") s.state.busy = false;
      else s.state.scene!.audioPreview.phase = phase;
      s.refresh();
      fireEvent.pointerUp(hit, { pointerId: 1, clientX: 352, clientY: 180 });
      expect(screen.queryByRole("button", { name: "Save mask" })).toBeNull();
      expect(s.write).not.toHaveBeenCalled();
      // Reopening reads the unchanged source; saving it is still a no-op.
      await begin(true);
      expect(
        Number(
          (
            screen.getByRole("spinbutton", {
              name: "Mask X (%)",
            }) as HTMLInputElement
          ).value,
        ),
      ).toBe(0);
      fireEvent.submit(screen.getByRole("form"));
      expect(s.write).not.toHaveBeenCalled();
      expect(screen.queryByRole("form")).toBeNull();
    } finally {
      s.f.controller.dispose();
    }
  },
);
