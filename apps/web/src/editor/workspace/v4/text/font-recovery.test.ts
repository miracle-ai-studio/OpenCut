import { afterEach, expect, it, vi } from "vitest";
import { EditorApi } from "../../api";
import { OpenCutApiError } from "../../../../lib/editor-sdk";
import type { V4SessionHost } from "../session";
import type { V4SessionState } from "../types";
import { initialSnapshot, setupV4 } from "../fixture";
import { V4FontRecovery, fontRecoveryBlocked } from "./font-recovery";
import type { FontRecoveryContext, FontRecoveryRecord } from "./font-recovery";

const context: FontRecoveryContext = {
  sequence_id: "scene",
  request: {
    project_id: "v4-project",
    expected_revision: "r0",
    idempotency_key: "original-font-key",
    catalog_id: "pinned-face",
    font_id: "locked-font",
    required_text: "OpenCut 你好",
  },
};
const receipt = {
  base_revision: "r0",
  revision: "r1",
  transaction_id: "font-tx",
  created_ids: ["locked-font"],
  changed_ids: [],
  deleted_ids: [],
};
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
async function fixture() {
  const data = new Map<string, string>();
  const storage = {
    getItem: (key: string) => data.get(key) ?? null,
    setItem: vi.fn((key: string, value: string) => {
      data.set(key, value);
    }),
  };
  vi.stubGlobal("sessionStorage", storage);
  const api = new EditorApi("http://localhost:8788", "never-store-token");
  const discovery = vi.spyOn(api.sdk, "hostCapabilities").mockResolvedValue({
    tools: ["opencut_font_recovery_v1_inspect"],
  } as never);
  let record: FontRecoveryRecord | null = null,
    state: V4SessionState,
    currentApi = api;
  const dispatch = vi.fn(async () => {
    record!.receipt = receipt;
    return receipt;
  });
  const inspect = vi.fn(async () => ({
    schema_version: 1,
    record: structuredClone(record),
  }));
  const ack = vi.fn(async () => {
    if (!record)
      throw new OpenCutApiError("NOT_FOUND", "Absent", null, 404, "ack", []);
    if (!record.receipt) throw new Error("Import outcome remains unknown");
    const result = {
      schema_version: 1,
      recovery_id: record.recovery_id,
      resolution: "saved",
      receipt: record.receipt,
    };
    record = null;
    return result;
  });
  const call = vi
    .spyOn(api.sdk, "call")
    .mockImplementation(async (name, input) => {
      if (name === "opencut_access_inspect")
        return {
          principal_id: "principal",
          credential_id: "credential",
          owner: false,
          grants: [],
        } as never;
      if (name === "opencut_font_recovery_v1_inspect")
        return (await inspect()) as never;
      if (name === "opencut_font_recovery_v1_ack")
        return (await ack()) as never;
      if (name === "opencut_font_recovery_v1_stage") {
        const requested = (input as { context: FontRecoveryContext }).context;
        record ??= {
          schema_version: 1,
          recovery_id: "font-recovery",
          project_id: "v4-project",
          request_digest: "host-digest",
          context: structuredClone(requested),
          receipt: null,
        };
        expect(record.context).toEqual(requested);
        return structuredClone(record) as never;
      }
      if (name === "opencut_font_v1_import") {
        expect(input).toEqual(record!.context.request);
        return (await dispatch()) as never;
      }
      throw new Error(`Unexpected ${name}`);
    });
  await api.authenticateConnection();
  const acquire = vi.fn(() => vi.fn());
  const host: V4SessionHost = {
    api: () => currentApi,
    state: () => state,
    generation: () => 1,
    viewGeneration: () => 1,
    changeView: () => 1,
    publish: (next) => {
      state = next;
    },
    acquire,
    refreshTasks: async () => {},
  };
  const fresh = () => {
    state = {
      head: initialSnapshot(),
      historical: null,
      sequenceId: "scene",
      timelineEdit: { attempt: null },
    } as V4SessionState;
    return new V4FontRecovery(host, async () => {});
  };
  return {
    api,
    discovery,
    dispatch,
    inspect,
    ack,
    call,
    data,
    storage,
    fresh,
    acquire,
    state: () => state,
    record: () => record,
    save: () => {
      record!.receipt = receipt;
    },
    switchApi: () => {
      currentApi = new EditorApi("http://elsewhere:8788", "other");
    },
  };
}
it("uses authoritative old-server discovery without invoking an unauthorized unknown tool", async () => {
  const f = await fixture(),
    recovery = f.fresh();
  f.discovery.mockResolvedValueOnce({
    tools: ["opencut_edit_recovery_v2_inspect"],
  } as never);
  const state = await recovery.restore(f.api, "v4-project");
  expect(state.supported).toBe(false);
  expect(fontRecoveryBlocked(state)).toBe(false);
  expect(f.inspect).not.toHaveBeenCalled();
});
it("transport or authorization failures retain the original blocking record", async () => {
  const f = await fixture(),
    recovery = f.fresh();
  await recovery.restore(f.api, "v4-project");
  f.dispatch.mockRejectedValueOnce(new Error("Response lost"));
  await expect(recovery.execute(f.api, context)).rejects.toThrow(
    "Response lost",
  );
  f.inspect.mockRejectedValueOnce(
    new OpenCutApiError(
      "NOT_AUTHORIZED",
      "Access revoked",
      null,
      403,
      "denied",
      [],
    ),
  );
  await recovery.restore(f.api, "v4-project");
  expect(f.state().fontRecovery!.record!.context).toEqual(context);
  expect(fontRecoveryBlocked(f.state().fontRecovery)).toBe(true);
  f.discovery.mockRejectedValueOnce(new Error("Offline"));
  await recovery.restore(f.api, "v4-project");
  expect(f.state().fontRecovery!.error).toBe("Offline");
  expect(f.dispatch).toHaveBeenCalledTimes(1);
});
it("unknown import survives a fresh page and retries only the exact original font request and key", async () => {
  const f = await fixture(),
    first = f.fresh();
  await first.restore(f.api, "v4-project");
  f.dispatch.mockRejectedValueOnce(new Error("Response lost"));
  await expect(first.execute(f.api, context)).rejects.toThrow("Response lost");
  const second = f.fresh();
  await second.restore(f.api, "v4-project");
  expect(f.state().fontRecovery!.phase).toBe("unconfirmed");
  expect(f.dispatch).toHaveBeenCalledTimes(1);
  await second.retry();
  expect(
    f.call.mock.calls
      .filter(([name]) => name === "opencut_font_v1_import")
      .map(([, input]) => input),
  ).toEqual([context.request, context.request]);
  expect(f.state().fontRecovery!.receipt).toEqual(receipt);
  expect(f.record()).toBeNull();
  expect([...f.data.values()].join()).not.toContain("never-store-token");
  expect([...f.data.values()].join()).not.toContain("required_text");
});
it("committed response loss is read back after reload without another import", async () => {
  const f = await fixture(),
    first = f.fresh();
  await first.restore(f.api, "v4-project");
  f.dispatch.mockImplementationOnce(async () => {
    f.save();
    throw new Error("Response lost");
  });
  await expect(first.execute(f.api, context)).rejects.toThrow("Response lost");
  const second = f.fresh();
  await second.restore(f.api, "v4-project");
  expect(f.state().fontRecovery!.receipt).toEqual(receipt);
  await second.release();
  expect(f.dispatch).toHaveBeenCalledTimes(1);
  expect(f.record()).toBeNull();
});
it("a locator write failure leaves the staged host original discoverable without dispatch", async () => {
  const f = await fixture(),
    first = f.fresh();
  await first.restore(f.api, "v4-project");
  f.storage.setItem.mockImplementationOnce(() => {
    throw new Error("Denied");
  });
  await expect(first.execute(f.api, context)).rejects.toThrow(
    "recovery storage",
  );
  expect(f.dispatch).not.toHaveBeenCalled();
  expect(f.record()!.context).toEqual(context);
  const second = f.fresh();
  await second.restore(f.api, "v4-project");
  await second.retry();
  expect(f.dispatch).toHaveBeenCalledTimes(1);
});
it("an incorrect-base receipt cannot mark a font saved or acknowledge its recovery", async () => {
  const f = await fixture(),
    recovery = f.fresh();
  await recovery.restore(f.api, "v4-project");
  f.dispatch.mockResolvedValueOnce({ ...receipt, base_revision: "other" });
  await expect(recovery.execute(f.api, context)).rejects.toThrow(
    "differs from the original",
  );
  expect(f.state().fontRecovery!.receipt).toBeNull();
  expect(f.ack).not.toHaveBeenCalled();
});
it("a late old connection receipt never acknowledges away the original host record", async () => {
  const f = await fixture(),
    recovery = f.fresh();
  await recovery.restore(f.api, "v4-project");
  f.dispatch.mockImplementationOnce(async () => {
    f.save();
    f.switchApi();
    return receipt;
  });
  expect(await recovery.execute(f.api, context)).toEqual(receipt);
  expect(f.ack).not.toHaveBeenCalled();
  expect(f.record()).not.toBeNull();
});
it("font discovery failure blocks original edit writes through the existing session fence", async () => {
  const f = await setupV4();
  try {
    vi.mocked(f.api.sdk.hostCapabilities).mockRejectedValueOnce(
      new Error("Offline"),
    );
    await f.controller.v4.recoverEdit();
    await expect(
      f.controller.v4.bookmark({
        kind: "put",
        bookmark: {
          id: "other",
          sequence_id: "scene",
          duration: null,
          color: null,
          time: { ticks: "0", timebase: 1 },
          note: "blocked",
        },
      }),
    ).rejects.toThrow("original font import");
    expect(
      f.call.mock.calls.some(
        ([name]) => name === "opencut_edit_recovery_v2_stage",
      ),
    ).toBe(false);
  } finally {
    f.controller.dispose();
  }
});
it("never retains or dispatches a mismatched staged font request", async () => {
  const f = await fixture(),
    recovery = f.fresh();
  await recovery.restore(f.api, "v4-project");
  f.inspect.mockImplementationOnce(async () => {
    const record = structuredClone(f.record()!);
    record.context.request.idempotency_key = "wrong-key";
    return { schema_version: 1, record };
  });
  await expect(recovery.execute(f.api, context)).rejects.toThrow(
    "staged font import differs",
  );
  expect(f.dispatch).not.toHaveBeenCalled();
  expect(f.state().fontRecovery!.record).toBeNull();
  expect(fontRecoveryBlocked(f.state().fontRecovery)).toBe(true);
  expect(f.record()!.context).toEqual(context);
});
it("known receipt survives lost acknowledgment and authorized absence cleanup", async () => {
  const f = await fixture(),
    recovery = f.fresh();
  await recovery.restore(f.api, "v4-project");
  const acknowledge = f.ack.getMockImplementation()!;
  f.ack.mockImplementationOnce(async () => {
    await acknowledge();
    throw new Error("Ack response lost");
  });
  expect(await recovery.execute(f.api, context)).toEqual(receipt);
  expect(f.state().fontRecovery!.record).not.toBeNull();
  await recovery.release();
  expect(f.state().fontRecovery!.receipt).toEqual(receipt);
  expect(f.state().fontRecovery!.record).toBeNull();
});
it("absence alone cannot resolve an unknown original font import", async () => {
  const f = await fixture(),
    recovery = f.fresh();
  await recovery.restore(f.api, "v4-project");
  f.dispatch.mockRejectedValueOnce(new Error("Response lost"));
  await expect(recovery.execute(f.api, context)).rejects.toThrow(
    "Response lost",
  );
  f.ack.mockRejectedValueOnce(
    new OpenCutApiError("NOT_FOUND", "Absent", null, 404, "ack", []),
  );
  await expect(recovery.release()).rejects.toThrow("Absent");
  expect(f.state().fontRecovery!.record).not.toBeNull();
  expect(f.state().fontRecovery!.receipt).toBeNull();
});
it("a missing discovery record is not proof that an unknown import resolved", async () => {
  const f = await fixture(),
    recovery = f.fresh();
  await recovery.restore(f.api, "v4-project");
  f.dispatch.mockRejectedValueOnce(new Error("Response lost"));
  await expect(recovery.execute(f.api, context)).rejects.toThrow(
    "Response lost",
  );
  f.inspect.mockResolvedValueOnce({ schema_version: 1, record: null });
  await recovery.restore(f.api, "v4-project");
  expect(f.state().fontRecovery!.record!.context).toEqual(context);
  expect(f.state().fontRecovery!.receipt).toBeNull();
  expect(fontRecoveryBlocked(f.state().fontRecovery)).toBe(true);
});

it("normalizes an omitted discovery receipt to an unconfirmed import and preserves the original retry", async () => {
  const f = await fixture(), first = f.fresh();
  const inspect = f.inspect.getMockImplementation()!;
  f.inspect.mockImplementation(async () => {
    const observed = await inspect();
    if (observed.record && !observed.record.receipt)
      delete observed.record.receipt;
    return observed;
  });
  await first.restore(f.api, "v4-project");
  f.dispatch.mockRejectedValueOnce(new Error("Response lost"));
  await expect(first.execute(f.api, context)).rejects.toThrow("Response lost");
  expect(f.state().fontRecovery).toMatchObject({ phase: "unconfirmed", receipt: null });
  const second = f.fresh();
  await second.restore(f.api, "v4-project");
  expect(f.state().fontRecovery).toMatchObject({ phase: "unconfirmed", receipt: null });
  expect(fontRecoveryBlocked(f.state().fontRecovery)).toBe(true);
  await second.retry();
  expect(f.state().fontRecovery).toMatchObject({ phase: "saved", receipt });
  expect(f.call.mock.calls.filter(([name]) => name === "opencut_font_v1_import").map(([, input]) => input))
    .toEqual([context.request, context.request]);
});
