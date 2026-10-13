// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { preferences } from "../../../shared/locale";
import { textFonts, textInitial } from "../text/fixture";
import { V4CaptionWorkspace, type CaptionWorkspaceOptions } from "./workspace";
const mocks = vi.hoisted(() => ({ state: {} as any, plan: vi.fn() }));
vi.mock("../../context", () => ({ useWorkspace: () => mocks.state }));
vi.mock("./planned-subtitles", () => ({ planEditableSubtitles: mocks.plan }));
beforeEach(() => preferences({ language: "en" }));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});
function fixture() {
  const transaction = {
    project_id: "project",
    expected_revision: "fixed",
    idempotency_key: "original",
    operations: [{ kind: "sequence_put" }],
  };
  const api = {
    sdk: {
      call: vi.fn(async (name: string) =>
        name === "opencut_text_v1_plan"
          ? { revision: "fixed", initial: textInitial() }
          : { available: false, reasons: [] },
      ),
    },
  };
  const apply = vi.fn(async () => {}),
    select = vi.fn();
  const head = {
    revision: "fixed",
    project: {
      id: "project",
      fonts: textFonts(),
      sequences: [
        {
          id: "scene",
          tracks: [
            {
              id: "legacy",
              kind: "caption",
              name: "Old captions",
              locked: false,
              elements: [],
            },
          ],
          captions: [{ track_id: "legacy" }],
        },
      ],
    },
  };
  mocks.state = {
    api,
    v4: { head, sequenceId: "scene" },
    jobs: [],
    busy: false,
    controller: {
      getSnapshot: () => mocks.state,
      sync: vi.fn(),
      v4: { applyPlanned: apply, select },
    },
  };
  mocks.plan.mockResolvedValue({
    transaction,
    selection: { track_id: "new-text", element_id: "cue-one" },
    cues: [{}, {}],
    warnings: [],
  });
  const snapshot = { phase: "idle", record: null, error: null, receipt: null };
  const mutateCaption = vi.fn(async () => ({
    cue_count: 1,
    warnings: [],
    transaction: { revision: "saved" },
  }));
  const props = {
    recovery: {
      subscribe: () => () => {},
      getSnapshot: () => snapshot,
      blocked: () => false,
    },
    mutateCaption,
  } as unknown as CaptionWorkspaceOptions;
  return { props, apply, select, transaction, mutateCaption };
}
function importFile() {
  fireEvent.change(screen.getByRole("textbox", { name: /^Subtitle content/ }), {
    target: { value: "1\n00:00:00,000 --> 00:00:01,000\nOriginal subtitle\n" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Import subtitles" }),
  );
}
it("defaults to editable text and applies the original transaction once with 24px real-face defaults", async () => {
  const f = fixture();
  render(<V4CaptionWorkspace {...f.props} />);
  expect(
    (
      screen.getByRole("combobox", {
        name: /^Caption track/,
      }) as HTMLSelectElement
    ).value,
  ).toBe("__new_subtitle_text_track__");
  importFile();
  await waitFor(() => expect(f.apply).toHaveBeenCalledOnce());
  expect(mocks.plan.mock.calls[0][1]).toMatchObject({
    project_id: "project",
    expected_revision: "fixed",
    sequence_id: "scene",
    track: { kind: "new" },
    source: {
      kind: "file",
      format: "srt",
      content: expect.stringContaining("Original subtitle"),
    },
    style: {
      content: "",
      font: { kind: "locked", font_id: "regular" },
      font_size: { unit: "canvas_pixels", value: 24 },
    },
    observations: null,
  });
  expect(f.apply).toHaveBeenCalledWith(f.transaction, "scene");
  expect(f.select).toHaveBeenCalledWith("cue-one");
  expect(f.mutateCaption).not.toHaveBeenCalled();
});
it("preserves legacy caption import recovery when that track is explicitly chosen", async () => {
  const f = fixture();
  render(<V4CaptionWorkspace {...f.props} />);
  fireEvent.change(screen.getByRole("combobox", { name: /^Caption track/ }), {
    target: { value: "legacy" },
  });
  importFile();
  await waitFor(() => expect(f.mutateCaption).toHaveBeenCalledOnce());
  expect(mocks.plan).not.toHaveBeenCalled();
  expect(f.apply).not.toHaveBeenCalled();
});
