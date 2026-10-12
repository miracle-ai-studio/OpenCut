import { AudioExecutionError, checkAbort, integer, natural, reject } from "../src/validation.mjs";
import { preparePreviewPage, nextPreviewPage } from "./preview-page.mjs";
import { packPreviewBinding, compactPreviewReceipt } from "./preview-binding.mjs";
import { PROTOCOL_LIMITS } from "./protocol.mjs";

const safeCode = error => typeof error?.code === "string" && /^[A-Z][A-Z0-9_]{0,63}$/.test(error.code) ? error.code : "AUDIO_PREVIEW_FAILED";
export async function previewEnvelope(work) {
  try { return { ok: true, value: await work() }; }
  catch (error) { return { ok: false, error: { code: safeCode(error) } }; }
}
export function unwrapPreviewEnvelope(result) {
  if (result?.ok === true && Object.keys(result).every(key => ["ok", "value"].includes(key))) return result.value;
  if (result?.ok === false && Object.keys(result).length === 2 && Object.keys(result.error ?? {}).length === 1 && /^[A-Z][A-Z0-9_]{0,63}$/.test(result.error?.code)) throw new AudioExecutionError(result.error.code, "Isolated preview operation failed");
  throw new AudioExecutionError("AUDIO_PROTOCOL_ERROR", "Invalid isolated preview result");
}

export async function configurePreviewPage(realm) {
  const fixed = new Map(realm.begin.plan.resources.map(r => [r.assetId, r])), handles = new Map(realm.begin.resources.map(r => [r.assetId, r.handle]));
  const voices = new Set(realm.begin.plan.voices.map(v => v.elementId));
  await realm.page.exposeFunction("__opencutPreviewCallback", (op, args) => previewEnvelope(async () => {
    checkAbort(realm.signal);
    if (!["resolveVerifiedBytes", "readVerifiedBytes", "releaseVerifiedBytes", "admitDecode", "observePreviewDecoded", "compilePitchPreparation", "compileSourcePositions", "bindPreviewSources", "compilePreviewBlock"].includes(op) || !args || typeof args !== "object") reject("INVALID_HOST_CALLBACK", "Unknown preview callback");
    if (["resolveVerifiedBytes", "admitDecode", "observePreviewDecoded"].includes(op)) {
      const resource = fixed.get(args.resource?.assetId);
      if (!resource || ["contentHash", "kind", "size"].some(k => args.resource[k] !== resource[k])) reject("NOT_AUTHORIZED", "Preview resource changed");
      if (op === "admitDecode") {
        const primary = args.primary;
        if (!primary || Object.keys(primary).length !== 4 || typeof primary.codec !== "string" || !/^[a-z0-9][a-z0-9.-]{0,127}$/.test(primary.codec)) reject("INVALID_HOST_CALLBACK", "Invalid preview primary selection");
        integer(primary.trackId, "preview track ID", 0, 0xffffffff); integer(primary.sampleRate, "preview source rate", 1, 384000); integer(primary.channels, "preview source channels", 1, 64);
      }
      if (op === "observePreviewDecoded") {
        integer(args.sampleRate, "observed preview rate", 1, 384000); natural(args.frameCount, "observed preview frames"); integer(args.channels, "observed preview channels", 1, 2);
        args = { resource, sampleRate: args.sampleRate, frameCount: args.frameCount, channels: args.channels };
      } else args = op === "resolveVerifiedBytes" ? { resource, handle: handles.get(resource.assetId) } : { resource, primary: args.primary };
    }
    if (["compilePitchPreparation", "compileSourcePositions"].includes(op) && !voices.has(args.elementId)) reject("NOT_AUTHORIZED", "Preview element changed");
    if (["compileSourcePositions", "compilePreviewBlock"].includes(op)) integer(args.frameCount, "preview callback frames", 1, 4096);
    if (op === "bindPreviewSources") {
      if (Object.keys(args).length !== 1 || !Object.hasOwn(args, "sourceFormats")) reject("INVALID_HOST_CALLBACK", "Invalid preview source binding fields");
      const binding = packPreviewBinding(realm.begin.plan, args.sourceFormats);
      const value = await realm.broker.request("bindPreviewSourcesSharedV1", binding);
      realm.previewBinding = binding;
      return value;
    }
    if (op === "readVerifiedBytes") { natural(args.offset, "preview byte offset"); integer(args.count, "preview byte chunk", 1, 65536); }
    const value = await realm.broker.request(op === "compilePreviewBlock" ? "compilePreviewBlockPackedV2" : op, args);
    if (op === "compileSourcePositions") {
      // Keep the exact authoritative positions, but avoid Playwright recursively
      // marshalling thousands of position objects for every preparation chunk.
      if (!Array.isArray(value) || value.length !== args.frameCount) reject("AUDIO_PROTOCOL_ERROR", "Invalid prepared source positions");
      const text = JSON.stringify(value);
      if (Buffer.byteLength(text, "utf8") > PROTOCOL_LIMITS.replyBytes) reject("AUDIO_BUDGET_EXCEEDED", "Prepared positions exceed the reply budget");
      return text;
    }
    if (op === "compilePreviewBlock") {
      // Native returns its exact compiled JSON text. Do not parse/stringify
      // here: that adds work and normalizes signed-zero floating facts.
      if (typeof value !== "string") reject("AUDIO_PROTOCOL_ERROR", "Missing compiled preview JSON");
      if (Buffer.byteLength(value, "utf8") > PROTOCOL_LIMITS.replyBytes) reject("AUDIO_BUDGET_EXCEEDED", "Compiled preview facts exceed the reply budget");
      return value;
    }
    return value;
  }));
  await realm.page.exposeFunction("__opencutTerminatePreviewRealm", () => realm.stop());
}
export async function preparePreviewRealm(realm) {
  const receipt = unwrapPreviewEnvelope(await realm.page.evaluate(preparePreviewPage, { plan: realm.begin.plan, engineIdentity: realm.identity.engine, admission: realm.begin.admission }));
  return compactPreviewReceipt(realm.begin.plan, receipt, realm.previewBinding);
}
export async function nextPreviewRealm(realm, command) {
  integer(command.frameCount, "preview PCM frames", 1, 4096);
  realm.previewCommand = command;
  try {
    const result = unwrapPreviewEnvelope(await realm.page.evaluate(nextPreviewPage, command));
    checkAbort(realm.signal);
    if (realm.previewCommand !== command || !result || Object.keys(result).length !== 3 || !Object.hasOwn(result, "receipt") ||
        result.frameCount !== command.frameCount || !Array.isArray(result.channelsHex) || result.channelsHex.length !== 2 ||
        result.channelsHex.some(bytes => typeof bytes !== "string" || bytes.length !== command.frameCount * 8 || !/^(?:[0-9a-f]{2})+$/.test(bytes)))
      reject("INVALID_OUTPUT_PCM", "Preview PCM exceeds captured command credit");
    // Admit both channels before publishing either. Recheck cancellation for
    // each unchanged private packet; Rust still fences every received packet.
    for (let channel = 0; channel < 2; channel++) {
      checkAbort(realm.signal);
      await realm.writer.send({ type: "pcm", commandId: command.id, generation: command.generation,
        overlayEpoch: command.overlayEpoch, startSample: command.startSample, channel,
        frames: command.frameCount, bytesHex: result.channelsHex[channel] });
    }
    return result.receipt;
  }
  finally { realm.previewCommand = undefined; }
}
