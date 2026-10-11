// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { ExportDialog } from "./export-dialog";
import { WorkspaceProvider } from "../workspace/context";
import { WorkspaceController } from "../workspace/controller";
import { EditorApi } from "../workspace/api";
import { projectRef, type Snapshot } from "../model/types";
beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
const snap = (revision = "head"): Snapshot => ({
  format: "visual",
  revision,
  visual: [],
  curves: [],
  project: {
    id: "project",
    name: "Project",
    schema_version: 2,
    default_sequence_id: "main",
    assets: [],
    sequences: [
      {
        id: "main",
        name: "Main",
        width: 640,
        height: 360,
        fps: { numerator: 24, denominator: 1 },
        tracks: [],
      },
    ],
  },
});
async function setup() {
  const api = new EditorApi("http://localhost:8788", "token");
  vi.spyOn(api.sdk, "hostCapabilities").mockResolvedValue({} as never);
  vi.spyOn(api, "projects").mockResolvedValue([projectRef(snap())]);
  vi.spyOn(api, "inspect").mockResolvedValue(snap());
  vi.spyOn(api, "history").mockResolvedValue({
    project_id: "project",
    head_revision: "head",
    revisions: [],
    navigation: null,
    next_cursor: null,
  });
  vi.spyOn(api, "tasks").mockResolvedValue({ jobs: [], next_cursor: null });
  const controller = new WorkspaceController(() => api);
  await controller.connect("x", "t");
  await controller.open(projectRef(snap()));
  render(
    <WorkspaceProvider controller={controller}>
      <ExportDialog onClose={vi.fn()} />
    </WorkspaceProvider>,
  );
  return { api, controller };
}
it("refuses an old preflight result after the project head changes", async () => {
  const { api, controller } = await setup();
  let resolve!: (value: unknown) => void;
  vi.spyOn(api.sdk, "call").mockImplementation(
    (() =>
      new Promise((r) => {
        resolve = r;
      })) as never,
  );
  fireEvent.click(screen.getByRole("button", { name: "Check project" }));
  vi.mocked(api.inspect).mockResolvedValue(snap("changed"));
  await act(async () => {
    await controller.write(async () => {});
    resolve({ ready: true, revision: "head", issues: [] });
  });
  expect(
    (screen.getByRole("button", { name: "Export" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
});
it("retries an uncertain export with the same key and changes key for changed configuration", async () => {
  const { api } = await setup(),
    keys: string[] = [];
  vi.spyOn(api.sdk, "call").mockImplementation((async (
    name: string,
    input: { idempotency_key?: string },
  ) => {
    if (name === "opencut_render_v3_preflight")
      return { ready: true, revision: "head", issues: [] };
    keys.push(input.idempotency_key!);
    throw new Error("Uncertain network result");
  }) as never);
  const check = async () => {
    fireEvent.click(screen.getByRole("button", { name: "Check project" }));
    await waitFor(() =>
      expect(
        (screen.getByRole("button", { name: "Export" }) as HTMLButtonElement)
          .disabled,
      ).toBe(false),
    );
  };
  await check();
  fireEvent.click(screen.getByRole("button", { name: "Export" }));
  await screen.findByText("Error: Uncertain network result");
  fireEvent.click(screen.getByRole("button", { name: "Export" }));
  await waitFor(() => expect(keys).toHaveLength(2));
  expect(keys[0]).toBe(keys[1]);
  await waitFor(() =>
    expect(
      (screen.getByLabelText("Format") as HTMLSelectElement).disabled,
    ).toBe(false),
  );
  fireEvent.change(screen.getByLabelText("Format"), {
    target: { value: "matroska" },
  });
  await check();
  fireEvent.click(screen.getByRole("button", { name: "Export" }));
  await waitFor(() => expect(keys).toHaveLength(3));
  expect(keys[2]).not.toBe(keys[0]);
});

it("rejects a file event while viewing immutable history", async () => {
  const { api, controller } = await setup();
  const { AssetsPanel } = await import("../assets/panel");
  render(
    <WorkspaceProvider controller={controller}>
      <AssetsPanel />
    </WorkspaceProvider>,
  );
  await act(async () => {
    await controller.inspectRevision("older");
  });
  fireEvent.change(screen.getByLabelText("Media files"), {
    target: {
      files: [new File(["synthetic"], "clip.mp4", { type: "video/mp4" })],
    },
  });
  await act(async () => {
    await Promise.resolve();
  });
  expect(api.sdk.hostCapabilities).toHaveBeenCalledTimes(1);
});
