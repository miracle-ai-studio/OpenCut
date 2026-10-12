import { afterEach, expect, it, vi } from "vitest";
import { RecoveryPointersV1 } from "./recovery";

afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
const key = "opencut.edit-recovery.v1";
const identity = (credential = "credential") => JSON.stringify(["http://localhost:8788/", "principal", credential, false]);
function fixture() {
  const data = new Map<string, string>();
  const storage = { getItem: vi.fn((key: string) => data.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => { data.set(key, value); }) };
  vi.stubGlobal("sessionStorage", storage);
  return { data, storage, pointers: new RecoveryPointersV1() };
}

it("reload preserves locators while credentials stay isolated and stale removal cannot erase a replacement", () => {
  const f = fixture(), scope = identity();
  const original = { projectId: "project", sequenceId: "scene", recoveryId: "original" };
  f.pointers.put(scope, original);
  const nextPage = new RecoveryPointersV1();
  expect(nextPage.list(scope)).toEqual([original]);
  expect(nextPage.list(identity("different-credential"))).toEqual([]);
  expect(nextPage.list(JSON.stringify(["http://localhost:8788/", "local-owner", null, true]))).toEqual([]);
  const copy = nextPage.list(scope)[0];
  copy.recoveryId = "altered";
  expect(nextPage.list(scope)).toEqual([original]);
  nextPage.remove(scope, "project", "wrong-id");
  expect(nextPage.list(scope)).toEqual([original]);
  nextPage.remove(scope, "project", "original");
  nextPage.put(scope, { ...original, recoveryId: "new" });
  f.pointers.remove(scope, "project", "original");
  expect(nextPage.list(scope)[0].recoveryId).toBe("new");
  const stored = JSON.parse(f.data.get(key)!);
  expect(Object.keys(stored.entries[0]).sort()).toEqual(["projectId", "recoveryId", "scope", "sequenceId"]);
  expect(Object.keys(stored.entries[0].scope).sort()).toEqual(["credentialId", "endpoint", "owner", "principalId"]);
});

it("entry and UTF8 byte quotas refuse additions without evicting pending locators", () => {
  const f = fixture(), scope = identity();
  for (let n = 0; n < 32; n++) f.pointers.put(scope, { projectId: `p${n}`, sequenceId: "scene", recoveryId: `r${n}` });
  const before = f.data.get(key);
  expect(() => f.pointers.put(scope, { projectId: "overflow", sequenceId: "scene", recoveryId: "r" })).toThrow("recovery storage");
  expect(f.data.get(key)).toBe(before);
  expect(f.pointers.list(scope)).toHaveLength(32);
  f.pointers.reset();
  const large = "中".repeat(1000);
  f.pointers.put(scope, { projectId: "p1", sequenceId: large, recoveryId: large });
  f.pointers.put(scope, { projectId: "p2", sequenceId: large, recoveryId: large });
  const bounded = f.data.get(key);
  expect(() => f.pointers.put(scope, { projectId: "p3", sequenceId: large, recoveryId: large })).toThrow("recovery storage");
  expect(f.data.get(key)).toBe(bounded);
  expect(new TextEncoder().encode(bounded!).byteLength).toBeLessThanOrEqual(16 * 1024);
});

it("corrupt or content-bearing records require explicit locator repair and invalid credential scopes are refused", () => {
  const f = fixture(), scope = identity();
  f.data.set(key, "{invalid");
  expect(() => f.pointers.list(scope)).toThrow("recovery storage");
  f.pointers.reset();
  expect(f.pointers.list(scope)).toEqual([]);
  f.pointers.put(scope, { projectId: "p", sequenceId: "s", recoveryId: "r" });
  const envelope = JSON.parse(f.data.get(key)!);
  envelope.entries[0].transaction = { userText: "should never be retained here" };
  f.data.set(key, JSON.stringify(envelope));
  expect(() => new RecoveryPointersV1().list(scope)).toThrow("recovery storage");
  expect(() => f.pointers.list(JSON.stringify(["http://user:secret@localhost:8788/", "principal", "credential", false]))).toThrow("recovery storage");
  expect(() => f.pointers.list(JSON.stringify(["http://localhost:8788/", "another-owner", null, true]))).toThrow("recovery storage");
  f.pointers.reset();
  expect(new RecoveryPointersV1().list(scope)).toEqual([]);
});

it("storage read denial, write denial and a silently dropped write are visible failures", () => {
  const f = fixture(), scope = identity(), pointer = { projectId: "p", sequenceId: "s", recoveryId: "r" };
  f.storage.getItem.mockImplementationOnce(() => { throw new Error("Denied"); });
  expect(() => f.pointers.list(scope)).toThrow("recovery storage");
  f.storage.setItem.mockImplementationOnce(() => { throw new Error("Quota"); });
  expect(() => f.pointers.put(scope, pointer)).toThrow("recovery storage");
  expect(f.data.has(key)).toBe(false);
  f.storage.setItem.mockImplementationOnce(() => undefined);
  expect(() => f.pointers.put(scope, pointer)).toThrow("recovery storage");
  f.pointers.put(scope, pointer);
  expect(new RecoveryPointersV1().list(scope)).toEqual([pointer]);
});
