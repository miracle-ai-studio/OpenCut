// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { initialSnapshot } from "./fixture";
import { preferences } from "../../shared/locale";
import { ProjectCoverEditor } from "./project-cover";
import { uploadMedia } from "../upload";
import type { Operation } from "./types";
const mocks = vi.hoisted(() => ({ workspace: null as any }));
vi.mock("../context", () => ({ useWorkspace: () => mocks.workspace }));
vi.mock("../upload", () => ({ uploadMedia: vi.fn() }));
beforeEach(() => {
  preferences({ language: "en" });
  HTMLDialogElement.prototype.showModal = vi.fn(function(this: HTMLDialogElement) { this.open = true; });
  HTMLDialogElement.prototype.close = vi.fn(function(this: HTMLDialogElement) { this.open = false; });
  URL.createObjectURL = vi.fn(() => "blob:cover-image");
  URL.revokeObjectURL = vi.fn();
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.clearAllMocks(); });
function fixture() {
  const head = initialSnapshot();
  head.project.assets[0].kind = "video";
  head.project.assets[0].duration = { ticks: "4", timebase: 1 };
  const write = vi.fn(async (_label: string, _operations: Operation[]) => ({}));
  const close = vi.fn();
  const state = {
    api: { coverSource: vi.fn(async () => "http://localhost/source"), sdk: {} },
    scene: { head, historical: null, audioPreview: { phase: "paused" }, recovery: null, media: { attempt: null }, propertyDrafts: [] }, busy: false,
    controller: { scene: { write, recovery: { blocked: () => false } }, getSnapshot: () => mocks.workspace },
  };
  mocks.workspace = state;
  const view = render(<ProjectCoverEditor onClose={close} />);
  return { ...state, write, close, view };
}
it("previews a selected source time and writes the shared typed cover operation", async () => {
  const f = fixture();
  const preview = await screen.findByLabelText("Cover preview");
  fireEvent.change(screen.getByLabelText("Select cover frame"), { target: { value: "2.125" } });
  fireEvent.loadedData(preview);
  fireEvent.seeked(preview);
  fireEvent.click(screen.getByRole("button", { name: "Save cover" }));
  await waitFor(() => expect(f.close).toHaveBeenCalled());
  expect(f.write).toHaveBeenCalledWith("Set project cover", [{ type: "cover_set", cover: { kind: "video_frame", asset_id: "asset", time: { ticks: "17", timebase: 8 } } }]);
});
it("keeps an unchanged rational source time exact when reopening its cover", async () => {
  const f = fixture();
  f.view.unmount();
  f.scene.head.project.cover = { kind: "video_frame", asset_id: "asset", time: { ticks: "23", timebase: 24 } };
  render(<ProjectCoverEditor onClose={f.close} />);
  const preview = await screen.findByLabelText("Cover preview");
  fireEvent.loadedData(preview); fireEvent.seeked(preview);
  fireEvent.click(screen.getByRole("button", { name: "Save cover" }));
  await waitFor(() => expect(f.write).toHaveBeenCalled());
  expect(f.write.mock.calls[0][1][0]).toMatchObject({ cover: { time: { ticks: "23", timebase: 24 } } });
});
it("uploads an image through existing media ingestion and binds it with the cover in one edit", async () => {
  const f = fixture();
  fireEvent.change(screen.getByLabelText("Cover source"), { target: { value: "image" } });
  const version = f.scene.head.project.assets[0].version;
  vi.mocked(uploadMedia).mockResolvedValue({ media_id: "upload-image", version, asset: { kind: "image" }, media: { project_id: f.scene.head.project.id } } as never);
  fireEvent.change(screen.getByLabelText("Upload cover image"), { target: { files: [new File(["image"], "cover.png", { type: "image/png" })] } });
  fireEvent.load(await screen.findByAltText("Cover preview"));
  fireEvent.click(screen.getByRole("button", { name: "Save cover" }));
  await waitFor(() => expect(f.close).toHaveBeenCalled());
  const operations = f.write.mock.calls[0][1];
  const binding = operations[0];
  expect(binding).toMatchObject({ type: "asset_bind", name: "cover.png", source: { kind: "project_media", media_id: "upload-image", version } });
  if (binding.type !== "asset_bind") throw Error("Expected an image binding");
  expect(operations[1]).toEqual({ type: "cover_set", cover: { kind: "image", asset_id: binding.id } });
  expect(URL.revokeObjectURL).not.toHaveBeenCalled();
  f.view.unmount();
  expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:cover-image");
});
it("does not attach a finished upload to a changed project revision", async () => {
  const f = fixture();
  let finish!: (result: never) => void;
  vi.mocked(uploadMedia).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
  fireEvent.change(screen.getByLabelText("Cover source"), { target: { value: "image" } });
  fireEvent.change(screen.getByLabelText("Upload cover image"), { target: { files: [new File(["image"], "cover.png", { type: "image/png" })] } });
  fireEvent.load(await screen.findByAltText("Cover preview"));
  fireEvent.click(screen.getByRole("button", { name: "Save cover" }));
  f.scene.head = { ...f.scene.head, revision: "another-head" };
  await act(async () => finish({ media_id: "finished", version: {}, asset: { kind: "image" }, media: { project_id: f.scene.head.project.id } } as never));
  expect(f.write).not.toHaveBeenCalled();
  expect(f.close).not.toHaveBeenCalled();
  expect(await screen.findByText("The project changed. Reopen the cover editor.", { exact: false })).toBeTruthy();
});
it("clears the saved cover through the same public edit operation", async () => {
  const f = fixture();
  fireEvent.change(screen.getByLabelText("Cover source"), { target: { value: "automatic" } });
  fireEvent.click(screen.getByRole("button", { name: "Save cover" }));
  await waitFor(() => expect(f.close).toHaveBeenCalled());
  expect(f.write).toHaveBeenCalledWith("Set project cover", [{ type: "cover_set", cover: null }]);
});
