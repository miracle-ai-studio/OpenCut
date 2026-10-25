import { reject } from "./validation.mjs";

const ISO_FIRST_BOXES = new Set(["ftyp", "moov", "mdat", "free", "skip", "wide", "styp", "moof", "sidx"]);
const MAX_BOXES = 2048, MAX_ENTRIES = 2048, MAX_DEPTH = 4;
const viewOf = bytes => new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
const tagOf = bytes => String.fromCharCode(...bytes.subarray(4, 8));

// One existing capability gate for memory-backed and disk-backed sources.
// Yield only bounded headers/edit entries; mdat is skipped by its declared box
// length. Mediabunny remains the demuxer and source-clock authority stays Rust.
function* inspectEditLists(length) {
  if (length < 8) return;
  const first = yield { start: 0, count: 8 };
  if (!ISO_FIRST_BOXES.has(tagOf(first))) return;
  let boxes = 0, identified = false;
  const invalid = () => reject("INVALID_AUDIO_CONTAINER", "Malformed ISO edit-list container");
  const unsupported = () => reject("UNSUPPORTED_AUDIO_CAPABILITY", "ISO edit-list semantics are not supported by this decoder profile");
  function* walk(start, end, depth, parent, inspect) {
    if (depth > MAX_DEPTH) unsupported();
    for (let at = start; at < end;) {
      if (++boxes > MAX_BOXES) unsupported();
      if (end - at < 8) invalid();
      const bytes = yield { start: at, count: 8 }, view = viewOf(bytes);
      let header = 8, size = BigInt(view.getUint32(0, false));
      const type = tagOf(bytes);
      if (size === 1n) {
        if (end - at < 16) invalid();
        const extended = yield { start: at + 8, count: 8 };
        header = 16; size = viewOf(extended).getBigUint64(0, false);
      } else if (size === 0n) size = BigInt(end - at);
      if (size < BigInt(header) || size > BigInt(end - at)) invalid();
      const next = at + Number(size), payload = at + header;
      if (depth === 0 && (type === "ftyp" || type === "moov")) identified = true;
      if (inspect) {
        if ((parent === "root" && type === "moov") || (parent === "moov" && type === "trak")
            || (parent === "trak" && type === "edts")) yield* walk(payload, next, depth + 1, type, true);
        else if (parent === "edts" && type === "elst") yield* checkEntries(payload, next);
      }
      at = next;
    }
  }
  function* checkEntries(start, end) {
    if (end - start < 8) invalid();
    const header = yield { start, count: 8 }, version = header[0];
    if (version !== 0 && version !== 1) unsupported();
    const count = viewOf(header).getUint32(4, false), stride = version === 1 ? 20 : 12;
    if (count > MAX_ENTRIES) unsupported();
    if (BigInt(end - start - 8) !== BigInt(count) * BigInt(stride)) invalid();
    if (!count) return;
    const entries = yield { start: start + 8, count: count * stride }, view = viewOf(entries);
    let mainEdit = false;
    for (let i = 0, at = 0; i < count; i++, at += stride) {
      const duration = version === 1 ? view.getBigUint64(at, false) : BigInt(view.getUint32(at, false));
      const mediaTime = version === 1 ? view.getBigInt64(at + 8, false) : BigInt(view.getInt32(at + 4, false));
      const rate = view.getInt32(at + stride - 4, false);
      if (duration === 0n) continue;
      if (mainEdit) unsupported();
      if (mediaTime === -1n) continue;
      if (rate !== 65536) unsupported();
      mainEdit = true;
    }
  }
  yield* walk(0, length, 0, "root", false);
  if (!identified) return;
  boxes = 0; yield* walk(0, length, 0, "root", true);
}
export function guardIsoEditLists(bytes) {
  if (!(bytes instanceof Uint8Array)) return;
  const work = inspectEditLists(bytes.length); let next = work.next();
  while (!next.done) { const { start, count } = next.value; next = work.next(bytes.subarray(start, start + count)); }
}
export async function guardIsoEditListsSource(source) {
  const work = inspectEditLists(source.length); let next = work.next();
  while (!next.done) { const { start, count } = next.value; next = work.next(await source.readBytes(start, count)); }
}
