// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { preferences } from "../../../shared/locale";
import { FontImport } from "./font-import";
import type { FontImportProps } from "./font-import";
import type { FontBrowserCatalog, FontBrowserSource } from "./font-browser";
import {
  synthesisFixture,
  derivedSynthesisFixture,
} from "./font-synthesis-fixture";

beforeEach(() => preferences({ language: "en" }));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
const catalog = (): FontBrowserCatalog => ({
  download_available: true,
  unavailable_reason: null,
  source_commit: "fixed-commit",
  classic_atlas_sha256: "a".repeat(64),
  classic_family_count: 2,
  pinned_face_count: 2,
  fonts: [staticFace("lato-regular"), staticFace("lato-bold-italic")],
  families: [
    {
      family: "Lato",
      category: "google",
      faces: [
        {
          weight: 400,
          style: "normal",
          catalog_id: "lato-regular",
          unavailable_reason: null,
        },
        {
          weight: 700,
          style: "italic",
          catalog_id: "lato-bold-italic",
          unavailable_reason: null,
        },
        {
          weight: 600,
          style: "normal",
          catalog_id: null,
          unavailable_reason: "unsupported_weight",
        },
      ],
    },
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
    {
      family: "Arial",
      category: "system",
      faces: [
        {
          weight: 400,
          style: "normal",
          catalog_id: null,
          unavailable_reason: "system_font_not_redistributable",
        },
      ],
    },
  ],
});
const staticSource = (id: string) => ({
  id,
  family: "Lato",
  weight: id === "lato-regular" ? 400 : 700,
  style: (id === "lato-regular" ? "normal" : "italic") as "normal" | "italic",
  source_commit: "fixed-commit",
  source_url: "https://example.test/font.ttf",
  content_hash: "b".repeat(64),
  size_bytes: "100",
  license_id: "OFL-1.1",
  license_notice: "Complete original license notice",
});
function staticFace(id: string) {
  const raw = staticSource(id);
  return {
    id,
    display_family: raw.family,
    family: raw.family,
    weight: raw.weight,
    style: raw.style,
    requested_style: null,
    kind: "static" as const,
    content_hash: raw.content_hash,
    size_bytes: raw.size_bytes,
    render_profile: "static_outline" as const,
    alias_reason: null,
    import_available: true,
    unavailable_reason: null,
  };
}
const source = (id: string): FontBrowserSource => ({
  face: staticFace(id),
  details: { kind: "static", source: staticSource(id) },
  license_url: "https://example.test/OFL.txt",
  license_sha256: "c".repeat(64),
});
const props = (): FontImportProps => ({
  scope: { projectId: "p1", revision: "r1" },
  requiredText: "Hello 你好",
  catalog: catalog(),
  readSource: vi.fn(async ({ catalog_id }) => source(catalog_id)),
  importFont: vi.fn(async () => {}),
  close: vi.fn(),
});

it("defaults to Classic synthesis over a real italic source while preserving both explicit import routes", async () => {
  const f = synthesisFixture(),
    input = props(),
    face = {
      id: f.source.source.face.id,
      display_family: "Example",
      family: "Example",
      weight: 700,
      style: "normal" as const,
      requested_style: "italic" as const,
      kind: "synthesis" as const,
      content_hash: f.hash,
      size_bytes: "3",
      render_profile: "static_outline" as const,
      alias_reason: null,
      import_available: true,
      unavailable_reason: null,
    };
  input.catalog = {
    ...catalog(),
    families: [
      {
        family: "Example",
        category: "google",
        faces: [
          {
            weight: 700,
            style: "italic",
            catalog_id: "example-real-bolditalic",
            unavailable_reason: null,
          },
        ],
      },
    ],
  };
  const real = {
    ...staticFace("example-real-bolditalic"),
    display_family: "Example",
    family: "Example",
  };
  input.catalog = { ...input.catalog, fonts: [face, real] };
  const synthesisSource: FontBrowserSource = {
    face,
    details: { kind: "synthesis" as const, source: f.source },
    license_url: f.source.source.license_url,
    license_sha256: JSON.parse(f.source.source.proof_json).license_sha256,
  };
  input.readSource = vi.fn(async ({ catalog_id }) =>
    catalog_id === face.id
      ? synthesisSource
      : {
          ...source(catalog_id),
          face: real,
          details: {
            kind: "static" as const,
            source: { ...staticSource(catalog_id), family: "Example" },
          },
        },
  );
  render(<FontImport {...input} />);
  await screen.findByText("Simulated italic", { selector: "p" });
  expect((screen.getByLabelText("Font style") as HTMLSelectElement).value).toBe(
    "700:italic",
  );
  expect(
    (screen.getByLabelText("Italic source") as HTMLSelectElement).value,
  ).toBe("classic");
  expect(input.readSource).toHaveBeenCalledWith(
    { catalog_id: face.id },
    expect.any(AbortSignal),
  );
  fireEvent.change(screen.getByLabelText("Italic source"), {
    target: { value: "real_italic" },
  });
  await screen.findByText("Complete original license notice");
  expect(input.readSource).toHaveBeenCalledWith(
    { catalog_id: "example-real-bolditalic" },
    expect.any(AbortSignal),
  );
  fireEvent.click(screen.getByRole("button", { name: "Import font" }));
  await waitFor(() =>
    expect(input.importFont).toHaveBeenCalledExactlyOnceWith(
      "example-real-bolditalic",
      "Hello 你好",
    ),
  );
  await waitFor(() => expect(input.close).toHaveBeenCalledOnce());
  vi.mocked(input.importFont).mockClear();
  // close is mocked, so explicitly return to the default path in this fixture.
  fireEvent.change(screen.getByLabelText("Italic source"), {
    target: { value: "classic" },
  });
  await screen.findByText("Simulated italic", { selector: "p" });
  fireEvent.click(screen.getByRole("button", { name: "Import font" }));
  await waitFor(() =>
    expect(input.importFont).toHaveBeenCalledExactlyOnceWith(
      f.source.source.face.id,
      "Hello 你好",
    ),
  );
});

it("shows the derived base identity and color profile without claiming its bytes are untouched upstream files", async () => {
  const f = derivedSynthesisFixture(),
    input = props(),
    face = {
      id: f.source.source.face.id,
      display_family: "Example",
      family: f.source.source.face.family,
      weight: 400,
      style: "normal" as const,
      requested_style: "italic" as const,
      kind: "synthesis" as const,
      content_hash: f.hash,
      size_bytes: "3",
      render_profile: "colr_v1" as const,
      alias_reason: null,
      import_available: true,
      unavailable_reason: null,
    };
  input.catalog = {
    ...catalog(),
    families: [
      {
        family: "Example",
        category: "google",
        faces: [
          {
            weight: 400,
            style: "italic",
            catalog_id: null,
            unavailable_reason: "no_pinned_static_source",
          },
        ],
      },
    ],
  };
  input.catalog = { ...input.catalog, fonts: [face] };
  const synthesisSource: FontBrowserSource = {
    face,
    details: { kind: "synthesis" as const, source: f.source },
    license_url: f.source.source.license_url,
    license_sha256: JSON.parse(f.source.source.proof_json).license_sha256,
  };
  input.readSource = vi.fn(async () => synthesisSource);
  render(<FontImport {...input} />);
  await screen.findByText("Base acquisition: License-safe named derivation");
  expect(
    screen.getByText("Native rendering profile: COLR v1 color font"),
  ).toBeTruthy();
  expect(
    screen.getByText(
      `Base derivation identity: ${f.base.source.face.derivation_id}`,
    ),
  ).toBeTruthy();
  expect(
    screen.getByText(`Upstream source hash: ${f.base.source.source_sha256}`),
  ).toBeTruthy();
  expect(screen.getByText(`Synthesis input hash: ${f.hash}`)).toBeTruthy();
  expect(
    screen.queryByText("Original font bytes and license are preserved."),
  ).toBeNull();
});

it("searches all families and loads only the selected exact static face license", async () => {
  const input = props();
  render(<FontImport {...input} />);
  await screen.findByText("Complete original license notice");
  expect(input.readSource).toHaveBeenCalledTimes(1);
  fireEvent.change(screen.getByLabelText("Font style"), {
    target: { value: "700:italic" },
  });
  await waitFor(() => expect(input.readSource).toHaveBeenCalledTimes(2));
  await waitFor(() =>
    expect(
      (screen.getByRole("button", { name: "Import font" }) as HTMLButtonElement)
        .disabled,
    ).toBe(false),
  );
  fireEvent.change(screen.getByLabelText("Text to check"), {
    target: { value: "  Exact\nText " },
  });
  fireEvent.submit(screen.getByRole("form"));
  await waitFor(() => expect(input.close).toHaveBeenCalledOnce());
  expect(input.importFont).toHaveBeenCalledExactlyOnceWith(
    "lato-bold-italic",
    "  Exact\nText ",
  );
});

it("shows unavailable families and unsupported styles without a download or fallback", async () => {
  const input = props();
  render(<FontImport {...input} />);
  await screen.findByText("Complete original license notice");
  fireEvent.change(screen.getByLabelText("Font style"), {
    target: { value: "600:normal" },
  });
  expect(screen.getAllByRole("status").some((node) => node.textContent?.includes("weight is not supported"))).toBe(true);
  fireEvent.submit(screen.getByRole("form"));
  expect(input.importFont).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("Search fonts"), {
    target: { value: "variable" },
  });
  expect(
    screen.queryByRole("option", { name: "Arial · System font" }),
  ).toBeNull();
  fireEvent.change(screen.getByLabelText("Font family"), {
    target: { value: "Variable Only" },
  });
  expect(screen.getAllByRole("status").some((node) => node.textContent?.includes("No fixed static source"))).toBe(true);
  expect(input.readSource).toHaveBeenCalledTimes(1);
  fireEvent.change(screen.getByLabelText("Search fonts"), {
    target: { value: "" },
  });
  fireEvent.change(screen.getByLabelText("Font family"), {
    target: { value: "Arial" },
  });
  expect(screen.getAllByRole("status").some((node) => node.textContent?.includes("System fonts cannot be imported"))).toBe(true);
});

it("allows offline catalog browsing but disables importing when host policy denies downloads", async () => {
  const input = props();
  input.catalog = {
    ...catalog(),
    download_available: false,
    unavailable_reason: "Downloads are disabled.",
    fonts: catalog().fonts.map((face) => ({
      ...face,
      import_available: false,
      unavailable_reason: "Downloads are disabled.",
    })),
  };
  render(<FontImport {...input} />);
  await screen.findByText("Complete original license notice");
  expect(screen.getByRole("status").textContent).toBe(
    "Downloads are disabled.",
  );
  expect(
    (screen.getByLabelText("Search fonts") as HTMLInputElement).disabled,
  ).toBe(false);
  fireEvent.submit(screen.getByRole("form"));
  expect(input.importFont).not.toHaveBeenCalled();
});

it("aborts obsolete source reads and cannot admit a mismatched or stale face", async () => {
  const input = props();
  let finish!: (value: FontBrowserSource) => void;
  input.readSource = vi.fn(
    (_id, _signal) =>
      new Promise<FontBrowserSource>((resolve) => {
        finish = resolve;
      }),
  );
  render(<FontImport {...input} />);
  const oldComplete = finish;
  const signal = vi.mocked(input.readSource!).mock.calls[0][1];
  fireEvent.change(screen.getByLabelText("Font style"), {
    target: { value: "700:italic" },
  });
  expect(signal.aborted).toBe(true);
  await act(async () => {
    oldComplete(source("lato-regular"));
  });
  expect(screen.queryByText("Complete original license notice")).toBeNull();
  await act(async () => {
    finish(source("lato-regular"));
  });
  expect(screen.getByRole("alert").textContent).toContain(
    "differs from the selected",
  );
  fireEvent.submit(screen.getByRole("form"));
  expect(input.importFont).not.toHaveBeenCalled();
});

it("keeps authored text across family/style changes and prevents IME confirmation from importing", async () => {
  const input = props();
  render(<FontImport {...input} />);
  await screen.findByText("Complete original license notice");
  fireEvent.change(screen.getByLabelText("Text to check"), {
    target: { value: "保留\nExact " },
  });
  fireEvent.change(screen.getByLabelText("Font style"), {
    target: { value: "700:italic" },
  });
  await waitFor(() =>
    expect(
      (screen.getByRole("button", { name: "Import font" }) as HTMLButtonElement)
        .disabled,
    ).toBe(false),
  );
  expect(
    (screen.getByLabelText("Text to check") as HTMLTextAreaElement).value,
  ).toBe("保留\nExact ");
  fireEvent.compositionStart(screen.getByLabelText("Text to check"));
  fireEvent.submit(screen.getByRole("form"));
  expect(input.importFont).not.toHaveBeenCalled();
  fireEvent.compositionEnd(screen.getByLabelText("Text to check"));
  fireEvent.submit(screen.getByRole("form"));
  await waitFor(() =>
    expect(input.importFont).toHaveBeenCalledExactlyOnceWith(
      "lato-bold-italic",
      "保留\nExact ",
    ),
  );
});
const derived = () => ({
  import_available: true,
  unavailable_reason: null,
  source_commit: "derived-commit",
  fonts: [
    {
      id: "variable-recipe",
      family: "Variable Only",
      weight: 400,
      style: "normal" as const,
      derivation_id: "derived-output-proof",
      content_hash: "d".repeat(64),
      size_bytes: "120",
    },
  ],
});
const derivedSource = () => ({
  face: derived().fonts[0],
  source_commit: "derived-commit",
  source_url: "https://example.test/pinned-variable.ttf",
  source_sha256: "e".repeat(64),
  source_size_bytes: "240",
  license_url: "https://example.test/OFL.txt",
  license_id: "OFL-1.1",
  license_notice: "Derived face original license",
  axes: [{ tag: "wght", coordinate: "400" }],
  proof_json: '{"derivation_id":"derived-output-proof"}',
});
function derivedFace() {
  return {
    ...derived().fonts[0],
    display_family: "Variable Only",
    requested_style: null,
    kind: "derived" as const,
    render_profile: "static_outline" as const,
    alias_reason: null,
    import_available: true,
    unavailable_reason: null,
  };
}
const derivedEnvelope = (): FontBrowserSource => ({
  face: derivedFace(),
  details: { kind: "derived", source: derivedSource() },
  license_url: "https://example.test/OFL.txt",
  license_sha256: "c".repeat(64),
});
it("merges an exact variable-only face into the atlas and imports it through the shared acquisition operation", async () => {
  const input = props();
  input.catalog.fonts.push(derivedFace());
  input.readSource = vi.fn(async ({ catalog_id }) =>
    catalog_id === "variable-recipe" ? derivedEnvelope() : source(catalog_id),
  );
  render(<FontImport {...input} />);
  fireEvent.change(screen.getByLabelText("Font family"), {
    target: { value: "Variable Only" },
  });
  await screen.findByText("Derived face original license");
  expect(
    screen.getByText("Verified static instance from a variable font"),
  ).toBeTruthy();
  expect(screen.getByText("wght=400")).toBeTruthy();
  expect(
    (screen.getByRole("button", { name: "Import font" }) as HTMLButtonElement)
      .disabled,
  ).toBe(false);
  fireEvent.click(screen.getByRole("button", { name: "Import font" }));
  await waitFor(() =>
    expect(input.importFont).toHaveBeenCalledExactlyOnceWith(
      "variable-recipe",
      "Hello 你好",
    ),
  );
});
it("retains static precedence and blocks a selected derived face when only static download is available", async () => {
  const input = props();
  input.catalog.fonts.push(
    {
      ...derivedFace(),
      import_available: false,
      unavailable_reason: "Offline derivation runtime",
    },
    {
      ...derivedFace(),
      id: "duplicate-static",
      family: "Lato",
      display_family: "Lato",
    },
  );
  input.readSource = vi.fn(async ({ catalog_id }) =>
    catalog_id === "variable-recipe" ? derivedEnvelope() : source(catalog_id),
  );
  render(<FontImport {...input} />);
  await screen.findByText("Complete original license notice");
  expect(input.readSource).toHaveBeenCalledExactlyOnceWith(
    { catalog_id: "lato-regular" },
    expect.any(AbortSignal),
  );
  fireEvent.change(screen.getByLabelText("Font family"), {
    target: { value: "Variable Only" },
  });
  await screen.findByText("Derived face original license");
  expect(screen.getByText("Offline derivation runtime")).toBeTruthy();
  expect(
    (screen.getByRole("button", { name: "Import font" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  expect(input.importFont).not.toHaveBeenCalled();
});
it("rejects a variable input hash asserted as the selected static output identity", async () => {
  const input = props();
  input.catalog.fonts.push(derivedFace());
  input.readSource = vi.fn(async ({ catalog_id }) =>
    catalog_id === "variable-recipe"
      ? {
          ...derivedEnvelope(),
          details: {
            kind: "derived" as const,
            source: {
              ...derivedSource(),
              face: { ...derived().fonts[0], content_hash: "e".repeat(64) },
            },
          },
        }
      : source(catalog_id),
  );
  render(<FontImport {...input} />);
  fireEvent.change(screen.getByLabelText("Font family"), {
    target: { value: "Variable Only" },
  });
  await screen.findByText(
    "The font source differs from the selected catalog face.",
  );
  expect(
    (screen.getByRole("button", { name: "Import font" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
});
