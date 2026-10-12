import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
export const digest = (text: string) =>
  bytesToHex(sha256(new TextEncoder().encode(text)));
export const legacyId = (label: string, value: unknown) =>
  `classic-${label}-${digest(JSON.stringify(value) ?? "null").slice(0, 32)}`;
export async function fileDigest(file: Blob, signal: AbortSignal) {
  const hash = sha256.create();
  for (let offset = 0; offset < file.size; offset += 4 * 1024 * 1024) {
    signal.throwIfAborted();
    hash.update(
      new Uint8Array(
        await file.slice(offset, offset + 4 * 1024 * 1024).arrayBuffer(),
      ),
    );
  }
  signal.throwIfAborted();
  return bytesToHex(hash.digest());
}
