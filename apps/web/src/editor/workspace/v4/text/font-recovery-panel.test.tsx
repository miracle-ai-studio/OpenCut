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
import { setupV4 } from "../fixture";
import { emptyFontRecovery } from "./font-recovery";
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
it("shows a dedicated explicit retry for the original font family", async () => {
  preferences({ language: "en" });
  const f = await setupV4();
  try {
    f.controller.getSnapshot().v4!.fontRecovery = {
      ...emptyFontRecovery(),
      supported: true,
      phase: "unconfirmed",
      error: "Response lost",
      record: {
        schema_version: 1,
        recovery_id: "original",
        project_id: "v4-project",
        request_digest: "host-digest",
        receipt: null,
        context: {
          sequence_id: "scene",
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
    };
    const retry = vi
      .spyOn(f.controller.v4.fontRecovery, "retry")
      .mockResolvedValue(receipt);
    render(
      <WorkspaceProvider controller={f.controller}>
        <EditRecoveryPanel />
      </WorkspaceProvider>,
    );
    expect(
      screen.getByRole("region", { name: "Recover original font import" }),
    ).toBeTruthy();
    expect(retry).not.toHaveBeenCalled();
    fireEvent.click(
      screen.getByRole("button", { name: "Retry original font import" }),
    );
    await waitFor(() => expect(retry).toHaveBeenCalledTimes(1));
  } finally {
    f.controller.dispose();
  }
});
it("a completed font notice cannot obscure a later unresolved edit", async () => {
  preferences({ language: "en" });
  const f = await setupV4();
  try {
    const state = f.controller.getSnapshot().v4!;
    state.fontRecovery = {
      ...emptyFontRecovery(),
      supported: true,
      phase: "saved",
      receipt,
    };
    state.recovery = {
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
      screen.getByRole("region", { name: "Recover original edit" }),
    ).toBeTruthy();
    expect(
      screen.queryByRole("region", { name: "Recover original font import" }),
    ).toBeNull();
  } finally {
    f.controller.dispose();
  }
});
