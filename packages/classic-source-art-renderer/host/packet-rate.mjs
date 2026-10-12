// Private Node-only source adapter. Packet statistics remain Mediabunny's algorithm.
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { pathToFileURL } from "node:url";

const [bundle, expectedHash, filename] = process.argv.slice(2);
let handle, input, closing;
const close = () => closing ??= handle?.close();
try {
  if (process.argv.length !== 5 || !path.isAbsolute(bundle ?? "") || !path.isAbsolute(filename ?? "")
      || !/^[a-f0-9]{64}$/.test(expectedHash ?? "")) throw new Error("Invalid fixed packet-rate invocation");
  const moduleStat = await fs.stat(bundle);
  if (!moduleStat.isFile() || moduleStat.size <= 0 || moduleStat.size > 8 * 1024 * 1024) throw new Error("Invalid packet-rate module");
  const moduleBytes = await fs.readFile(bundle);
  if (createHash("sha256").update(moduleBytes).digest("hex") !== expectedHash) throw new Error("Packet-rate module identity changed");
  const { Input, StreamSource, ALL_FORMATS } = await import(pathToFileURL(bundle).href);
  handle = await fs.open(filename, "r");
  const stat = await handle.stat();
  if (!stat.isFile() || !Number.isSafeInteger(stat.size) || stat.size <= 0) throw new Error("Invalid fixed packet-rate input");
  const source = new StreamSource({
    getSize: () => stat.size,
    async read(start, end) {
      if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 0 || end < start || end > stat.size) throw new Error("Invalid packet-rate read range");
      const bytes = new Uint8Array(end - start);
      let offset = 0;
      while (offset < bytes.length) {
        const { bytesRead } = await handle.read(bytes, offset, bytes.length - offset, start + offset);
        if (!bytesRead) throw new Error("Fixed packet-rate input ended early");
        offset += bytesRead;
      }
      return bytes;
    },
    dispose: close,
    maxCacheSize: 8 * 1024 * 1024,
    prefetchProfile: "fileSystem",
  });
  input = new Input({ source, formats: ALL_FORMATS });
  const track = await input.getPrimaryVideoTrack();
  const stats = track ? await track.computePacketStats(100) : { packetCount: 0, averagePacketRate: 0 };
  if (!Number.isSafeInteger(stats.packetCount) || stats.packetCount < 0) throw new Error("Invalid packet-rate count");
  const rate = stats.packetCount > 0 && Number.isFinite(stats.averagePacketRate) && stats.averagePacketRate > 0
    ? stats.averagePacketRate : null;
  process.stdout.write(JSON.stringify({ schema_version: 1, packet_count: stats.packetCount, average_packet_rate: rate }) + "\n");
} catch {
  process.stderr.write("Classic packet-rate probing failed\n");
  process.exitCode = 1;
} finally {
  input?.dispose();
  await close();
}
