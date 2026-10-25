import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { runHost as sharedHost, validateEnvelope } from "../../classic-audio-renderer/host/shared.mjs";
import { browserHooks, resolveIdentity } from "./browser.mjs";
import { requireFact, algorithm, decodeProfile } from "../src/validation.mjs";
export function validateAdmission(value) {
  requireFact(value && value.quotaProfile === "monitored-terminate" && Number.isSafeInteger(value.deadlineMs) && value.deadlineMs > 0 && value.deadlineMs <= 60000);
  for (const [key, max] of [["maxEncodedBytes", 32 * 1024 * 1024], ["maxPcmBytes", 256 * 1024 * 1024]]) requireFact(Number.isSafeInteger(value[key]) && value[key] > 0 && value[key] <= max);
  requireFact(typeof value.maxRssBytes === "string" && /^[1-9][0-9]*$/.test(value.maxRssBytes) && BigInt(value.maxRssBytes) <= 2147483648n);
  return { ...value, monitorIntervalMs: 100 };
}
export function validateBegin(packet) {
  validateEnvelope(packet); requireFact(packet.type === "begin");
  const begin = structuredClone(packet); begin.admission = validateAdmission(begin.admission);
  const plan = begin.plan, resource = plan?.resource;
  requireFact(plan && ["waveform", "thumbnail", "source_frame"].includes(plan.query?.kind) && plan.identity?.algorithm === algorithm(plan.query.kind) && plan.identity.decode_profile === decodeProfile);
  if (plan.query.kind === "source_frame") requireFact(Number.isFinite(plan.query.seconds) && plan.query.seconds >= 0);
  requireFact(resource && typeof resource.assetId === "string" && resource.assetId.length > 0 && /^[a-f0-9]{64}$/.test(resource.contentHash)
    && ["audio", "video", "image"].includes(resource.kind) && Number.isSafeInteger(resource.size) && resource.size > 0 && (["audio", "video"].includes(resource.kind) || resource.size <= begin.admission.maxEncodedBytes));
  if (["audio", "video"].includes(resource.kind)) {
    requireFact(Number.isSafeInteger(plan.rangeCacheBytes) && plan.rangeCacheBytes >= 65536 && plan.rangeCacheBytes <= 8 * 1024 * 1024);
    const resident = Math.min(plan.rangeCacheBytes, resource.size) * 3 + 6 * 65536;
    requireFact(resident <= begin.admission.maxEncodedBytes && resident <= begin.admission.maxPcmBytes);
    if (plan.query.kind === "waveform") {
      const working = Math.floor((begin.admission.maxPcmBytes - resident) / 2);
      requireFact(working >= 65536 && plan.waveformWorkingBytes === working && plan.maxWaveformBytes === Math.floor(working / 12) * 4);
    }
  }
  requireFact(plan.query.kind === "waveform" ? ["audio", "video"].includes(resource.kind) && [44100, 48000].includes(plan.query.sampleRate) : ["video", "image"].includes(resource.kind));
  requireFact(Array.isArray(begin.resources) && begin.resources.length === 1 && begin.resources[0].assetId === resource.assetId && typeof begin.resources[0].handle === "string");
  return begin;
}
export const runHost = (input = process.stdin, output = process.stdout) => sharedHost(input, output, { validateBegin, validateAdmission, browser: browserHooks, executionName: "source-art" });
if (process.argv[1] && await fs.realpath(process.argv[1]).catch(() => undefined) === fileURLToPath(import.meta.url)) {
  if (process.argv[2] === "--identity") {
    requireFact(process.argv.length === 4 && path.isAbsolute(process.argv[3]));
    process.stdout.write(JSON.stringify(await resolveIdentity(process.argv[3])) + "\n");
  } else await runHost();
}
