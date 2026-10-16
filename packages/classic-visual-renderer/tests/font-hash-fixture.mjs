import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { enableFontHashWorker, clearFontHashWorker } from "../src/font-hash.mjs";

// Model real ownership transfer/detachment while controlling worker completion.
export function controlledWorkers(t) {
  const original = Object.getOwnPropertyDescriptor(globalThis, "Worker");
  const workers = [], pending = [], realms = [];
  class Worker {
    constructor(url, options) {
      assert.ok(url instanceof URL); assert.ok(url.pathname.endsWith("/font-hash-worker.mjs"));
      assert.deepEqual(options, { type: "module" });
      this.terminated = false; this.terminations = 0; workers.push(this);
    }
    postMessage(data, transfer) {
      assert.equal(transfer.length, 1); assert.equal(transfer[0], data.buffer);
      const received = structuredClone(data, { transfer });
      assert.equal(data.buffer.byteLength, 0);
      const entry = {
        worker: this, data: received,
        reply: value => this.onmessage?.({ data: value }),
        resolve: () => {
          const { id, identity, byteLength, buffer } = received;
          const hash = createHash("sha256").update(new Uint8Array(buffer)).digest("hex");
          const reply = structuredClone({ version: 1, kind: "sha256-result", id, identity, byteLength, buffer, hash }, { transfer: [buffer] });
          assert.equal(received.buffer.byteLength, 0);
          this.onmessage?.({ data: reply });
        },
        reject: () => this.onerror?.({ preventDefault() {} }),
      };
      pending.push(entry);
    }
    terminate() { this.terminated = true; this.terminations++; }
  }
  Object.defineProperty(globalThis, "Worker", { configurable: true, writable: true, value: Worker });
  t.after(() => {
    for (const realm of realms) clearFontHashWorker(realm);
    if (original) Object.defineProperty(globalThis, "Worker", original); else delete globalThis.Worker;
  });
  return { workers, pending, newRealm() { const realm = {}; realms.push(realm); enableFontHashWorker(realm); return realm; } };
}
