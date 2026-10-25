/** Independent acceptance reference: the unchanged whole-source decoder plus
 * an explicit analytic source clock. Its larger oracle workspace is never a
 * product admission or a substitute for the normal256MiB public render job. */
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { BrowserRealm } from "../../../packages/classic-audio-renderer/host/browser.mjs";
import { QuotaMonitor } from "../../../packages/classic-audio-renderer/host/limits.mjs";

const args = Object.fromEntries(process.argv.slice(2).reduce((all, value, index, values) =>
  index % 2 ? all : [...all, [value.replace(/^--/, ""), values[index + 1]]], []));
const workspace = path.resolve(args.workspace), reportPath = path.resolve(args.report);
await fs.access(path.join(workspace, "workspace.sqlite"));
const source = path.join(workspace, "agent-long-vfr-300s.mp4"), bytes = await fs.readFile(source);
const sha = value => createHash("sha256").update(value).digest("hex");
const sourceHash = sha(bytes);
if (sourceHash !== "052d9162da2d7797400e3184ca923e349961c01689fceb8644bbf634f87b8597")
  throw Error("Reuse the original independently authored five-minute fixture");
const resource = { assetId: "long-source", kind: "video", size: bytes.length, contentHash: sourceHash };
const admission = { maxPcmBytes: 512 * 1024 * 1024, maxEncodedBytes: 32 * 1024 * 1024,
  maxRssBytes: "2147483648", deadlineMs: 120000, quotaProfile: "monitored-terminate", monitorIntervalMs: 100 };
const begin = { type: "begin", resources: [{ assetId: resource.assetId, handle: "oracle-source" }], plan: { resources: [resource], voices: [] }, admission,
  runtime: { chromiumExecutable: process.env.OPENCUT_CHROME } };
const controller = new AbortController(); let realm;
const monitor = new QuotaMonitor(admission, error => { controller.abort(error); void realm?.stop(); });
await monitor.start();
realm = new BrowserRealm(begin, { request() { throw Error("Reference must not call an application port"); } },
  { async send() {} }, monitor, controller.signal, { ownedLocal: true });
try {
  await realm.start();
  const pitch = args.pitch === "true";
  const pts = args.pts === "true";
  if (pts && pitch) throw Error("Select one original source algorithm per reference");
  const result = await realm.page.evaluate(async ({ resource, sourceHash, encodedBase64, pitch, pts }) => {
    const { decodeResource } = await import("/renderer/src/decoder.mjs");
    const { decodePreviewResource } = await import("/renderer/src/preview-decoder.mjs");
    const { prepareVoice } = await import("/renderer/src/preparation.mjs");
    const { PcmBudget } = await import("/renderer/src/validation.mjs");
    const encoded = Uint8Array.fromBase64(encodedBase64);
    const observedHash = new Uint8Array(await crypto.subtle.digest("SHA-256", encoded)).toHex();
    if (observedHash !== sourceHash) throw Error("Reference bytes changed");
    const context = new AudioContext({ sampleRate: 48000 });
    const budget = new PcmBudget(512 * 1024 * 1024), releaseEncoded = budget.reserve(encoded.length * 3);
    let decoded, prepared;
    try {
      // Deliberately omit the new optional window capability: this is the old
      // concatenate-all-chunks + one OfflineAudioContext decoder implementation.
      const decoder = pts ? decodePreviewResource : decodeResource;
      decoded = await decoder(resource, encoded, context, {
        admitDecode: async (_resource, primary) => {
          if (primary.sampleRate !== 44100 || primary.channels !== 2) throw Error("Original primary format changed");
          return { maxFrames: 301 * 48000, maxChannels: 2 };
        },
        createOfflineContext: (channels, frames, rate) => new OfflineAudioContext(channels, frames, rate),
        abortRendering: async () => context.close(),
      }, budget);
      let buffer = decoded.buffer;
      if (pitch) {
        // Independently authored oracle: source296s, four source seconds at5/4
        // gives3.2s prepared output. The original whole-buffer slice path and
        // pinned SoundTouch execute without the new sparse storage capability.
        const facts = { mode: "prepared-soundtouch", sourceSliceStartSample: "14208000",
          requestedSourceFrames: "192000", actualSourceFrames: "192000", resampledFrames: "192000",
          outputFrames: "153600", numChannels: 2, decodedSampleRate: 48000, targetSampleRate: 48000,
          tempo: { numerator: "5", denominator: "4" }, prefixFrames: "14400000",
          decodedBytes: String(buffer.length * 8), preparedBytes: "4300800",
          totalBytes: String(buffer.length * 8 + 4300800), totalChannelFrames: String(buffer.length * 2 + 1075200) };
        prepared = await prepareVoice({ elementId: "legacy-reference" }, buffer, context, {
          compilePitchPreparation: async () => facts,
          createOfflineContext: (channels, frames, rate) => new OfflineAudioContext(channels, frames, rate),
          abortRendering: async () => context.close(),
        }, budget);
        buffer = prepared.buffer;
      }
      const pcm = new Float32Array(48000 * 2);
      const nativeSegments = pts ? decoded.facts.map(fact => ({start: Math.round(fact.timestampSeconds * decoded.sampleRate), buffer: decoded.segments.get(fact.segmentId)})) : [];
      let segmentIndex = 0;
      // Original authored source296s at scene2s,1.25x; selected scene4.2..5.2s
      // independently gives source298.75..300s. No product clock is called.
      for (let frame = 0; frame < 48000; frame++) {
        if (pts) {
          // Authored scene4.2s -> source298.75s at5/4. Native44.1k
          // positions have exact147/128 steps on the48k output grid.
          // The original profile clamps its upper tap within each PTS packet.
          const position = 13174875 + frame * 147 / 128, lower = Math.floor(position), fraction = position - lower;
          while (segmentIndex + 1 < nativeSegments.length && nativeSegments[segmentIndex + 1].start <= lower) segmentIndex++;
          const segment = nativeSegments[segmentIndex], local = lower - segment.start;
          for (let channel = 0; channel < 2; channel++) {
            const data = segment.buffer.getChannelData(channel);
            pcm[frame * 2 + channel] = local < 0 || local >= data.length ? 0
              : data[local] * (1 - fraction) + data[Math.min(data.length - 1, local + 1)] * fraction;
          }
          continue;
        }
        const position = pitch ? 105600 + frame : 14340000 + frame * 5 / 4, lower = Math.floor(position), fraction = position - lower;
        for (let channel = 0; channel < 2; channel++) {
          const data = buffer.getChannelData(channel);
          pcm[frame * 2 + channel] = lower >= data.length ? 0
            : data[lower] * (1 - fraction) + data[Math.min(data.length - 1, lower + 1)] * fraction;
        }
      }
      return { bytesHex: new Uint8Array(pcm.buffer).toHex(), logicalFrames: pts ? Number(decoded.frameCount) : decoded.buffer.length,
        decoderProfile: pts ? "mediabunny-1.41.0-preserve-pts" : decoded.profile, primary: decoded.primary, peakAccountedBytes: budget.peak };
    } finally { prepared?.release(); decoded?.release(); releaseEncoded(); await context.close(); }
  }, { resource, sourceHash, encodedBase64: bytes.toString("base64"), pitch, pts });
  const pcm = Buffer.from(result.bytesHex, "hex");
  if (pcm.length !== 48000 * 2 * 4) throw Error("Reference PCM length changed");
  const output = path.join(workspace, pts ? "agent-long-reference-legacy-pts-pcm.f32" : pitch ? "agent-long-reference-legacy-pitch-pcm.f32" : "agent-long-reference-legacy-pcm.f32");
  await fs.writeFile(output, pcm);
  const report = { status: "reference_generated", role: "oracle_only_not_product_policy_pass",
    source_sha256: sourceHash, pcm_sha256: sha(pcm), pcm_path: output, frames: 48000, channels: 2,
    sample_rate: 48000, source_start_seconds: 298.75, source_rate: "5/4", logical_frames: result.logicalFrames,
    decoder_profile: result.decoderProfile, pitch_mode: pts ? "preserve-pts" : pitch ? "prepared-soundtouch" : "unprepared-linear",
    primary: result.primary, peak_accounted_bytes: result.peakAccountedBytes,
    reference_admission: admission, runtime: realm.identity, quota: monitor.receipt(),
    scope: "Legacy whole-source decoder compared with windowed output; separate known colors/frequencies supply the independent A/V oracle" };
  await fs.writeFile(reportPath, JSON.stringify(report, null, 2) + "\n");
  console.log(JSON.stringify({ status: report.status, pcm_path: output, logical_frames: report.logical_frames }));
} finally { await realm.stop(); monitor.stop(); }
