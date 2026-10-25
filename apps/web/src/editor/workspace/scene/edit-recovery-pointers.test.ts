import { afterEach, expect, it, vi } from "vitest";
import { RecoveryPointers } from "./edit-recovery-pointers";

afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
const key = "opencut.edit-recovery";
const identity = (credential = "credential") => JSON.stringify(["http://localhost:8788/", "principal", credential, false]);
function fixture() {
  const data = new Map<string, string>();
  const storage = { getItem: vi.fn((key: string) => data.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => { data.set(key, value); }) };
  vi.stubGlobal("sessionStorage", storage);
  return { data, storage, pointers: new RecoveryPointers() };
}

it("reload preserves locators while credentials stay isolated and stale removal cannot erase a replacement", () => {
  const f = fixture(), scope = identity();
  const original = { projectId: "project", recoveryId: "original" };
  const historical = JSON.stringify({schemaVersion:2,tabId:"original-tab",entries:[{identity:scope,...original}]});
  f.data.set("opencut.edit-recovery.v2",historical);
  expect(f.pointers.list(scope,"project")).toEqual([{identity:scope,...original}]);expect(f.storage.setItem).not.toHaveBeenCalled();
  f.pointers.put(scope, original.projectId, original.recoveryId);
  expect(f.data.get("opencut.edit-recovery.v2")).toBe(historical);
  expect(Object.keys(JSON.parse(f.data.get(key)!)).sort()).toEqual(["entries","tabId"]);
  const nextPage = new RecoveryPointers();
  expect(nextPage.list(scope, "project")).toEqual([expect.objectContaining(original)]);
  expect(nextPage.list(identity("different-credential"), "project")).toEqual([]);
  expect(nextPage.list(JSON.stringify(["http://localhost:8788/", "local-owner", null, true]), "project")).toEqual([]);
  const copy = nextPage.list(scope, "project")[0];
  copy.recoveryId = "altered";
  expect(nextPage.list(scope, "project")).toEqual([expect.objectContaining(original)]);
  nextPage.remove(scope, "project", "wrong-id");
  expect(nextPage.list(scope, "project")).toEqual([expect.objectContaining(original)]);
  nextPage.remove(scope, "project", "original");
  nextPage.put(scope, original.projectId, "new");
  f.pointers.remove(scope, "project", "original");
  expect(nextPage.list(scope, "project")[0].recoveryId).toBe("new");
  const stored = JSON.parse(f.data.get(key)!);
  expect(Object.keys(stored.entries[0]).sort()).toEqual(["identity", "projectId", "recoveryId"]);
});

it("entry and UTF8 byte quotas refuse additions without evicting pending locators", () => {
  const f = fixture(), scope = identity();
  for (let n = 0; n < 32; n++) f.pointers.put(scope, `p${n}`, `r${n}`);
  const before = f.data.get(key);
  expect(() => f.pointers.put(scope, "overflow", "r")).toThrow("recovery storage");
  expect(f.data.get(key)).toBe(before);
  expect(JSON.parse(f.data.get(key)!).entries).toHaveLength(32);
  f.pointers.reset();
  const large = "中".repeat(1000);
  f.pointers.put(scope, large + "1", large);
  f.pointers.put(scope, large + "2", large);
  const bounded = f.data.get(key);
  expect(() => f.pointers.put(scope, large + "3", large)).toThrow("recovery storage");
  expect(f.data.get(key)).toBe(bounded);
  expect(new TextEncoder().encode(bounded!).byteLength).toBeLessThanOrEqual(16 * 1024);
});

it("corrupt or content-bearing records require explicit locator repair and invalid credential scopes are refused", () => {
  const f = fixture(), scope = identity();
  f.data.set(key, "{invalid");
  expect(() => f.pointers.list(scope, "project")).toThrow("recovery storage");
  f.pointers.reset();
  expect(f.pointers.list(scope, "project")).toEqual([]);
  f.pointers.put(scope, "p", "r");
  const envelope = JSON.parse(f.data.get(key)!);
  envelope.entries[0].transaction = { userText: "should never be retained here" };
  f.data.set(key, JSON.stringify(envelope));
  expect(() => new RecoveryPointers().list(scope, "project")).toThrow("recovery storage");
  expect(() => f.pointers.list(JSON.stringify(["http://user:secret@localhost:8788/", "principal", "credential", false]), "project")).toThrow("recovery storage");
  expect(() => f.pointers.list(JSON.stringify(["http://localhost:8788/", "another-owner", null, true]), "project")).toThrow("recovery storage");
  f.pointers.reset();
  expect(new RecoveryPointers().list(scope, "project")).toEqual([]);
});

it("storage read denial, write denial and a silently dropped write are visible failures", () => {
  const f = fixture(), scope = identity(), pointer = { projectId: "p", recoveryId: "r" };
  f.storage.getItem.mockImplementationOnce(() => { throw new Error("Denied"); });
  expect(() => f.pointers.list(scope, "project")).toThrow("recovery storage");
  f.storage.setItem.mockImplementationOnce(() => { throw new Error("Quota"); });
  expect(() => f.pointers.put(scope, pointer.projectId, pointer.recoveryId)).toThrow("recovery storage");
  expect(f.data.has(key)).toBe(false);
  f.storage.setItem.mockImplementationOnce(() => undefined);
  expect(() => f.pointers.put(scope, pointer.projectId, pointer.recoveryId)).toThrow("recovery storage");
  f.pointers.put(scope, pointer.projectId, pointer.recoveryId);
  expect(new RecoveryPointers().list(scope, "p")).toEqual([expect.objectContaining(pointer)]);
});
