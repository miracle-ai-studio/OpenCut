import { chromium, expect } from "@playwright/test";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";

const binary = path.resolve(process.env.OPENCUT_BINARY ?? "target/release/opencut");
const web = path.resolve(process.env.OPENCUT_WEB_ROOT ?? "apps/web/dist");
// Only mkdtemp-owned data is removed. Failed runs or an explicit keep request
// retain their report/media for diagnosis; success prints the small receipt.
const keepWorkspace = process.env.OPENCUT_KEEP_WORKSPACE === "1";
const directory = await fs.mkdtemp(path.join(os.tmpdir(), "opencut-upload-"));
const file = path.join(directory, "performance.wav");
const dataSize = 64 * 1024 * 1024;
const wav = Buffer.alloc(dataSize + 44);
wav.write("RIFF", 0); wav.writeUInt32LE(dataSize + 36, 4); wav.write("WAVEfmt ", 8);
wav.writeUInt32LE(16, 16); wav.writeUInt16LE(1, 20); wav.writeUInt16LE(2, 22);
wav.writeUInt32LE(48000, 24); wav.writeUInt32LE(192000, 28); wav.writeUInt16LE(4, 32);
wav.writeUInt16LE(16, 34); wav.write("data", 36); wav.writeUInt32LE(dataSize, 40);
await fs.writeFile(file, wav);
const digest = createHash("sha256").update(wav).digest("hex");
const browser = await chromium.launch({ headless: true, executablePath: process.env.OPENCUT_CHROME });
const report = { bytes: wav.length, sha256: digest, platform: process.platform, arch: process.arch, browser: await browser.version(), runs: [], checks: [] };
async function measure(label, executable, webRoot, round) {
  const workspace = path.join(directory, `${label}-${round}`);
  await fs.mkdir(workspace);
  const cli = (...args) => {
    const reply = JSON.parse(execFileSync(executable, ["--json", "--database", path.join(workspace, "workspace.sqlite"),
      "server", "--directory", path.join(workspace, "service"), ...args], { cwd: workspace, encoding: "utf8", timeout: 60000 }));
    if (reply.error !== null) throw new Error(`Server ${args[0]} failed: ${JSON.stringify(reply.error)}`);
    return reply.data;
  };
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1000 } });
  const errors = [];
  page.on("pageerror", e => errors.push(e.message));
  try {
    const state = cli("start", "--listen", "127.0.0.1:0", "--web-root", webRoot);
    await page.addInitScript(() => {
      const timing = window.importTiming = { readBytes: 0, readCalls: 0, firstRead: null, firstPut: null, lastPut: null, seal: null, chunks: 0 };
      const slice = File.prototype.slice;
      File.prototype.slice = function(start = 0, end = this.size, ...rest) {
        timing.firstRead ??= performance.now();
        timing.readBytes += Math.max(0, Math.min(end, this.size) - start);
        timing.readCalls++;
        return slice.call(this, start, end, ...rest);
      };
      const fetch = window.fetch;
      window.fetch = async (...args) => {
        const url = String(args[0]);
        if (url.includes("/chunks/")) { timing.firstPut ??= performance.now(); timing.chunks++; }
        if (/opencut_upload_(seal|stream_complete)/.test(url)) timing.seal = performance.now();
        const response = await fetch(...args);
        if (url.includes("/chunks/")) timing.lastPut = performance.now();
        return response;
      };
    });
    await page.goto(`${state.endpoint}/editor`);
    await page.getByRole("button", { name: "New project", exact: true }).click();
    await page.getByLabel("Project name").fill(`Import performance ${label}`);
    await page.getByRole("button", { name: "Create project", exact: true }).click();
    await page.getByLabel("Media files", { exact: true }).first().setInputFiles(file);
    await expect(page.getByRole("button", { name: "Add performance.wav to timeline", exact: true })).toBeVisible({ timeout: 180000 });
    const timing = await page.evaluate(() => ({ ...window.importTiming, ready: performance.now() }));
    const metrics = { label, round, binarySha256: createHash("sha256").update(await fs.readFile(executable)).digest("hex"), readBytes: timing.readBytes, readCalls: timing.readCalls, chunks: timing.chunks,
      firstChunkMs: timing.firstPut - timing.firstRead, transferMs: timing.seal - timing.firstRead,
      preparationMs: timing.ready - timing.seal, totalMs: timing.ready - timing.firstRead };
    expect(errors).toEqual([]);
    expect(timing.readBytes).toBe(wav.length * (label === "before" ? 2 : 1));
    expect(timing.chunks).toBe(Math.ceil(wav.length / (4 * 1024 * 1024)));
    report.runs.push(metrics);
    console.log(JSON.stringify(metrics));
    if (label === "after" && round === 0) {
      const projectId = new URL(page.url()).pathname.split("/").at(-1);
      const validation = await page.evaluate(async ({ projectId, digest }) => {
        const call = async (name, input) => {
          const r = await fetch(`/v1/tools/${name}`, { method:"POST", headers:{"content-type":"application/json"}, body:JSON.stringify(input) });
          return { status: r.status, ...await r.json() };
        };
        const created = await call("opencut_upload_stream_begin", { project_id:projectId, idempotency_key:crypto.randomUUID(), name:"invalid.wav", size_bytes:"3" });
        if (created.error) throw new Error(JSON.stringify(created));
        const id = created.data.upload_id;
        const bytes = new Uint8Array([1,2,3]);
        const chunkHash = [...new Uint8Array(await crypto.subtle.digest("SHA-256",bytes))].map(b=>b.toString(16).padStart(2,"0")).join("");
        const sent = await fetch(`/v1/stream-uploads/${id}/chunks/0?size_bytes=3&content_hash=${chunkHash}`, {method:"PUT",body:bytes});
        if (!sent.ok) throw new Error(await sent.text());
        const sealed = await call("opencut_upload_stream_complete", {upload_id:id,content_hash:digest});
        const replay = await call("opencut_upload_stream_complete", {upload_id:id,content_hash:digest});
        const conflict = await call("opencut_upload_stream_complete", {upload_id:id,content_hash:"f".repeat(64)});
        let job;
        for(let i=0;i<200;i++) {
          const result = await call("opencut_job_view",{job_id:sealed.data.job_id});
          if (result.error) throw new Error(JSON.stringify(result));
          job=result.data;
          if (["failed","succeeded","cancelled"].includes(job.state)) break;
          await new Promise(r=>setTimeout(r,100));
        }
        const released = await call("opencut_upload_release",{upload_id:id});
        return { sealedJob:sealed.data.job_id, replayJob:replay.data.job_id, conflictStatus:conflict.status, conflict:conflict.error?.code, job, releaseError:released.error };
      }, { projectId, digest });
      expect(validation.replayJob).toBe(validation.sealedJob);
      expect(validation.conflictStatus).toBe(409);
      expect(validation.conflict).toBe("UPLOAD_HASH_CONFLICT");
      expect(validation.job.state).toBe("failed");
      expect(validation.job.failure.code).toBe("ARTIFACT_VERIFICATION_FAILED");
      expect(validation.releaseError).toBeNull();
      report.checks.push("complete replay accepts one job; conflicting digest returns 409; independent worker rejects wrong whole-file digest; terminal staging releases");
    }
  } finally {
    try { await page.close(); }
    finally { expect(cli("stop").state, "Owned service must stop before its workspace can be deleted").toBe("stopped"); }
  }
}
try {
  for (let round = 0; round < 3; round++) {
    if (process.env.OPENCUT_BASELINE_BINARY && process.env.OPENCUT_BASELINE_WEB)
      await measure("before", path.resolve(process.env.OPENCUT_BASELINE_BINARY), path.resolve(process.env.OPENCUT_BASELINE_WEB), round);
    await measure("after", binary, web, round);
  }
  report.status = "passed";
} catch (error) { report.status="failed"; report.error=String(error); process.exitCode=1; }
finally {
  await browser.close();
  report.workspace_retained = report.status !== "passed" || keepWorkspace;
  const output = path.join(directory, "report.json");
  await fs.writeFile(output, JSON.stringify(report,null,2)+"\n");
  if (report.workspace_retained) console.log(output);
  else { await fs.rm(directory, { recursive: true }); console.log(JSON.stringify(report)); }
}
