import { afterEach, expect, it, vi } from "vitest";
import { EditorApi } from "../../api";
import { OpenCutApiError } from "../../../../lib/editor-sdk";
import type { SessionHost } from "../session";
import type { SessionState } from "../types";
import { initialSnapshot } from "../fixture";
import { EditRecovery } from "../edit-recovery";
import type { RecoveryRecord } from "../edit-recovery";
import type { CaptionContext, CaptionReceipt } from "./recovery";
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
const context: CaptionContext = {
  sequence_id: "scene",
  intent: {
    kind: "subtitle_import",
    request: {
      project_id: "v4-project",
      expected_revision: "r0",
      sequence_id: "scene",
      track_id: "captions",
      idempotency_key: "original-import",
      format: "srt",
      content: "1\n00:00:00,000 --> 00:00:01,000\nHello\n\n",
      style: {
        font: { kind: "locked", font_id: "font" },
        font_size: { unit: "canvas_pixels", value: 24 },
        color: { encoding: "srgb", red: 1, green: 1, blue: 1, alpha: 1 },
        background_color: null,
        max_width_px: null,
        line_height_px: null,
        text_align: "center",
        layout: "pinned_caption_v4",
      },
    },
  },
};
const receipt: CaptionReceipt = {
  transaction: {
    base_revision: "r0",
    revision: "saved",
    transaction_id: "tx",
    created_ids: ["cue"],
    changed_ids: ["scene"],
    deleted_ids: [],
  },
  cue_count: 1,
  warnings: [],
};
async function fixture() {
  const data = new Map<string, string>();
  const storage = {
    getItem: vi.fn((key: string) => data.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => {
      data.set(key, value);
    }),
    removeItem: vi.fn((key: string) => {
      data.delete(key);
    }),
  };
  vi.stubGlobal("sessionStorage", storage);
  const api = new EditorApi("http://localhost:8788", "secret-token");
  let record: RecoveryRecord | null = null,
    view = 1;
  const inspect = vi.fn(async () => ({

    record: record ? structuredClone(record) : null,
  }));
  const dispatch = vi.fn(async () => {
    record!.receipt = { kind: "subtitle_import", caption: receipt };
    return receipt;
  });
  const ack = vi.fn(async () => {
    if (!record)
      throw new OpenCutApiError("NOT_FOUND", "Absent", null, 404, "ack", []);
    if (!record.receipt) throw new Error("Unresolved");
    const reply = {

      recovery_id: record.recovery_id,
      resolution: "saved",
      receipt: record.receipt,
    };
    record = null;
    return reply;
  });
  vi.spyOn(api.sdk, "call").mockImplementation(async (name, input) => {
    if (name === "opencut_access_inspect")
      return {
        principal_id: "principal",
        credential_id: "credential",
        owner: false,
        grants: [],
      } as never;
    if (name === "opencut_edit_recovery_inspect")
      return (await inspect()) as never;
    if (name === "opencut_edit_recovery_stage") {
      const original = (input as { context: CaptionContext }).context;
      record ??= {

        recovery_id: "recovery",
        project_id: "v4-project",
        request_digest: "host-digest",
        context: structuredClone(original),
        receipt: null,
      };
      expect(record.context).toEqual(original);
      return structuredClone(record) as never;
    }
    if (name === "opencut_subtitle_import") {
      expect(input).toEqual(record!.context.intent.request);
      return (await dispatch()) as never;
    }
    if (name === "opencut_edit_recovery_ack") return (await ack()) as never;
    throw new Error("Unexpected " + name);
  });
  await api.authenticateConnection();
  const state = {
    head: initialSnapshot(),
    historical: null,
    sequenceId: "scene",
  } as SessionState;
  const host: SessionHost = {
    api: () => api,
    state: () => state,
    generation: () => 1,
    viewGeneration: () => view,
    changeView: () => ++view,
    publish: next => { Object.assign(state, next); },
    acquire: () => () => {},
    refreshTasks: async () => {},
  };
  const refresh = vi.fn(async () => {});
  return {
    api,
    data,
    storage,
    inspect,
    dispatch,
    ack,
    fresh: () => new EditRecovery(host, refresh),
    record: () => record,
    saved: () => {
      record!.receipt = { kind: "subtitle_import", caption: receipt };
    },
    changeView: () => ++view,
  };
}
it("restores a committed response loss as the exact caption receipt and never redispatches on read or ack", async () => {
  const f = await fixture(),
    first = f.fresh();
  await first.restore(f.api, "v4-project");
  f.dispatch.mockImplementationOnce(async () => {
    f.saved();
    throw new Error("Response lost");
  });
  await expect(first.execute(f.api, context)).rejects.toThrow("Response lost");
  expect(first.blocked()).toBe(true);
  const fresh = f.fresh();
  await fresh.restore(f.api, "v4-project");
  expect(fresh.getSnapshot().receipt).toEqual({ kind: "subtitle_import", caption: receipt });
  expect(f.dispatch).toHaveBeenCalledTimes(1);
  await fresh.release();
  expect(f.record()).toBeNull();
  expect(fresh.blocked()).toBe(false);
  expect(f.dispatch).toHaveBeenCalledTimes(1);
  const raw = [...f.data.values()].join("");
  expect(raw).not.toContain("secret-token");
  expect(raw).not.toContain("Hello");
});
it("a pointer write failure preserves a host-staged original for explicit identical retry", async () => {
  const f = await fixture(),
    first = f.fresh();
  await first.restore(f.api, "v4-project");
  f.storage.setItem.mockImplementationOnce(() => {
    throw new Error("Storage denied");
  });
  await expect(first.execute(f.api, context)).rejects.toThrow(
    "recovery storage",
  );
  expect(f.dispatch).not.toHaveBeenCalled();
  const fresh = f.fresh();
  await fresh.restore(f.api, "v4-project");
  await fresh.retry();
  expect(f.dispatch).toHaveBeenCalledTimes(1);
  expect(f.record()).toBeNull();
});
it("inspection failures stay blocked and never fall through to another protocol", async () => {
  const f = await fixture(), fresh = f.fresh();
  f.inspect.mockRejectedValueOnce(new OpenCutApiError("UNKNOWN_TOOL", "Unavailable", null, 404, "read", []));
  expect((await fresh.restore(f.api, "v4-project")).phase).toBe("failed");
  expect(fresh.blocked()).toBe(true);
  f.inspect.mockRejectedValueOnce(new Error("Network lost"));
  expect((await fresh.restore(f.api, "v4-project")).error).toContain("Network lost");
  expect(f.dispatch).not.toHaveBeenCalled();
});
it("late old-view success keeps the original durable slot for the new view instead of acknowledging it", async () => {
  const f = await fixture(),
    first = f.fresh();
  await first.restore(f.api, "v4-project");
  f.dispatch.mockImplementationOnce(async () => {
    f.saved();
    f.changeView();
    return receipt;
  });
  expect(await first.execute(f.api, context)).toEqual({ kind: "subtitle_import", caption: receipt });
  expect(f.ack).not.toHaveBeenCalled();
  expect(f.record()).not.toBeNull();
  const fresh = f.fresh();
  await fresh.restore(f.api, "v4-project");
  expect(fresh.getSnapshot().receipt).toEqual({ kind: "subtitle_import", caption: receipt });
});
