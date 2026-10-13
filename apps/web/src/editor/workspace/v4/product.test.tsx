// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { WorkspaceProvider } from "../context";
import { V4Product } from "./product";
import { setupV4 } from "./fixture";
import { BookmarkEditor } from "./bookmarks";
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
it("native file paste captures the selected track and exact playhead, while text input retains clipboard ownership", async () => {
  const { controller, current } = await setupV4();
  try {
    current().project.sequences[0].tracks.push({ ...structuredClone(current().project.sequences[0].tracks[0]),
      id: "selected-audio", name: "Audio", kind: "audio", elements: [] });
    await controller.v4.refresh();
    controller.v4.select("selected-audio"); controller.v4.seek({ ticks: "1", timebase: 3 });
    const importing = vi.spyOn(controller.v4.media, "importFiles").mockResolvedValue(undefined);
    const view = render(<WorkspaceProvider controller={controller}><V4Product /></WorkspaceProvider>);
    const file = new File(["wav"], "sound.wav", { type: "audio/wav" });
    const data = { files: [file], getData: () => "" };
    fireEvent.paste(view.container.querySelector(".v4-timeline-panel")!, { clipboardData: data });
    await waitFor(() => expect(importing).toHaveBeenCalledOnce());
    expect(importing).toHaveBeenCalledWith([file], undefined, {
      scope: { projectId: current().project.id, revision: "r0", sequenceId: "scene" },
      trackId: "selected-audio", at: { ticks: "1", timebase: 3 },
    });
    fireEvent.paste(screen.getByLabelText("Playhead (seconds)"), { clipboardData: data });
    expect(importing).toHaveBeenCalledOnce();
  } finally { controller.dispose(); }
});

it("keeps a focused or dirty seek intention through confirmed consumer progress and submits that exact time", async () => {
  const { controller } = await setupV4();
  try {
    render(<WorkspaceProvider controller={controller}><V4Product /></WorkspaceProvider>);
    const input = screen.getByLabelText("Playhead (seconds)") as HTMLInputElement;
    // The callback is the same Session boundary used for Rust-confirmed Time;
    // this component test does not claim an actual audio device observation.
    act(() => controller.v4["presentAudio"]({ ticks: "1", timebase: 2 }));
    expect(input.value).toBe("0.500000");
    fireEvent.focus(input);
    act(() => controller.v4["presentAudio"]({ ticks: "3", timebase: 4 }));
    expect(input.value).toBe("0.500000");
    fireEvent.blur(input);
    expect(input.value).toBe("0.750000");
    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: "296.250000" } });
    act(() => controller.v4["presentAudio"]({ ticks: "1", timebase: 1 }));
    expect(input.value).toBe("296.250000");
    fireEvent.blur(input);
    act(() => controller.v4["presentAudio"]({ ticks: "5", timebase: 4 }));
    expect(input.value).toBe("296.250000");
    const seek = vi.spyOn(controller.v4, "seek");
    fireEvent.click(screen.getByRole("button", { name: "Go to time" }));
    expect(seek).toHaveBeenCalledExactlyOnceWith({ ticks: "1185", timebase: 4 });
    act(() => controller.v4["presentAudio"]({ ticks: "297", timebase: 1 }));
    expect(input.value).toBe("297.000000");
    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: "12.345678" } });
    act(() => controller.v4["presentAudio"]({ ticks: "299", timebase: 1 }));
    fireEvent.keyDown(input, { key: "Enter" });
    expect(seek).toHaveBeenLastCalledWith({ ticks: "6172839", timebase: 500000 });
  } finally { controller.dispose(); }
});
it("cancels a seek draft without dispatch and clears it when the scene changes", async () => {
  const { controller, current } = await setupV4();
  try {
    const next = structuredClone(current().project.sequences[0]);
    next.id = "next-scene"; next.main_track_id = "next-track";
    next.tracks[0].id = "next-track"; next.tracks[0].elements[0].id = "next-image";
    current().project.sequences.push(next);
    await controller.v4.refresh();
    render(<WorkspaceProvider controller={controller}><V4Product /></WorkspaceProvider>);
    const input = screen.getByLabelText("Playhead (seconds)") as HTMLInputElement;
    const seek = vi.spyOn(controller.v4, "seek");
    fireEvent.focus(input); fireEvent.change(input, { target: { value: "42" } });
    act(() => controller.v4["presentAudio"]({ ticks: "2", timebase: 1 }));
    fireEvent.keyDown(input, { key: "Escape" });
    expect(input.value).toBe("2.000000"); expect(seek).not.toHaveBeenCalled();
    act(() => controller.v4["presentAudio"]({ ticks: "3", timebase: 1 }));
    expect(input.value).toBe("3.000000");
    fireEvent.change(input, { target: { value: "99" } });
    fireEvent.change(screen.getByLabelText("V4 scene"), { target: { value: "next-scene" } });
    expect(input.value).toBe("0.000000");
    act(() => controller.v4["presentAudio"]({ ticks: "1", timebase: 4 }));
    expect(input.value).toBe("0.250000"); expect(seek).not.toHaveBeenCalled();
  } finally { controller.dispose(); }
});
it("synchronizes the selected V4 scene URL without changing another project's route or navigation state", async () => {
  const originalUrl = location.href, originalState = history.state;
  const { controller, current } = await setupV4();
  try {
    const next = structuredClone(current().project.sequences[0]);
    next.id = "next-scene"; next.name = "Next scene";
    next.main_track_id = "next-track"; next.tracks[0].id = "next-track";
    next.tracks[0].elements[0].id = "next-image";
    current().project.sequences.push(next);
    await controller.v4.refresh();
    const navigation = { panel: "timeline", position: 7 };
    history.replaceState(navigation, "", "/editor/v4-project?scene=scene&layout=compact#timeline");
    const view = render(<WorkspaceProvider controller={controller}><V4Product /></WorkspaceProvider>);
    fireEvent.change(screen.getByLabelText("V4 scene"), { target: { value: "next-scene" } });
    expect(controller.getSnapshot().v4!.sequenceId).toBe("next-scene");
    expect(location.pathname + location.search + location.hash).toBe("/editor/v4-project?scene=next-scene&layout=compact#timeline");
    expect(history.state).toEqual(navigation);
    expect(new URL(location.href).searchParams.get("scene")).toBe(controller.getSnapshot().v4!.sequenceId);
    view.unmount();
    history.replaceState(navigation, "", "/editor/different-project?scene=other&layout=compact#timeline");
    render(<WorkspaceProvider controller={controller}><V4Product /></WorkspaceProvider>);
    fireEvent.change(screen.getByLabelText("V4 scene"), { target: { value: "scene" } });
    expect(controller.getSnapshot().v4!.sequenceId).toBe("scene");
    expect(location.pathname + location.search + location.hash).toBe("/editor/different-project?scene=other&layout=compact#timeline");
    expect(history.state).toEqual(navigation);
  } finally {
    cleanup(); controller.dispose(); history.replaceState(originalState, "", originalUrl);
  }
});
it("uses the shared timeline selection for modifier clicks and preserves bookmark selection", async () => {
  const { controller, current } = await setupV4();
  try {
    const sequence = current().project.sequences[0];
    const second = structuredClone(sequence.tracks[0].elements[0]);
    second.id = "second-image";
    second.name = "Second image";
    sequence.tracks[0].elements.push(second);
    sequence.bookmarks.push({ id: "bookmark", sequence_id: sequence.id, time: { ticks: "0", timebase: 1 }, note: "Scene note" });
    await controller.v4.refresh();
    render(<WorkspaceProvider controller={controller}><V4Product /></WorkspaceProvider>);
    const firstButton = screen.getByRole("button", { name: "Whole image · image" });
    const secondButton = screen.getByRole("button", { name: "Second image · image" });
    fireEvent.click(firstButton);
    fireEvent.click(secondButton, { shiftKey: true });
    expect(controller.getSnapshot().v4!.timelineEdit.selection.map(target => target.element_id)).toEqual(["image", "second-image"]);
    expect(firstButton.getAttribute("aria-pressed")).toBe("true");
    expect(secondButton.getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("button", { name: "Move selection" }).hasAttribute("disabled")).toBe(false);
    fireEvent.click(firstButton, { ctrlKey: true });
    expect(controller.getSnapshot().v4!.timelineEdit.selection.map(target => target.element_id)).toEqual(["second-image"]);
    fireEvent.click(screen.getByRole("button", { name: "Bookmark at 00:00.00: Scene note" }));
    expect(controller.getSnapshot().v4!.selectedId).toBe("bookmark");
    expect(controller.getSnapshot().v4!.timelineEdit.selection).toEqual([]);
    expect(screen.getByRole("button", { name: "Move selection" }).hasAttribute("disabled")).toBe(true);
  } finally { controller.dispose(); }
});
it("provides bookmark CRUD through timeline markers, canvas notes and selective history in the product workspace", async () => {
  const { controller, current, ref } = await setupV4();
  try {
    render(
      <WorkspaceProvider controller={controller}>
        <V4Product />
      </WorkspaceProvider>,
    );
    const timeline = () => within(screen.getByRole("region", { name: "Timeline" }));
    const history = () => within(screen.getByRole("region", { name: "History" }));
    const clickReady = async (name: string) => {
      await waitFor(() => expect(controller.getSnapshot().busy).toBe(false));
      const button = await screen.findByText(name, { selector: "button" });
      await waitFor(() => expect(button.hasAttribute("disabled")).toBe(false));
      fireEvent.click(button);
    };
    expect(screen.getByText("Preview unavailable")).toBeTruthy();
    expect(
      screen.getByText(
        "Video, audio and image timeline editing, media import and placement, source media preview, scene notes, history, source audio separation/restoration, element volume and mute are available. Still scene frames and scene sound depend on this service’s rendering capability. Animated trim/split and other element editing are not available yet. Scene preview supports normal video/static image composition and fixed-font text on WebGPU browsers. Video MP4 export depends on this service’s rendering capability.",
      ),
    ).toBeTruthy();
    await clickReady("Add bookmark");
    fireEvent.change(screen.getByLabelText("Bookmark time (seconds)"), {
      target: { value: "2" },
    });
    fireEvent.change(screen.getByLabelText("Canvas note"), {
      target: { value: "Review this frame" },
    });
    await clickReady("Save bookmark");
    await waitFor(() =>
      expect(current().project.sequences[0].bookmarks).toHaveLength(1),
    );
    const marker = await timeline().findByLabelText("Bookmark at 00:02.00: Review this frame");
    fireEvent.click(marker);
    expect(screen.getByRole("region", { name: "Canvas notes" })).toBeTruthy();
    expect(
      screen.getByRole("region", { name: "Canvas notes" }).textContent,
    ).toContain("Review this frame");
    await clickReady("Edit bookmark");
    fireEvent.change(screen.getByLabelText("Canvas note"), {
      target: { value: "Updated note" },
    });
    await clickReady("Save bookmark");
    await timeline().findByLabelText("Bookmark at 00:02.00: Updated note");
    fireEvent.change(screen.getByLabelText("Playhead (seconds)"), {
      target: { value: "4" },
    });
    await clickReady("Go to time");
    await clickReady("Move to playhead");
    await timeline().findByLabelText("Bookmark at 00:04.00: Updated note");
    await act(async () => {
      await controller.open(ref());
    });
    fireEvent.click(
      timeline().getByLabelText("Bookmark at 00:04.00: Updated note"),
    );
    await clickReady("Delete bookmark");
    await waitFor(() =>
      expect(current().project.sequences[0].bookmarks).toHaveLength(0),
    );
    await waitFor(() =>
      expect(
        timeline().queryByLabelText("Bookmark at 00:04.00: Updated note"),
      ).toBeNull(),
    );
    await clickReady("Undo");
    await history().findByText("Undo plan");
    await clickReady("Apply undo");
    await timeline().findByLabelText("Bookmark at 00:04.00: Updated note");
    await clickReady("Redo");
    await history().findByText("Redo plan");
    await clickReady("Apply redo");
    await waitFor(() =>
      expect(current().project.sequences[0].bookmarks).toHaveLength(0),
    );
  } finally {
    controller.dispose();
  }
});
it("editing a note preserves its original exact non-terminating bookmark time and duration", async () => {
  const save = vi.fn().mockResolvedValue({});
  render(
    <BookmarkEditor
      bookmark={{
        id: "b",
        sequence_id: "s",
        time: { ticks: "1", timebase: 3 },
        duration: { ticks: "2", timebase: 3 },
        note: "old",
      }}
      sequenceId="s"
      playhead={{ ticks: "0", timebase: 1 }}
      disabled={false}
      save={save}
      close={() => {}}
    />,
  );
  fireEvent.change(screen.getByLabelText("Canvas note"), {
    target: { value: "new" },
  });
  fireEvent.click(screen.getByText("Save bookmark"));
  await waitFor(() => expect(save).toHaveBeenCalled());
  expect(save.mock.calls[0][0].bookmark.time).toEqual({
    ticks: "1",
    timebase: 3,
  });
  expect(save.mock.calls[0][0].bookmark.duration).toEqual({
    ticks: "2",
    timebase: 3,
  });
});
it("shows canvas notes at inclusive exact endpoints, excludes whitespace and never forces selected notes outside their interval", async () => {
  const { controller, current } = await setupV4();
  try {
    await controller.v4.bookmark({
      kind: "put",
      bookmark: {
        id: "note",
        sequence_id: "scene",
        time: { ticks: "1", timebase: 3 },
        duration: { ticks: "1", timebase: 6 },
        note: "Exact endpoint",
      },
    });
    await controller.v4.bookmark({
      kind: "put",
      bookmark: {
        id: "blank",
        sequence_id: "scene",
        time: { ticks: "1", timebase: 3 },
        duration: { ticks: "1", timebase: 6 },
        note: "  \n ",
      },
    });
    controller.v4.seek({ ticks: "1", timebase: 3 });
    controller.v4.select("blank");
    render(
      <WorkspaceProvider controller={controller}>
        <V4Product />
      </WorkspaceProvider>,
    );
    const canvas = screen.getByRole("region", { name: "Canvas notes" });
    expect(canvas.textContent).toContain("Exact endpoint");
    expect(canvas.querySelectorAll(".v4-canvas-note")).toHaveLength(1);
    act(() => controller.v4.seek({ ticks: "1", timebase: 2 }));
    expect(canvas.textContent).toContain("Exact endpoint");
    act(() => {
      controller.v4.select("note");
      controller.v4.seek({ ticks: "501", timebase: 1000 });
    });
    expect(canvas.textContent).not.toContain("Exact endpoint");
    expect(current().project.sequences[0].bookmarks).toHaveLength(2);
  } finally {
    controller.dispose();
  }
});
it("a new note uses the exact playhead when its displayed time is untouched", async () => {
  const save = vi.fn().mockResolvedValue({});
  render(
    <BookmarkEditor
      bookmark={null}
      sequenceId="s"
      playhead={{ ticks: "1", timebase: 3 }}
      disabled={false}
      save={save}
      close={() => {}}
    />,
  );
  fireEvent.change(screen.getByLabelText("Canvas note"), {
    target: { value: "new" },
  });
  fireEvent.click(screen.getByText("Save bookmark"));
  await waitFor(() => expect(save).toHaveBeenCalled());
  expect(save.mock.calls[0][0].bookmark.time).toEqual({
    ticks: "1",
    timebase: 3,
  });
});
