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
  type Record = Extract<NonNullable<ToolOutput<"opencut_edit_recovery_v2_inspect">["record"]>, { kind: "current_v2" }>["record"];
  type Legacy = NonNullable<ToolOutput<"opencut_edit_recovery_v1_read">>;
  const records = new Map<string, Record>(), legacy = new Map<string, Legacy>();
  const receipts = new Map<string, ReturnType<typeof f.commit>>();
  const commit = (request: ToolInput<"opencut_edit_v4_apply">) => {
    const receipt = f.commit(request); receipts.set(request.idempotency_key, receipt); return receipt;
  };
  const apply = vi.fn(async (request: ToolInput<"opencut_edit_v4_apply">) => commit(request));
  const scope = (token: string, project: string) => JSON.stringify([token === "other-token" ? "other-credential" : "credential", project]);
  let recoveryId = 0;
  // Only recorded transport facts: SQL owns digest, quota, permissions and
  // head/receipt proof. This fixture never reproduces those algorithms.
  const stage = vi.fn(async (token: string, input: ToolInput<"opencut_edit_recovery_v2_stage">) => {
    const key = scope(token, input.context.intent.request.project_id);
    let record = records.get(key);
    if (!record) {
      record = { schema_version: 2, recovery_id: `recovery-${++recoveryId}`,
        project_id: input.context.intent.request.project_id, request_digest: "opaque-exact-stage-digest",
        context: structuredClone(input.context), receipt: null }; records.set(key, record);
    }
    expect(record.context).toEqual(input.context); return structuredClone(record);
  });
  const read = vi.fn(async (token: string, input: ToolInput<"opencut_edit_recovery_v2_inspect">): Promise<ToolOutput<"opencut_edit_recovery_v2_inspect">> => {
    const key = scope(token, input.project_id), old = legacy.get(key), record = records.get(key);
    if (old) return { schema_version: 2, record: { kind: "legacy_v1", record: structuredClone(old) } };
    if (!record || input.recovery_id && input.recovery_id !== record.recovery_id) return { schema_version: 2, record: null };
    const saved = receipts.get(record.context.intent.request.idempotency_key);
    return { schema_version: 2, record: { kind: "current_v2", record: structuredClone({ ...record,
      receipt: saved ? { kind: "public_apply", transaction: saved } : null }) } };
  });
  const ack = vi.fn(async (token: string, input: ToolInput<"opencut_edit_recovery_v2_ack">): Promise<ToolOutput<"opencut_edit_recovery_v2_ack">> => {
    const key = scope(token, input.project_id), record = records.get(key);
    if (!record || record.recovery_id !== input.recovery_id) throw new OpenCutApiError("NOT_FOUND", "Absent original record", null, 404, "ack", []);
    const receipt = receipts.get(record.context.intent.request.idempotency_key);
    if (!receipt) throw new Error("Original receipt remains unknown");
    records.delete(key); return { schema_version: 2, recovery_id: input.recovery_id, resolution: "saved", receipt: { kind: "public_apply", transaction: structuredClone(receipt) } };
  });
  const legacyAck = vi.fn(async (token: string, input: ToolInput<"opencut_edit_recovery_v1_ack">) => {
    const key = scope(token, input.project_id), record = legacy.get(key)!;
    const receipt = receipts.get(record.context.request.idempotency_key)!;
    expect(receipt).toBeDefined(); legacy.delete(key);
    return { schema_version: 1, recovery_id: input.recovery_id, resolution: "saved", receipt };
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
      if (name === "opencut_edit_timeline_plan_v4")
        return f.planFor(input as ToolInput<"opencut_edit_timeline_plan_v4">) as never;
      if (name === "opencut_edit_v4_apply")
        return await apply(input as ToolInput<"opencut_edit_v4_apply">) as never;
      if (name === "opencut_edit_recovery_v2_stage") return await stage(token, input as ToolInput<typeof name>) as never;
      if (name === "opencut_edit_recovery_v2_inspect") return await read(token, input as ToolInput<typeof name>) as never;
      if (name === "opencut_edit_recovery_v2_ack") return await ack(token, input as ToolInput<typeof name>) as never;
      if (name === "opencut_edit_recovery_v1_inspect") return { schema_version: 1, record: legacy.get(scope(token, (input as ToolInput<typeof name>).project_id)) ?? null } as never;
      if (name === "opencut_edit_recovery_v1_stage") return legacy.get(scope(token, (input as ToolInput<typeof name>).context.request.project_id)) as never;
      if (name === "opencut_edit_recovery_v1_ack") return await legacyAck(token, input as ToolInput<typeof name>) as never;
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
  const plan = () => controller.v4.timelineEdit.plan({ kind: "move_selection", ripple: false,
    moves: [{ target: { track_id: "track", element_id: "image" }, target_track_id: "track", new_start: { ticks: "4", timebase: 3 } }] });
  return { ...f, get controller() { return controller; }, connect, plan, apis, apply, commit,
    pointers, stage, read, ack, records, legacy, legacyAck, scope, reload: () => { controller.dispose(); controller = createController(); } };
}

it("whole-page V2 recovery replays the exact original timeline transaction without another planner", async () => {
  const f = await setupConnection();
  try {
    await f.connect(); await f.plan();
    const tx = f.controller.getSnapshot().v4!.timelineEdit.attempt!.plan!.transaction!;
    f.apply.mockRejectedValueOnce(new Error("Unknown transport"));
    await expect(f.controller.v4.timelineEdit.apply()).rejects.toThrow("Unknown transport");
    f.reload(); await f.connect();
    const recovery = f.controller.getSnapshot().v4!.recovery!;
    expect(recovery.record!.context.intent).toEqual({ kind: "public_apply", request: tx });
    expect(f.apply).toHaveBeenCalledTimes(1);
    expect(f.apis.at(-1)!.sdk.call).not.toHaveBeenCalledWith("opencut_edit_timeline_plan_v4", expect.anything(), expect.anything());
    await f.controller.v4.retryOriginalEdit();
    expect(f.apply.mock.calls.map(([request]) => request)).toEqual([tx, tx]);
    expect(f.records.size).toBe(0); expect(f.controller.getSnapshot().v4!.recovery!.phase).toBe("saved");
    expect(f.controller.getSnapshot().v4!.timelineEdit.attempt).toBeNull();
    expect(f.pointers.data.get("opencut.edit-recovery.v2")).not.toContain("operations");
  } finally { f.controller.dispose(); }
});
it("an orphan stage survives denied pointer storage; repair discovers it without a dispatch", async () => {
  const f = await setupConnection();
  try {
    await f.connect(); await f.plan();
    f.pointers.setItem.mockImplementationOnce(() => { throw new Error("Denied"); });
    await expect(f.controller.v4.timelineEdit.apply()).rejects.toThrow("recovery storage");
    expect(f.records.size).toBe(1); expect(f.apply).not.toHaveBeenCalled();
    f.pointers.data.set("opencut.edit-recovery.v2", "{broken");
    f.reload(); await f.connect();
    expect(f.controller.getSnapshot().v4!.recovery!.error).toMatch(/recovery storage/);
    await f.controller.v4.recoverEdit(true);
    expect(f.controller.getSnapshot().v4!.recovery!.error).toBeNull(); expect(f.apply).not.toHaveBeenCalled();
    await f.controller.v4.retryOriginalEdit(); expect(f.apply).toHaveBeenCalledTimes(1);
  } finally { f.controller.dispose(); }
});
it("same principal's other credential cannot recover the original write; late old API cannot ack its current view", async () => {
  const f = await setupConnection();
  try {
    await f.connect(); await f.plan();
    const tx = f.controller.getSnapshot().v4!.timelineEdit.attempt!.plan!.transaction!;
    let resolve!: (value: ReturnType<typeof f.commit>) => void;
    f.apply.mockImplementationOnce(() => new Promise(r => { resolve = r; }));
    const pending = f.controller.v4.timelineEdit.apply();
    await vi.waitFor(() => expect(f.apply).toHaveBeenCalledTimes(1));
    await f.connect("other-token");
    expect(f.controller.getSnapshot().v4!.recovery!.record).toBeNull();
    const other = f.controller.getSnapshot().v4;
    resolve(f.commit(tx)); await pending;
    expect(f.controller.getSnapshot().v4).toBe(other); expect(f.ack).not.toHaveBeenCalled();
    await f.connect(); expect(f.controller.getSnapshot().v4!.recovery!.receipt!.kind).toBe("public_apply");
    await f.controller.v4.recovery.release(); expect(f.records.size).toBe(0);
  } finally { f.controller.dispose(); }
});
it("a stale same-identity inspect after another API has acknowledged cannot resurrect record or locator", async () => {
  const f = await setupConnection();
  try {
    await f.connect(); await f.plan(); f.apply.mockRejectedValueOnce(new Error("Unknown"));
    await expect(f.controller.v4.timelineEdit.apply()).rejects.toThrow("Unknown");
    const read = f.read.getMockImplementation()!; let release!: () => void;
    f.read.mockImplementationOnce(async (token, input) => {
      const snapshot = await read(token, input); return new Promise(resolve => { release = () => resolve(snapshot); });
    });
    const openingA = f.connect(); await vi.waitFor(() => expect(release).toBeTypeOf("function"));
    await f.connect(); await f.controller.v4.retryOriginalEdit();
    const saved = f.controller.getSnapshot().v4!.recovery!;
    expect(saved.phase).toBe("saved"); expect(saved.record).toBeNull();
    const pointers = f.pointers.data.get("opencut.edit-recovery.v2");
    release(); await openingA;
    expect(f.controller.getSnapshot().v4!.recovery).toBe(saved);
    expect(f.pointers.data.get("opencut.edit-recovery.v2")).toBe(pointers);
    await f.connect(); expect(f.controller.getSnapshot().v4!.recovery!.receipt).toEqual(saved.receipt);
  } finally { f.controller.dispose(); }
});
it("known Saved survives ack loss and pointer cleanup failure without another apply", async () => {
  const f = await setupConnection();
  try {
    await f.connect(); await f.plan(); f.ack.mockRejectedValueOnce(new Error("Ack lost"));
    await f.controller.v4.timelineEdit.apply();
    expect(f.controller.getSnapshot().v4!.recovery!.phase).toBe("saved");
    f.pointers.setItem.mockImplementationOnce(() => { throw new Error("Cleanup denied"); });
    await expect(f.controller.v4.recovery.release()).rejects.toThrow("recovery storage");
    expect(f.records.size).toBe(0); expect(f.controller.getSnapshot().v4!.recovery!.receipt).not.toBeNull();
    await f.controller.v4.recovery.release();
    expect(f.controller.getSnapshot().v4!.recovery!.record).toBeNull(); expect(f.apply).toHaveBeenCalledTimes(1);
  } finally { f.controller.dispose(); }
});
it("discovery prioritizes a real legacy V1 context and only its original V1 ack releases it", async () => {
  const f = await setupConnection();
  try {
    await f.connect(); await f.plan();
    const attempt = f.controller.getSnapshot().v4!.timelineEdit.attempt!;
    f.legacy.set(f.scope("original-token", f.ref().id), { schema_version: 1, recovery_id: "legacy-original", project_id: f.ref().id,
      sequence_id: "scene", request_digest: "old-digest", context: { kind: "timeline_v4", request: attempt.request, plan: attempt.plan! }, receipt: null });
    f.reload(); await f.connect();
    expect(f.controller.getSnapshot().v4!.timelineEdit.attempt!.plan).toEqual(attempt.plan);
    expect(f.controller.getSnapshot().v4!.recovery!.phase).toBe("idle");
    await expect(f.controller.v4.write("Another edit", [])).rejects.toThrow("original timeline operation");
    expect(f.stage).not.toHaveBeenCalled();
    await f.controller.v4.timelineEdit.apply();
    expect(f.apply.mock.calls[0][0]).toEqual(attempt.plan!.transaction);
    expect(f.legacyAck).toHaveBeenCalledTimes(1); expect(f.ack).not.toHaveBeenCalled(); expect(f.legacy.size).toBe(0);
  } finally { f.controller.dispose(); }
});
