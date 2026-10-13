import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import {createHash} from "node:crypto";

// The authored hot interval is constant2/1. Align using the independently
// observed first nonzero output and first real copied quantum, never a fitted
// silence offset or a cursor advanced through underflow.
export function correlateHotWindow(record) {
  assert.equal(record.quantum_overflows, 0);
  const {quanta, window: output} = record;
  assert.ok(quanta.length > 0 && quanta.length <= 1024);
  assert.equal(output.values.length, 8192);
  assert.ok(output.values.every(Number.isFinite));
  let consumed = 0, previous = null;
  for (const q of quanta) {
    assert.equal(q.token, record.token);
    assert.ok(Number.isSafeInteger(q.renderFrame) && q.renderFrame >= 0);
    assert.equal(q.quantumFrames, 128);
    assert.ok(Number.isInteger(q.copied) && q.copied >= 0 && q.copied <= 128);
    assert.equal(Number(q.consumedFrames) - consumed, q.copied);
    if (previous) assert.equal(q.renderFrame, previous.renderFrame + 128);
    consumed = Number(q.consumedFrames); previous = q;
  }
  const first = quanta.find(q => q.copied > 0);
  assert.ok(first);
  const delay = output.first_nonzero_frame - first.renderFrame;
  assert.ok(Number.isInteger(delay) && delay >= 0 && delay <= 4800);
  const start = output.start_context_frame - delay, end = start + 4096;
  let covered = 0, missing = 0, zeros = 0, mismatches = 0, sourceStart = null, sourceEnd = null;
  for (const q of quanta) {
    const from = Math.max(start, q.renderFrame), to = Math.min(end, q.renderFrame + 128);
    if (from >= to) continue;
    const before = Number(q.consumedFrames) - q.copied;
    sourceStart ??= before + Math.min(from - q.renderFrame, q.copied);
    sourceEnd = before + Math.min(to - q.renderFrame, q.copied);
    for (let frame = from; frame < to; frame++) {
      const i = frame - start, absent = frame >= q.renderFrame + q.copied;
      const silent = output.values[2 * i] === 0 && output.values[2 * i + 1] === 0;
      covered++; missing += Number(absent); zeros += Number(silent); mismatches += Number(absent !== silent);
    }
  }
  assert.equal(covered, 4096, "Incomplete actual quantum coverage");
  return {measured_first_output_delay_frames: delay, source_context_start: start,
    source_context_end: end, covered_frames: covered, missing_frames: missing,
    post_zero_frames: zeros, zero_mask_mismatches: mismatches,
    consumed_start: String(sourceStart), consumed_end: String(sourceEnd), quantum_records: quanta.length};
}

// One actual failed event, outside Git. A unique directory prevents replacement
// of earlier failures. Buffers remain raw Float32, no renderer-derived oracle.
export async function retainAcousticWindow(directory, {pre, post, session, correlation, gate, failure}) {
  for (const values of [pre.values, post.window.values]) {
    assert.equal(values.length, 8192); assert.ok(values.every(Number.isFinite));
  }
  assert.ok(post.quanta.length <= 1024);
  const target = await fs.mkdtemp(path.join(directory, "acoustic-window-"));
  const buffers = [Buffer.from(Float32Array.from(pre.values).buffer), Buffer.from(Float32Array.from(post.window.values).buffer)];
  const metadata = {schema_version: 1, session, token: post.token, pre_start_sample: pre.start_sample,
    post_context_start: post.window.start_context_frame, first_nonzero_frame: post.window.first_nonzero_frame,
    frames: 4096, channels: 2, encoding: "float32-native-endian", correlation, gate, failure,
    quanta: post.quanta, quantum_overflows: post.quantum_overflows,
    sha256: buffers.map(bytes => createHash("sha256").update(bytes).digest("hex"))};
  const json = JSON.stringify(metadata, null, 2);
  assert.ok(Buffer.byteLength(json) + 65536 <= 320 * 1024, "Failure evidence budget exceeded");
  await fs.writeFile(path.join(target, "pre.f32"), buffers[0], {flag: "wx"});
  await fs.writeFile(path.join(target, "post.f32"), buffers[1], {flag: "wx"});
  await fs.writeFile(path.join(target, "window.json"), json, {flag: "wx"});
  return {directory: target, bytes: Buffer.byteLength(json) + 65536, sha256: metadata.sha256};
}

// Explicit fault injection, not an observation-only or performance candidate.
// Delay block10 and its next credit (at most two original transferable blocks)
// until30 actual zero-copy receipts. Never edit PCM, tokens or consumption.
export function installAcousticDeliveryGate() {
  const witness = globalThis.__scenePumpWitness;
  if (!witness || witness.blocks >= 10) throw Error("Delivery gate missed its installation fence");
  const gate = witness.deliveryGate = {kind: "controlled-original-block-delay", held: 0,
    max_held: 0, zero_quanta: 0, released: false, timed_out: false, cancelled: false};
  const post = MessagePort.prototype.postMessage;
  let ordinal = witness.blocks, pending = [], timer = null, device = null;
  const cancel = () => { gate.cancelled = true; pending = []; gate.held = 0; clearTimeout(timer); };
  const release = () => {
    clearTimeout(timer); gate.released = true;
    const batch = pending; pending = []; gate.held = 0;
    for (const [port, data, rest] of batch) post.call(port, data, ...rest);
  };
  addEventListener("message", ({data}) => {
    if (data.token === witness.token && ["stop", "abort", "freeze"].includes(data.kind)) cancel();
  });
  MessagePort.prototype.postMessage = function(data, ...rest) {
    if (this === device && data.kind === "stop") cancel();
    if (data.kind !== "block" || data.token !== witness.token) return post.call(this, data, ...rest);
    if (!device) {
      device = this;
      this.addEventListener("message", ({data: receipt}) => {
        if (receipt.kind !== "progress" || receipt.token !== witness.token || !pending.length) return;
        if (receipt.copied === 0 && ++gate.zero_quanta === 30) release();
      });
    }
    if (gate.cancelled) return;
    if (++ordinal >= 10 && !gate.released) {
      if (pending.length >= 2) throw Error("Delivery gate exceeds original two credits");
      pending.push([this, data, rest]); gate.held = pending.length; gate.max_held = Math.max(gate.max_held, gate.held);
      timer ??= setTimeout(() => { gate.timed_out = true; release(); }, 1000);
      return;
    }
    return post.call(this, data, ...rest);
  };
}
