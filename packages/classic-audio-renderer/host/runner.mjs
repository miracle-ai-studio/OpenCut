import { AUDIO_EXECUTION_LIMITS, AudioExecutionError, checkAbort, integer, natural, reject } from "../src/validation.mjs";
import { PacketReader, PacketWriter, CallbackBroker, validateEnvelope } from "./protocol.mjs";
import { QuotaMonitor, validateAdmission } from "./limits.mjs";
import { BrowserRealm } from "./browser.mjs";
import fs from "node:fs/promises";
import { fileURLToPath } from "node:url";

const STDERR_MAX = 16384;
let stderrBytes = 0;
function diagnostic(code) {
  const bytes = Buffer.from(`OpenCut audio host: ${String(code).slice(0, 128)}\n`);
  if (bytes.length <= STDERR_MAX - stderrBytes) { stderrBytes += bytes.length; process.stderr.write(bytes); }
}
export function validateBegin(packet) {
  validateEnvelope(packet);
  if (packet.type !== "begin") reject("INVALID_HOST_PROTOCOL", "The first packet must begin one fixed execution");
  const begin = structuredClone(packet); begin.admission = validateAdmission(begin.admission);
  const plan = begin.plan;
  if (!plan || plan.identity?.profile !== "classic-export-v31-web-audio" || !Array.isArray(plan.resources) || !Array.isArray(plan.voices) || plan.resources.length > AUDIO_EXECUTION_LIMITS.resources || plan.voices.length > AUDIO_EXECUTION_LIMITS.voices || !Array.isArray(begin.resources)) reject("INVALID_HOST_PROTOCOL", "Invalid bounded fixed audio plan");
  const frames = natural(plan.frameCount, "scene frames");
  if (frames === 0n) reject("INVALID_HOST_PROTOCOL", "The fixed scene is empty");
  let outputFrames = frames;
  if (plan.range != null) {
    const start = natural(plan.range?.startSample, "range start"), count = natural(plan.range?.frameCount, "range frames");
    if (count === 0n || start + count > frames) reject("INVALID_HOST_PROTOCOL", "The output range exceeds the fixed whole scene");
    outputFrames = count;
  }
  if (plan.outputFrameCount !== undefined && natural(plan.outputFrameCount, "output frames") !== outputFrames) reject("INVALID_HOST_PROTOCOL", "Output length differs from the fixed range");
  integer(plan.identity.sampleRate, "scene sample rate", 8000, 192000);
  if (["projectId", "sequenceId", "revision"].some(key => typeof plan.identity[key] !== "string" || !plan.identity[key])) reject("INVALID_HOST_PROTOCOL", "Missing fixed audio identity");
  const known = new Set();
  for (const resource of plan.resources) {
    if (typeof resource.assetId !== "string" || !resource.assetId || known.has(resource.assetId) || !/^[a-f0-9]{64}$/.test(resource.contentHash) || !["audio", "video"].includes(resource.kind)) reject("INVALID_HOST_PROTOCOL", "Invalid fixed CAS resource identity");
    integer(resource.size, "encoded resource bytes", 1, begin.admission.maxEncodedBytes); known.add(resource.assetId);
  }
  const handles = new Set();
  for (const resource of begin.resources) {
    if (!known.has(resource.assetId) || handles.has(resource.assetId) || typeof resource.handle !== "string" || !resource.handle || resource.handle.length > 1024) reject("INVALID_HOST_PROTOCOL", "Private resource handles do not match the fixed plan");
    handles.add(resource.assetId);
  }
  if (handles.size !== known.size) reject("INVALID_HOST_PROTOCOL", "Every fixed resource requires its private host handle");
  const voices = new Set();
  for (const voice of plan.voices) {
    if (!known.has(voice.assetId) || typeof voice.elementId !== "string" || !voice.elementId || voices.has(voice.elementId)) reject("INVALID_HOST_PROTOCOL", "Invalid fixed voice binding");
    voices.add(voice.elementId);
  }
  return begin;
}
export async function runHost(input = process.stdin, output = process.stdout, hooks = {}) {
  let begin, writer, broker, monitor, realm, work, terminal = false;
  const controller = new AbortController(), reader = new PacketReader(input);
  function abort(error) {
    if (terminal || controller.signal.aborted) return;
    controller.abort(error); broker?.close(error);
    if (!begin) { input.destroy(); return; }
    void realm?.stop().catch(() => {});
  }
  async function execute() {
    try {
      monitor = new QuotaMonitor(begin.admission, abort, undefined, hooks.validateAdmission); await monitor.start(); checkAbort(controller.signal);
      realm = new BrowserRealm(begin, broker, writer, monitor, controller.signal, hooks.browser);
      await realm.start(); checkAbort(controller.signal);
      const receipt = await realm.render(); checkAbort(controller.signal);
      await realm.stop(); checkAbort(controller.signal);
      monitor.stop(); broker.close();
      // PCM is tentative until this terminal packet follows process exit.
      await writer.send({ type: "complete", receipt, quota: monitor.receipt() });
    } catch (error) {
      let failure = controller.signal.aborted ? controller.signal.reason : error;
      if (hooks.executionName === "visual" && typeof failure?.code === "string") failure = { ...failure, code: failure.code.replaceAll("AUDIO", "VISUAL") };
      broker.close(failure);
      try { await realm?.stop(); } catch (termination) { failure = termination; }
      monitor?.stop();
      const quota = monitor?.receipt();
      if (failure?.code === "CANCELLED") await writer.send({ type: "cancelled", ...(quota ? { quota } : {}) });
      else {
        const code = typeof failure?.code === "string" ? failure.code.slice(0, 128) : "AUDIO_BROWSER_EXECUTION_FAILED";
        diagnostic(code);
        await writer.send({ type: "failed", error: { code, message: `Isolated browser ${hooks.executionName ?? "audio"} execution failed` }, ...(quota ? { quota } : {}) });
      }
    } finally { terminal = true; monitor?.stop(); input.destroy(); }
  }
  reader.on("packet", packet => {
    try {
      if (!begin) {
        begin = (hooks.validateBegin ?? validateBegin)(packet); writer = new PacketWriter(output, begin.executionId); broker = new CallbackBroker(writer);
        work = execute().catch(error => { diagnostic(error.code ?? "HOST_PIPE_CLOSED"); process.exitCode = 1; });
      } else {
        validateEnvelope(packet, begin.executionId);
        if (terminal) reject("INVALID_HOST_PROTOCOL", "Execution has already terminated");
        if (packet.type === "cancel") abort(new AudioExecutionError("CANCELLED", "Rust host cancelled the fixed audio execution"));
        else if (packet.type === "reply" || packet.type === "reply_chunk") { if (!controller.signal.aborted) broker.receive(packet); }
        else reject("INVALID_HOST_PROTOCOL", "Only replies and cancellation may follow begin");
      }
    } catch (error) { if (begin && writer) abort(error); else { diagnostic(error.code ?? "INVALID_HOST_PROTOCOL"); process.exitCode = 1; input.destroy(); } }
  });
  reader.on("failure", error => { if (begin) abort(error); else { diagnostic(error.code ?? "INVALID_HOST_PROTOCOL"); process.exitCode = 1; input.destroy(); } });
  reader.on("end", () => {
    if (!terminal) abort(new AudioExecutionError("HOST_PIPE_CLOSED", "Rust host closed stdin before completion"));
    if (!begin) { process.exitCode = 1; input.destroy(); }
  });
  const stopSignal = () => abort(new AudioExecutionError("CANCELLED", "Native supervisor stopped audio execution"));
  process.on("SIGTERM", stopSignal); process.on("SIGINT", stopSignal);
  try {
    await new Promise(resolve => input.once("close", resolve));
    await work;
  } finally { process.off("SIGTERM", stopSignal); process.off("SIGINT", stopSignal); }
}

if (process.argv[1] && await fs.realpath(process.argv[1]).catch(() => undefined) === fileURLToPath(import.meta.url)) await runHost();
