/** Recovery locators only. Exact transactions belong to the authorized host.
 * sessionStorage is tab-local convenience, never an authentication/locking port.
 * The host discovers orphan records even when this pointer could not be saved. */
export type RecoveryPointerV1 = {
  projectId: string;
  sequenceId: string;
  recoveryId: string;
};
type Scope = {
  endpoint: string;
  principalId: string;
  credentialId: string | null;
  owner: boolean;
};
type Entry = RecoveryPointerV1 & { scope: Scope };
type Envelope = { schemaVersion: 1; tabId: string; entries: Entry[] };
const MAX_ENTRIES = 32, MAX_BYTES = 16 * 1024;
const problem = () => new Error("Timeline recovery storage is unavailable or invalid. Resolve the original operation before editing.");
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const exactKeys = (value: Record<string, unknown>, keys: string[]) =>
  Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
const id = (value: unknown): value is string => typeof value === "string" &&
  value.length > 0 && value.length <= 1024 && value.trim().length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
function scopeValue(value: unknown): Scope {
  if (!object(value) || !exactKeys(value, ["endpoint", "principalId", "credentialId", "owner"]) ||
      !id(value.endpoint) || !id(value.principalId) || typeof value.owner !== "boolean" ||
      (value.credentialId !== null && !id(value.credentialId))) throw problem();
  const url = new URL(value.endpoint);
  if (!["http:", "https:"].includes(url.protocol) || url.username || url.password || url.search || url.hash) throw problem();
  if (value.owner ? value.principalId !== "local-owner" || value.credentialId !== null : value.credentialId === null) throw problem();
  return { endpoint: value.endpoint, principalId: value.principalId, credentialId: value.credentialId as string | null, owner: value.owner };
}
/** The identity must be captured by EditorApi.authenticateConnection(), never
 * assembled from user-provided credentials or a project response. */
function authenticatedScope(identity: string): Scope {
  let tuple: unknown;
  try { tuple = JSON.parse(identity); } catch { throw problem(); }
  if (!Array.isArray(tuple) || tuple.length !== 4) throw problem();
  return scopeValue({ endpoint: tuple[0], principalId: tuple[1], credentialId: tuple[2], owner: tuple[3] });
}
const scopeKey = (scope: Scope) => JSON.stringify([scope.endpoint, scope.principalId, scope.credentialId, scope.owner]);
function pointer(value: unknown): RecoveryPointerV1 {
  if (!object(value) || !exactKeys(value, ["projectId", "sequenceId", "recoveryId"]) ||
      !id(value.projectId) || !id(value.sequenceId) || !id(value.recoveryId)) throw problem();
  return { projectId: value.projectId, sequenceId: value.sequenceId, recoveryId: value.recoveryId };
}
function parse(raw: string): Envelope {
  if (new TextEncoder().encode(raw).byteLength > MAX_BYTES) throw problem();
  let value: unknown;
  try { value = JSON.parse(raw); } catch { throw problem(); }
  if (!object(value) || !exactKeys(value, ["schemaVersion", "tabId", "entries"]) ||
      value.schemaVersion !== 1 || !id(value.tabId) || !Array.isArray(value.entries) || value.entries.length > MAX_ENTRIES) throw problem();
  const seen = new Set<string>();
  const entries = value.entries.map((entry: unknown): Entry => {
    if (!object(entry) || !exactKeys(entry, ["projectId", "sequenceId", "recoveryId", "scope"])) throw problem();
    const scope = scopeValue(entry.scope), p = pointer({ projectId: entry.projectId, sequenceId: entry.sequenceId, recoveryId: entry.recoveryId });
    const key = JSON.stringify([scopeKey(scope), p.projectId]);
    if (seen.has(key)) throw problem();
    seen.add(key);
    return { ...p, scope };
  });
  return { schemaVersion: 1, tabId: value.tabId, entries };
}

export class RecoveryPointersV1 {
  private tabId: string | null = null;
  private read(): Envelope {
    let raw: string | null;
    try { raw = sessionStorage.getItem("opencut.edit-recovery.v1"); } catch { throw problem(); }
    const envelope = raw === null
      ? { schemaVersion: 1 as const, tabId: this.tabId ?? crypto.randomUUID(), entries: [] }
      : parse(raw);
    this.tabId = envelope.tabId;
    return envelope;
  }
  private write(envelope: Envelope) {
    const raw = JSON.stringify(envelope);
    if (envelope.entries.length > MAX_ENTRIES || new TextEncoder().encode(raw).byteLength > MAX_BYTES) throw problem();
    try {
      sessionStorage.setItem("opencut.edit-recovery.v1", raw);
      if (sessionStorage.getItem("opencut.edit-recovery.v1") !== raw) throw problem();
    } catch { throw problem(); }
  }
  list(authenticatedIdentity: string): RecoveryPointerV1[] {
    const key = scopeKey(authenticatedScope(authenticatedIdentity));
    return this.read().entries.filter(entry => scopeKey(entry.scope) === key)
      .map(({ projectId, sequenceId, recoveryId }) => ({ projectId, sequenceId, recoveryId }));
  }
  put(authenticatedIdentity: string, value: RecoveryPointerV1) {
    const scope = authenticatedScope(authenticatedIdentity), key = scopeKey(scope), p = pointer(value), envelope = this.read();
    const existing = envelope.entries.find(entry => scopeKey(entry.scope) === key && entry.projectId === p.projectId);
    if (existing && existing.recoveryId !== p.recoveryId) throw problem();
    envelope.entries = envelope.entries.filter(entry => !(scopeKey(entry.scope) === key && entry.projectId === p.projectId));
    envelope.entries.push({ ...p, scope });
    this.write(envelope);
  }
  /** Expected identity protects a newer pointer from an older terminal callback. */
  remove(authenticatedIdentity: string, projectId: string, recoveryId: string) {
    const key = scopeKey(authenticatedScope(authenticatedIdentity));
    if (!id(projectId) || !id(recoveryId)) throw problem();
    const envelope = this.read();
    const entries = envelope.entries.filter(entry => !(scopeKey(entry.scope) === key && entry.projectId === projectId && entry.recoveryId === recoveryId));
    if (entries.length === envelope.entries.length) return;
    this.write({ ...envelope, entries });
  }
  /** Explicitly clears locators after storage repair. Host unresolved records
   * remain authoritative and must still be discovered before a new dispatch. */
  reset() {
    this.write({ schemaVersion: 1, tabId: this.tabId ?? crypto.randomUUID(), entries: [] });
  }
}
