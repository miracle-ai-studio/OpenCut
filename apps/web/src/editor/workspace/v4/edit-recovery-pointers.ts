/** Non-secret tab locators only; exact requests remain in the authorized host. */
type Pointer = { identity: string; projectId: string; recoveryId: string };
type Envelope = { schemaVersion: 2; tabId: string; entries: Pointer[] };
const invalid = () => new Error("Edit recovery storage is unavailable or invalid. Resolve the original edit before continuing.");
const id = (value: unknown): value is string => typeof value === "string" && value.length > 0 && value.length <= 1024 && !!value.trim() && !/[\u0000-\u001f\u007f-\u009f]/.test(value);
function identity(value: string) {
  let tuple: unknown;
  try { tuple = JSON.parse(value); } catch { throw invalid(); }
  if (!Array.isArray(tuple) || tuple.length !== 4 || !id(tuple[0]) || !id(tuple[1]) || typeof tuple[3] !== "boolean" ||
    (tuple[3] ? tuple[1] !== "local-owner" || tuple[2] !== null : !id(tuple[2]))) throw invalid();
  const endpoint = new URL(tuple[0]);
  if (!["http:", "https:"].includes(endpoint.protocol) || endpoint.username || endpoint.password || endpoint.search || endpoint.hash) throw invalid();
  return JSON.stringify(tuple);
}
function parse(raw: string): Envelope {
  if (new TextEncoder().encode(raw).length > 16384) throw invalid();
  let value: Envelope;
  try { value = JSON.parse(raw); } catch { throw invalid(); }
  if (!value || Object.keys(value).sort().join() !== "entries,schemaVersion,tabId" || value.schemaVersion !== 2 || !id(value.tabId) || !Array.isArray(value.entries) || value.entries.length > 32) throw invalid();
  const seen = new Set<string>();
  for (const entry of value.entries) {
    if (!entry || Object.keys(entry).sort().join() !== "identity,projectId,recoveryId" || !id(entry.projectId) || !id(entry.recoveryId) || typeof entry.identity !== "string") throw invalid();
    identity(entry.identity);
    const key = JSON.stringify([entry.identity, entry.projectId]); if (seen.has(key)) throw invalid(); seen.add(key);
  }
  return value;
}
export class RecoveryPointersV2 {
  private read(): Envelope {
    try { const raw = sessionStorage.getItem("opencut.edit-recovery.v2"); return raw === null ? { schemaVersion: 2, tabId: crypto.randomUUID(), entries: [] } : parse(raw); }
    catch { throw invalid(); }
  }
  private write(value: Envelope) {
    const raw = JSON.stringify(value); parse(raw);
    try { sessionStorage.setItem("opencut.edit-recovery.v2", raw); if (sessionStorage.getItem("opencut.edit-recovery.v2") !== raw) throw invalid(); }
    catch { throw invalid(); }
  }
  list(scope: string, projectId: string) { const canonical = identity(scope); return this.read().entries.filter(entry => entry.identity === canonical && entry.projectId === projectId); }
  put(scope: string, projectId: string, recoveryId: string) {
    const canonical = identity(scope), value = this.read();
    if (!id(projectId) || !id(recoveryId)) throw invalid();
    const existing = value.entries.find(entry => entry.identity === canonical && entry.projectId === projectId);
    if (existing && existing.recoveryId !== recoveryId) throw invalid();
    value.entries = value.entries.filter(entry => !(entry.identity === canonical && entry.projectId === projectId));
    value.entries.push({ identity: canonical, projectId, recoveryId }); this.write(value);
  }
  remove(scope: string, projectId: string, recoveryId: string) {
    const canonical = identity(scope), value = this.read();
    const entries = value.entries.filter(entry => !(entry.identity === canonical && entry.projectId === projectId && entry.recoveryId === recoveryId));
    if (entries.length !== value.entries.length) this.write({ ...value, entries });
  }
  reset() { this.write({ schemaVersion: 2, tabId: crypto.randomUUID(), entries: [] }); }
}
