import fs from "node:fs/promises";
import path from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { AudioExecutionError, integer, natural, reject } from "../src/validation.mjs";
const execute = promisify(execFile);

export function validateAdmission(value) {
  if (!value || !["monitored-terminate", "strict-hard"].includes(value.quotaProfile)) reject("INVALID_AUDIO_ADMISSION", "An explicit browser quota profile is required");
  integer(value.maxPcmBytes, "admitted PCM bytes", 4, 1024 * 1024 * 1024);
  integer(value.maxEncodedBytes, "admitted encoded bytes", 1, 32 * 1024 * 1024);
  integer(value.deadlineMs, "browser deadline", 1, 3600000);
  const maxRss = natural(value.maxRssBytes, "browser RSS quota");
  if (maxRss < 1048576n || maxRss > 8589934592n) reject("INVALID_AUDIO_ADMISSION", "Browser RSS quota is outside the host profile bounds");
  const interval = integer(value.monitorIntervalMs ?? 100, "RSS monitor interval", 20, 250);
  return { ...structuredClone(value), monitorIntervalMs: interval };
}
async function strictCgroup(admission) {
  if (process.platform !== "linux" || typeof admission.cgroupDirectory !== "string" || !path.isAbsolute(admission.cgroupDirectory)) reject("UNSUPPORTED_AUDIO_QUOTA", "strict-hard requires an existing effective Linux cgroup v2 quota");
  const root = await fs.realpath("/sys/fs/cgroup"), directory = await fs.realpath(admission.cgroupDirectory);
  if (directory !== root && !directory.startsWith(root + path.sep)) reject("UNSUPPORTED_AUDIO_QUOTA", "Quota directory is outside the cgroup v2 mount");
  if ((await fs.statfs(directory)).type !== 0x63677270) reject("UNSUPPORTED_AUDIO_QUOTA", "Quota directory is not a cgroup v2 filesystem");
  const membership = (await fs.readFile("/proc/self/cgroup", "utf8")).split("\n").find(v => v.startsWith("0::"))?.slice(3);
  if (!membership) reject("UNSUPPORTED_AUDIO_QUOTA", "Cannot establish runner cgroup membership");
  const current = await fs.realpath(path.join(root, membership));
  if (current !== directory && !current.startsWith(directory + path.sep)) reject("UNSUPPORTED_AUDIO_QUOTA", "Runner is not inside the admitted cgroup");
  const maximum = (await fs.readFile(path.join(directory, "memory.max"), "utf8")).trim();
  if (maximum === "max" || natural(maximum, "kernel memory quota") > natural(admission.maxRssBytes, "admitted RSS quota")) reject("UNSUPPORTED_AUDIO_QUOTA", "Kernel memory.max does not enforce the admitted quota");
  return directory;
}
export function processTreeBytes(text, rootPid, browserPid) {
  const rows = text.trim().split("\n").filter(Boolean).map(line => {
    const fields = line.trim().split(/\s+/).map(Number);
    if (fields.length !== 4 || fields.some(v => !Number.isSafeInteger(v) || v < 0)) reject("AUDIO_RSS_MONITOR_FAILED", "OS process snapshot is invalid");
    return { pid: fields[0], parent: fields[1], group: fields[2], bytes: BigInt(fields[3]) * 1024n };
  });
  const roots = new Set([rootPid, ...(browserPid ? [browserPid] : [])]);
  const reportedGroup = rows.find(row => row.pid === browserPid)?.group;
  // Include reparented browser children only in a dedicated browser group.
  // Never count unrelated siblings in a shared supervisor process group.
  const browserGroup = reportedGroup === browserPid ? reportedGroup : undefined;
  let changed = true;
  while (changed) {
    changed = false;
    for (const row of rows) if (roots.has(row.parent) || browserGroup && row.group === browserGroup) {
      if (!roots.has(row.pid)) { roots.add(row.pid); changed = true; }
    }
  }
  if (!rows.some(row => row.pid === rootPid)) reject("AUDIO_RSS_MONITOR_FAILED", "OS process snapshot omitted the audio runner");
  return rows.reduce((sum, row) => sum + (roots.has(row.pid) ? row.bytes : 0n), 0n);
}
export class QuotaMonitor {
  constructor(admission, abort, now = () => performance.now(), validate = validateAdmission) {
    this.admission = validate(admission); this.abort = abort; this.now = now; this.started = now(); this.deadlineAt = this.started + this.admission.deadlineMs; this.peak = 0n; this.browserPid = undefined; this.stopped = false;
  }
  remainingTimeout(maximumMs = 10000) {
    // Never let Playwright's zero timeout disable the remaining deadline.
    const remaining = Math.floor(this.deadlineAt - this.now());
    if (remaining < 1) {
      const error = new AudioExecutionError("AUDIO_DEADLINE_EXCEEDED", "Browser audio execution exceeded its deadline");
      this.fail(error.code, error.message);
      throw error;
    }
    return Math.min(integer(maximumMs, "operation timeout cap", 1), remaining);
  }
  async start() {
    this.deadline = setTimeout(() => this.fail("AUDIO_DEADLINE_EXCEEDED", "Browser audio execution exceeded its deadline"), this.remainingTimeout(this.admission.deadlineMs));
    if (this.admission.quotaProfile === "strict-hard") this.cgroup = await strictCgroup(this.admission);
    this.remainingTimeout();
    await this.sample();
  }
  setBrowserPid(pid) { this.browserPid = integer(pid, "browser PID", 1); }
  async sample() {
    if (this.stopped) return;
    try {
      const bytes = this.cgroup
        ? natural((await fs.readFile(path.join(this.cgroup, "memory.current"), "utf8")).trim(), "kernel memory usage")
        : processTreeBytes((await execute("/bin/ps", ["-axo", "pid=,ppid=,pgid=,rss="], { timeout: 1000, maxBuffer: 4 * 1024 * 1024, encoding: "utf8" })).stdout, process.pid, this.browserPid);
      this.peak = bytes > this.peak ? bytes : this.peak;
      if (bytes > natural(this.admission.maxRssBytes, "admitted RSS quota")) this.fail("AUDIO_RSS_QUOTA_EXCEEDED", "Audio execution exceeded the admitted process memory quota");
    } catch (error) { this.fail(error.code === "UNSUPPORTED_AUDIO_QUOTA" ? error.code : "AUDIO_RSS_MONITOR_FAILED", "Could not enforce the browser memory monitor"); }
    if (!this.stopped) this.timer = setTimeout(() => void this.sample(), this.admission.monitorIntervalMs);
  }
  fail(code, message) { if (!this.stopped) { this.stop(); this.abort(new AudioExecutionError(code, message)); } }
  stop() { this.stopped = true; clearTimeout(this.deadline); clearTimeout(this.timer); }
  receipt() {
    return { profile: this.admission.quotaProfile, maxRssBytes: this.admission.maxRssBytes, observedPeakRssBytes: String(this.peak), monitorIntervalMs: this.admission.monitorIntervalMs, elapsedMs: Math.round(this.now() - this.started), hardMemoryEnforced: Boolean(this.cgroup) };
  }
}
