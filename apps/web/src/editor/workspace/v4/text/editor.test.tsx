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
