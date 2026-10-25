import { friendlyError } from "../shared/error-notice";
// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import type { ButtonHTMLAttributes, ReactNode } from "react";
import {
  OpenCutApiError,
  OpenCutClient,
  OpenCutTransportError,
} from "../../lib/editor-sdk";
import type { ToolInput, ToolOutput } from "../../lib/editor-sdk";
import {
  ExportProjectPackage,
  RestoreProjectPackage,
} from "../workspace/scene/package";

const state = vi.hoisted(() => ({ workspace: {} as Record<string, unknown> }));
vi.mock("../workspace/context", () => ({
  useWorkspace: () => state.workspace,
}));
vi.mock("../shared/locale", () => ({
  useLocale: () => ({ language: "en" }),
  t: (key: string, values: Record<string, unknown> = {}) =>
    key.replace(/\{([^}]+)\}/g, (_, name) => String(values[name] ?? name)),
}));
vi.mock("../shared/ui", () => ({
  Modal: ({ children, title }: { children: ReactNode; title: string }) => (
    <div role="dialog" aria-label={title}>
      {children}
    </div>
  ),
  Field: ({ children, label }: { children: ReactNode; label: string }) => (
    <label>
      {label}
      {children}
    </label>
  ),
  Button: (props: ButtonHTMLAttributes<HTMLButtonElement>) => (
    <button {...props} />
  ),
}));

type Inspection = ToolOutput<"opencut_project_package_inspect">;
type ImportRequest = ToolInput<"opencut_project_package_import">;
type ImportReceipt = ToolOutput<"opencut_project_package_import">;
type ExportRequest = ToolInput<"opencut_project_package_export">;
type ExportReceipt = ToolOutput<"opencut_project_package_export">;

const reviewed: Inspection = {
  manifest_sha256: "a".repeat(64),
  source_project_id: "source-project",
  source_revision: "source-fixed-revision",
  project_name: "Portable project",
  asset_version_count: 3, font_version_count: 2, derived_font_count: 1,
  scope:"saved_revision", mode:"self_contained", authoring_available:true,
  revision_count:1, transaction_count:0, branch_count:0, merge_count:0,
  plugin_bundle_count:0, external_font_families:[], fonts_missing:[],
  tool_locks:[], tool_environment_matches:true, tool_lock_mismatches:[],
};
const source = {
  project_id: reviewed.source_project_id,
  revision: reviewed.source_revision,
  name: reviewed.project_name,
};
const packagePath = "/Projects/Portable package";
const destination = "/Projects/Exported package";

function receiptFor(request: ImportRequest): ImportReceipt {
  return {
    target_project_id: request.target_project_id,
    revision: "restored-fixed-revision",
    package: structuredClone(reviewed), plugin_restore_plan: [],
  };
}

let nextLocation: string | null | undefined;
const packageCalls = (call: { mock: { calls: unknown[][] } }) => call.mock.calls.filter(([name]) => name !== "opencut_project_package_choose_location");

function workspace(
  handlers: {
    inspect?: () => Promise<Inspection>;
    restore?: (request: ImportRequest) => Promise<ImportReceipt>;
    save?: (request: ExportRequest) => Promise<ExportReceipt>;
    choose?: () => Promise<ToolOutput<"opencut_project_package_choose_location">>;
  } = {},
) {
  nextLocation = undefined;
  const sdk = new OpenCutClient({ baseUrl: "http://localhost", token: "" });
  const call = vi.spyOn(sdk, "call").mockImplementation(async (name, input) => {
    if (name === "opencut_project_package_choose_location") {
      if (handlers.choose) return await handlers.choose() as never;
      const choice = nextLocation === undefined ? ((input as { purpose: string }).purpose === "save" ? destination : packagePath) : nextLocation;
      nextLocation = undefined;
      return { path: choice } as never;
    }
    if (name === "opencut_project_package_inspect")
      return (await (handlers.inspect?.() ??
        Promise.resolve(reviewed))) as never;
    if (name === "opencut_project_package_import") {
      const request = input as ImportRequest;
      return (await (handlers.restore?.(request) ??
        Promise.resolve(receiptFor(request)))) as never;
    }
    if (name === "opencut_project_package_export" && handlers.save)
      return (await handlers.save(input as ExportRequest)) as never;
    throw new Error(`Unexpected call ${name}`);
  });
  const api = { sdk, connectionIdentity: JSON.stringify(["http://localhost","local-owner",null,true]) };
  const controller = {
    getSnapshot: () => ({ api: state.workspace.api, projects: [] }),
    refreshProjects: vi.fn(async () => {}),
    open: vi.fn(async () => {}),
  };
  state.workspace = { api, controller };
  return { api, call, controller, onClose: vi.fn(), onOpen: vi.fn() };
}

function button(name: string) {
  return screen.getByRole<HTMLButtonElement>("button", { name });
}
async function chooseLocation(name: string, path: string | null) {
  nextLocation = path;
  await act(async () => fireEvent.click(button(name)));
}
async function reviewPackage(path = packagePath) {
  await chooseLocation("Choose project package…", path);
  fireEvent.click(button("Check package"));
  await screen.findByRole("button", { name: "Restore as new project" });
}
async function beginExport() {
  await chooseLocation("Choose save location…", destination);
  fireEvent.click(button("Save package"));
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  sessionStorage.clear();
});

it.each([
  ["unknown outcome", new OpenCutTransportError(new Error("response lost"))],
  [
    "server error",
    new OpenCutApiError(
      "BUSY",
      "Temporary import error",
      null,
      503,
      "request-1",
      [],
    ),
  ],
])(
  "retries a restore after %s with the same target, key, manifest and path",
  async (_, error) => {
    const ids = vi.spyOn(crypto, "randomUUID");
    const restore = vi.fn(async (request: ImportRequest) =>
      receiptFor(request),
    );
    restore.mockRejectedValueOnce(error);
    const h = workspace({ restore });
    render(<RestoreProjectPackage onClose={h.onClose} onOpen={h.onOpen} />);
    await reviewPackage(packagePath);
    fireEvent.click(button("Restore as new project"));
    expect((await screen.findByRole("alert")).textContent).toBe(friendlyError(error));
    expect(screen.getByText(error.message)).toBeTruthy();
    const original = structuredClone(restore.mock.calls[0][0]);
    expect(original).toEqual({
      package_path: packagePath,
      expected_manifest_sha256: reviewed.manifest_sha256,
      target_project_id: expect.any(String),
      idempotency_key: expect.any(String),
    });
    expect(original.target_project_id).not.toBe(original.idempotency_key);
    expect(
      button("Choose another package…").disabled,
    ).toBe(true);
    expect(button("Retry restore").disabled).toBe(false);
    expect(h.controller.open).not.toHaveBeenCalled();
    fireEvent.click(button("Retry restore"));
    await screen.findByText("Project restored");
    expect(restore.mock.calls.map(([request]) => request)).toEqual([
      original,
      original,
    ]);
    expect(ids).toHaveBeenCalledTimes(2);
    expect(
      packageCalls(h.call).filter(
        ([name]) => name === "opencut_project_package_inspect",
      ),
    ).toHaveLength(1);
    await waitFor(() =>
      expect(button("Open restored project").disabled).toBe(false),
    );
    expect(h.controller.refreshProjects).toHaveBeenCalledTimes(1);
    fireEvent.click(button("Open restored project"));
    await waitFor(() => expect(h.onOpen).toHaveBeenCalledTimes(1));
    expect(h.controller.open).toHaveBeenCalledWith({
      id: original.target_project_id,
      revision: "restored-fixed-revision",
      name: reviewed.project_name,
      format: "editor",
    });
    expect(h.onClose).toHaveBeenCalledTimes(1);
  },
);

it("clears the reviewed identity when the path changes and requires checking the new package", async () => {
  const second = {
    ...reviewed,
    project_name: "Second project",
    manifest_sha256: "b".repeat(64),
  };
  const inspect = vi
    .fn(async () => reviewed)
    .mockResolvedValueOnce(reviewed)
    .mockResolvedValueOnce(second);
  const restore = vi.fn(async (_request: ImportRequest) => {
    throw new Error("Stopped after request capture");
  });
  const h = workspace({ inspect, restore });
  render(<RestoreProjectPackage onClose={h.onClose} onOpen={h.onOpen} />);
  await reviewPackage();
  expect(
    screen.getByRole("heading", { name: reviewed.project_name }),
  ).toBeTruthy();
  await chooseLocation("Choose another package…", "/Projects/Second package");
  expect(
    screen.queryByRole("heading", { name: reviewed.project_name }),
  ).toBeNull();
  expect(
    screen.queryByRole("button", { name: "Restore as new project" }),
  ).toBeNull();
  expect(restore).not.toHaveBeenCalled();
  fireEvent.click(button("Check package"));
  await screen.findByRole("heading", { name: "Second project" });
  fireEvent.click(button("Restore as new project"));
  await screen.findByRole("alert");
  expect(restore.mock.calls[0][0]).toMatchObject({
    package_path: "/Projects/Second package",
    expected_manifest_sha256: second.manifest_sha256,
  });
  expect(
    packageCalls(h.call)
      .filter(([name]) => name === "opencut_project_package_inspect")
      .map(([, input]) => input),
  ).toEqual([
    { package_path: packagePath },
    { package_path: "/Projects/Second package" },
  ]);
});

it.each([
  ["target project", { target_project_id: "wrong-target" }],
  ["manifest", { manifest_sha256: "b".repeat(64) }],
  ["source project", { source_project_id: "wrong-source" }],
  ["source revision", { source_revision: "wrong-source-revision" }],
  ["restored revision", { revision: "" }],
  ["media inventory", { asset_version_count: 4 }],
  ["font inventory", { font_version_count: 3 }],
  ["font source inventory", { derived_font_count: 0 }],
  ["self-contained resources", { mode: "linked" }],
  ["history claim", { scope: "history" }],
  ["plugin inventory", { plugin_bundle_count: 1 }],
] satisfies [string, Partial<ImportReceipt> | Partial<Inspection>][])(
  "does not open or refresh a restored project when the receipt has mismatched %s",
  async (_, mismatch) => {
    const h = workspace({
      restore: async (request) => {
        const original=receiptFor(request);
        return ("target_project_id" in mismatch || "revision" in mismatch) ? {...original,...mismatch} as ImportReceipt : {...original,package:{...original.package,...mismatch}} as ImportReceipt;
      },
    });
    render(<RestoreProjectPackage onClose={h.onClose} onOpen={h.onOpen} />);
    await reviewPackage();
    fireEvent.click(button("Restore as new project"));
    await screen.findByRole("alert");
    expect(screen.getByText(/Technical details/).parentElement!.textContent).toMatch(
      /receipt does not match/,
    );
    expect(
      screen.queryByRole("button", { name: "Open restored project" }),
    ).toBeNull();
    expect(screen.queryByText("Project restored")).toBeNull();
    expect(h.controller.open).not.toHaveBeenCalled();
    expect(h.controller.refreshProjects).not.toHaveBeenCalled();
    expect(h.onOpen).not.toHaveBeenCalled();
    expect(h.onClose).not.toHaveBeenCalled();
  },
);

it.each(["review", "retry", "open"] as const)(
  "prevents continuing restore after the connection changes at %s",
  async (stage) => {
    const h = workspace({
      restore: async (request) => {
        if (stage === "retry") throw new OpenCutTransportError("response lost");
        return receiptFor(request);
      },
    });
    const view = render(
      <RestoreProjectPackage onClose={h.onClose} onOpen={h.onOpen} />,
    );
    await reviewPackage();
    if (stage !== "review") {
      fireEvent.click(button("Restore as new project"));
      if (stage === "retry") await screen.findByRole("alert");
      else await screen.findByText("Project restored");
    }
    const before = packageCalls(h.call).length;
    const replacementCall = vi.fn();
    state.workspace = {
      ...state.workspace,
      api: { sdk: { call: replacementCall } },
    };
    view.rerender(
      <RestoreProjectPackage onClose={h.onClose} onOpen={h.onOpen} />,
    );
    expect(
      screen.getByText(
        "The connection changed. Reopen this dialog before continuing.",
      ),
    ).toBeTruthy();
    const action = button(
      stage === "review"
        ? "Restore as new project"
        : stage === "retry"
          ? "Retry restore"
          : "Open restored project",
    );
    expect(action.disabled).toBe(true);
    fireEvent.click(action);
    expect(packageCalls(h.call)).toHaveLength(before);
    expect(replacementCall).not.toHaveBeenCalled();
    expect(h.controller.open).not.toHaveBeenCalled();
    expect(h.onOpen).not.toHaveBeenCalled();
  },
);

it("does not publish a late restore receipt into a different connection", async () => {
  let finish!: (receipt: ImportReceipt) => void;
  const pending = new Promise<ImportReceipt>((resolve) => {
    finish = resolve;
  });
  const restore = vi.fn((_request: ImportRequest) => pending);
  const h = workspace({ restore });
  const view = render(
    <RestoreProjectPackage onClose={h.onClose} onOpen={h.onOpen} />,
  );
  await reviewPackage();
  fireEvent.click(button("Restore as new project"));
  await waitFor(() => expect(restore).toHaveBeenCalledTimes(1));
  state.workspace = { ...state.workspace, api: { sdk: { call: vi.fn() } } };
  view.rerender(
    <RestoreProjectPackage onClose={h.onClose} onOpen={h.onOpen} />,
  );
  await act(async () => {
    finish(receiptFor(restore.mock.calls[0][0]));
  });
  expect(h.controller.refreshProjects).not.toHaveBeenCalled();
  expect(screen.queryByText("Project restored")).toBeNull();
  expect(h.controller.open).not.toHaveBeenCalled();
  expect(h.onOpen).not.toHaveBeenCalled();
});

it("does not navigate after a pending open finishes on a changed connection", async () => {
  let finish!: () => void;
  const pending = new Promise<void>((resolve) => {
    finish = resolve;
  });
  const h = workspace();
  h.controller.open.mockReturnValueOnce(pending);
  const view = render(
    <RestoreProjectPackage onClose={h.onClose} onOpen={h.onOpen} />,
  );
  await reviewPackage();
  fireEvent.click(button("Restore as new project"));
  await screen.findByText("Project restored");
  await waitFor(() =>
    expect(button("Open restored project").disabled).toBe(false),
  );
  fireEvent.click(button("Open restored project"));
  await waitFor(() => expect(h.controller.open).toHaveBeenCalledTimes(1));
  state.workspace = { ...state.workspace, api: { sdk: { call: vi.fn() } } };
  view.rerender(
    <RestoreProjectPackage onClose={h.onClose} onOpen={h.onOpen} />,
  );
  await act(async () => {
    finish();
  });
  expect(h.onOpen).not.toHaveBeenCalled();
  expect(h.onClose).not.toHaveBeenCalled();
});

it.each([
  ["manifest identity", { manifest_sha256: "invalid-manifest" }],
  ["unknown storage mode", { mode: "invalid" }],
  ["invalid saved revision graph", { transaction_count: 1 }],
])(
  "reports an invalid reviewed package with %s without submitting a restore",
  async (_, invalid) => {
    const restore = vi.fn(async (request: ImportRequest) =>
      receiptFor(request),
    );
    const h = workspace({
      // Deliberately malformed wire input exercises runtime validation.
      inspect: async () => ({ ...reviewed, ...invalid }) as Inspection,
      restore,
    });
    render(<RestoreProjectPackage onClose={h.onClose} onOpen={h.onOpen} />);
    await chooseLocation("Choose project package…", packagePath);
    fireEvent.click(button("Check package"));
    await screen.findByRole("alert");
    expect(screen.getByText(/Technical details/).parentElement!.textContent).toMatch(
      /package inspection is incomplete/,
    );
    expect(restore).not.toHaveBeenCalled();
    expect(h.controller.refreshProjects).not.toHaveBeenCalled();
    expect(button("Check package").disabled).toBe(false);
  },
);

it("keeps the fixed export source and checks the same destination after a lost response", async () => {
  const h = workspace({
    save: async () => {
      throw new OpenCutTransportError("response lost after publication");
    },
  });
  const view = render(
    <ExportProjectPackage source={source} onClose={h.onClose} />,
  );
  await beginExport();
  await screen.findByRole("alert");
  const original = packageCalls(h.call).find(
    ([name]) => name === "opencut_project_package_export",
  )?.[1];
  expect(original).toEqual({
    project_id: source.project_id,
    revision: source.revision,
    destination, scope:"saved_revision", mode:"self_contained",
  });
  expect(
    screen.queryByRole("button", { name: "Change save location…" }) === null,
  ).toBe(true);
  expect(screen.queryByRole("button", { name: "Save package" })).toBeNull();
  view.rerender(
    <ExportProjectPackage
      source={{
        project_id: "new-project",
        revision: "new-head",
        name: "New selection",
      }}
      onClose={h.onClose}
    />,
  );
  fireEvent.click(button("Check destination"));
  await screen.findByText("Project package saved and verified");
  expect(packageCalls(h.call).map(([name, input]) => [name, input])).toEqual([
    ["opencut_project_package_export", original],
    ["opencut_project_package_inspect", { package_path: destination }],
  ]);
  expect(screen.getByRole("status").textContent).toContain(destination);
  expect(h.controller.open).not.toHaveBeenCalled();
  fireEvent.click(button("Done"));
  expect(h.onClose).toHaveBeenCalledTimes(1);
});

it.each([
  ["project", { source_project_id: "different-project" }],
  ["revision", { source_revision: "different-revision" }],
])(
  "does not confirm export if the destination contains a different %s",
  async (_, mismatch) => {
    const h = workspace({
      save: async () => {
        throw new OpenCutTransportError("response lost");
      },
      inspect: async () => ({ ...reviewed, ...mismatch }),
    });
    render(<ExportProjectPackage source={source} onClose={h.onClose} />);
    await beginExport();
    await screen.findByRole("alert");
    fireEvent.click(button("Check destination"));
    await screen.findByRole("alert");
    expect(screen.getByText(/Technical details/).parentElement!.textContent).toMatch(
      /different project revision/,
    );
    expect(screen.queryByRole("status")).toBeNull();
    expect(button("Check destination").disabled).toBe(false);
  },
);

it.each(["save", "check"] as const)(
  "prevents export %s after the connection changes",
  async (stage) => {
    const h = workspace({
      save: async () => {
        throw new OpenCutTransportError("response lost");
      },
    });
    const view = render(
      <ExportProjectPackage source={source} onClose={h.onClose} />,
    );
    if (stage === "check") {
      await beginExport();
      await screen.findByRole("alert");
    } else
      await chooseLocation("Choose save location…", destination);
    const before = packageCalls(h.call).length;
    const replacementCall = vi.fn();
    state.workspace = {
      ...state.workspace,
      api: { sdk: { call: replacementCall } },
    };
    view.rerender(<ExportProjectPackage source={source} onClose={h.onClose} />);
    const action = button(
      stage === "save" ? "Save package" : "Check destination",
    );
    expect(action.disabled).toBe(true);
    fireEvent.click(action);
    expect(packageCalls(h.call)).toHaveLength(before);
    expect(replacementCall).not.toHaveBeenCalled();
    expect(screen.queryByRole("status")).toBeNull();
  },
);

it.each(["save", "check"] as const)(
  "does not confirm a late export %s response after the connection changes",
  async (stage) => {
    let finish!: (receipt: ExportReceipt & Inspection) => void;
    const pending = new Promise<ExportReceipt & Inspection>((resolve) => {
      finish = resolve;
    });
    const h = workspace({
      save: async () => {
        if (stage === "check") throw new OpenCutTransportError("response lost");
        return pending;
      },
      inspect: async () => pending,
    });
    const view = render(
      <ExportProjectPackage source={source} onClose={h.onClose} />,
    );
    await beginExport();
    if (stage === "check") {
      await screen.findByRole("alert");
      fireEvent.click(button("Check destination"));
    }
    await waitFor(() =>
      expect(packageCalls(h.call)).toHaveLength(stage === "check" ? 2 : 1),
    );
    state.workspace = { ...state.workspace, api: { sdk: { call: vi.fn() } } };
    view.rerender(<ExportProjectPackage source={source} onClose={h.onClose} />);
    await act(async () => {
      finish({ ...reviewed, package_path: destination, package:reviewed });
    });
    expect(screen.queryByRole("status")).toBeNull();
    expect(h.onClose).not.toHaveBeenCalled();
    expect(button("Check destination").disabled).toBe(true);
  },
);

it.each(["PACKAGE_INVALID", "PACKAGE_IO_ERROR"])("offers an explicit new location after %s without silently resubmitting an uncertain copy", async code => {
  const h=workspace({save:async()=>{throw new OpenCutApiError(code,"Package destination cannot be used",null,400,"r",[]);}});
  render(<ExportProjectPackage source={source} onClose={h.onClose}/>); await beginExport();
  await screen.findByRole("alert"); expect(screen.queryByRole("button", { name: "Change save location…" }) === null).toBe(true);
  await chooseLocation("Save to another location", "/Projects/Another package");
  expect(screen.queryByRole("button", { name: "Change save location…" }) === null).toBe(false);
  expect(screen.getByText(`Previous save location: ${destination}`)).toBeTruthy();
  expect(packageCalls(h.call).filter(([name])=>name==="opencut_project_package_export")).toHaveLength(1);
});

it("cancelling the location dialog writes nothing and preserves an already selected save location", async () => {
  const h = workspace();
  render(<ExportProjectPackage source={source} onClose={h.onClose} />);
  expect(screen.queryByRole("textbox", { name: "Save location on this computer" })).toBeNull();
  await chooseLocation("Choose save location…", null);
  expect(button("Save package").disabled).toBe(true);
  expect(packageCalls(h.call)).toHaveLength(0);
  await chooseLocation("Choose save location…", destination);
  await chooseLocation("Change save location…", null);
  expect(screen.getByLabelText("Save location on this computer").textContent).toBe(destination);
  expect(button("Save package").disabled).toBe(false);
  expect(packageCalls(h.call)).toHaveLength(0);
});

it("does not use a late native selection after the authenticated connection changes", async () => {
  let finish!: (value: ToolOutput<"opencut_project_package_choose_location">) => void;
  const pending = new Promise<ToolOutput<"opencut_project_package_choose_location">>(resolve => { finish = resolve; });
  const h = workspace({ choose: () => pending });
  const view = render(<ExportProjectPackage source={source} onClose={h.onClose} />);
  fireEvent.click(button("Choose save location…"));
  expect(button("Choosing location…").disabled).toBe(true);
  expect(button("Close").disabled).toBe(true);
  state.workspace = { ...state.workspace, api: { sdk: { call: vi.fn() } } };
  view.rerender(<ExportProjectPackage source={source} onClose={h.onClose} />);
  await act(async () => finish({ path: destination }));
  expect(screen.getByLabelText("Save location on this computer").textContent).toBe("No location selected");
  expect(button("Save package").disabled).toBe(true);
  expect(packageCalls(h.call)).toHaveLength(0);
});

it("cancelling a different location preserves the original uncertain save request", async () => {
  const h = workspace({ save: async () => { throw new OpenCutTransportError("response lost"); } });
  render(<ExportProjectPackage source={source} onClose={h.onClose} />);
  await beginExport();
  await screen.findByRole("alert");
  const original = packageCalls(h.call)[0][1];
  await chooseLocation("Save to another location", null);
  expect(screen.getByLabelText("Save location on this computer").textContent).toBe(destination);
  fireEvent.click(button("Retry save"));
  await waitFor(() => expect(packageCalls(h.call)).toHaveLength(2));
  expect(packageCalls(h.call)[1][1]).toEqual(original);
});
