// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { WorkspaceProvider } from "../../context";
import { preferences } from "../../../shared/locale";
import { EditRecoveryPanel } from "../edit-recovery-panel";
import { setup } from "../fixture";
import { emptyEditRecovery } from "../edit-recovery";
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
const receipt = {
  base_revision: "r0",
  revision: "r1",
  transaction_id: "font-tx",
  created_ids: ["font"],
  changed_ids: [],
  deleted_ids: [],
};
it("shows the shared explicit retry for the original font import", async () => {
  preferences({ language: "en" });
  const f = await setup();
  try {
    f.controller.getSnapshot().scene!.recovery = {
      ...emptyEditRecovery(),
      phase: "unconfirmed",
      error: "Response lost",
      record: {
        recovery_id: "original",
        project_id: "v4-project",
        request_digest: "host-digest",
        receipt: null,
        context: {
          sequence_id: "scene",
          intent: {
            kind: "font_import",
            request: {
              project_id: "v4-project",
              expected_revision: "r0",
              idempotency_key: "original-key",
              font_id: "font",
              catalog_id: "catalog",
              required_text: "Text",
            },
          },
        },
      },
    };
    const retry = vi
      .spyOn(f.controller.scene.recovery, "retry")
      .mockResolvedValue({ kind: "font_import", transaction: receipt });
    render(
      <WorkspaceProvider controller={f.controller}>
        <EditRecoveryPanel />
      </WorkspaceProvider>,
    );
    expect(
      screen.getByRole("region", { name: "Check the last save" }),
    ).toBeTruthy();
    expect(retry).not.toHaveBeenCalled();
    fireEvent.click(
      screen.getByRole("button", { name: "Retry saving" }),
    );
    await waitFor(() => expect(retry).toHaveBeenCalledTimes(1));
  } finally {
    f.controller.dispose();
  }
});
it("a completed font notice cannot obscure a later unresolved edit", async () => {
  preferences({ language: "en" });
  const f = await setup();
  try {
    const state = f.controller.getSnapshot().scene!;
    state.recovery = {
      ...emptyEditRecovery(),
      phase: "saved",
      receipt: { kind: "font_import", transaction: receipt },
    };
    state.recovery = {
      ...emptyEditRecovery(),
      phase: "failed",
      record: null,
      receipt: null,
      error: "Edit recovery failed",
    };
    render(
      <WorkspaceProvider controller={f.controller}>
        <EditRecoveryPanel />
      </WorkspaceProvider>,
    );
    expect(
      screen.getByRole("region", { name: "Check the last save" }),
    ).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain("could not be completed");
    expect(screen.getByText("Edit recovery failed")).toBeTruthy();
    expect(screen.queryByText("Your last change was saved.")).toBeNull();
  } finally {
    f.controller.dispose();
  }
});
