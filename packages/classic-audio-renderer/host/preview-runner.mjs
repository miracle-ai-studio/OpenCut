import fs from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { AudioExecutionError, checkAbort, integer, natural, reject } from "../src/validation.mjs";
import { validateBegin } from "./runner.mjs";
import { PacketReader, PacketWriter, CallbackBroker, validateEnvelope } from "./protocol.mjs";
import { BrowserRealm } from "./browser.mjs";
import { QuotaMonitor } from "./limits.mjs";
import { configurePreviewPage, preparePreviewRealm, nextPreviewRealm } from "./preview-browser.mjs";

export function validatePreviewBegin(packet) {
  if (packet.plan?.identity?.profile !== "classic-realtime-v31-web-audio" || packet.plan?.range != null) reject("INVALID_HOST_PROTOCOL", "Invalid fixed preview profile");
  const copy = structuredClone(packet); copy.plan.identity.profile = "classic-export-v31-web-audio";
  const begin = validateBegin(copy); begin.plan.identity.profile = "classic-realtime-v31-web-audio";
  if (begin.admission.deadlineMs !== 3600000) reject("INVALID_HOST_PROTOCOL", "Preview requires the fixed one-hour TTL");
  integer(begin.preparationDeadlineMs, "preview preparation deadline", 1, 120000);
  for (const voice of begin.plan.voices) if (!["preserve-pts", "prepared-concat"].includes(voice.sourceMode)) reject("INVALID_HOST_PROTOCOL", "Unknown preview source branch");
  return begin;
}
export function validatePreviewNext(packet) {
  if (typeof packet.id !== "string" || !/^[1-9][0-9]{0,19}$/.test(packet.id) || packet.op !== "next") reject("INVALID_HOST_PROTOCOL", "Invalid preview command identity");
  natural(packet.generation, "preview generation"); natural(packet.overlayEpoch, "preview overlay epoch"); natural(packet.startSample, "preview start"); integer(packet.frameCount, "preview block", 1, 4096);
  if (packet.sourceWindows !== undefined && (!Array.isArray(packet.sourceWindows) || packet.sourceWindows.length > 32 || packet.sourceWindows.some(w =>
    !w || Object.keys(w).length !== 3 || typeof w.assetId !== "string" || !w.assetId || !Number.isFinite(w.startSeconds) || !Number.isFinite(w.endSeconds) || w.startSeconds < 0 || w.endSeconds <= w.startSeconds)))
    reject("INVALID_HOST_PROTOCOL", "Invalid captured source windows");
  return structuredClone(packet);
}

export async function runPreviewHost(input = process.stdin, output = process.stdout) {
  const reader = new PacketReader(input), controller = new AbortController();
  let begin, writer, broker, monitor, realm, prepTimer, ready = false, terminal = false, busy = false, ending, initialization;
  let lastId = 0n;
  async function end(failure) {
    if (ending) return ending;
    ending = (async () => {
      terminal = true; clearTimeout(prepTimer);
      controller.abort(failure ?? new AudioExecutionError("CANCELLED", "Preview producer stopped"));
      broker?.close(failure);
      let cause = failure;
      try { await realm?.stop(); } catch (error) { cause = error; }
      monitor?.stop();
      if (writer) await writer.send(cause ? { type: cause.code === "CANCELLED" ? "cancelled" : "failed", error: { code: cause.code ?? "AUDIO_PREVIEW_FAILED", message: "Isolated preview execution failed" }, quota: monitor?.receipt() } : { type: "stopped", quota: monitor.receipt(), identity: realm.identity });
      input.destroy();
    })();
    return ending;
  }
  reader.on("packet", packet => {
    try {
      if (!begin) {
        begin = validatePreviewBegin(packet);
        writer = new PacketWriter(output, begin.executionId); broker = new CallbackBroker(writer);
        initialization = (async () => {
          monitor = new QuotaMonitor(begin.admission, error => void end(error)); await monitor.start(); checkAbort(controller.signal);
          prepTimer = setTimeout(() => void end(new AudioExecutionError("AUDIO_TIME_LIMIT", "Preview preparation deadline expired")), begin.preparationDeadlineMs);
          realm = new BrowserRealm(begin, broker, writer, monitor, controller.signal, { configurePage: configurePreviewPage, ownedLocal: true });
          await realm.start(); checkAbort(controller.signal);
          const receipt = await preparePreviewRealm(realm); checkAbort(controller.signal);
          clearTimeout(prepTimer); ready = true; await writer.send({ type: "prepared", receipt });
        })().catch(error => end(error));
      } else {
        validateEnvelope(packet, begin.executionId);
        if (terminal) return;
        if (packet.type === "reply" || packet.type === "reply_chunk") broker.receive(packet);
        else if (packet.type === "cancel") void end(new AudioExecutionError("CANCELLED", "Preview cancelled by host"));
        else if (packet.type === "command" && packet.op === "stop") void end();
        else if (packet.type === "command") {
          if (!ready || busy) reject("INVALID_HOST_PROTOCOL", "Preview has no free block credit");
          const command = validatePreviewNext(packet), id = BigInt(command.id);
          if (id <= lastId) reject("INVALID_HOST_PROTOCOL", "Preview command ID was reused");
          lastId = id; busy = true;
          void nextPreviewRealm(realm, command).then(receipt => { checkAbort(controller.signal); return writer.send({ type: "block", commandId: command.id, generation: command.generation, overlayEpoch: command.overlayEpoch, startSample: command.startSample, frameCount: command.frameCount, receipt }); }).catch(error => end(error)).finally(() => { busy = false; });
        } else reject("INVALID_HOST_PROTOCOL", "Unknown preview control packet");
      }
    } catch (error) { void end(error).catch(() => { input.destroy(); }); }
  });
  reader.on("failure", error => void end(error));
  reader.on("end", () => { if (!terminal) void end(new AudioExecutionError("HOST_PIPE_CLOSED", "Preview host input closed")); });
  const stop = () => void end(new AudioExecutionError("CANCELLED", "Preview supervisor stopped execution"));
  process.on("SIGTERM", stop); process.on("SIGINT", stop);
  try { await new Promise(resolve => input.once("close", resolve)); await initialization; await ending; }
  finally { clearTimeout(prepTimer); monitor?.stop(); process.off("SIGTERM", stop); process.off("SIGINT", stop); }
}
if (process.argv[1] && await fs.realpath(process.argv[1]).catch(() => undefined) === fileURLToPath(import.meta.url)) await runPreviewHost();
