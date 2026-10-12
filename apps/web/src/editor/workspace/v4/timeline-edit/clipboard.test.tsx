// @vitest-environment jsdom
import { useSyncExternalStore } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { EditorApi } from "../../api";
import { setupTimelineEdit } from "./timeline-fixture";
import { clipboardFromText, emptyClipboardState, V4TimelineClipboard } from "./clipboard";
import type { ClipboardBuffer, ClipboardCopyRequest, ClipboardHost, ClipboardPastePlan, ClipboardPasteRequest } from "./clipboard";
import { V4ClipboardControls } from "./clipboard-controls";
import type { ToolInput } from "../../../../lib/editor-sdk";
import type { V4EditReceipt } from "../types";

beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function () { this.setAttribute("open", ""); };
  HTMLDialogElement.prototype.close = function () { this.removeAttribute("open"); };
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const target = { track_id: "track", element_id: "image" };
const deferred = <T,>() => { let resolve!: (value: T) => void, reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };

/** Typed service-response lifecycle fixture; no track, source-clock or nested
 * identity algorithm is duplicated here as evidence of domain correctness. */
async function setup() {
  const f = await setupTimelineEdit();
  f.edit.select(target);
  vi.spyOn(f.api, "connectionIdentity", "get").mockReturnValue("authenticated-caller-one");
  let api: EditorApi | null = f.api, generation = 1, view = 1, clipboard = emptyClipboardState();
  const listeners = new Set<() => void>();
  const publish: ClipboardHost["publish"] = state => { clipboard = state; listeners.forEach(listener => listener()); };
  const host: ClipboardHost = { api: () => api, state: f.state, clipboard: () => clipboard, generation: () => generation, viewGeneration: () => view,
    publish, acquire: f.host.acquire, selectMany: vi.fn() };
  const refresh = vi.fn(async () => { f.set({ head: structuredClone(f.current()) }); });
  const controller = new V4TimelineClipboard(host, refresh);
  const copyFor = (request: ClipboardCopyRequest): ClipboardBuffer => ({ schema_version: 1, source_project_id: request.project_id,
    source_revision: request.revision, source_sequence_id: request.sequence_id, selection_digest: "opaque-shared-source-digest",
    items: request.targets.map(row => ({ source_track_id: row.track_id, source_track_kind: "video", source_element_id: row.element_id })) });
  const planFor = (request: ClipboardPasteRequest): ClipboardPastePlan => {
    const source = f.current().project.sequences[0].tracks[0], placement = request.placements[0];
    if (placement.target.kind !== "classic_above_source") throw new Error("Unexpected target");
    const track = structuredClone(source); track.id = placement.target.new_track_id; track.name = placement.target.name;
    track.elements = placement.elements.map(row => ({ ...structuredClone(source.elements[0]), id: row.new_element_id, name: "Copied image" }));
    return { transaction: { project_id: request.project_id, expected_revision: request.expected_revision, idempotency_key: request.idempotency_key,
      label: request.label, operations: [{ type: "track_put", sequence_id: request.sequence_id, track }] },
    decision: { selection: track.elements.map(element => ({ track_id: track.id, element_id: element.id })), created_ids: track.elements.map(element => element.id),
      new_track_ids: [track.id], restored_asset_ids: [], restored_font_ids: [], actual_starts: track.elements.map(element => ({ element_id: element.id, start: request.at })) } };
  };
  const copy = vi.fn(async (request: ClipboardCopyRequest) => copyFor(request));
  const plan = vi.fn(async (request: ClipboardPasteRequest) => planFor(request));
  const receipts = new Map<string, V4EditReceipt>();
  const commit = (request: ToolInput<"opencut_edit_v4_apply">) => {
    let receipt = receipts.get(request.idempotency_key); if (receipt) return receipt;
    for (const operation of request.operations) if (operation.type === "track_put") f.current().project.sequences[0].tracks.push(structuredClone(operation.track));
    receipt = { base_revision: request.expected_revision, revision: "clipboard-" + request.idempotency_key,
      transaction_id: "tx-" + request.idempotency_key, changed_ids: [], created_ids: [], deleted_ids: [] };
    f.current().revision = receipt.revision; receipts.set(request.idempotency_key, receipt); return receipt;
  };
  const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
  const apply = vi.fn(async (request: ToolInput<"opencut_edit_v4_apply">) => commit(request));
  vi.mocked(f.api.sdk.call).mockImplementation(async (name, input, options) => {
    if (name === "opencut_timeline_v4_copy") return copy(input as ClipboardCopyRequest) as never;
    if (name === "opencut_timeline_v4_paste_plan") return plan(input as ClipboardPasteRequest) as never;
    if (name === "opencut_edit_v4_apply") return apply(input as ToolInput<"opencut_edit_v4_apply">) as never;
    return previous(name, input, options);
  });
  const reconnect = (identity: string) => {
    const next = new EditorApi("http://localhost:8788", "test");
    vi.spyOn(next, "connectionIdentity", "get").mockReturnValue(identity);
    vi.spyOn(next.sdk, "call").mockImplementation(vi.mocked(f.api.sdk.call));
    api = next; generation++; publish(controller.resume(next, f.state().head.project.id)); return next;
  };
  return { f, controller, host, copy, plan, apply, copyFor, planFor, commit, refresh, reconnect,
    clipboard: () => clipboard, setClipboard: publish,
    changeView: () => { view++; controller.invalidate(); },
    subscribe: (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; } };
}

it("historical copy is a fixed Read with only typed references; a scene caption alone is not an element selection", async () => {
  const s = await setup();
  try {
    const historical = structuredClone(s.f.state().head); historical.revision = "fixed-history";
    s.f.set({ historical }); await s.controller.copy();
    expect(s.copy).toHaveBeenCalledWith({ project_id: historical.project.id, revision: "fixed-history", sequence_id: "scene", targets: [target] });
    expect(clipboardFromText(s.controller.text()!)).toEqual(s.clipboard().buffer);
    expect(s.controller.text()).not.toContain("source_span"); expect(s.controller.text()).not.toContain("asset_id");
    await expect(s.controller.planPaste()).rejects.toThrow("current revision"); expect(s.apply).not.toHaveBeenCalled();
    s.f.set({ historical: null, selectedId: "caption", timelineEdit: { ...s.f.state().timelineEdit, selection: [] } });
    await expect(s.controller.copy()).rejects.toThrow("Select timeline elements");
  } finally { s.f.controller.dispose(); }
});

it("copy groups and fresh IDs are captured once; failed plan retries the identical request and cross-project paste refuses", async () => {
  const s = await setup();
  try {
    await s.controller.copy(); s.plan.mockRejectedValueOnce(new Error("offline"));
    await expect(s.controller.planPaste()).rejects.toThrow("offline");
    const request = s.clipboard().attempt!.request;
    expect(request.placements).toHaveLength(1); expect(request.placements[0].target.kind).toBe("classic_above_source");
    expect(request.placements[0].elements[0].new_element_id).not.toBe("image");
    await s.controller.retryPlan(); expect(s.plan.mock.calls[1][0]).toBe(request);
    s.controller.cancel();
    const head = structuredClone(s.f.state().head); head.project.id = "other-project"; s.f.set({ head });
    await expect(s.controller.planPaste()).rejects.toThrow("original project"); expect(s.apply).not.toHaveBeenCalled();
  } finally { s.f.controller.dispose(); }
});

it("cancelled planning and a changed view cannot resurrect a late plan or publish any write", async () => {
  const s = await setup();
  try {
    await s.controller.copy(); const pending = deferred<ClipboardPastePlan>();
    s.plan.mockImplementationOnce(() => pending.promise);
    const planning = s.controller.planPaste(); const request = s.clipboard().attempt!.request;
    s.changeView(); pending.resolve(s.planFor(request));
    await expect(planning).rejects.toThrow("target changed");
    expect(s.clipboard().attempt).toBeNull(); expect(s.apply).not.toHaveBeenCalled();
  } finally { s.f.controller.dispose(); }
});

it("a genuinely lost committed response replays only the original transaction/key after the head advances", async () => {
  const s = await setup();
  try {
    await s.controller.copy(); await s.controller.planPaste(); const transaction = s.clipboard().attempt!.plan!.transaction;
    s.apply.mockImplementationOnce(async input => { s.commit(input); throw new Error("response lost"); });
    await expect(s.controller.apply()).rejects.toThrow("response lost"); expect(s.clipboard().attempt!.phase).toBe("unconfirmed");
    const receipt = await s.controller.apply();
    expect(s.apply.mock.calls.map(call => call[0])).toEqual([transaction, transaction]);
    expect(s.apply.mock.calls[1][0]).toBe(transaction); expect(s.plan).toHaveBeenCalledTimes(1);
    expect(s.clipboard().attempt!.receipt).toBe(receipt); expect(s.clipboard().attempt!.phase).toBe("saved");
    expect(s.host.selectMany).toHaveBeenCalledWith(s.clipboard().attempt!.plan!.decision.selection, s.clipboard().attempt!.plan!.decision.selection.at(-1)!.element_id);
  } finally { s.f.controller.dispose(); }
});

it("new API same-identity resumes the exact pending paste; a late old receipt never contaminates a different credential", async () => {
  const s = await setup();
  try {
    await s.controller.copy(); await s.controller.planPaste(); const transaction = s.clipboard().attempt!.plan!.transaction;
    const pending = deferred<V4EditReceipt>(); s.apply.mockImplementationOnce(() => pending.promise);
    const saving = s.controller.apply();
    s.reconnect("authenticated-caller-one");
    expect(s.clipboard().attempt!.phase).toBe("unconfirmed"); expect(s.clipboard().attempt!.plan!.transaction).toBe(transaction);
    await expect(s.controller.apply()).rejects.toThrow("current edit"); // Actual shared acquire stays held by the old request.
    expect(s.apply).toHaveBeenCalledTimes(1);
    s.reconnect("authenticated-caller-two"); expect(s.clipboard().attempt).toBeNull(); expect(s.clipboard().buffer).toBeNull();
    pending.resolve(s.commit(transaction)); await saving;
    expect(s.clipboard().attempt).toBeNull(); expect(s.clipboard().buffer).toBeNull();
    s.reconnect("authenticated-caller-one"); expect(s.clipboard().attempt!.phase).toBe("saved");
    expect(s.clipboard().attempt!.plan!.transaction).toBe(transaction);
  } finally { s.f.controller.dispose(); }
});

function ControlsHarness({ s, files }: { s: Awaited<ReturnType<typeof setup>>; files: (files: File[], scope: { projectId: string; revision: string; sequenceId: string }) => void }) {
  const state = useSyncExternalStore(s.subscribe, s.clipboard);
  return <><div className="v4-timeline-panel" tabIndex={0} data-testid="timeline"><input aria-label="Text entry" /><button role="slider">Volume</button></div>
    <V4ClipboardControls controller={s.controller} state={state} scope={{ projectId: s.f.state().head.project.id, revision: s.f.state().head.revision, sequenceId: "scene" }}
      canCopy canPaste disabled={false} onFiles={files} /></>;
}

it("system paste delegates actual Files once; text/IME/gain focus is isolated and typed references open Review without saving", async () => {
  const s = await setup(), files = vi.fn();
  try {
    render(<ControlsHarness s={s} files={files} />);
    const timeline = screen.getByTestId("timeline"), file = new File(["bytes"], "sample.png", { type: "image/png" });
    const data = { files: [file], getData: vi.fn(() => ""), setData: vi.fn() };
    fireEvent.paste(timeline, { clipboardData: data });
    await waitFor(() => expect(files).toHaveBeenCalledWith([file], { projectId: "v4-project", revision: "r0", sequenceId: "scene" }));
    fireEvent.keyDown(screen.getByLabelText("Text entry"), { key: "d", ctrlKey: true });
    fireEvent.keyDown(screen.getByRole("slider"), { key: "d", metaKey: true });
    fireEvent.compositionStart(timeline); fireEvent.keyDown(timeline, { key: "d", ctrlKey: true }); fireEvent.compositionEnd(timeline);
    expect(s.copy).not.toHaveBeenCalled();
    await act(async () => { await s.controller.copy(); });
    const text = s.controller.text()!;
    fireEvent.paste(timeline, { clipboardData: { files: [], getData: () => text } });
    await screen.findByRole("dialog", { name: "Review paste" }); await waitFor(() => expect(s.plan).toHaveBeenCalledOnce());
    expect(s.apply).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Apply copied elements" }));
    await screen.findByText("Pasted elements saved."); expect(s.apply).toHaveBeenCalledOnce();
    expect(() => clipboardFromText(text.replace('"schema_version":1', '"uri":"/private/media","schema_version":1'))).toThrow("invalid references");
  } finally { s.f.controller.dispose(); }
});
