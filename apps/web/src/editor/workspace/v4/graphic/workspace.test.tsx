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
import { graphicInitial } from "./fixture";
import { V4ResourceCatalogPanel } from "../catalog_resources_v1-panel";
import {
  V4GraphicActions,
  V4GraphicAuthoringProvider,
  V4GraphicEditButton,
  V4GraphicPanel,
} from "./workspace";
const mocks = vi.hoisted(() => ({ state: {} as any }));
vi.mock("../../context", () => ({ useWorkspace: () => mocks.state }));
beforeEach(() => preferences({ language: "en" }));
afterEach(() => cleanup());
function fixture(
  options: {
    old?: boolean;
    locked?: boolean;
    badKey?: boolean;
    blocked?: boolean;
    catalog?: boolean;
  } = {},
) {
  const initial = graphicInitial();
  const element = {
    id: "shape",
    name: initial.name,
    timeline_range: initial.timeline_range,
    content: { kind: "graphic", graphic: initial.graphic },
    visual: {
      transform: initial.transform,
      opacity: initial.opacity,
      blend_mode: "multiply",
      effects: [{ id: "untouched" }],
      masks: [],
      animation: { opaque: "preserved" },
    },
  };
  const head = {
    revision: "fixed",
    project: {
      id: "project",
      assets: [],
      sequences: [
        {
          id: "scene",
          tracks: [
            {
              id: "graphics",
              kind: "graphic",
              name: "Graphics",
              locked: !!options.locked,
              elements: [element],
            },
          ],
        },
      ],
    },
  };
  const call = vi.fn(async (_tool: string, request: any) =>
    _tool === "opencut_catalog_resources_v1_list"
      ? { schema_version: 1, sound_download_available: false, sound_unavailable_reason: null, brands_available: false,
          entries: [
            ["rectangle", ["rectangle", "square", "box"]],
            ["ellipse", ["ellipse", "circle", "oval"]],
            ["polygon", ["polygon", "triangle", "pentagon", "hexagon", "diamond"]],
            ["star", ["star", "sparkle", "burst"]],
          ].map(([name, keywords]) => ({ id: `shapes:${name}`, name, category: "shapes", keywords,
            source_url: "https://github.com/OpenCut-app/OpenCut", source_sha256: null, license: { identifier: "MIT", source_url: null, notice: null, attribution: null }, width: null, height: null })) }
      : request.command.kind === "defaults"
      ? {
          schema_version: 1,
          revision: "fixed",
          initial: {
            ...initial,
            graphic: {
              ...initial.graphic,
              geometry: {
                kind: request.command.geometry,
                ...(request.command.geometry === "star"
                  ? { points: 5, depth_percent: 50 }
                  : {}),
              },
            },
          },
          transaction: null,
          selection: null,
        }
      : {
          schema_version: 1,
          revision: "fixed",
          initial,
          transaction: {
            project_id: "project",
            expected_revision: "fixed",
            idempotency_key: options.badKey
              ? "swapped"
              : request.idempotency_key,
            operations: [
              { type: "sequence", operation: { kind: "element_put", element } },
            ],
          },
          selection:
            request.command.kind === "edit"
              ? request.command.target
              : {
                  track_id: request.command.track.track_id,
                  element_id: request.command.element_id,
                },
        },
  );
  const apply = vi.fn(async (_transaction: unknown, _sequenceId: string) => {}),
    select = vi.fn();
  const api = {
    connectionIdentity: JSON.stringify(["https://catalog.example", "principal", "credential-id", false]),
    authenticateConnection: vi.fn(async () => {}),
    sdk: {
      hostCapabilities: vi.fn(async () => ({
        tools: options.old ? [] : ["opencut_graphic_v1_plan"],
      })),
      call,
    },
  };
  mocks.state = {
    api,
    busy: false,
    v4: {
      head,
      media: { attempt: null },
      sequenceId: "scene",
      playhead: { ticks: "7", timebase: 30 },
      selectedId: "shape",
      ...(options.blocked ? { recovery: { record: { original: true } } } : {}),
    },
    controller: {
      getSnapshot: () => mocks.state,
      report: vi.fn(),
      v4: {
        captionRecovery: { blocked: () => false },
        applyPlanned: apply,
        select,
      },
    },
  };
  render(
    <V4GraphicAuthoringProvider>
      {options.catalog ? <V4ResourceCatalogPanel /> : <V4GraphicActions />}
      <V4GraphicEditButton elementId="shape" locked={!!options.locked} />
      <V4GraphicPanel />
    </V4GraphicAuthoringProvider>,
  );
  return { call, apply, select, head, initial, element };
}
it("creates selected shape using Rust defaults, reviews without a write, applies exactly the original transaction and selects its target", async () => {
  const f = fixture();
  await screen.findByRole("button", { name: "Add graphic" });
  fireEvent.change(screen.getByRole("combobox", { name: "Graphic shape" }), {
    target: { value: "star" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Add graphic" }));
  await screen.findByRole("form", { name: "Edit graphic" });
  expect(f.call.mock.calls[0][1].command).toEqual({
    kind: "defaults",
    geometry: "star",
    start: { kind: "exact", time: { ticks: "7", timebase: 30 } },
  });
  fireEvent.submit(screen.getByRole("form"));
  await screen.findByRole("button", { name: "Apply graphic changes" });
  expect(f.apply).not.toHaveBeenCalled();
  expect(f.head.revision).toBe("fixed");
  const request = f.call.mock.calls[1][1];
  expect(request.command.track.kind).toBe("new");
  fireEvent.click(
    screen.getByRole("button", { name: "Apply graphic changes" }),
  );
  await waitFor(() => expect(f.apply).toHaveBeenCalledOnce());
  expect(f.apply.mock.calls[0][0]).toMatchObject({
    idempotency_key: request.idempotency_key,
    expected_revision: "fixed",
  });
  expect(f.select).toHaveBeenCalledWith(request.command.element_id);
});
it("uses the catalog's single query for Classic shape keywords, empty results and the actual create target", async () => {
  const f = fixture({ catalog: true });
  fireEvent.change(await screen.findByRole("combobox", { name: "Category" }), { target: { value: "shapes" } });
  await screen.findByRole("combobox", { name: "Graphic shape" });
  const search = screen.getByRole("textbox", { name: "Search catalog" });
  fireEvent.change(search, { target: { value: " CiRcLe " } });
  expect(Array.from((screen.getByRole("combobox", { name: "Graphic shape" }) as HTMLSelectElement).options).map(option => option.value)).toEqual(["ellipse"]);
  fireEvent.change(search, { target: { value: "triangle" } });
  expect((screen.getByRole("combobox", { name: "Graphic shape" }) as HTMLSelectElement).value).toBe("polygon");
  fireEvent.change(search, { target: { value: "no-such-shape" } });
  expect(screen.getByRole("status").textContent).toBe("No matching shapes.");
  expect(screen.queryByRole("button", { name: "Add graphic" })).toBeNull();
  fireEvent.change(search, { target: { value: "sparkle" } });
  fireEvent.click(screen.getByRole("button", { name: "Add graphic" }));
  await screen.findByRole("form", { name: "Edit graphic" });
  const planned = f.call.mock.calls.find(([tool]) => tool === "opencut_graphic_v1_plan")![1];
  expect(planned.command).toMatchObject({ kind: "defaults", geometry: "star" });
  expect(f.apply).not.toHaveBeenCalled();
});
it("edits selected graphic through the same planner with exact untouched colors and does not reconstruct blend, effects or animation", async () => {
  const f = fixture();
  fireEvent.click(await screen.findByRole("button", { name: "Edit graphic" }));
  fireEvent.change(screen.getByLabelText("Graphic name"), {
    target: { value: "Renamed" },
  });
  fireEvent.submit(screen.getByRole("form"));
  await screen.findByRole("button", { name: "Apply graphic changes" });
  const request = f.call.mock.calls[0][1];
  expect(request.command.target).toEqual({
    track_id: "graphics",
    element_id: "shape",
  });
  expect(request.command.values.graphic).toEqual(f.initial.graphic);
  expect(request.command.values.transform).toEqual(f.initial.transform);
  expect(request.command.values).not.toHaveProperty("effects");
  expect(f.element.visual.blend_mode).toBe("multiply");
});
it("rejects a swapped transaction key before any public apply", async () => {
  const f = fixture({ badKey: true });
  fireEvent.click(await screen.findByRole("button", { name: "Edit graphic" }));
  fireEvent.submit(screen.getByRole("form"));
  await screen.findByRole("alert");
  expect(f.apply).not.toHaveBeenCalled();
  expect(
    screen.queryByRole("button", { name: "Apply graphic changes" }),
  ).toBeNull();
});
it("hides entry on an old server", async () => {
  const f = fixture({ old: true });
  await waitFor(() =>
    expect(mocks.state.api.sdk.hostCapabilities).toHaveBeenCalled(),
  );
  expect(screen.queryByRole("button", { name: "Add graphic" })).toBeNull();
  expect(f.call).not.toHaveBeenCalled();
});
it.each([{ locked: true }, { blocked: true }])(
  "blocks locked targets and another public recovery slot %j",
  async (options) => {
    const f = fixture(options);
    const edit = await screen.findByRole("button", { name: "Edit graphic" });
    expect((edit as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(edit);
    expect(f.call).not.toHaveBeenCalled();
  },
);
it("discards a plan if the fixed head changes while the read is in flight", async () => {
  const f = fixture();
  fireEvent.click(await screen.findByRole("button", { name: "Edit graphic" }));
  let release!: (result: unknown) => void;
  f.call.mockImplementationOnce(
    (_tool: string, _request: any) =>
      new Promise((resolve) => {
        release = resolve;
      }) as any,
  );
  fireEvent.submit(screen.getByRole("form"));
  mocks.state.v4.head.revision = "new-head";
  release({
    schema_version: 1,
    revision: "fixed",
    initial: f.initial,
    transaction: null,
    selection: { track_id: "graphics", element_id: "shape" },
  });
  await screen.findByRole("alert");
  expect(f.apply).not.toHaveBeenCalled();
  expect(
    screen.queryByRole("button", { name: "Apply graphic changes" }),
  ).toBeNull();
});
it("keeps an unknown public apply in the original recovery controller instead of issuing a new plan or key", async () => {
  const f = fixture();
  fireEvent.click(await screen.findByRole("button", { name: "Edit graphic" }));
  fireEvent.submit(screen.getByRole("form"));
  await screen.findByRole("button", { name: "Apply graphic changes" });
  f.apply.mockImplementationOnce(async () => {
    mocks.state.v4.recovery = { record: { original: true } };
    throw new Error("Receipt lost; confirm the original edit.");
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Apply graphic changes" }),
  );
  await screen.findByRole("alert");
  expect(
    (
      screen.getByRole("button", {
        name: "Apply graphic changes",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  fireEvent.click(
    screen.getByRole("button", { name: "Apply graphic changes" }),
  );
  expect(f.apply).toHaveBeenCalledOnce();
  expect(f.call).toHaveBeenCalledOnce();
});
