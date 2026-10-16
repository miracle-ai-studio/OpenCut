// @vitest-environment jsdom
import { useSyncExternalStore } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { V4TimelineEdit, type TrackRemovalRequest, type TrackRemovalReview } from "./controller";
import { TimelineEditControls } from "./controls";
import { setupTimelineEdit } from "./timeline-fixture";
import type { V4EditReceipt } from "../types";
import { preferences } from "../../../shared/locale";

beforeAll(() => {
  preferences({ language: "en" });
  HTMLDialogElement.prototype.showModal = function () { this.setAttribute("open", ""); };
  HTMLDialogElement.prototype.close = function () { this.removeAttribute("open"); };
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
async function fixture() {
  const f = await setupTimelineEdit(), head = structuredClone(f.state().head), scene = head.project.sequences[0], main = structuredClone(scene.tracks[0]);
  main.id = "main"; main.elements = []; main.channels = []; scene.tracks.unshift(main); scene.main_track_id = main.id;
  f.current().project = structuredClone(head.project); f.set({ head });
  const prior = f.call.getMockImplementation()!, request: TrackRemovalRequest = {
    project_id: head.project.id, expected_revision: head.revision, sequence_id: "scene", track_id: "track", idempotency_key: "original-remove", label: "Remove track",
  };
  const planFor = (input: TrackRemovalRequest): TrackRemovalReview["transaction"] => ({
    project_id: input.project_id, expected_revision: input.expected_revision, idempotency_key: input.idempotency_key, label: input.label,
    operations: [{ type: "track_delete", sequence_id: input.sequence_id, track_id: input.track_id }],
  });
  f.call.mockImplementation(async (name, input, options) => name === "opencut_track_v1_remove_plan" ? planFor(input as TrackRemovalRequest) as never : prior(name, input, options));
  let receipt: V4EditReceipt | null = null, loseResponse = false;
  const applyPublic = vi.fn(async (_api: typeof f.api, tx: TrackRemovalReview["transaction"], _scene: string | null) => {
    if (!receipt) {
      receipt = { base_revision: tx.expected_revision, revision: "removed", transaction_id: "remove-tx", created_ids: [], changed_ids: [], deleted_ids: ["track", "image"] };
      f.current().revision = receipt.revision;
      f.current().project.sequences[0].tracks = f.current().project.sequences[0].tracks.filter(t => t.id !== "track");
    }
    if (loseResponse) { loseResponse = false; throw Error("Response lost after commit"); }
    return receipt;
  });
  const refresh = async () => { await f.refresh(); edit.invalidate(false); };
  const edit = new V4TimelineEdit(f.host, f.planner, refresh, true, applyPublic);
  const planCalls = () => f.call.mock.calls.filter(([name]) => name === "opencut_track_v1_remove_plan");
  return { ...f, edit, request, planFor, applyPublic, planCalls, loseResponse: () => { loseResponse = true; } };
}
function Product({ f }: { f: Awaited<ReturnType<typeof fixture>> }) {
  const state = useSyncExternalStore(f.subscribe, f.state);
  return <TimelineEditControls controller={f.edit} sequence={state.head.project.sequences[0]}
    scope={{ projectId: state.head.project.id, revision: state.head.revision, sequenceId: state.sequenceId }}
    playhead={state.playhead} selection={state.timelineEdit.selection} selectedTrackId="track"
    disabled={f.busy() || !!state.historical} onReview={() => {}} />;
}
it("reviews counts without a write, then submits the identical public plan and removes stale selection", async () => {
  const f = await fixture();
  try {
    f.edit.select({ track_id: "track", element_id: "image" });
    const review = await f.edit.prepareTrackRemoval(f.request);
    expect(review).toMatchObject({ elements: 1, captions: 0, transitions: 0 });
    expect(Object.isFrozen(review.transaction)).toBe(true); expect(f.applyPublic).not.toHaveBeenCalled();
    await f.edit.applyTrackRemoval(review);
    expect(f.applyPublic).toHaveBeenCalledTimes(1);
    expect(f.applyPublic.mock.calls[0]).toEqual([f.api, review.transaction, "scene"]);
    expect(f.applyPublic.mock.calls[0][1]).toBe(review.transaction);
    expect(f.state().timelineEdit.selection).toEqual([]); expect(f.state().selectedId).toBeNull(); expect(f.busy()).toBe(false);
  } finally { f.controller.dispose(); }
});
it("rejects forged reviews, a planner removing unrelated content, main/locked tracks and a changed pre-confirmation head", async () => {
  const f = await fixture();
  try {
    await expect(f.edit.prepareTrackRemoval({ ...f.request, track_id: "main" })).rejects.toThrow(/unlocked non-main/);
    const head = structuredClone(f.state().head); head.project.sequences[0].tracks[1].locked = true; f.set({ head });
    await expect(f.edit.prepareTrackRemoval(f.request)).rejects.toThrow(/unlocked non-main/);
    head.project.sequences[0].tracks[1].locked = false; f.set({ head });
    const prior = f.call.getMockImplementation()!;
    f.call.mockImplementationOnce(async () => ({ ...f.planFor(f.request), operations: [{ type: "asset_delete", asset_id: "asset" }] }) as never);
    await expect(f.edit.prepareTrackRemoval(f.request)).rejects.toThrow(/outside/);
    f.call.mockImplementation(prior);
    const review = await f.edit.prepareTrackRemoval(f.request);
    await expect(f.edit.applyTrackRemoval(structuredClone(review))).rejects.toThrow(/original/);
    f.set({ head: { ...head, revision: "advanced" } });
    await expect(f.edit.applyTrackRemoval(review)).rejects.toThrow(/target changed/);
    expect(f.applyPublic).not.toHaveBeenCalled();
  } finally { f.controller.dispose(); }
});
it("discards a delayed plan after cancellation or view replacement and releases its write fence", async () => {
  for (const change of ["cancel", "view", "api"] as const) {
    const f = await fixture();
    try {
      let resolve!: (value: TrackRemovalReview["transaction"]) => void;
      f.call.mockImplementationOnce(async () => await new Promise(done => { resolve = done; }) as never);
      const cancel = new AbortController(), pending = f.edit.prepareTrackRemoval(f.request, f.edit.identity(), cancel.signal);
      if (change === "cancel") cancel.abort(); else if (change === "view") f.changeView(); else f.changeApi();
      resolve(f.planFor(f.request)); await expect(pending).rejects.toThrow();
      expect(f.applyPublic).not.toHaveBeenCalled(); expect(f.busy()).toBe(false);
    } finally { f.controller.dispose(); }
  }
});
it("replays a lost response with the same plan and key after refresh without replanning or a second commit", async () => {
  const f = await fixture();
  try {
    const review = await f.edit.prepareTrackRemoval(f.request); f.loseResponse();
    await expect(f.edit.applyTrackRemoval(review)).rejects.toThrow(/lost/);
    await f.refresh(); f.edit.invalidate(false);
    await f.edit.applyTrackRemoval(review);
    expect(f.planCalls()).toHaveLength(1); expect(f.applyPublic).toHaveBeenCalledTimes(2);
    expect(f.applyPublic.mock.calls.every(call => call[1] === review.transaction)).toBe(true);
    expect(f.current().project.sequences[0].tracks.map(t => t.id)).toEqual(["main"]);
    expect(f.current().project.assets).toHaveLength(1);
  } finally { f.controller.dispose(); }
});
it("the UI requires an explicit contents review and confirms once; cancellation never submits", async () => {
  const f = await fixture();
  try {
    render(<Product f={f} />);
    fireEvent.click(screen.getByRole("button", { name: "Delete track and contents" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(f.planCalls()).toHaveLength(0); expect(f.applyPublic).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Delete track and contents" }));
    const dialog = screen.getByRole("dialog", { name: "Review track removal" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Review track contents" }));
    await within(dialog).findByRole("button", { name: "Confirm track removal" });
    expect(dialog.textContent).toContain("1 elements"); expect(f.applyPublic).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm track removal" }));
    await waitFor(() => expect(f.applyPublic).toHaveBeenCalledTimes(1));
  } finally { f.controller.dispose(); }
});
it("the UI retains the original planning key after a read failure and blocks a stale confirmation", async () => {
  const f = await fixture();
  try {
    render(<Product f={f} />); f.call.mockRejectedValueOnce(Error("Read interrupted"));
    fireEvent.click(screen.getByRole("button", { name: "Delete track and contents" }));
    fireEvent.click(screen.getByRole("button", { name: "Review track contents" }));
    await screen.findByText("Read interrupted");
    const first = f.planCalls()[0][1];
    fireEvent.click(screen.getByRole("button", { name: "Retry original track plan" }));
    await screen.findByRole("button", { name: "Confirm track removal" });
    expect(f.planCalls()[1][1]).toEqual(first);
    act(() => f.set({ head: { ...f.state().head, revision: "changed" } }));
    expect((screen.getByRole("button", { name: "Confirm track removal" }) as HTMLButtonElement).disabled).toBe(true);
    expect(f.applyPublic).not.toHaveBeenCalled();
  } finally { f.controller.dispose(); }
});
