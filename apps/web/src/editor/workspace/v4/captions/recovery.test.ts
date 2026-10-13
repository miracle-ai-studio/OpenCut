import { afterEach, expect, it, vi } from "vitest";
import { EditorApi } from "../../api";
import { OpenCutApiError } from "../../../../lib/editor-sdk";
import type { V4SessionHost } from "../session";
import type { V4SessionState } from "../types";
import { initialSnapshot } from "../fixture";
import { V4CaptionRecovery } from "./recovery";
import type { CaptionContext, CaptionRecord, CaptionReceipt } from "./recovery";
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
const context: CaptionContext = {
  sequence_id: "scene",
  intent: {
    kind: "subtitle_import_v4",
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
  let record: CaptionRecord | null = null,
    view = 1;
  const inspect = vi.fn(async () => ({
    schema_version: 3,
    record: record ? structuredClone(record) : null,
  }));
  const dispatch = vi.fn(async () => {
    record!.receipt = receipt;
    return receipt;
  });
  const ack = vi.fn(async () => {
    if (!record)
      throw new OpenCutApiError("NOT_FOUND", "Absent", null, 404, "ack", []);
    if (!record.receipt) throw new Error("Unresolved");
    const reply = {
      schema_version: 3,
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
    if (name === "opencut_edit_recovery_v3_inspect")
      return (await inspect()) as never;
    if (name === "opencut_edit_recovery_v3_stage") {
      const original = (input as { context: CaptionContext }).context;
      record ??= {
        schema_version: 3,
        recovery_id: "recovery",
        project_id: "v4-project",
        request_digest: "host-digest",
        context: structuredClone(original),
        receipt: null,
      };
      expect(record.context).toEqual(original);
      return structuredClone(record) as never;
    }
    if (name === "opencut_subtitle_import_v4") {
      expect(input).toEqual(record!.context.intent.request);
      return (await dispatch()) as never;
    }
    if (name === "opencut_edit_recovery_v3_ack") return (await ack()) as never;
    throw new Error("Unexpected " + name);
  });
  await api.authenticateConnection();
  const state = {
    head: initialSnapshot(),
    historical: null,
    sequenceId: "scene",
  } as V4SessionState;
  const host: V4SessionHost = {
    api: () => api,
    state: () => state,
    generation: () => 1,
    viewGeneration: () => view,
    changeView: () => ++view,
    publish: () => {},
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
    fresh: () => new V4CaptionRecovery(host, refresh),
    record: () => record,
    saved: () => {
      record!.receipt = receipt;
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
  expect(fresh.getSnapshot().receipt).toEqual(receipt);
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
it("old service unknown-tool compatibility requires no known V3 record or pointer; network errors stay blocked without throwing", async () => {
  const f = await fixture(),
    fresh = f.fresh();
  f.inspect.mockRejectedValueOnce(
    new OpenCutApiError("UNKNOWN_TOOL", "Unavailable", null, 404, "read", []),
  );
  expect(await fresh.restore(f.api, "v4-project")).toBe(false);
  expect(fresh.blocked()).toBe(false);
  f.inspect.mockRejectedValueOnce(new Error("Network lost"));
  expect(await fresh.restore(f.api, "v4-project")).toBe(false);
  expect(fresh.blocked()).toBe(true);
  expect(fresh.getSnapshot().error).toContain("Network lost");
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
  expect(await first.execute(f.api, context)).toEqual(receipt);
  expect(f.ack).not.toHaveBeenCalled();
  expect(f.record()).not.toBeNull();
  const fresh = f.fresh();
  await fresh.restore(f.api, "v4-project");
  expect(fresh.getSnapshot().receipt).toEqual(receipt);
});
