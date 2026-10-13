// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import type { ReactNode, ButtonHTMLAttributes } from "react";
import { OpenCutClient } from "../../../lib/editor-sdk";
import { MigrateClassic } from "../../projects/migrate";
const state = vi.hoisted(() => ({ workspace: {} }));
vi.mock("../context", () => ({ useWorkspace: () => state.workspace }));
vi.mock("../../shared/locale", () => ({
  useLocale: () => {},
  t: (key: string, values: Record<string, unknown> = {}) =>
    key.replace(/\{([^}]+)\}/g, (_, name) => String(values[name] ?? name)),
}));
vi.mock("../../shared/ui", () => ({
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
vi.mock("./source", () => ({
  discoverClassic: async () => [{ id: "old", name: "Original", version: 31 }],
  captureClassic: async () => ({
    origin: "https://old.example",
    sourceId: "old",
    media: [],
    context: { legacyTracksBySceneId: {}, mediaTypesById: {} },
    raw: { metadata: { id: "old", name: "Original" }, version: 31, scenes: [] },
  }),
  readClassicFile: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});
it("maps only exact fixed-revision fonts and freezes original request after an uncertain publication", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => ({ ok: true, text: async () => "exporter" })),
  );
  const sdk = new OpenCutClient({ baseUrl: "http://localhost", token: "" });
  const stage = {
    revision: "stage-fixed",
    project: { id: "stage", name: "Preparing", assets: [] },
  };
  const analysis = {
    schema_version: 4,
    source_content_id: "source-hash",
    blockers: [],
    losses: [],
    unresolved_fonts: [
      { family: "Lato", font_weight: "normal", font_style: "normal" },
    ],
    proposed_project: { schema_version: 4 },
  };
  const call = vi.spyOn(sdk, "call").mockImplementation(async (name, input) => {
    if (name === "opencut_project_create" || name === "opencut_project_inspect")
      return stage as never;
    if (name === "opencut_event_changes")
      return { events: [], next_cursor: null } as never;
    if (name === "opencut_classic_analyze_v4")
      return {
        ...analysis,
        unresolved_fonts: (input as { font_mappings: unknown[] }).font_mappings
          .length
          ? []
          : analysis.unresolved_fonts,
      } as never;
    if (name === "opencut_project_v4_list")
      return {
        items: [
          { project_id: "fonts", name: "Font library", schema_version: 4 },
        ],
        next_cursor: null,
      } as never;
    if (name === "opencut_project_v4_inspect")
      return {
        revision: "font-fixed",
        project: {
          id: "fonts",
          fonts: [
            { id: "exact", family: "Lato", weight: 400, style: "normal" },
            {
              id: "wrong-family",
              family: "Arial",
              weight: 400,
              style: "normal",
            },
            {
              id: "wrong-weight",
              family: "Lato",
              weight: 700,
              style: "normal",
            },
          ],
        },
      } as never;
    throw new Error(`Unexpected call ${name}`);
  });
  const mutation = vi
    .spyOn(sdk, "callWithAdmissionRetry")
    .mockImplementation(async (name, input) => {
      if (name === "opencut_project_create") return stage as never;
      throw new Error(
        `Unknown acknowledgment for ${name}: ${JSON.stringify(input)}`,
      );
    });
  const api = { sdk };
  state.workspace = {
    api,
    controller: {
      getSnapshot: () => ({ api, projects: [] }),
      refreshProjects: async () => {},
    },
  };
  render(<MigrateClassic onClose={() => {}} onOpen={() => {}} />);
  await waitFor(() =>
    expect(
      (
        screen.getByRole("button", {
          name: "Prepare recovery",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false),
  );
  fireEvent.click(screen.getByRole("button", { name: "Prepare recovery" }));
  const project = await screen.findByRole("combobox", {
    name: "Font source project",
  });
  expect(screen.getByText(/Unmapped fonts can be saved/)).toBeTruthy();
  fireEvent.change(project, { target: { value: "fonts" } });
  const font = screen.getByRole("combobox", {
    name: "Font for Lato (normal, normal)",
  });
  await waitFor(() => expect((font as HTMLSelectElement).disabled).toBe(false));
  expect(
    Array.from((font as HTMLSelectElement).options).map(
      (option) => option.value,
    ),
  ).toEqual(["", "exact"]);
  fireEvent.change(font, { target: { value: "exact" } });
  expect(
    (
      screen.getByRole("button", {
        name: "Recover project",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "Review font mappings" }));
  await waitFor(() =>
    expect(
      (
        screen.getByRole("button", {
          name: "Recover project",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false),
  );
  const analyze = call.mock.calls
    .filter(([name]) => name === "opencut_classic_analyze_v4")
    .at(-1);
  expect(analyze?.[1]).toMatchObject({
    font_mappings: [
      {
        classic_family: "Lato",
        source_project_id: "fonts",
        source_revision: "font-fixed",
        font_id: "exact",
      },
    ],
  });
  fireEvent.click(screen.getByRole("button", { name: "Recover project" }));
  await screen.findByRole("alert");
  expect((project as HTMLSelectElement).disabled).toBe(true);
  expect((font as HTMLSelectElement).disabled).toBe(true);
  expect(
    (
      screen.getByRole("button", {
        name: "Retry recovery",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  const original = mutation.mock.calls.find(
    ([name]) => name === "opencut_classic_import_v4",
  );
  fireEvent.click(screen.getByRole("button", { name: "Recover project" }));
  await waitFor(() =>
    expect(
      mutation.mock.calls.filter(
        ([name]) => name === "opencut_classic_import_v4",
      ),
    ).toHaveLength(2),
  );
  expect(mutation.mock.calls.at(-1)?.slice(0, 2)).toEqual(
    original?.slice(0, 2),
  );
});
