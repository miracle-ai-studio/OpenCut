// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { WorkspaceProvider } from "../../context";
import { preferences } from "../../../shared/locale";
import { setup as createFixture } from "../fixture";
import {
  TextActions,
  TextAuthoringProvider,
  TextPanel,
  useFontApplication,
} from "./workspace";
import { textFonts, textInitial } from "./fixture";
import type { TextPlanRequest, TextPlanResult } from "./planning";
import { textPlanValues, checkTextPlan } from "./planning";

beforeEach(() => {
  preferences({ language: "en" });
});

function FontApplicationSurface() {
  const application = useFontApplication("bold-italic")!;
  return (
    <button
      disabled={!!application.disabledReason}
      title={application.disabledReason ?? ""}
      onClick={application.apply}
    >
      Apply target font
    </button>
  );
}
async function fontTargetFixture() {
  const f = await setup();
  const track = f.current().project.sequences[0].tracks[0];
  track.kind = "text";
  Object.assign(track.elements[0], textInitial());
  track.elements[0].content = { kind: "text", text: textInitial().text };
  await f.controller.scene.refresh();
  f.controller.scene.select("image");
  return f;
}
it("applies only the chosen font to the explicit text target with one original edit transaction", async () => {
  const f = await fontTargetFixture();
  try {
    const commit = vi
      .spyOn(f.controller.scene, "applyPlanned")
      .mockResolvedValue({
        base_revision: "r0",
        revision: "r1",
        transaction_id: "font-edit",
        changed_ids: ["image"],
        created_ids: [],
        deleted_ids: [],
      });
    render(
      <WorkspaceProvider controller={f.controller}>
        <TextAuthoringProvider available>
          <FontApplicationSurface />
          <TextPanel />
        </TextAuthoringProvider>
      </WorkspaceProvider>,
    );
    const apply = screen.getByRole("button", { name: "Apply target font" });
    fireEvent.click(apply);
    fireEvent.click(apply);
    await waitFor(() => expect(commit).toHaveBeenCalledOnce());
    expect(f.plans).toHaveLength(1);
    expect(f.plans[0].command).toEqual({
      kind: "edit",
      target: { track_id: "track", element_id: "image" },
      values: textPlanValues({
        ...textInitial(),
        text: {
          ...textInitial().text,
          font: { kind: "locked", font_id: "bold-italic" },
          font_weight: "bold",
          font_style: "italic",
        },
      }),
    });
    expect(commit.mock.calls[0][0].idempotency_key).toBe(
      f.plans[0].idempotency_key,
    );
  } finally {
    f.controller.dispose();
  }
});
it("does not apply a font plan after the selected target changes", async () => {
  const f = await fontTargetFixture();
  try {
    const original = f.call.getMockImplementation()!;
    let complete!: (result: TextPlanResult) => void;
    f.call.mockImplementation(async (...args) =>
      args[0] === "opencut_text_plan"
        ? ((await new Promise<TextPlanResult>((resolve) => {
            f.plans.push(args[1] as TextPlanRequest);
            complete = resolve;
          })) as never)
        : original(...args),
    );
    const commit = vi.spyOn(f.controller.scene, "applyPlanned");
    render(
      <WorkspaceProvider controller={f.controller}>
        <TextAuthoringProvider available>
          <FontApplicationSurface />
        </TextAuthoringProvider>
      </WorkspaceProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Apply target font" }));
    await waitFor(() => expect(f.plans).toHaveLength(1));
    await act(async () => {
      f.controller.scene.select(null);
      complete(f.result(f.plans[0]));
    });
    expect(commit).not.toHaveBeenCalled();
  } finally {
    f.controller.dispose();
  }
});
it("keeps locked and incompatible targets unavailable without a mutation plan", async () => {
  const f = await fontTargetFixture();
  try {
    f.current().project.sequences[0].tracks[0].locked = true;
    await f.controller.scene.refresh();
    render(
      <WorkspaceProvider controller={f.controller}>
        <TextAuthoringProvider available>
          <FontApplicationSurface />
        </TextAuthoringProvider>
      </WorkspaceProvider>,
    );
    const button = screen.getByRole("button", {
      name: "Apply target font",
    }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    expect(button.title).toMatch(/Unlock/);
    fireEvent.click(button);
    await act(async () => f.controller.scene.select("track"));
    expect(button.disabled).toBe(true);
    expect(button.title).toMatch(/Select one/);
    expect(f.plans).toHaveLength(0);
  } finally {
    f.controller.dispose();
  }
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
async function setup() {
  const f = await createFixture();
  f.current().project.fonts = textFonts();
  await f.controller.scene.refresh();
  const original = f.call.getMockImplementation()!;
  const plans: TextPlanRequest[] = [];
  const result = (request: TextPlanRequest): TextPlanResult => {
    const initial = textInitial();
    if (request.command.kind === "defaults")
      return {
        revision: "r0",
        initial,
        transaction: null,
        selection: null,
      };
    const target =
      request.command.kind === "create"
        ? {
            track_id: request.command.track.track_id,
            element_id: request.command.element_id,
          }
        : request.command.target;
    initial.text = request.command.values.text;
    initial.timeline_range = {
      start: { ticks: "1", timebase: 24 },
      duration: { ticks: "5", timebase: 4 },
    };
    return {
      revision: request.expected_revision,
      initial,
      selection: target,
      transaction: {
        project_id: request.project_id,
        expected_revision: request.expected_revision,
        idempotency_key: request.idempotency_key,
        label: "Create text",
        operations: [
          {
            type: "element_put",
            sequence_id: request.sequence_id,
            track_id: target.track_id,
            element: {
              ...f.current().project.sequences[0].tracks[0].elements[0],
              id: target.element_id,
              name: initial.name,
              timeline_range: initial.timeline_range,
              content: { kind: "text", text: initial.text },
            },
          },
        ],
      },
    };
  };
  f.call.mockImplementation(async (...args) => {
    const [name, input] = args;
    if (name === "opencut_text_plan") {
      plans.push(structuredClone(input as TextPlanRequest));
      return result(input as TextPlanRequest) as never;
    }
    return original(...args);
  });
  return { ...f, plans, result };
}
function Surface({ available = true }: { available?: boolean }) {
  return (
    <TextAuthoringProvider available={available}>
      <TextActions />
      <TextPanel />
    </TextAuthoringProvider>
  );
}

it("keeps the old service surface unchanged until text-frame execution is confirmed", async () => {
  const f = await setup();
  try {
    render(
      <WorkspaceProvider controller={f.controller}>
        <Surface available={false} />
      </WorkspaceProvider>,
    );
    expect(
      screen.queryByRole("button", { name: "Add text at playhead" }),
    ).toBeNull();
    expect(f.plans).toHaveLength(0);
  } finally {
    f.controller.dispose();
  }
});

it("uses Rust defaults, displays actual planned time and submits only the confirmed original transaction", async () => {
  const f = await setup();
  try {
    const commit = vi
      .spyOn(f.controller.scene, "applyPlanned")
      .mockResolvedValue({
        base_revision: "r0",
        revision: "r1",
        transaction_id: "text-tx",
        changed_ids: [],
        created_ids: [],
        deleted_ids: [],
      });
    render(
      <WorkspaceProvider controller={f.controller}>
        <Surface />
      </WorkspaceProvider>,
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Add text at playhead" }),
    );
    await screen.findByRole("form", { name: "New text" });
    expect(f.plans[0].command).toEqual({
      kind: "defaults",
      font_id: "regular",
      start: { kind: "exact", time: { ticks: "0", timebase: 1 } },
    });
    fireEvent.change(screen.getByLabelText("Text content"), {
      target: { value: "Title\n你好" },
    });
    fireEvent.change(screen.getByLabelText("Start (seconds)"), {
      target: { value: "0.051" },
    });
    fireEvent.change(screen.getByLabelText("Duration (seconds)"), {
      target: { value: "1.234567891" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save text" }));
    const review = await screen.findByRole("region", {
      name: "Review text changes",
    });
    expect(f.plans).toHaveLength(2);
    expect(commit).not.toHaveBeenCalled();
    expect(f.plans[1].command).toMatchObject({
      kind: "create",
      values: {
        start: { kind: "decimal_seconds", value: "0.051" },
        duration: { kind: "decimal_seconds", value: "1.234567891" },
      },
    });
    expect(within(review).getByText("0.041666667")).toBeTruthy();
    expect(within(review).getByText("1.250000000")).toBeTruthy();
    fireEvent.click(
      within(review).getByRole("button", { name: "Apply text changes" }),
    );
    await waitFor(() =>
      expect(commit).toHaveBeenCalledExactlyOnceWith(
        f.result(f.plans[1]).transaction,
        "scene",
      ),
    );
  } finally {
    f.controller.dispose();
  }
});

it.each(["revision", "selection"])(
  "external %s changes remove a reviewed proposal before it can be applied",
  async (change) => {
    const f = await setup();
    try {
      const commit = vi.spyOn(f.controller.scene, "applyPlanned");
      render(
        <WorkspaceProvider controller={f.controller}>
          <Surface />
        </WorkspaceProvider>,
      );
      fireEvent.click(
        screen.getByRole("button", { name: "Add text at playhead" }),
      );
      await screen.findByRole("form");
      fireEvent.click(screen.getByRole("button", { name: "Save text" }));
      await screen.findByRole("region", { name: "Review text changes" });
      await act(async () => {
        if (change === "revision") {
          f.current().revision = "external";
          await f.controller.scene.refresh();
        } else f.controller.scene.select("image");
      });
      expect(
        screen.queryByRole("button", { name: "Apply text changes" }),
      ).toBeNull();
      expect(commit).not.toHaveBeenCalled();
    } finally {
      f.controller.dispose();
    }
  },
);

it("rejects a planner reply that swaps revision, idempotency key or selected title", async () => {
  const f = await setup();
  try {
    const initial = textInitial();
    const request: TextPlanRequest = {
      project_id: "v4-project",
      expected_revision: "r0",
      sequence_id: "scene",
      idempotency_key: "original",
      command: {
        kind: "create",
        element_id: "title",
        track: { kind: "new", track_id: "titles" },
        values: textPlanValues(initial),
      },
    };
    for (const corrupt of [
      (value: TextPlanResult) => {
        value.revision = "other";
      },
      (value: TextPlanResult) => {
        value.transaction!.idempotency_key = "replacement";
      },
      (value: TextPlanResult) => {
        value.selection!.element_id = "other";
      },
    ]) {
      const result = f.result(request);
      corrupt(result);
      expect(() => checkTextPlan(request, result)).toThrow("text plan differs");
    }
  } finally {
    f.controller.dispose();
  }
});

it("routes a variable-only atlas face through the shared catalog,source and import action", async () => {
  const f = await setup(),
    importFont = vi.fn(async () => ({
      base_revision: "r0",
      revision: "r1",
      transaction_id: "font-tx",
      changed_ids: [],
      created_ids: [],
      deleted_ids: [],
    }));
  const original = f.call.getMockImplementation()!;
  vi.spyOn(f.api.sdk, "hostCapabilities").mockResolvedValue({
    tools: ["opencut_font_catalog"],
  } as never);
  const face = {
    id: "variable-recipe",
    family: "Variable Only",
    weight: 400,
    style: "normal",
    derivation_id: "derived-proof",
    content_hash: "d".repeat(64),
    size_bytes: "120",
  };
  f.call.mockImplementation(async (...args) => {
    if (args[0] === "opencut_font_catalog")
      return {
        download_available: false,
        unavailable_reason: "Static unavailable",
        source_commit: "static",
        classic_atlas_sha256: "a".repeat(64),
        classic_family_count: 1,
        pinned_face_count: 1,
        fonts: [
          {
            ...face,
            display_family: face.family,
            requested_style: null,
            kind: "derived",
            render_profile: "static_outline",
            alias_reason: null,
            import_available: true,
            unavailable_reason: null,
          },
        ],
        families: [
          {
            family: "Variable Only",
            category: "google",
            faces: [
              {
                weight: 400,
                style: "normal",
                catalog_id: null,
                unavailable_reason: "no_pinned_static_source",
              },
            ],
          },
        ],
      } as never;
    if (args[0] === "opencut_font_source")
      return {
        face: {
          ...face,
          display_family: face.family,
          requested_style: null,
          kind: "derived",
          render_profile: "static_outline",
          alias_reason: null,
          import_available: true,
          unavailable_reason: null,
        },
        license_url: "https://example.test/OFL.txt",
        license_sha256: "c".repeat(64),
        details: {
          kind: "derived",
          source: {
            face,
            source_commit: "derived",
            source_url: "https://example.test/variable.ttf",
            source_sha256: "e".repeat(64),
            source_size_bytes: "240",
            license_url: "https://example.test/OFL.txt",
            license_id: "OFL-1.1",
            license_notice: "Variable license",
            axes: [{ tag: "wght", coordinate: "400" }],
            proof_json: "{}",
          },
        },
      } as never;
    return original(...args);
  });
  render(
    <WorkspaceProvider controller={f.controller}>
      <TextAuthoringProvider available importFont={importFont}>
        <TextActions />
        <TextPanel />
      </TextAuthoringProvider>
    </WorkspaceProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Import font" }));
  await screen.findByText("Variable license");
  const form = screen.getByRole("form", { name: "Import font" });
  fireEvent.submit(form);
  await waitFor(() => expect(importFont).toHaveBeenCalledOnce());
  expect(importFont.mock.calls[0]).toMatchObject([
    {
      project_id: "v4-project",
      expected_revision: "r0",
      catalog_id: "variable-recipe",
      required_text: "OpenCut 你好",
    },
  ]);
  expect(
    f.call.mock.calls.some(([name]) => name === "opencut_font_source"),
  ).toBe(true);
});

it("inserts default text with native defaults and one original transaction without opening a confirmation form", async () => {
  const f = await setup();
  try {
    const commit = vi
      .spyOn(f.controller.scene, "applyPlanned")
      .mockResolvedValue({
        base_revision: "r0",
        revision: "r1",
        transaction_id: "text-card",
        changed_ids: [],
        created_ids: [],
        deleted_ids: [],
      });
    render(
      <WorkspaceProvider controller={f.controller}>
        <TextAuthoringProvider available>
          <TextActions card />
          <TextPanel />
        </TextAuthoringProvider>
      </WorkspaceProvider>,
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Add text at playhead" }),
    );
    await waitFor(() => expect(commit).toHaveBeenCalledOnce());
    expect(f.plans.map((p) => p.command.kind)).toEqual(["defaults", "create"]);
    expect(f.plans[0].command).toMatchObject({
      font_id: "regular",
      start: { kind: "exact", time: { ticks: "0", timebase: 1 } },
    });
    expect(commit.mock.calls[0][0].idempotency_key).toBe(
      f.plans[1].idempotency_key,
    );
    expect(screen.queryByRole("form", { name: "New text" })).toBeNull();
  } finally {
    f.controller.dispose();
  }
});
