import { test } from "node:test";
import assert from "node:assert/strict";
import { deflateRawSync } from "node:zlib";
import { acquirePinnedSound, crc32, extractZipEntry } from "../host/catalog-archive.mjs";
function zip(name = "Audio/click1.ogg", body = Buffer.from("verified archive member"), changes = {}) {
  const data = deflateRawSync(body), text = Buffer.from(name), local = Buffer.alloc(30), central = Buffer.alloc(46), end = Buffer.alloc(22);
  local.writeUInt32LE(0x04034b50); local.writeUInt16LE(20, 4); local.writeUInt16LE(changes.flags ?? 0, 6); local.writeUInt16LE(8, 8); local.writeUInt32LE(crc32(body), 14); local.writeUInt32LE(data.length, 18); local.writeUInt32LE(changes.size ?? body.length, 22); local.writeUInt16LE(text.length, 26);
  central.writeUInt32LE(0x02014b50); central.writeUInt16LE(0x0314, 4); central.writeUInt16LE(20, 6); central.writeUInt16LE(changes.flags ?? 0, 8); central.writeUInt16LE(8, 10); central.writeUInt32LE(crc32(body), 16); central.writeUInt32LE(data.length, 20); central.writeUInt32LE(changes.size ?? body.length, 24); central.writeUInt16LE(text.length, 28); central.writeUInt32LE(((changes.mode ?? 0x81a4) * 65536) >>> 0, 38);
  end.writeUInt32LE(0x06054b50); end.writeUInt16LE(1, 8); end.writeUInt16LE(1, 10); end.writeUInt32LE(central.length + text.length, 12); end.writeUInt32LE(local.length + text.length + data.length, 16);
  return Buffer.concat([local, text, data, central, text, end]);
}
test("bounded inflater returns exact member bytes, size, CRC and digest", () => {
  const actual = extractZipEntry(zip(), "Audio/click1.ogg"); assert.equal(actual.bytes.toString(), "verified archive member"); assert.equal(actual.size, actual.bytes.length); assert.equal(actual.crc32, crc32(actual.bytes)); assert.match(actual.sha256, /^[a-f0-9]{64}$/);
  assert.equal(crc32(Buffer.from("123456789")), 0xcbf43926);
});
test("rejects path traversal, absolute, alternate separators, symlinks and encryption", () => {
  for (const name of ["../Audio/a.ogg", "/Audio/a.ogg", "Audio\\a.ogg", "Audio/./a.ogg", "Audio//a.ogg", "C:/a.ogg"]) assert.throws(() => extractZipEntry(zip(name), name));
  assert.throws(() => extractZipEntry(zip("Audio/a.ogg", Buffer.from("x"), { mode: 0xa1ff }), "Audio/a.ogg"));
  assert.throws(() => extractZipEntry(zip("Audio/a.ogg", Buffer.from("x"), { flags: 1 }), "Audio/a.ogg"));
});
test("rejects expanded budgets, wrong CRC, truncated directories and extra bytes", () => {
  assert.throws(() => extractZipEntry(zip("a", Buffer.alloc(262145)), "a"));
  const corrupt = zip(); corrupt.writeUInt32LE(123, 14); assert.throws(() => extractZipEntry(corrupt, "Audio/click1.ogg"));
  assert.throws(() => extractZipEntry(zip().subarray(0, -1), "Audio/click1.ogg"));
  assert.throws(() => extractZipEntry(Buffer.concat([zip(), Buffer.from("extra")]), "Audio/click1.ogg"));
});
test("public fixed acquisition rejects every synthetic archive and arbitrary source", () => {
  assert.throws(() => acquirePinnedSound(zip(), "kenney-ui-audio:click1"));
  for (const id of ["https://example.test/a", "kenney-ui-audio:click9", "kenney-ui-audio:../License"]) assert.throws(() => acquirePinnedSound(zip(), id));
});
