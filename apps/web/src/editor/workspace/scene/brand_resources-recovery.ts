import type { BrandImport } from "./brand_resources";
import { canonicalRecoveryIdentity, validRecoveryId } from "./edit-recovery-pointers";
type StoragePort = Pick<Storage, "getItem" | "setItem">;
type Entry = { identity: string; request: BrandImport };
type Envelope = { entries: Entry[] };
const invalid = () => Error("Brand recovery is unavailable or invalid. Resolve the original acquisition before continuing.");
const exact = (value: unknown, keys: string): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value) && Object.keys(value).sort().join() === keys;
function request(value: unknown, historical = false): BrandImport {
  if (!exact(value, historical ? "asset_id,catalog_id,expected_revision,idempotency_key,project_id,schema_version,uploaded" : "asset_id,catalog_id,expected_revision,idempotency_key,project_id,uploaded") || (historical && value.schema_version !== 1) || new TextEncoder().encode(JSON.stringify(value)).length > 4096 || !validRecoveryId(value.project_id) || !validRecoveryId(value.expected_revision) || !validRecoveryId(value.asset_id) || !validRecoveryId(value.idempotency_key) || typeof value.catalog_id !== "string" || !/^simple-icons:16\.35\.0:[a-z0-9_]{1,128}$/.test(value.catalog_id) || !exact(value.uploaded, "media_id,version") || !validRecoveryId(value.uploaded.media_id) || !exact(value.uploaded.version, "content_hash,id,size")) throw invalid();
  const version = value.uploaded.version;
  if (typeof version.content_hash !== "string" || !/^[a-f0-9]{64}$/.test(version.content_hash) || version.id !== `av_${version.content_hash}` || typeof version.size !== "number" || !Number.isSafeInteger(version.size) || version.size < 1 || version.size > 1048576) throw invalid();
  return { project_id: value.project_id, expected_revision: value.expected_revision, asset_id: value.asset_id, idempotency_key: value.idempotency_key, catalog_id: value.catalog_id, uploaded: { media_id: value.uploaded.media_id, version: { id: `av_${version.content_hash}`, content_hash: version.content_hash, size: version.size } } };
}
function parse(raw: string | null, historical = false): Envelope {
  if (raw === null) return { entries: [] };
  if (raw.length > 65536 || new TextEncoder().encode(raw).length > 65536) throw invalid();
  const value: unknown = JSON.parse(raw);
  if (!exact(value, historical ? "entries,version" : "entries") || (historical && value.version !== 1) || !Array.isArray(value.entries) || value.entries.length > 16) throw invalid();
  const seen = new Set<string>();
  return { entries: value.entries.map((item: unknown) => {
    if (!exact(item, "identity,request") || typeof item.identity !== "string") throw invalid();
    const identity = canonicalRecoveryIdentity(item.identity), original = request(item.request, historical), key = JSON.stringify([identity, original.project_id]);
    if (seen.has(key)) throw invalid(); seen.add(key); return { identity, request: original };
  }) };
}
/** One fixed session key. No SVG, pixels, license text, credentials or private paths. */
export class BrandRecovery {
  constructor(private readonly storage?: StoragePort) {}
  private readRaw() { return this.storage ? this.storage.getItem("opencut.brand-resources") : sessionStorage.getItem("opencut.brand-resources"); }
  private readOriginal() { return this.storage ? this.storage.getItem("opencut.brand-resources.v1") : sessionStorage.getItem("opencut.brand-resources.v1"); }
  private writeRaw(raw: string) { if (this.storage) this.storage.setItem("opencut.brand-resources", raw); else sessionStorage.setItem("opencut.brand-resources", raw); if (this.readRaw() !== raw) throw invalid(); }
  private read() { try { const current = this.readRaw(); return current === null ? parse(this.readOriginal(), true) : parse(current); } catch { throw invalid(); } }
  private write(value: Envelope) { const raw = JSON.stringify(value); parse(raw); this.writeRaw(raw); }
  get(scope: string, project: string) { const identity = canonicalRecoveryIdentity(scope); return this.read().entries.find(e => e.identity === identity && e.request.project_id === project)?.request ?? null; }
  put(scope: string, input: BrandImport) { const identity = canonicalRecoveryIdentity(scope), original = request(input), value = this.read(), old = value.entries.find(e => e.identity === identity && e.request.project_id === original.project_id); if (old && JSON.stringify(old.request) !== JSON.stringify(original)) throw invalid(); if (!old) value.entries.push({ identity, request: original }); this.write(value); }
  remove(scope: string, project: string, key: string) { const identity = canonicalRecoveryIdentity(scope), value = this.read(); value.entries = value.entries.filter(e => !(e.identity === identity && e.request.project_id === project && e.request.idempotency_key === key)); this.write(value); }
}
