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

beforeEach(() => {
  preferences({ language: "en" });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
const props = (): V4FontImportProps => ({
  scope: { projectId: "project", revision: "r0" },
  available: true,
  choices: [
    {
      id: "fixed-latin",
      label: "Fixed Latin Regular",
      family: "Example",
      weight: 400,
      style: "normal",
      licenseName: "OFL-1.1",
      sourceUrl: "https://example.test/latin",
    },
    {
      id: "fixed-cjk",
      label: "Fixed CJK Regular",
      family: "Example CJK",
      weight: 400,
      style: "normal",
      licenseName: "OFL-1.1",
      sourceUrl: "https://example.test/cjk",
    },
  ],
  requiredText: "OpenCut 你好",
  importFont: vi.fn(async () => {}),
  close: vi.fn(),
});

it("offers only host catalog choices and submits an opaque choice plus exact authored text", async () => {
  const input = props();
  render(<V4FontImport {...input} />);
  expect(
    screen.queryByRole("textbox", { name: /URL|license|family/i }),
  ).toBeNull();
  fireEvent.change(screen.getByLabelText("Font face"), {
    target: { value: "fixed-cjk" },
  });
  fireEvent.change(screen.getByLabelText("Text to check"), {
    target: { value: " 你好\nHello " },
  });
  expect(
    screen
      .getByRole("link", { name: "Font source and license" })
      .getAttribute("href"),
  ).toBe("https://example.test/cjk");
  fireEvent.submit(screen.getByRole("form"));
  await waitFor(() => expect(input.close).toHaveBeenCalledOnce());
  expect(input.importFont).toHaveBeenCalledExactlyOnceWith(
    "fixed-cjk",
    " 你好\nHello ",
  );
});

it("does not expose unavailable import and disables a withdrawn catalog choice", () => {
  const input = props();
  const view = render(<V4FontImport {...input} available={false} />);
  expect(screen.queryByRole("form")).toBeNull();
  view.rerender(<V4FontImport {...input} />);
  view.rerender(<V4FontImport {...input} choices={[]} />);
  fireEvent.submit(screen.getByRole("form"));
  expect(input.importFont).not.toHaveBeenCalled();
  expect(
    (screen.getByRole("button", { name: "Import font" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
});

it("keeps a rejected font unimported, retains text and supports explicit retry", async () => {
  const input = props();
  input.importFont = vi.fn(async () => {
    throw new Error("Font does not contain glyph U+4F60");
  });
  render(<V4FontImport {...input} />);
  fireEvent.submit(screen.getByRole("form"));
  await waitFor(() =>
    expect(screen.getByRole("alert").textContent).toContain("U+4F60"),
  );
  expect(input.close).not.toHaveBeenCalled();
  expect(
    (screen.getByLabelText("Text to check") as HTMLTextAreaElement).value,
  ).toBe(input.requiredText);
  fireEvent.change(screen.getByLabelText("Font face"), {
    target: { value: "fixed-cjk" },
  });
  expect(screen.queryByRole("alert")).toBeNull();
});

it("prevents duplicate imports and a completed old request from closing another project", async () => {
  const input = props();
  let complete!: () => void;
  input.importFont = vi.fn(
    () =>
      new Promise<void>((resolve) => {
        complete = resolve;
      }),
  );
  const view = render(<V4FontImport {...input} />);
  fireEvent.submit(screen.getByRole("form"));
  fireEvent.submit(screen.getByRole("form"));
  expect(input.importFont).toHaveBeenCalledOnce();
  view.rerender(
    <V4FontImport
      {...input}
      scope={{ projectId: "another", revision: "r0" }}
    />,
  );
  await act(async () => {
    complete();
  });
  expect(input.close).not.toHaveBeenCalled();
});
