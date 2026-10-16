import { checkAbort, reject, VisualExecutionError } from "./validation.mjs";

const realms = new WeakMap();
const enabledRealms = new WeakSet();
export const hasFontHashWorker = realm => !!realm && enabledRealms.has(realm);
export function enableFontHashWorker(realm) {
  enabledRealms.add(realm);
  workerState(realm);
}
const failure = message => new VisualExecutionError("FONT_LOAD_FAILED", message);

// One private worker per GPU realm; IDs survive a failed worker's replacement.
// There is no cross-frame digest cache: every request transfers a fresh snapshot.
export function clearFontHashWorker(realm, reason = failure("Font hash realm cleared")) {
  const state = realm && realms.get(realm);
  if (!state) return;
  const worker = state.worker; state.worker = null;
  worker?.terminate();
  for (const item of state.pending.values()) {
    item.signal?.removeEventListener("abort", item.abort);
    item.reject(reason);
  }
  state.pending.clear();
}

function workerState(realm) {
  if (!realm || realm.closed) reject("INVALID_VISUAL_PLAN", "Font hash realm is unavailable");
  let state = realms.get(realm);
  if (!state) { state = { worker: null, nextId: 1, pending: new Map() }; realms.set(realm, state); }
  if (!state.worker) {
    if (typeof Worker !== "function") reject("CAPABILITY_UNAVAILABLE", "Font hash worker is unavailable");
    const worker = new Worker(new URL("./font-hash-worker.mjs", import.meta.url), { type: "module" });
    state.worker = worker;
    worker.onmessage = ({ data }) => {
      if (state.worker !== worker) return;
      const item = data && state.pending.get(data.id);
      const valid = item && data.version === 1 && data.identity === item.identity && data.byteLength === item.byteLength;
      if (!valid || data.kind !== "sha256-result" || !(data.buffer instanceof ArrayBuffer) || data.buffer.byteLength !== item.byteLength || typeof data.hash !== "string" || !/^[a-f0-9]{64}$/.test(data.hash) || Object.keys(data).sort().join() !== "buffer,byteLength,hash,id,identity,kind,version") {
        clearFontHashWorker(realm, failure("Font hash worker returned an invalid result")); return;
      }
      state.pending.delete(data.id); item.signal?.removeEventListener("abort", item.abort);
      item.resolve({ snapshot: new Uint8Array(data.buffer), hash: data.hash });
    };
    worker.onerror = event => {
      event.preventDefault?.();
      if (state.worker === worker) clearFontHashWorker(realm, failure("Font hash worker failed"));
    };
    worker.onmessageerror = () => {
      if (state.worker === worker) clearFontHashWorker(realm, failure("Font hash worker message failed"));
    };
  }
  return state;
}

export function hashFontSnapshot(realm, identity, snapshot, signal) {
  checkAbort(signal);
  if (!hasFontHashWorker(realm)) reject("INVALID_VISUAL_PLAN", "Font hash worker was not enabled for this realm");
  if (!(snapshot instanceof Uint8Array) || snapshot.byteOffset !== 0 || snapshot.byteLength !== snapshot.buffer.byteLength || snapshot.byteLength === 0 || !(snapshot.buffer instanceof ArrayBuffer)) reject("INVALID_VISUAL_PLAN", "Font hash requires a complete private snapshot");
  const state = workerState(realm);
  if (state.pending.size >= 32 || !Number.isSafeInteger(state.nextId)) reject("VISUAL_BUDGET_EXCEEDED", "Font hash worker request limit exceeded");
  const id = state.nextId++, byteLength = snapshot.byteLength;
  return new Promise((resolve, rejectPromise) => {
    const abort = () => clearFontHashWorker(realm, signal.reason ?? new VisualExecutionError("CANCELLED", "Font hash cancelled"));
    state.pending.set(id, { identity, byteLength, resolve, reject: rejectPromise, signal, abort });
    signal?.addEventListener("abort", abort, { once: true });
    try {
      checkAbort(signal);
      state.worker.postMessage({ version: 1, kind: "sha256", id, identity, byteLength, buffer: snapshot.buffer }, [snapshot.buffer]);
    } catch (error) { clearFontHashWorker(realm, error); }
  });
}
