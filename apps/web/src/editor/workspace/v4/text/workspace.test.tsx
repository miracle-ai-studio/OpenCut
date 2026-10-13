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
import { setupV4 } from "../fixture";
import {
  V4TextActions,
  V4TextAuthoringProvider,
  V4TextPanel,
} from "./workspace";
import { textFonts, textInitial } from "./fixture";
import type { TextPlanRequest, TextPlanResult } from "./planning";
import { textPlanValues, checkTextPlan } from "./planning";

beforeEach(() => {
  preferences({ language: "en" });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
async function setup() {
  const f = await setupV4();
  f.current().project.fonts = textFonts();
  await f.controller.v4.refresh();
  const original = f.call.getMockImplementation()!;
  const plans: TextPlanRequest[] = [];
  const result = (request: TextPlanRequest): TextPlanResult => {
    const initial = textInitial();
    if (request.command.kind === "defaults")
      return {
        schema_version: 1,
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
      schema_version: 1,
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
    if (name === "opencut_text_v1_plan") {
      plans.push(structuredClone(input as TextPlanRequest));
      return result(input as TextPlanRequest) as never;
    }
    return original(...args);
  });
  return { ...f, plans, result };
}
function Surface({ available = true }: { available?: boolean }) {
  return (
    <V4TextAuthoringProvider available={available}>
      <V4TextActions />
      <V4TextPanel />
    </V4TextAuthoringProvider>
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
    const commit = vi.spyOn(f.controller.v4, "applyPlanned").mockResolvedValue({
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
      const commit = vi.spyOn(f.controller.v4, "applyPlanned");
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
          await f.controller.v4.refresh();
        } else f.controller.v4.select("image");
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
