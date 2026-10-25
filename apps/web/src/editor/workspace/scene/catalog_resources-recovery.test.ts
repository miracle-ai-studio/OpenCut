import { expect, it } from "vitest";
import { CatalogRecovery, type CatalogImportRequest } from "./catalog_resources-recovery";
const scope = (service = "one") => JSON.stringify([`https://${service}.example`, "principal", "credential-id", false]);
const request = (project = "project"): CatalogImportRequest => ({ project_id: project, expected_revision: "fixed", idempotency_key: "original", catalog_id: "kenney-ui-audio:click1", asset_id: "sound", uploaded: null });
function fixture() { const data = new Map<string, string>(), storage = { getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, value: string) => { data.set(key, value); } }; return { data, storage, recovery: new CatalogRecovery(storage) }; }
it("retains exact original request across reopen and isolates connection/project", () => {
  const f = fixture(), original = JSON.stringify({version:1,entries:[{identity:scope(),request:request()}]});
  f.data.set("opencut.catalog-resources.v1", original);
  expect(f.recovery.get(scope(), "project")).toEqual(request());expect(f.data.size).toBe(1);
  f.recovery.put(scope(), request()); f.recovery.put(scope("two"), request());
  expect(f.data.get("opencut.catalog-resources.v1")).toBe(original);
  expect([...f.data.keys()]).toEqual(["opencut.catalog-resources.v1","opencut.catalog-resources"]);
  expect(new CatalogRecovery(f.storage).get(scope(), "project")).toEqual(request());
  expect(f.recovery.get(scope(), "other")).toBeNull();
  f.recovery.remove(scope(), "project", "different-key"); expect(f.recovery.get(scope(), "project")).toEqual(request());
  f.recovery.remove(scope(), "project", "original"); expect(f.recovery.get(scope(), "project")).toBeNull(); expect(f.recovery.get(scope("two"), "project")).toEqual(request());
});
it("refuses changed original key/revision and keeps the prior request", () => {
  const f = fixture(); f.recovery.put(scope(), request());
  expect(() => f.recovery.put(scope(), { ...request(), expected_revision: "new-head" })).toThrow();
  expect(() => f.recovery.put(scope(), { ...request(), idempotency_key: "new-key" })).toThrow(); expect(f.recovery.get(scope(), "project")).toEqual(request());
});
it("bounds entries and UTF-8 command bytes before writes", () => {
  const f = fixture(); for (let i = 0; i < 16; i++) f.recovery.put(scope(), request(`p-${i}`)); const before = [...f.data.values()][0];
  expect(() => f.recovery.put(scope(), request("seventeenth"))).toThrow(); expect([...f.data.values()][0]).toBe(before);
  expect(() => fixture().recovery.put(scope(), { ...request("字".repeat(1024)), expected_revision: "字".repeat(1024) })).toThrow();
});
it("rejects credentials, arbitrary sources, unknown fields and malformed flag selectors", () => {
  const f = fixture(); expect(() => f.recovery.put(JSON.stringify(["https://user:secret@example.test", "principal", "credential-id", false]), request())).toThrow();
  expect(() => f.recovery.put(scope(), { ...request(), catalog_id: "https://example.test/a" })).toThrow();
  expect(() => f.recovery.put(scope(), { ...request(), catalog_id: "flags:fr" })).toThrow();
  f.recovery.put(scope(), request()); const key = [...f.data.keys()][0], raw = JSON.parse(f.data.get(key)!); raw.entries[0].request.source_path = "/private/user-media"; f.data.set(key, JSON.stringify(raw)); expect(() => f.recovery.get(scope(), "project")).toThrow();
});
it("retains complete local recipe separately from downloaded resource selectors", () => {
  const f = fixture(), base = request();
  const generated = { project_id: base.project_id, expected_revision: base.expected_revision, idempotency_key: base.idempotency_key, asset_id: base.asset_id, recipe: { kind: "whoosh" as const, duration_ms: 750, seed: 123456, level_db: -9 } };
  f.recovery.put(scope(), generated); expect(new CatalogRecovery(f.storage).get(scope(), "project")).toEqual(generated);
  expect(() => fixture().recovery.put(scope(), { ...generated, recipe: { ...generated.recipe, duration_ms: 5001 } })).toThrow();
  expect(() => fixture().recovery.put(scope(), { ...generated, recipe: { ...generated.recipe, level_db: Number.NaN } })).toThrow();
  expect(() => fixture().recovery.put(scope(), { ...generated, recipe: { ...generated.recipe, seed: -1 } })).toThrow();
});
