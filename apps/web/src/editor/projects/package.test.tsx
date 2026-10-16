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
} from "../workspace/v4/package";

const state = vi.hoisted(() => ({ workspace: {} as Record<string, unknown> }));
vi.mock("../workspace/context", () => ({
  useWorkspace: () => state.workspace,
}));
vi.mock("../shared/locale", () => ({
  useLocale: () => {},
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

type Inspection = ToolOutput<"opencut_project_v4_package_inspect">;
type ImportRequest = ToolInput<"opencut_project_v4_package_import">;
type ImportReceipt = ToolOutput<"opencut_project_v4_package_import">;
type ExportRequest = ToolInput<"opencut_project_v4_package_export">;
type ExportReceipt = ToolOutput<"opencut_project_v4_package_export">;

const reviewed: Inspection = {
  manifest_sha256: "a".repeat(64),
  source_project_id: "source-project",
  source_revision: "source-fixed-revision",
  project_name: "Portable project",
  asset_count: 3,
  font_count: 2,
  derived_font_count: 1,
  self_contained_resources: true,
  history_included: false,
  fully_reproducible: false,
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
    manifest_sha256: reviewed.manifest_sha256,
    source_project_id: reviewed.source_project_id,
    source_revision: reviewed.source_revision,
    asset_count: reviewed.asset_count,
    font_count: reviewed.font_count,
    derived_font_count: reviewed.derived_font_count,
    self_contained_resources: true,
    history_included: false,
    fully_reproducible: false,
  };
}

function workspace(
  handlers: {
    inspect?: () => Promise<Inspection>;
    restore?: (request: ImportRequest) => Promise<ImportReceipt>;
    save?: (request: ExportRequest) => Promise<ExportReceipt>;
  } = {},
) {
  const sdk = new OpenCutClient({ baseUrl: "http://localhost", token: "" });
  const call = vi.spyOn(sdk, "call").mockImplementation(async (name, input) => {
    if (name === "opencut_project_v4_package_inspect")
      return (await (handlers.inspect?.() ??
        Promise.resolve(reviewed))) as never;
    if (name === "opencut_project_v4_package_import") {
      const request = input as ImportRequest;
      return (await (handlers.restore?.(request) ??
        Promise.resolve(receiptFor(request)))) as never;
    }
    if (name === "opencut_project_v4_package_export" && handlers.save)
      return (await handlers.save(input as ExportRequest)) as never;
    throw new Error(`Unexpected call ${name}`);
  });
  const api = { sdk };
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
async function reviewPackage(path = packagePath) {
  fireEvent.change(screen.getByLabelText("Package folder"), {
    target: { value: path },
  });
  fireEvent.click(button("Check package"));
  await screen.findByRole("button", { name: "Restore as new project" });
}
function beginExport() {
  fireEvent.change(screen.getByLabelText("New package folder"), {
    target: { value: ` ${destination} ` },
  });
  fireEvent.click(button("Save package"));
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
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
    await reviewPackage(` ${packagePath} `);
    fireEvent.click(button("Restore as new project"));
    expect((await screen.findByRole("alert")).textContent).toBe(error.message);
    const original = structuredClone(restore.mock.calls[0][0]);
    expect(original).toEqual({
      package_path: packagePath,
      expected_manifest_sha256: reviewed.manifest_sha256,
      target_project_id: expect.any(String),
      idempotency_key: expect.any(String),
    });
    expect(original.target_project_id).not.toBe(original.idempotency_key);
    expect(
      (screen.getByLabelText("Package folder") as HTMLInputElement).disabled,
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
      h.call.mock.calls.filter(
        ([name]) => name === "opencut_project_v4_package_inspect",
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
      format: "v4",
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
  fireEvent.change(screen.getByLabelText("Package folder"), {
    target: { value: "/Projects/Second package" },
  });
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
    h.call.mock.calls
      .filter(([name]) => name === "opencut_project_v4_package_inspect")
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
  ["media inventory", { asset_count: 4 }],
  ["font inventory", { font_count: 3 }],
  ["font source inventory", { derived_font_count: 0 }],
  ["self-contained resources", { self_contained_resources: false }],
  ["history claim", { history_included: true }],
  ["reproducibility claim", { fully_reproducible: true }],
] satisfies [string, Partial<ImportReceipt>][])(
  "does not open or refresh a restored project when the receipt has mismatched %s",
  async (_, mismatch) => {
    const h = workspace({
      restore: async (request) => ({ ...receiptFor(request), ...mismatch }),
    });
    render(<RestoreProjectPackage onClose={h.onClose} onOpen={h.onOpen} />);
    await reviewPackage();
    fireEvent.click(button("Restore as new project"));
    expect((await screen.findByRole("alert")).textContent).toMatch(
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
    const before = h.call.mock.calls.length;
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
    expect(h.call).toHaveBeenCalledTimes(before);
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
  ["missing resources", { self_contained_resources: false }],
  ["unsupported history", { history_included: true }],
])(
  "reports an invalid reviewed package with %s without submitting a restore",
  async (_, invalid) => {
    const restore = vi.fn(async (request: ImportRequest) =>
      receiptFor(request),
    );
    const h = workspace({
      inspect: async () => ({ ...reviewed, ...invalid }),
      restore,
    });
    render(<RestoreProjectPackage onClose={h.onClose} onOpen={h.onOpen} />);
    await reviewPackage();
    fireEvent.click(button("Restore as new project"));
    expect((await screen.findByRole("alert")).textContent).toMatch(
      /cannot be restored as a complete saved revision/,
    );
    expect(restore).not.toHaveBeenCalled();
    expect(h.controller.refreshProjects).not.toHaveBeenCalled();
    expect(button("Restore as new project").disabled).toBe(false);
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
  beginExport();
  await screen.findByRole("alert");
  const original = h.call.mock.calls.find(
    ([name]) => name === "opencut_project_v4_package_export",
  )?.[1];
  expect(original).toEqual({
    project_id: source.project_id,
    revision: source.revision,
    destination,
  });
  expect(
    (screen.getByLabelText("New package folder") as HTMLInputElement).disabled,
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
  expect(h.call.mock.calls.map(([name, input]) => [name, input])).toEqual([
    ["opencut_project_v4_package_export", original],
    ["opencut_project_v4_package_inspect", { package_path: destination }],
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
    beginExport();
    await screen.findByRole("alert");
    fireEvent.click(button("Check destination"));
    expect((await screen.findByRole("alert")).textContent).toMatch(
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
      beginExport();
      await screen.findByRole("alert");
    } else
      fireEvent.change(screen.getByLabelText("New package folder"), {
        target: { value: destination },
      });
    const before = h.call.mock.calls.length;
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
    expect(h.call).toHaveBeenCalledTimes(before);
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
    beginExport();
    if (stage === "check") {
      await screen.findByRole("alert");
      fireEvent.click(button("Check destination"));
    }
    await waitFor(() =>
      expect(h.call).toHaveBeenCalledTimes(stage === "check" ? 2 : 1),
    );
    state.workspace = { ...state.workspace, api: { sdk: { call: vi.fn() } } };
    view.rerender(<ExportProjectPackage source={source} onClose={h.onClose} />);
    await act(async () => {
      finish({ ...reviewed, package_path: destination });
    });
    expect(screen.queryByRole("status")).toBeNull();
    expect(h.onClose).not.toHaveBeenCalled();
    expect(button("Check destination").disabled).toBe(true);
  },
);
