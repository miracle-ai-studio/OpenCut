import { expect, it, vi } from "vitest";
import { OpenCutApiError } from "../../../../lib/editor-sdk";
import type { EditorApi } from "../../api";
import { PreviewRecoveryV1 } from "./recovery";
import type { V4AudioPreviewCreate, V4AudioPreviewSession } from "../types";

const names = ["stage", "inspect", "dispatch", "ack"].map(
  (name) => `opencut_audio_preview_recovery_v1_${name}`,
);
const create: V4AudioPreviewCreate = {
  project_id: "project",
  revision: "fixed",
  sequence_id: "scene",
  sample_rate: 48000,
  start_time: { ticks: "0", timebase: 120000 },
  idempotency_key: "original-create",
};
const session = {
  schema_version: 1,
  session_id: "session",
  identity: {
    project_id: "project",
    revision: "fixed",
    sequence_id: "scene",
    sample_rate: 48000,
    channels: 2,
    profile: "classic-realtime-v31",
  },
  generation: "0",
  overlay_epoch: "0",
  owner_id: "owner-secret",
  lease_id: "lease-secret",
  state: "ready",
} as V4AudioPreviewSession;
const record = {
  schema_version: 1,
  recovery_id: "opaque-slot",
  project_id: "project",
  revision: "fixed",
  sequence_id: "scene",
  request_digest: "sha256:original",
  kind: "create",
  status: "staged",
};
function fixture() {
  let identity = "service/principal/credential";
  const call = vi.fn(
    async (tool: string, _input: unknown): Promise<unknown> => {
      if (tool.endsWith("_inspect")) return { schema_version: 1, record: null };
      if (tool.endsWith("_stage")) return structuredClone(record);
      if (tool.endsWith("_dispatch")) return structuredClone(session);
      if (tool.endsWith("_ack"))
        return { schema_version: 1, recovery_id: record.recovery_id };
      throw new Error(`Unexpected tool ${tool}`);
    },
  );
  const hostCapabilities = vi.fn(async () => ({ tools: names }));
  const legacyCreate = vi.fn(),
    legacyControl = vi.fn();
  const api = {
    get connectionIdentity() {
      return identity;
    },
    sdk: { call, hostCapabilities },
    v4: {
      audioPreviewCreate: legacyCreate,
      audioPreviewControl: legacyControl,
    },
  } as unknown as EditorApi;
  return {
    api,
    call,
    hostCapabilities,
    legacyCreate,
    legacyControl,
    identity: (next: string) => {
      identity = next;
    },
  };
}

it("retries an unknown create only by the original staged ID and pins its protocol", async () => {
  const f = fixture(),
    recovery = new PreviewRecoveryV1();
  let unknown = true;
  const normal = f.call.getMockImplementation()!;
  f.call.mockImplementation(async (tool, input) => {
    if (tool.endsWith("_dispatch") && unknown) {
      unknown = false;
      throw new TypeError("response lost");
    }
    return normal(tool, input);
  });
  await expect(
    recovery.create(f.api, create, new AbortController().signal, () => true),
  ).rejects.toThrow("response lost");
  f.hostCapabilities.mockResolvedValue({ tools: [] });
  await expect(
    recovery.create(
      f.api,
      structuredClone(create),
      new AbortController().signal,
      () => true,
    ),
  ).resolves.toEqual(session);
  const dispatched = f.call.mock.calls.filter(([tool]) =>
    tool.endsWith("_dispatch"),
  );
  expect(dispatched).toHaveLength(2);
  expect(dispatched[0][1]).toEqual(dispatched[1][1]);
  expect(
    f.call.mock.calls.filter(([tool]) => tool.endsWith("_stage")),
  ).toHaveLength(1);
  expect(f.hostCapabilities).toHaveBeenCalledTimes(1);
  expect(f.legacyCreate).not.toHaveBeenCalled();
});

it("a new document discovers and retires an unknown control without replaying it", async () => {
  const f = fixture(),
    recovery = new PreviewRecoveryV1(),
    normal = f.call.getMockImplementation()!;
  f.call.mockImplementation(async (tool, input) =>
    tool.endsWith("_inspect")
      ? { schema_version: 1, record: { ...record, kind: "control" } }
      : normal(tool, input),
  );
  await recovery.restore(f.api, create.project_id);
  expect(f.call.mock.calls).toEqual([
    [names[1], { project_id: "project" }],
    [
      names[3],
      {
        project_id: "project",
        recovery_id: "opaque-slot",
        disposition: "retire",
      },
    ],
  ]);
  expect(JSON.stringify(f.call.mock.calls)).not.toContain("lease-secret");
  expect(f.legacyControl).not.toHaveBeenCalled();
});

it("a lost Stage response retries the exact original request before retirement", async () => {
  const f = fixture(),
    recovery = new PreviewRecoveryV1(),
    normal = f.call.getMockImplementation()!;
  let lost = true;
  f.call.mockImplementation(async (tool, input) => {
    if (tool.endsWith("_stage") && lost) {
      lost = false;
      throw new TypeError("stage reply lost");
    }
    return normal(tool, input);
  });
  await expect(
    recovery.create(f.api, create, new AbortController().signal, () => true),
  ).rejects.toThrow("stage reply lost");
  await recovery.retire(f.api, create);
  const staged = f.call.mock.calls.filter(([tool]) => tool.endsWith("_stage"));
  expect(staged).toHaveLength(2);
  expect(staged[0][1]).toEqual(staged[1][1]);
  expect(f.call.mock.calls.some(([tool]) => tool.endsWith("_dispatch"))).toBe(
    false,
  );
  await expect(
    recovery.create(f.api, create, new AbortController().signal, () => true),
  ).rejects.toMatchObject({ name: "AbortError" });
});

it("a late successful dispatch cannot restore a retired document's generation", async () => {
  const f = fixture(),
    recovery = new PreviewRecoveryV1(),
    normal = f.call.getMockImplementation()!;
  let resolve!: (value: unknown) => void;
  let entered!: () => void;
  const dispatched = new Promise<void>((r) => {
    entered = r;
  });
  f.call.mockImplementation((tool, input) =>
    tool.endsWith("_dispatch")
      ? new Promise((r) => {
          resolve = r;
          entered();
        })
      : normal(tool, input),
  );
  const playing = recovery.create(
    f.api,
    create,
    new AbortController().signal,
    () => true,
  );
  await dispatched;
  await recovery.retire(f.api, create);
  resolve(session);
  await expect(playing).rejects.toMatchObject({ name: "AbortError" });
  expect(
    f.call.mock.calls.filter(
      ([tool, input]) =>
        tool.endsWith("_ack") &&
        (input as { disposition: string }).disposition === "retire",
    ).length,
  ).toBeGreaterThan(0);
});

it("connection identity changes cannot dispatch a slot staged under another credential", async () => {
  const f = fixture(),
    recovery = new PreviewRecoveryV1(),
    normal = f.call.getMockImplementation()!;
  f.call.mockImplementation(async (tool, input) => {
    const result = await normal(tool, input);
    if (tool.endsWith("_stage")) f.identity("other/credential");
    return result;
  });
  await expect(
    recovery.create(f.api, create, new AbortController().signal, () => true),
  ).rejects.toThrow("connection changed");
  expect(f.call.mock.calls.some(([tool]) => tool.endsWith("_dispatch"))).toBe(
    false,
  );
});

it("a slow reload retirement completes before any new Stage or Dispatch", async () => {
  const f = fixture(),
    recovery = new PreviewRecoveryV1(),
    normal = f.call.getMockImplementation()!;
  let resolve!: (value: unknown) => void, entered!: () => void;
  const inspecting = new Promise<void>((r) => {
    entered = r;
  });
  f.call.mockImplementation((tool, input) =>
    tool.endsWith("_inspect")
      ? new Promise((r) => {
          resolve = r;
          entered();
        })
      : normal(tool, input),
  );
  const restoring = recovery.restore(f.api, "project");
  await inspecting;
  const creating = recovery.create(
    f.api,
    create,
    new AbortController().signal,
    () => true,
  );
  expect(f.call.mock.calls.some(([tool]) => tool.endsWith("_stage"))).toBe(
    false,
  );
  resolve({ schema_version: 1, record });
  await restoring;
  await creating;
  expect(f.call.mock.calls.map(([tool]) => tool)).toEqual([
    names[1],
    names[3],
    names[0],
    names[2],
    names[3],
  ]);
});

it("an unresolved original project does not replace or block another project's slot", async () => {
  const f = fixture(),
    recovery = new PreviewRecoveryV1(),
    normal = f.call.getMockImplementation()!;
  let lost = true;
  f.call.mockImplementation(async (tool, input) => {
    if (
      tool.endsWith("_stage") &&
      (input as { request: { request: V4AudioPreviewCreate } }).request.request
        .project_id === "other"
    )
      return { ...record, project_id: "other", recovery_id: "other-slot" };
    if (tool.endsWith("_dispatch")) {
      if ((input as { project_id: string }).project_id === "other")
        return {
          ...session,
          identity: { ...session.identity, project_id: "other" },
        };
      if (lost) {
        lost = false;
        throw new TypeError("original reply lost");
      }
    }
    return normal(tool, input);
  });
  await expect(
    recovery.create(f.api, create, new AbortController().signal, () => true),
  ).rejects.toThrow("original reply lost");
  await expect(
    recovery.create(
      f.api,
      { ...create, project_id: "other", idempotency_key: "other-create" },
      new AbortController().signal,
      () => true,
    ),
  ).resolves.toMatchObject({ identity: { project_id: "other" } });
  await expect(
    recovery.create(f.api, create, new AbortController().signal, () => true),
  ).resolves.toEqual(session);
  expect(
    f.call.mock.calls.filter(([tool]) => tool.endsWith("_stage")),
  ).toHaveLength(2);
});

it("a definitive Stage admission rejection does not create an unresolvable local slot or use legacy controls", async () => {
  const f = fixture(),
    recovery = new PreviewRecoveryV1(),
    normal = f.call.getMockImplementation()!;
  let rejected = true;
  f.call.mockImplementation(async (tool, input) => {
    if (tool.endsWith("_stage") && rejected) {
      rejected = false;
      throw new OpenCutApiError(
        "RECOVERY_QUOTA_EXCEEDED",
        "full",
        null,
        409,
        "request",
        [],
      );
    }
    return normal(tool, input);
  });
  await expect(
    recovery.create(f.api, create, new AbortController().signal, () => true),
  ).rejects.toThrow("full");
  await expect(
    recovery.create(
      f.api,
      { ...create, idempotency_key: "fresh-user-admission" },
      new AbortController().signal,
      () => true,
    ),
  ).resolves.toEqual(session);
  expect(f.legacyCreate).not.toHaveBeenCalled();
  expect(f.legacyControl).not.toHaveBeenCalled();
});
