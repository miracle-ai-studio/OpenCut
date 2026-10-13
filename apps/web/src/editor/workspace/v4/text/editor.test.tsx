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
import { V4TextEditor } from "./editor";
import { textFonts, textInitial } from "./fixture";
import type { V4TextEditorProps } from "./types";

beforeEach(() => {
  preferences({ language: "en" });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
const props = (): V4TextEditorProps => ({
  mode: "edit",
  scope: {
    projectId: "project",
    revision: "r0",
    sequenceId: "scene",
    elementId: "title",
  },
  available: true,
  initial: textInitial(),
  fonts: textFonts(),
  save: vi.fn(async () => {}),
  close: vi.fn(),
});

it("publishes no entry until locked-font frames are executable and blocks a missing face", () => {
  const input = props();
  const view = render(<V4TextEditor {...input} available={false} />);
  expect(screen.queryByRole("form")).toBeNull();
  view.rerender(<V4TextEditor {...input} fonts={[]} />);
  fireEvent.change(screen.getByLabelText("Text content"), {
    target: { value: "Updated" },
  });
  expect(
    (screen.getByRole("button", { name: "Save text" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  fireEvent.submit(screen.getByRole("form"));
  expect(input.save).not.toHaveBeenCalled();
});

it("submits one authored title with exact timing and closes only after the caller has saved", async () => {
  const input = props();
  let complete!: () => void;
  input.save = vi.fn(
    () =>
      new Promise<void>((resolve) => {
        complete = resolve;
      }),
  );
  render(<V4TextEditor {...input} />);
  expect(
    (screen.getByRole("button", { name: "Save text" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  fireEvent.change(screen.getByLabelText("Text content"), {
    target: { value: "New title\n你好" },
  });
  fireEvent.change(screen.getByLabelText("Font family"), {
    target: { value: "bold-italic" },
  });
  fireEvent.submit(screen.getByRole("form"));
  fireEvent.submit(screen.getByRole("form"));
  expect(input.save).toHaveBeenCalledOnce();
  expect(input.close).not.toHaveBeenCalled();
  const value = vi.mocked(input.save).mock.calls[0][0];
  expect(value.text.content).toBe("New title\n你好");
  expect(value.text.font_weight).toBe("bold");
  expect(value.text.font_style).toBe("italic");
  expect(value.timeline_range).toEqual(input.initial.timeline_range);
  await act(async () => {
    complete();
  });
  expect(input.close).toHaveBeenCalledOnce();
});

it("keeps composition local, prevents timeline keyboard propagation and cancels drafts without writing", () => {
  const input = props(),
    timelineKey = vi.fn();
  render(
    <div onKeyDown={timelineKey}>
      <V4TextEditor {...input} />
    </div>,
  );
  const content = screen.getByLabelText("Text content");
  fireEvent.compositionStart(content);
  fireEvent.change(content, { target: { value: "正在输入" } });
  fireEvent.keyDown(content, { key: "Escape", isComposing: true });
  fireEvent.submit(screen.getByRole("form"));
  expect(input.save).not.toHaveBeenCalled();
  expect(input.close).not.toHaveBeenCalled();
  fireEvent.compositionEnd(content);
  fireEvent.keyDown(content, { key: "s" });
  fireEvent.keyDown(content, { key: " " });
  expect(timelineKey).not.toHaveBeenCalled();
  fireEvent.keyDown(content, { key: "Escape" });
  expect(input.close).toHaveBeenCalledOnce();
  expect(input.save).not.toHaveBeenCalled();
});

it("changes in revision or selection discard only the old local draft and ignore its late save closure", async () => {
  const input = props();
  let complete!: () => void;
  input.save = vi.fn(
    () =>
      new Promise<void>((resolve) => {
        complete = resolve;
      }),
  );
  const view = render(<V4TextEditor {...input} />);
  fireEvent.change(screen.getByLabelText("Text content"), {
    target: { value: "Old draft" },
  });
  fireEvent.submit(screen.getByRole("form"));
  const next = {
    ...input,
    scope: { ...input.scope, revision: "r1", elementId: "next" },
    initial: textInitial(),
    close: vi.fn(),
  };
  next.initial.text.content = "Other title";
  view.rerender(<V4TextEditor {...next} />);
  expect(
    (screen.getByLabelText("Text content") as HTMLTextAreaElement).value,
  ).toBe("Other title");
  await act(async () => {
    complete();
  });
  expect(input.close).not.toHaveBeenCalled();
  expect(next.close).not.toHaveBeenCalled();
});

it("shows planner failures while retaining the draft and never commits on blur or cancel", async () => {
  const input = props();
  input.save = vi.fn(async () => {
    throw new Error("Font does not contain glyph U+4F60");
  });
  render(<V4TextEditor {...input} />);
  fireEvent.change(screen.getByLabelText("Text content"), {
    target: { value: "你" },
  });
  fireEvent.blur(screen.getByLabelText("Text content"));
  expect(input.save).not.toHaveBeenCalled();
  fireEvent.submit(screen.getByRole("form"));
  await waitFor(() =>
    expect(screen.getByRole("alert").textContent).toContain("U+4F60"),
  );
  expect(
    (screen.getByLabelText("Text content") as HTMLTextAreaElement).value,
  ).toBe("你");
  expect(input.close).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(input.close).toHaveBeenCalledOnce();
  expect(input.save).toHaveBeenCalledOnce();
});

it("blocks a historical or locked host scope even when the form is submitted programmatically", () => {
  const input = props();
  input.mode = "create";
  input.disabledReason = "Return to the current revision before editing.";
  render(<V4TextEditor {...input} />);
  expect(screen.getByRole("status").textContent).toContain("current revision");
  fireEvent.submit(screen.getByRole("form"));
  expect(input.save).not.toHaveBeenCalled();
});

it("submits decoration, spacing and all background controls in one save without changing other fields", async () => {
  const input = props();
  render(<V4TextEditor {...input} />);
  const values = {
    "Text decoration": "line_through",
    "Letter spacing (pixels)": "2.5",
    "Line height": "1.5",
    "Background color": "#112233",
    "Background opacity (%)": "75",
    "Background corner radius (%)": "55",
    "Background horizontal padding (Classic size 15)": "12",
    "Background vertical padding (Classic size 15)": "6",
    "Background horizontal offset (pixels)": "-15",
    "Background vertical offset (pixels)": "4",
  };
  for (const [label, value] of Object.entries(values))
    fireEvent.change(screen.getByLabelText(label), { target: { value } });
  fireEvent.submit(screen.getByRole("form"));
  await waitFor(() => expect(input.close).toHaveBeenCalledOnce());
  expect(input.save).toHaveBeenCalledOnce();
  const value = vi.mocked(input.save).mock.calls[0][0];
  expect(value.text).toEqual({
    ...input.initial.text,
    decoration: "line_through",
    letter_spacing_px: 2.5,
    line_height_multiplier: 1.5,
    background: {
      enabled: true,
      color: {
        encoding: "srgb",
        red: 17 / 255,
        green: 34 / 255,
        blue: 51 / 255,
        alpha: 0.75,
      },
      corner_radius_percent: 55,
      padding_x_at_size15: 12,
      padding_y_at_size15: 6,
      offset_x_px: -15,
      offset_y_px: 4,
    },
  });
  expect(value.timeline_range).toEqual(input.initial.timeline_range);
});
it("weight and italic controls switch only actual same-family locked faces", async () => {
  const input = props();
  input.fonts = [
    { ...input.fonts[0], id: "another-regular-face" },
    ...input.fonts,
    { ...input.fonts[0], id: "bold-normal", weight: 700 },
    { ...input.fonts[0], id: "regular-italic", style: "italic" },
  ];
  render(<V4TextEditor {...input} />);
  fireEvent.change(screen.getByLabelText("Font weight"), {
    target: { value: "400" },
  });
  expect(
    (screen.getByLabelText("Font family") as HTMLSelectElement).value,
  ).toBe("regular");
  fireEvent.change(screen.getByLabelText("Font weight"), {
    target: { value: "700" },
  });
  expect(
    (screen.getByLabelText("Font family") as HTMLSelectElement).value,
  ).toBe("bold-normal");
  fireEvent.change(screen.getByLabelText("Font style"), {
    target: { value: "italic" },
  });
  expect(
    (screen.getByLabelText("Font family") as HTMLSelectElement).value,
  ).toBe("bold-italic");
  fireEvent.submit(screen.getByRole("form"));
  await waitFor(() => expect(input.save).toHaveBeenCalledOnce());
  expect(vi.mocked(input.save).mock.calls[0][0].text).toEqual({
    ...input.initial.text,
    font: { kind: "locked", font_id: "bold-italic" },
    font_weight: "bold",
    font_style: "italic",
  });
});
it("missing real variants cannot synthesize styles, and invalid background never calls save", async () => {
  const input = props();
  input.fonts = [
    input.fonts[0],
    { ...input.fonts[0], id: "other-bold", family: "Other", weight: 700 },
  ];
  render(<V4TextEditor {...input} />);
  const weight = screen.getByLabelText("Font weight") as HTMLSelectElement;
  const style = screen.getByLabelText("Font style") as HTMLSelectElement;
  expect(
    Array.from(weight.options).find((option) => option.value === "700")
      ?.disabled,
  ).toBe(true);
  expect(
    Array.from(style.options).find((option) => option.value === "italic")
      ?.disabled,
  ).toBe(true);
  fireEvent.change(weight, { target: { value: "700" } });
  fireEvent.change(style, { target: { value: "italic" } });
  expect(
    (screen.getByLabelText("Font family") as HTMLSelectElement).value,
  ).toBe("regular");
  fireEvent.change(screen.getByLabelText("Background corner radius (%)"), {
    target: { value: "101" },
  });
  fireEvent.submit(screen.getByRole("form"));
  await waitFor(() =>
    expect(screen.getByRole("alert").textContent).toContain(
      "between 0 and 100",
    ),
  );
  expect(input.save).not.toHaveBeenCalled();
  fireEvent.click(screen.getByLabelText("Enable text background"));
  expect(
    (screen.getByLabelText("Background color") as HTMLInputElement).matches(
      ":disabled",
    ),
  ).toBe(true);
  expect(input.save).not.toHaveBeenCalled();
});
