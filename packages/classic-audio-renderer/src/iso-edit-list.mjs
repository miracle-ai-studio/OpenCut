import { reject } from "./validation.mjs";

const ISO_FIRST_BOXES = new Set(["ftyp", "moov", "mdat", "free", "skip", "wide", "styp", "moof", "sidx"]);
const MAX_BOXES = 2048, MAX_ENTRIES = 2048, MAX_DEPTH = 4;

// This is a capability gate, not a second demuxer. Mediabunny1.41 discards
// unsupported edit-list facts after warning. Inspect the already-admitted
// immutable bytes before calling it; never derive PCM positions here.
// The current profile rejects unsupported edits in ANY ISO track, including
// non-primary/video tracks, rather than advertising partial-file support.
export function guardIsoEditLists(bytes) {
  if (!(bytes instanceof Uint8Array) || bytes.length < 8) return;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const tag = at => String.fromCharCode(...bytes.subarray(at, at + 4));
  if (!ISO_FIRST_BOXES.has(tag(4))) return; // WAV/Matroska/other demuxers are unaffected.
  let boxes = 0, identified = false;
  const invalid = () => reject("INVALID_AUDIO_CONTAINER", "Malformed ISO edit-list container");
  const unsupported = () => reject("UNSUPPORTED_AUDIO_CAPABILITY", "ISO edit-list semantics are not supported by this decoder profile");
  function walk(start, end, depth, parent, inspect) {
    if (depth > MAX_DEPTH) unsupported();
    for (let at = start; at < end;) {
      if (++boxes > MAX_BOXES) unsupported();
      if (end - at < 8) invalid();
      let header = 8, size = BigInt(view.getUint32(at, false));
      const type = tag(at + 4);
      if (size === 1n) {
        if (end - at < 16) invalid();
        header = 16; size = view.getBigUint64(at + 8, false);
      } else if (size === 0n) size = BigInt(end - at);
      if (size < BigInt(header) || size > BigInt(end - at)) invalid();
      const next = at + Number(size), payload = at + header;
      if (depth === 0 && (type === "ftyp" || type === "moov")) identified = true;
      if (inspect) {
        if ((parent === "root" && type === "moov")
          || (parent === "moov" && type === "trak")
          || (parent === "trak" && type === "edts")) walk(payload, next, depth + 1, type, true);
        else if (parent === "edts" && type === "elst") checkEntries(payload, next);
      }
      at = next;
    }
  }
  function checkEntries(start, end) {
    if (end - start < 8) invalid();
    const version = bytes[start];
    if (version !== 0 && version !== 1) unsupported();
    const count = view.getUint32(start + 4, false), stride = version === 1 ? 20 : 12;
    if (count > MAX_ENTRIES) unsupported();
    if (BigInt(end - start - 8) !== BigInt(count) * BigInt(stride)) invalid();
    let mainEdit = false;
    for (let i = 0, at = start + 8; i < count; i++, at += stride) {
      const duration = version === 1 ? view.getBigUint64(at, false) : BigInt(view.getUint32(at, false));
      const mediaTime = version === 1 ? view.getBigInt64(at + 8, false) : BigInt(view.getInt32(at + 4, false));
      const rate = view.getInt32(at + stride - 4, false);
      if (duration === 0n) continue; // Same ordering as the pinned demuxer.
      if (mainEdit) unsupported(); // It also warns for a trailing nonzero empty edit.
      if (mediaTime === -1n) continue; // Leading empty segments are supported upstream.
      if (rate !== 65536) unsupported();
      mainEdit = true;
    }
  }
  // Identify actual top-level ISO boxes before inspecting their metadata.
  walk(0, bytes.length, 0, "root", false);
  if (!identified) return;
  boxes = 0; walk(0, bytes.length, 0, "root", true);
}
