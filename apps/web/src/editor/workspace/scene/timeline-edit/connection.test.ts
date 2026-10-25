import { afterEach, expect, it, vi } from "vitest";
import { EditorApi } from "../../api";
import { WorkspaceController } from "../../controller";
import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import { OpenCutApiError } from "../../../../lib/editor-sdk";
import { setupTimelineEdit } from "./timeline-fixture";

afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });

function storage() {
  const data = new Map<string, string>();
  const port = { getItem: vi.fn((key: string) => data.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => { data.set(key, value); }) };
  vi.stubGlobal("sessionStorage", port);
  return { ...port, data };
}

async function setupConnection() {
  const pointers = storage();
  const f = await setupTimelineEdit();
  f.controller.dispose();
  const original = f.call.getMockImplementation()!;
  type Record = NonNullable<ToolOutput<"opencut_edit_recovery_inspect">["record"]>;
  const records = new Map<string, Record>();
  const receipts = new Map<string, ReturnType<typeof f.commit>>();
  const commit = (request: ToolInput<"opencut_edit_apply">) => {
    const receipt = f.commit(request); receipts.set(request.idempotency_key, receipt); return receipt;
  };
  const apply = vi.fn(async (request: ToolInput<"opencut_edit_apply">) => commit(request));
  const scope = (token: string, project: string) => JSON.stringify([token === "other-token" ? "other-credential" : "credential", project]);
  let recoveryId = 0;
  // Only recorded transport facts: SQL owns digest, quota, permissions and
  // head/receipt proof. This fixture never reproduces those algorithms.
  const stage = vi.fn(async (token: string, input: ToolInput<"opencut_edit_recovery_stage">) => {
    const key = scope(token, input.context.intent.request.project_id);
    let record = records.get(key);
    if (!record) {
      record = { recovery_id: `recovery-${++recoveryId}`,
        project_id: input.context.intent.request.project_id, request_digest: "opaque-exact-stage-digest",
        context: structuredClone(input.context), receipt: null }; records.set(key, record);
    }
    expect(record.context).toEqual(input.context); return structuredClone(record);
  });
  const read = vi.fn(async (token: string, input: ToolInput<"opencut_edit_recovery_inspect">): Promise<ToolOutput<"opencut_edit_recovery_inspect">> => {
    const key = scope(token, input.project_id), record = records.get(key);
    if (!record || input.recovery_id && input.recovery_id !== record.recovery_id) return { record: null };
    const saved = receipts.get(record.context.intent.request.idempotency_key);
    return { record: structuredClone({ ...record,
      receipt: saved ? { kind: "public_apply", transaction: saved } : null }) };
  });
  const ack = vi.fn(async (token: string, input: ToolInput<"opencut_edit_recovery_ack">): Promise<ToolOutput<"opencut_edit_recovery_ack">> => {
    const key = scope(token, input.project_id), record = records.get(key);
    if (!record || record.recovery_id !== input.recovery_id) throw new OpenCutApiError("NOT_FOUND", "Absent original record", null, 404, "ack", []);
    const receipt = receipts.get(record.context.intent.request.idempotency_key);
    if (!receipt) throw new Error("Original receipt remains unknown");
    records.delete(key); return { recovery_id: input.recovery_id, resolution: "saved", receipt: { kind: "public_apply", transaction: structuredClone(receipt) } };
  });
  const apis: EditorApi[] = [];
  const createController = () => new WorkspaceController((_endpoint, token) => {
    const api = new EditorApi("http://localhost:8788/", token);
    vi.spyOn(api.sdk, "hostCapabilities").mockResolvedValue({ tools: [] } as never);
    vi.spyOn(api, "projects").mockImplementation(async () => [f.ref()]);
    vi.spyOn(api, "tasks").mockResolvedValue({ jobs: [], next_cursor: null });
    vi.spyOn(api.sdk, "call").mockImplementation(async (name, input, options) => {
      if (name === "opencut_access_inspect")
        return { principal_id: "same-principal", credential_id: token === "other-token" ? "other-credential" : "credential", owner: false, grants: [] } as never;
      if (name === "opencut_edit_timeline_plan")
        return f.planFor(input as ToolInput<"opencut_edit_timeline_plan">) as never;
      if (name === "opencut_edit_apply")
        return await apply(input as ToolInput<"opencut_edit_apply">) as never;
      if (name === "opencut_edit_recovery_stage") return await stage(token, input as ToolInput<typeof name>) as never;
      if (name === "opencut_edit_recovery_inspect") return await read(token, input as ToolInput<typeof name>) as never;
      if (name === "opencut_edit_recovery_ack") return await ack(token, input as ToolInput<typeof name>) as never;
      return original(name, input, options);
    });
    apis.push(api);
    return api;
  });
  let controller = createController();
  const connect = async (token = "original-token") => {
    await controller.connect("http://localhost:8788", token);
    expect(controller.getSnapshot().connection).toBe("connected");
    await controller.open(f.ref());
  };
  const plan = () => controller.scene.timelineEdit.plan({ kind: "move_selection", ripple: false,
    moves: [{ target: { track_id: "track", element_id: "image" }, target_track_id: "track", new_start: { ticks: "4", timebase: 3 } }] });
  return { ...f, get controller() { return controller; }, connect, plan, apis, apply, commit,
    pointers, stage, read, ack, records, scope, reload: () => { controller.dispose(); controller = createController(); } };
}

it("whole-page recovery replays the exact original timeline transaction without another planner", async () => {
  const f = await setupConnection();
  try {
    await f.connect(); await f.plan();
    const tx = f.controller.getSnapshot().scene!.timelineEdit.attempt!.plan!.transaction!;
    f.apply.mockRejectedValueOnce(new Error("Unknown transport"));
    await expect(f.controller.scene.timelineEdit.apply()).rejects.toThrow("Unknown transport");
    f.reload(); await f.connect();
    const recovery = f.controller.getSnapshot().scene!.recovery!;
    expect(recovery.record!.context.intent).toEqual({ kind: "public_apply", request: tx });
    expect(f.apply).toHaveBeenCalledTimes(1);
    expect(f.apis.at(-1)!.sdk.call).not.toHaveBeenCalledWith("opencut_edit_timeline_plan", expect.anything(), expect.anything());
    await f.controller.scene.retryOriginalEdit();
    expect(f.apply.mock.calls.map(([request]) => request)).toEqual([tx, tx]);
    expect(f.records.size).toBe(0); expect(f.controller.getSnapshot().scene!.recovery!.phase).toBe("saved");
    expect(f.controller.getSnapshot().scene!.timelineEdit.attempt).toBeNull();
    expect(f.pointers.data.get("opencut.edit-recovery")).not.toContain("operations");
  } finally { f.controller.dispose(); }
});
it("an orphan stage survives denied pointer storage; repair discovers it without a dispatch", async () => {
  const f = await setupConnection();
  try {
    await f.connect(); await f.plan();
    f.pointers.setItem.mockImplementationOnce(() => { throw new Error("Denied"); });
    await expect(f.controller.scene.timelineEdit.apply()).rejects.toThrow("recovery storage");
    expect(f.records.size).toBe(1); expect(f.apply).not.toHaveBeenCalled();
    f.pointers.data.set("opencut.edit-recovery", "{broken");
    f.reload(); await f.connect();
    expect(f.controller.getSnapshot().scene!.recovery!.error).toMatch(/recovery storage/);
    await f.controller.scene.recoverEdit(true);
    expect(f.controller.getSnapshot().scene!.recovery!.error).toBeNull(); expect(f.apply).not.toHaveBeenCalled();
    await f.controller.scene.retryOriginalEdit(); expect(f.apply).toHaveBeenCalledTimes(1);
  } finally { f.controller.dispose(); }
});
it("same principal's other credential cannot recover the original write; late old API cannot ack its current view", async () => {
  const f = await setupConnection();
  try {
    await f.connect(); await f.plan();
    const tx = f.controller.getSnapshot().scene!.timelineEdit.attempt!.plan!.transaction!;
    let resolve!: (value: ReturnType<typeof f.commit>) => void;
    f.apply.mockImplementationOnce(() => new Promise(r => { resolve = r; }));
    const pending = f.controller.scene.timelineEdit.apply();
    await vi.waitFor(() => expect(f.apply).toHaveBeenCalledTimes(1));
    await f.connect("other-token");
    await vi.waitFor(() => expect(f.controller.getSnapshot().scene!.frameCapability).not.toBeNull());
    expect(f.controller.getSnapshot().scene!.recovery!.record).toBeNull();
    const other = f.controller.getSnapshot().scene;
    resolve(f.commit(tx)); await pending;
    expect(f.controller.getSnapshot().scene).toBe(other); expect(f.ack).not.toHaveBeenCalled();
    await f.connect(); expect(f.controller.getSnapshot().scene!.recovery!.receipt!.kind).toBe("public_apply");
    await f.controller.scene.recovery.release(); expect(f.records.size).toBe(0);
  } finally { f.controller.dispose(); }
});
it("a stale same-identity inspect after another API has acknowledged cannot resurrect record or locator", async () => {
  const f = await setupConnection();
  try {
    await f.connect(); await f.plan(); f.apply.mockRejectedValueOnce(new Error("Unknown"));
    await expect(f.controller.scene.timelineEdit.apply()).rejects.toThrow("Unknown");
    const read = f.read.getMockImplementation()!; let release!: () => void;
    f.read.mockImplementationOnce(async (token, input) => {
      const snapshot = await read(token, input); return new Promise(resolve => { release = () => resolve(snapshot); });
    });
    const openingA = f.connect(); await vi.waitFor(() => expect(release).toBeTypeOf("function"));
    await f.connect(); await f.controller.scene.retryOriginalEdit();
    const saved = f.controller.getSnapshot().scene!.recovery!;
    expect(saved.phase).toBe("saved"); expect(saved.record).toBeNull();
    const pointers = f.pointers.data.get("opencut.edit-recovery");
    release(); await openingA;
    expect(f.controller.getSnapshot().scene!.recovery).toBe(saved);
    expect(f.pointers.data.get("opencut.edit-recovery")).toBe(pointers);
    await f.connect(); expect(f.controller.getSnapshot().scene!.recovery!.receipt).toEqual(saved.receipt);
  } finally { f.controller.dispose(); }
});
it("known Saved survives ack loss and pointer cleanup failure without another apply", async () => {
  const f = await setupConnection();
  try {
    await f.connect(); await f.plan(); f.ack.mockRejectedValueOnce(new Error("Ack lost"));
    await f.controller.scene.timelineEdit.apply();
    expect(f.controller.getSnapshot().scene!.recovery!.phase).toBe("saved");
    f.pointers.setItem.mockImplementationOnce(() => { throw new Error("Cleanup denied"); });
    await expect(f.controller.scene.recovery.release()).rejects.toThrow("recovery storage");
    expect(f.records.size).toBe(0); expect(f.controller.getSnapshot().scene!.recovery!.receipt).not.toBeNull();
    await f.controller.scene.recovery.release();
    expect(f.controller.getSnapshot().scene!.recovery!.record).toBeNull(); expect(f.apply).toHaveBeenCalledTimes(1);
  } finally { f.controller.dispose(); }
});
it("a normalized historical record uses the same explicit retry and acknowledgment", async () => {
  const f = await setupConnection();
  try {
    await f.connect(); await f.plan();
    const attempt = f.controller.getSnapshot().scene!.timelineEdit.attempt!;
    const request = attempt.plan!.transaction!;
    f.records.set(f.scope("original-token", f.ref().id), { recovery_id: "historical-original", project_id: f.ref().id,
      request_digest: "canonical-context-digest", context: { sequence_id: "scene", intent: { kind: "public_apply", request } }, receipt: null });
    f.reload(); await f.connect();
    expect(f.controller.getSnapshot().scene!.recovery!.record!.context.intent.request).toEqual(request);
    await expect(f.controller.scene.write("Another edit", [])).rejects.toThrow("original edit");
    expect(f.apply).not.toHaveBeenCalled();
    await f.controller.scene.retryOriginalEdit();
    expect(f.apply.mock.calls[0][0]).toEqual(request);
    expect(f.ack).toHaveBeenCalledTimes(1);expect(f.records.size).toBe(0);
  } finally { f.controller.dispose(); }
});
