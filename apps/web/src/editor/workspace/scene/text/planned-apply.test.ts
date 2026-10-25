// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { setup } from "../fixture";
import type { ToolInput } from "../../../../lib/editor-sdk";

afterEach(() => {
  vi.restoreAllMocks();
});
// Exercise the common public-write port with the existing known bookmark
// fixture. Text operation semantics are proved in the Rust planner's tests.
const original = (): ToolInput<"opencut_edit_apply"> => ({
  project_id: "v4-project",
  expected_revision: "r0",
  idempotency_key: "planner-original-key",
  label: "Original planned edit",
  operations: [
    {
      type: "bookmark",
      operation: {
        kind: "put",
        bookmark: {
          id: "note",
          sequence_id: "scene",
          time: { ticks: "0", timebase: 1 },
          duration: null,
          note: "Original",
          color: null,
        },
      },
    },
  ],
});

it("stages and applies the exact planned request once without re-planning or replacing its key", async () => {
  const f = await setup();
  try {
    const request = original();
    await f.controller.scene.applyPlanned(request, "scene");
    const stage = f.call.mock.calls.find(
      ([name]) => name === "opencut_edit_recovery_stage",
    )![1] as ToolInput<"opencut_edit_recovery_stage">;
    expect(stage.context).toEqual({
      intent: { kind: "public_apply", request },
      sequence_id: "scene",
    });
    expect(
      f.call.mock.calls
        .filter(([name]) => name === "opencut_edit_apply")
        .map(([, value]) => value),
    ).toEqual([request]);
    expect(
      f.call.mock.calls.filter(([name]) => name === "opencut_edit_plan"),
    ).toHaveLength(0);
    expect(f.controller.getSnapshot().busy).toBe(false);
    expect(f.controller.getSnapshot().scene!.head.revision).toBe("r1");
  } finally {
    f.controller.dispose();
  }
});

it("unknown outcomes retain the original host intent and retry it through the existing recovery coordinator", async () => {
  const f = await setup();
  try {
    const base = f.call.getMockImplementation()!;
    let unknown = true;
    f.call.mockImplementation(async (...args) => {
      if (args[0] === "opencut_edit_apply" && unknown) {
        unknown = false;
        throw new Error("Connection interrupted");
      }
      return base(...args);
    });
    const request = original();
    await expect(
      f.controller.scene.applyPlanned(request, "scene"),
    ).rejects.toThrow("Connection interrupted");
    expect(f.controller.getSnapshot().scene!.recovery?.phase).toBe("unconfirmed");
    expect(f.controller.getSnapshot().busy).toBe(false);
    await f.controller.scene.retryOriginalEdit();
    expect(
      f.call.mock.calls
        .filter(([name]) => name === "opencut_edit_apply")
        .map(([, value]) => value),
    ).toEqual([request, request]);
    expect(f.controller.getSnapshot().scene!.recovery?.record).toBeNull();
  } finally {
    f.controller.dispose();
  }
});

it("rejects a planned write after changing project, scene, revision or historical view", async () => {
  const f = await setup();
  try {
    await expect(
      f.controller.scene.applyPlanned(
        { ...original(), project_id: "other" },
        "scene",
      ),
    ).rejects.toThrow("target changed");
    await expect(
      f.controller.scene.applyPlanned(original(), "other-scene"),
    ).rejects.toThrow("target changed");
    await expect(
      f.controller.scene.applyPlanned(
        { ...original(), expected_revision: "old" },
        "scene",
      ),
    ).rejects.toThrow("target changed");
    await f.controller.scene.inspectRevision("r0");
    await expect(
      f.controller.scene.applyPlanned(original(), "scene"),
    ).rejects.toThrow("target changed");
    expect(
      f.call.mock.calls.filter(
        ([name]) => name === "opencut_edit_recovery_stage",
      ),
    ).toHaveLength(0);
  } finally {
    f.controller.dispose();
  }
});
