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
import { V4FontImport } from "./font-import";
import type { V4FontImportProps } from "./font-import";
import type { FontBrowserCatalog, FontBrowserSource } from "./font-browser";

beforeEach(() => preferences({ language: "en" }));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
const catalog = (): FontBrowserCatalog => ({
  schema_version: 2,
  download_available: true,
  unavailable_reason: null,
  source_commit: "fixed-commit",
  classic_atlas_sha256: "a".repeat(64),
  classic_family_count: 2,
  pinned_face_count: 2,
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
          unavailable_reason: "weight_not_expressed_by_text_v4",
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
const source = (id: string): FontBrowserSource => ({
  id,
  family: "Lato",
  weight: id === "lato-regular" ? 400 : 700,
  style: id === "lato-regular" ? "normal" : "italic",
  source_commit: "fixed-commit",
  source_url:
    "https://raw.githubusercontent.com/google/fonts/fixed-commit/lato.ttf",
  content_hash: "b".repeat(64),
  size_bytes: "100",
  license_id: "OFL-1.1",
  license_notice: "Complete original license notice",
});
const props = (): V4FontImportProps => ({
  scope: { projectId: "p1", revision: "r1" },
  available: true,
  choices: [],
  requiredText: "Hello 你好",
  browserCatalog: catalog(),
  readSource: vi.fn(async (id) => source(id)),
  importFont: vi.fn(async () => {}),
  close: vi.fn(),
});

it("searches all families and loads only the selected exact static face license", async () => {
  const input = props();
  render(<V4FontImport {...input} />);
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
  render(<V4FontImport {...input} />);
  await screen.findByText("Complete original license notice");
  fireEvent.change(screen.getByLabelText("Font style"), {
    target: { value: "600:normal" },
  });
  expect(screen.getByRole("status").textContent).toContain(
    "weight is not supported",
  );
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
  expect(screen.getByRole("status").textContent).toContain(
    "No fixed static source",
  );
  expect(input.readSource).toHaveBeenCalledTimes(1);
  fireEvent.change(screen.getByLabelText("Search fonts"), {
    target: { value: "" },
  });
  fireEvent.change(screen.getByLabelText("Font family"), {
    target: { value: "Arial" },
  });
  expect(screen.getByRole("status").textContent).toContain(
    "System fonts cannot be imported",
  );
});

it("allows offline catalog browsing but disables importing when host policy denies downloads", async () => {
  const input = props();
  input.browserCatalog = {
    ...catalog(),
    download_available: false,
    unavailable_reason: "Downloads are disabled.",
  };
  render(<V4FontImport {...input} />);
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
  render(<V4FontImport {...input} />);
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
  render(<V4FontImport {...input} />);
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
  schema_version: 1,
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
it("merges an exact variable-only face into the same atlas and imports it through its distinct available protocol", async () => {
  const input = props();
  input.browserCatalog!.download_available = false;
  input.derivedCatalog = derived();
  input.readDerivedSource = vi.fn(async () => derivedSource());
  render(<V4FontImport {...input} />);
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
      "derived",
    ),
  );
});
it("retains static precedence and blocks a selected derived face when only static download is available", async () => {
  const input = props();
  input.derivedCatalog = {
    ...derived(),
    import_available: false,
    unavailable_reason: "Offline derivation runtime",
    fonts: [
      ...derived().fonts,
      { ...derived().fonts[0], id: "duplicate-static", family: "Lato" },
    ],
  };
  input.readDerivedSource = vi.fn(async () => derivedSource());
  render(<V4FontImport {...input} />);
  await screen.findByText("Complete original license notice");
  expect(input.readDerivedSource).not.toHaveBeenCalled();
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
  input.derivedCatalog = derived();
  input.readDerivedSource = vi.fn(async () => ({
    ...derivedSource(),
    face: { ...derived().fonts[0], content_hash: "e".repeat(64) },
  }));
  render(<V4FontImport {...input} />);
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
