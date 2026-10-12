import fs from "node:fs/promises";
import { createReadStream } from "node:fs";
import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { moduleRegistry, playwrightRuntime } from "./modules.mjs";
import { executeSourceArtPage } from "./page.mjs";
import { requireFact, algorithm, SourceArtError } from "../src/validation.mjs";
const execute = promisify(execFile);
export async function fileHash(filename) {
  const stat = await fs.stat(filename); requireFact(stat.size > 0 && stat.size <= 1024 * 1024 * 1024);
  const hash = createHash("sha256"); let bytes = 0;
  for await (const chunk of createReadStream(filename)) { bytes += chunk.length; requireFact(bytes <= 1024 * 1024 * 1024); hash.update(chunk); }
  return hash.digest("hex");
}
export async function resolveIdentity(chromium) {
  const modules = await moduleRegistry({});
  const { stdout } = await execute(chromium, ["--version"], { timeout: 10000, maxBuffer: 4096, encoding: "utf8" });
  const version = stdout.match(/\b(\d+(?:\.\d+){3})\b/)?.[1]; requireFact(version);
  return { algorithm: algorithm("waveform"), browser_version: version, browser_sha256: await fileHash(chromium), node_sha256: await fileHash(process.execPath), module_sha256: modules.moduleSha256 };
}
const envelope = async work => { try { return { ok: true, value: await work() }; } catch (e) {
  return { ok: false, error: { code: typeof e?.code === "string" && /^[A-Z][A-Z0-9_]{0,63}$/.test(e.code) ? e.code : "SOURCE_ART_PROTOCOL_ERROR" } };
} };
export const browserHooks = {
  moduleRegistry, playwrightRuntime,
  async configurePage(realm) {
    const resource = realm.begin.plan.resource;
    realm.outputBytes = 0;
    await realm.page.exposeFunction("__opencutSourceArtCallback", (op, args) => envelope(async () => {
      requireFact(!realm.signal.aborted && args && typeof args === "object");
      if (op === "resolve") {
        requireFact(Object.keys(args).length === 1 && args.resource && ["assetId", "contentHash", "kind", "size"].every(key => args.resource[key] === resource[key]));
        return realm.broker.request(op, { resource, handle: realm.begin.resources[0].handle });
      }
      if (op === "read") {
        requireFact(Object.keys(args).length === 3 && typeof args.transfer_id === "string" && /^(0|[1-9][0-9]*)$/.test(args.offset) && Number.isSafeInteger(args.count) && args.count > 0 && args.count <= 65536);
      } else if (op === "release") requireFact(Object.keys(args).length === 1 && typeof args.transfer_id === "string");
      else throw new SourceArtError("SOURCE_ART_PROTOCOL_ERROR", "Unknown source-art callback");
      return realm.broker.request(op, args);
    }));
    await realm.page.exposeFunction("__opencutSourceArtBytes", packet => envelope(async () => {
      requireFact(!realm.signal.aborted && packet.kind === realm.begin.plan.query.kind && packet.offset === String(realm.outputBytes) && typeof packet.bytes_hex === "string" && /^(?:[0-9a-f]{2})+$/.test(packet.bytes_hex) && packet.bytes_hex.length <= 65536);
      const size = packet.bytes_hex.length / 2;
      const limit = packet.kind === "waveform" ? Math.ceil(realm.begin.admission.maxPcmBytes / 128) + 4 : 1024 * 1024;
      requireFact(size <= limit - realm.outputBytes, "Source-art output exceeds byte budget");
      realm.outputBytes += size;
      await realm.writer.send({ type: "art_bytes", ...packet });
    }));
  },
  async observeRuntime(realm, modules) {
    const identity = { algorithm: algorithm(realm.begin.plan.query.kind), browser_version: realm.identity.browserVersion,
      browser_sha256: realm.identity.browserSha256, node_sha256: await fileHash(process.execPath), module_sha256: modules.moduleSha256 };
    requireFact(Object.keys(identity).every(key => identity[key] === realm.begin.plan.identity[key]), "Source-art runtime identity changed");
    realm.sourceArtIdentity = identity;
    return { sourceArtIdentity: identity };
  },
  async render(realm) {
    const result = await realm.page.evaluate(executeSourceArtPage, { ...realm.begin.plan, admission: { ...realm.begin.admission, maxJpegBytes: 1024 * 1024 } });
    if (result?.ok !== true) throw new SourceArtError(result?.error?.code ?? "SOURCE_ART_DECODE_FAILED", "Isolated source-art operation failed");
    requireFact(result.value.output_bytes === realm.outputBytes, "Source-art completion byte count changed");
    return { ...result.value, identity: realm.sourceArtIdentity };
  },
};
