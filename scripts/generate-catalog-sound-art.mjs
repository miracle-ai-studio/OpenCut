#!/usr/bin/env node
// Offline analysis of the fixed licensed catalog. Classic's standalone audio
// path uses Web Audio decodeAudioData, not video/container concatenation.
// Reuse its peakBuckets and sealed BrowserRealm; never decode in the product UI.
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { acquirePinnedSound } from "../packages/classic-visual-renderer/host/catalog-archive.mjs";
import { BrowserRealm, QuotaMonitor } from "../packages/classic-audio-renderer/host/shared.mjs";
import { browserHooks, resolveIdentity } from "../packages/classic-source-art-renderer/host/browser.mjs";
import { validateAdmission } from "../packages/classic-source-art-renderer/host/runner.mjs";

const script = fileURLToPath(import.meta.url), root = path.resolve(path.dirname(script), "..");
const args = process.argv.slice(2), option = name => {
  const index = args.indexOf(name); assert(index >= 0 && args[index + 1], `Required ${name}`);
  return path.resolve(args[index + 1]);
};
const archivePath = option("--archive"), chromium = option("--chromium"), evidence = option("--evidence"), output = option("--output");
const sha = bytes => createHash("sha256").update(bytes).digest("hex");
const archive = await fs.readFile(archivePath), producer_sha256 = sha(await fs.readFile(script));
const sources = JSON.parse(await fs.readFile(path.join(root, "crates/editor-application/src/catalog_resources/sounds.json"), "utf8"));
assert.equal(sources.length, 51);
const baseIdentity = await resolveIdentity(chromium);
const identity = { ...baseIdentity, decode_profile: "fixed-catalog-webaudio-decode-source-peaks" };
const admission = { quotaProfile: "monitored-terminate", maxEncodedBytes: 33554432,
  maxPcmBytes: 268435456, maxRssBytes: "2147483648", deadlineMs: 60000 };
const controller = new AbortController(), monitor = new QuotaMonitor(admission, error => controller.abort(error), undefined, validateAdmission);
const begin = { runtime: { chromiumExecutable: chromium }, admission,
  plan: { resource: { assetId: "fixed-catalog" }, query: { kind: "waveform", sampleRate: 48000 }, identity: baseIdentity }, resources: [] };
const realm = new BrowserRealm(begin, { request() { throw Error("No project or CAS callback is permitted"); } },
  { async send() {} }, monitor, controller.signal, browserHooks);
const entries = [];
await fs.mkdir(evidence, { recursive: true });
await monitor.start();
try {
  await realm.start();
  for (const [index, source] of sources.entries()) {
    monitor.remainingTimeout(); assert(!controller.signal.aborted);
    const extracted = acquirePinnedSound(archive, `kenney-ui-audio:${source.name}`);
    assert.equal(extracted.sha256, source.sha256); assert.equal(extracted.size, source.size);
    const observed = await realm.page.evaluate(async ({ bytesHex, expectedHash }) => {
      const bytes = Uint8Array.from(bytesHex.match(/../g), b => parseInt(b, 16));
      if (bytes.length > 262144) throw Error("Catalog sound exceeds its fixed encoded budget");
      const actualHash = Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)), b => b.toString(16).padStart(2, "0")).join("");
      if (actualHash !== expectedHash) throw Error("Pinned source changed before decoding");
      const context = new OfflineAudioContext(2, 1, 48000);
      const buffer = await context.decodeAudioData(bytes.buffer.slice());
      if (buffer.length <= 0 || buffer.length > 48000 * 60 || buffer.numberOfChannels > 64) throw Error("Catalog PCM exceeds its fixed decoded budget");
      const { peakBuckets } = await import("/renderer/src/art.mjs");
      const amplitudes = [...peakBuckets(buffer, 268435456)];
      const url = URL.createObjectURL(new Blob([bytes], { type: "audio/ogg" })), media = new Audio(url);
      try {
        media.preload = "metadata";
        await new Promise((resolve, reject) => {
          const timer = setTimeout(() => reject(Error("Catalog media metadata timed out")), 3000);
          media.onloadedmetadata = () => { clearTimeout(timer); resolve(); };
          media.onerror = () => { clearTimeout(timer); reject(Error("Catalog media metadata failed")); };
        });
        if (!Number.isFinite(media.duration) || media.duration <= 0 || media.duration > 60) throw Error("Catalog media duration is outside its bounded source");
        return { sample_rate: buffer.sampleRate, channels: buffer.numberOfChannels, total_samples: String(buffer.length),
          bucket_size: 128, amplitudes, audition_duration_seconds: media.duration };
      } finally { media.pause(); media.removeAttribute("src"); media.load(); URL.revokeObjectURL(url); }
    }, { bytesHex: extracted.bytes.toString("hex"), expectedHash: source.sha256 });
    assert(observed.amplitudes.every(peak => Number.isFinite(peak) && peak >= 0));
    assert.equal(observed.amplitudes.length, Math.ceil(Number(observed.total_samples) / 128));
    const peaks = Buffer.alloc(observed.amplitudes.length * 4);
    observed.amplitudes.forEach((peak, i) => peaks.writeFloatLE(peak, i * 4));
    const entry = { name: source.name, source_sha256: source.sha256, ...observed };
    entries.push(entry);
    await fs.writeFile(path.join(evidence, `${source.name}-receipt.json`), JSON.stringify({ source, producer_sha256, identity,
      sample_rate: entry.sample_rate, channels: entry.channels, total_samples: entry.total_samples,
      audition_duration_seconds: entry.audition_duration_seconds, summary_sha256: sha(peaks) }, null, 2) + "\n", { flag: "wx" });
    await fs.writeFile(path.join(evidence, `${source.name}.ogg`), extracted.bytes, { flag: "wx" });
    process.stdout.write(`${index + 1}/${sources.length} ${source.name}\n`);
  }
} finally { await realm.stop(); monitor.stop(); }
// No metadata publishes before the actual browser process has exited.
assert(!controller.signal.aborted); assert.equal(entries.length, sources.length);
await fs.writeFile(path.join(evidence, "quota.json"), JSON.stringify(monitor.receipt(), null, 2) + "\n", { flag: "wx" });
await fs.writeFile(output, JSON.stringify({ archive_sha256: sha(archive), producer_sha256, identity, entries }, null, 2) + "\n", { flag: "wx" });
