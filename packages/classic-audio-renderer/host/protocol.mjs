import { EventEmitter, once } from "node:events";
import { AudioExecutionError, integer, reject } from "../src/validation.mjs";

export const PROTOCOL_LIMITS = Object.freeze({ packetBytes: 1048576, replyBytes: 16777216, outstanding: 8, chunkBytes: 65536, queuedBytes: 8388608 });
const envelope = packet => packet && packet.protocolVersion === 1 && typeof packet.executionId === "string" && /^[A-Za-z0-9._:-]{1,128}$/.test(packet.executionId);
export function validateEnvelope(packet, executionId) {
  if (!envelope(packet) || executionId && packet.executionId !== executionId) reject("INVALID_HOST_PROTOCOL", "Invalid execution envelope");
}
export function decodeHex(value, maximum = PROTOCOL_LIMITS.chunkBytes) {
  if (typeof value !== "string" || value.length > maximum * 2 || !/^(?:[0-9a-f]{2})*$/.test(value)) reject("INVALID_HOST_PROTOCOL", "Byte payload is not bounded canonical hex");
  return Buffer.from(value, "hex");
}
export class PacketReader extends EventEmitter {
  constructor(input) {
    super(); this.parts = []; this.length = 0; this.failed = false;
    input.on("data", bytes => { try { this.feed(Buffer.from(bytes)); } catch (error) { this.failed = true; this.emit("failure", error); } });
    input.on("end", () => {
      if (this.length) this.emit("failure", new AudioExecutionError("INVALID_HOST_PROTOCOL", "stdin ended inside a protocol packet"));
      this.emit("end");
    });
    input.on("error", () => this.emit("failure", new AudioExecutionError("HOST_PIPE_CLOSED", "Host input pipe failed")));
  }
  feed(bytes) {
    if (this.failed) return;
    let start = 0;
    for (let i = 0; i <= bytes.length; i++) {
      if (i !== bytes.length && bytes[i] !== 10) continue;
      const part = bytes.subarray(start, i);
      if (this.length + part.length > PROTOCOL_LIMITS.packetBytes) reject("HOST_PACKET_TOO_LARGE", "Host packet exceeds 1 MiB");
      this.parts.push(part); this.length += part.length; start = i + 1;
      if (this.parts.length > 1024) this.parts = [Buffer.concat(this.parts, this.length)];
      if (i === bytes.length) break;
      const line = Buffer.concat(this.parts, this.length); this.parts = []; this.length = 0;
      if (!line.length) reject("INVALID_HOST_PROTOCOL", "Empty protocol packet");
      let packet;
      try { packet = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(line)); }
      catch { reject("INVALID_HOST_PROTOCOL", "Protocol packet is not UTF-8 JSON"); }
      validateEnvelope(packet); this.emit("packet", packet);
    }
  }
}
export class PacketWriter {
  constructor(output, executionId) { this.output = output; this.executionId = executionId; this.tail = Promise.resolve(); this.queued = 0; }
  send(packet) {
    const bytes = Buffer.from(JSON.stringify({ ...packet, protocolVersion: 1, executionId: this.executionId }) + "\n");
    if (bytes.length - 1 > PROTOCOL_LIMITS.packetBytes || bytes.length > PROTOCOL_LIMITS.queuedBytes - this.queued) reject("HOST_PACKET_TOO_LARGE", "Host output exceeds the bounded protocol queue");
    this.queued += bytes.length;
    const work = this.tail.then(async () => {
      if (this.output.destroyed || this.output.writableEnded) reject("HOST_PIPE_CLOSED", "Host output pipe is closed");
      if (!this.output.write(bytes)) await once(this.output, "drain");
    }).finally(() => { this.queued -= bytes.length; });
    this.tail = work.catch(() => {}); return work;
  }
}
export class CallbackBroker {
  constructor(writer, timeoutMs = 30000) { this.writer = writer; this.timeout = integer(timeoutMs, "callback timeout", 1, 120000); this.pending = new Map(); this.next = 0; this.chunkBytes = 0; this.closed = false; }
  request(op, args) {
    if (this.closed) return Promise.reject(new AudioExecutionError("HOST_PIPE_CLOSED", "Host callbacks have been closed"));
    if (this.pending.size >= PROTOCOL_LIMITS.outstanding) return Promise.reject(new AudioExecutionError("HOST_CALLBACK_BUDGET", "Too many outstanding host callbacks"));
    const id = String(++this.next);
    return new Promise((resolve, fail) => {
      const timer = setTimeout(() => this.finish(id, new AudioExecutionError("HOST_CALLBACK_TIMEOUT", "Host callback timed out")), this.timeout);
      this.pending.set(id, { resolve, fail, timer, parts: [], bytes: 0, sequence: 0 });
      Promise.resolve().then(() => this.writer.send({ type: "request", id, op, args })).catch(error => this.finish(id, error));
    });
  }
  receive(packet) {
    const entry = this.pending.get(packet.id);
    if (!entry) reject("INVALID_HOST_PROTOCOL", "Reply references an unknown or completed callback");
    if (packet.type === "reply") {
      if (entry.sequence || typeof packet.ok !== "boolean") reject("INVALID_HOST_PROTOCOL", "Invalid callback reply");
      if (!packet.ok) {
        const code = typeof packet.error?.code === "string" ? packet.error.code.slice(0, 128) : "HOST_CALLBACK_FAILED";
        return this.finish(packet.id, new AudioExecutionError(code, "Host rejected the audio callback"));
      }
      return this.finish(packet.id, undefined, packet.value);
    }
    if (packet.type !== "reply_chunk" || packet.sequence !== entry.sequence++ || typeof packet.final !== "boolean") reject("INVALID_HOST_PROTOCOL", "Callback reply chunks are not strictly ordered");
    const part = decodeHex(packet.bytesHex);
    if (part.length > PROTOCOL_LIMITS.replyBytes - entry.bytes || part.length > PROTOCOL_LIMITS.replyBytes * 2 - this.chunkBytes) reject("HOST_REPLY_TOO_LARGE", "Reassembled callback facts exceed the reply budget");
    entry.parts.push(part); entry.bytes += part.length; this.chunkBytes += part.length;
    if (packet.final) {
      let value;
      try { value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(Buffer.concat(entry.parts, entry.bytes))); }
      catch { reject("INVALID_HOST_PROTOCOL", "Reassembled callback is not UTF-8 JSON"); }
      this.finish(packet.id, undefined, value);
    }
  }
  finish(id, error, value) {
    const entry = this.pending.get(id); if (!entry) return;
    clearTimeout(entry.timer); this.pending.delete(id); this.chunkBytes -= entry.bytes;
    if (error) entry.fail(error); else entry.resolve(value);
  }
  close(error = new AudioExecutionError("CANCELLED", "Audio host execution stopped")) { this.closed = true; for (const id of this.pending.keys()) this.finish(id, error); }
}
