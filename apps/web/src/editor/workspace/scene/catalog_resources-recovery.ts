import type { ToolInput } from "../../../lib/editor-sdk";
import { canonicalRecoveryIdentity, validRecoveryId } from "./edit-recovery-pointers";
export type CatalogImportRequest = ToolInput<"opencut_catalog_resources_import">;
export type CatalogGenerateRequest = ToolInput<"opencut_catalog_resources_generate">;
export type CatalogRequest = CatalogImportRequest | CatalogGenerateRequest;
type Entry = { identity: string; request: CatalogRequest };
type Envelope = { entries: Entry[] };
type StoragePort = Pick<Storage, "getItem" | "setItem">;
const invalid = () => Error("Catalog recovery is unavailable or invalid. Resolve the original acquisition before continuing.");
const exact = (v: unknown, keys: string): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v) && Object.keys(v).sort().join() === keys;
const digest = (v: unknown): v is string => typeof v === "string" && /^[a-f0-9]{64}$/.test(v);
function request(value: unknown): CatalogRequest {
  if (exact(value, "asset_id,expected_revision,idempotency_key,project_id,recipe")) {
    if (new TextEncoder().encode(JSON.stringify(value)).length > 4096 || !validRecoveryId(value.asset_id) || !validRecoveryId(value.expected_revision) || !validRecoveryId(value.idempotency_key) || !validRecoveryId(value.project_id) || !exact(value.recipe, "duration_ms,kind,level_db,seed")) throw invalid();
    const r = value.recipe;
    if (r.kind !== "whoosh" && r.kind !== "click" && r.kind !== "impact") throw invalid();
    const min = r.kind === "click" ? 40 : 100, max = r.kind === "click" ? 500 : r.kind === "impact" ? 3000 : 5000;
    if (typeof r.duration_ms !== "number" || !Number.isSafeInteger(r.duration_ms) || r.duration_ms < min || r.duration_ms > max || typeof r.seed !== "number" || !Number.isSafeInteger(r.seed) || r.seed < 0 || r.seed > 4294967295 || typeof r.level_db !== "number" || !Number.isFinite(r.level_db) || r.level_db < -60 || r.level_db > 0) throw invalid();
    return { project_id: value.project_id, expected_revision: value.expected_revision, idempotency_key: value.idempotency_key, asset_id: value.asset_id, recipe: { kind: r.kind, duration_ms: r.duration_ms, seed: r.seed, level_db: r.level_db } };
  }
  if (!exact(value, "asset_id,catalog_id,expected_revision,idempotency_key,project_id,uploaded") || new TextEncoder().encode(JSON.stringify(value)).length > 4096 || !validRecoveryId(value.asset_id) || !validRecoveryId(value.expected_revision) || !validRecoveryId(value.idempotency_key) || !validRecoveryId(value.project_id) || typeof value.catalog_id !== "string") throw invalid();
  let uploaded: CatalogImportRequest["uploaded"] = null;
  if (value.uploaded !== null) {
    if (!exact(value.uploaded, "media_id,version") || !validRecoveryId(value.uploaded.media_id) || !exact(value.uploaded.version, "content_hash,id,size")) throw invalid();
    const v = value.uploaded.version;
    if (!digest(v.content_hash) || v.id !== `av_${v.content_hash}` || typeof v.size !== "number" || !Number.isSafeInteger(v.size) || v.size < 1 || v.size > 1048576) throw invalid();
    uploaded = { media_id: value.uploaded.media_id, version: { content_hash: v.content_hash, id: `av_${v.content_hash}`, size: v.size } };
  }
  if (uploaded ? !/^flags:[a-z]{2}(?:-[a-z]{3})?$/.test(value.catalog_id) : !/^kenney-ui-audio:(click[1-5]|mouseclick1|mouserelease1|rollover[1-6]|switch(?:[1-9]|[12][0-9]|3[0-8]))$/.test(value.catalog_id)) throw invalid();
  return { project_id: value.project_id, expected_revision: value.expected_revision, idempotency_key: value.idempotency_key, catalog_id: value.catalog_id, asset_id: value.asset_id, uploaded };
}
function parse(raw: string | null, historical = false): Envelope {
  if (raw === null) return { entries: [] };
  if (raw.length > 65536 || new TextEncoder().encode(raw).length > 65536) throw invalid();
  const value: unknown = JSON.parse(raw);
  if (!exact(value, historical ? "entries,version" : "entries") || (historical && value.version !== 1) || !Array.isArray(value.entries) || value.entries.length > 16) throw invalid();
  const seen = new Set<string>();
  const entries = value.entries.map((entry: unknown) => {
    if (!exact(entry, "identity,request") || typeof entry.identity !== "string") throw invalid();
    const identity = canonicalRecoveryIdentity(entry.identity), r = request(entry.request), key = JSON.stringify([identity, r.project_id]);
    if (seen.has(key)) throw invalid(); seen.add(key); return { identity, request: r };
  });
  return { entries };
}
/** Only original commands, no pixels, credentials, paths or project document. */
export class CatalogRecovery {
  constructor(private readonly storage?: StoragePort) {}
  private readRaw() { return this.storage ? this.storage.getItem("opencut.catalog-resources") : sessionStorage.getItem("opencut.catalog-resources"); }
  private readOriginal() { return this.storage ? this.storage.getItem("opencut.catalog-resources.v1") : sessionStorage.getItem("opencut.catalog-resources.v1"); }
  private writeRaw(raw: string) {
    if (this.storage) this.storage.setItem("opencut.catalog-resources", raw); else sessionStorage.setItem("opencut.catalog-resources", raw);
    if (this.readRaw() !== raw) throw invalid();
  }
  private read() { try { const current = this.readRaw(); return current === null ? parse(this.readOriginal(), true) : parse(current); } catch { throw invalid(); } }
  private write(value: Envelope) { const raw = JSON.stringify(value); parse(raw); this.writeRaw(raw); }
  get(scope: string, project: string) { const identity = canonicalRecoveryIdentity(scope); return this.read().entries.find(e => e.identity === identity && e.request.project_id === project)?.request ?? null; }
  put(scope: string, input: CatalogRequest) {
    const identity = canonicalRecoveryIdentity(scope), r = request(input), value = this.read(), old = value.entries.find(e => e.identity === identity && e.request.project_id === r.project_id);
    if (old && JSON.stringify(old.request) !== JSON.stringify(r)) throw invalid();
    if (!old) value.entries.push({ identity, request: r }); this.write(value);
  }
  remove(scope: string, project: string, key: string) { const identity = canonicalRecoveryIdentity(scope), value = this.read(); value.entries = value.entries.filter(e => !(e.identity === identity && e.request.project_id === project && e.request.idempotency_key === key)); this.write(value); }
}
