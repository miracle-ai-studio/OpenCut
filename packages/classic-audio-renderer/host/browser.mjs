import fs from "node:fs/promises";
import { createReadStream } from "node:fs";
import { createHash } from "node:crypto";
import path from "node:path";
import { AudioExecutionError, checkAbort, integer, natural, reject } from "../src/validation.mjs";
import { ORIGIN, moduleRegistry, moduleResponse, playwrightRuntime } from "./modules.mjs";
import { executeAudioPage } from "./page.mjs";
import { launchOwnedBrowser } from "./owned-browser.mjs";
import { compiledFactsJson } from "./protocol.mjs";

export class BrowserRealm {
  constructor(begin, broker, writer, monitor, signal, hooks = {}) { Object.assign(this, { begin, broker, writer, monitor, signal, hooks }); }
  async start() {
    checkAbort(this.signal); this.monitor.remainingTimeout();
    const runtime = this.begin.runtime;
    if (!runtime || typeof runtime.chromiumExecutable !== "string" || !path.isAbsolute(runtime.chromiumExecutable)) reject("INVALID_AUDIO_RUNTIME", "Chromium executable must be an explicit absolute host path");
    const executable = await fs.realpath(runtime.chromiumExecutable);
    const stat = await fs.stat(executable); integer(stat.size, "Chromium executable size", 1, 1024 * 1024 * 1024);
    const hash = createHash("sha256");
    for await (const chunk of createReadStream(executable)) { checkAbort(this.signal); hash.update(chunk); }
    const browserSha256 = hash.digest("hex");
    if (runtime.chromiumSha256 !== undefined && runtime.chromiumSha256 !== browserSha256) reject("AUDIO_RUNTIME_IDENTITY_MISMATCH", "Chromium executable does not match the admitted hash");
    const [modules, { chromium }] = await Promise.all([(this.hooks.moduleRegistry ?? moduleRegistry)(runtime), (this.hooks.playwrightRuntime ?? playwrightRuntime)(runtime)]);
    checkAbort(this.signal);this.fixedModules=modules;
    const options = {
      executablePath: executable, headless: true, chromiumSandbox: true, timeout: this.monitor.remainingTimeout(),
      args: ["--disable-background-networking", "--disable-component-update", "--disable-sync", "--disable-default-apps", "--disable-crash-reporter", "--no-first-run", "--no-proxy-server", "--host-resolver-rules=MAP * ~NOTFOUND"+(this.hooks.loopbackTransport?", EXCLUDE 127.0.0.1":"")],
      env: Object.fromEntries(Object.entries(process.env).filter(([name]) => !/^(DEBUG|PWDEBUG|NODE_OPTIONS|HTTPS?_PROXY|ALL_PROXY|https?_proxy|all_proxy)$/.test(name))),
    };
    this.starting = this.hooks.ownedLocal ? launchOwnedBrowser(chromium, options, this.signal) : chromium.launchServer(options);
    this.server = await this.starting;
    const pid = this.server.process().pid; integer(pid, "actual browser PID", 1); this.monitor.setBrowserPid(pid);
    await this.writer.send({ type: "started", browserPid: pid });
    checkAbort(this.signal);
    this.browser = this.hooks.ownedLocal ? this.server.browser : await chromium.connect(this.server.wsEndpoint(), { timeout: this.monitor.remainingTimeout() });
    const browserVersion = this.browser.version();
    this.identity = { engine: `Chromium ${browserVersion} · ${process.platform}/${process.arch} · ${browserSha256} · ${modules.moduleSha256}`, browserVersion, browserSha256, moduleSha256: modules.moduleSha256, playwrightVersion: "1.63.0", mediabunnyVersion: "1.41.0" };
    if (this.hooks.ownedLocal) this.identity.engine += ` · preview-direct-v1 · Node ${process.versions.node}`;
    this.context = await this.browser.newContext({ acceptDownloads: false, serviceWorkers: "block" });
    await this.context.route("**/*", async route => {
      const response = moduleResponse(modules, route.request().url(), route.request().method());
      if (!response) return route.abort("blockedbyclient");
      return route.fulfill(response);
    });
    await this.context.routeWebSocket("**", route => route.close());
    this.page = await this.context.newPage();
    if (this.hooks.configurePage) await this.hooks.configurePage(this);
    else await this.configureAudioPage();
    checkAbort(this.signal);
    await this.page.goto(ORIGIN + "/", { waitUntil: "domcontentloaded", timeout: this.monitor.remainingTimeout() });
    checkAbort(this.signal); this.monitor.remainingTimeout();
    if (this.hooks.observeRuntime) Object.assign(this.identity, await this.hooks.observeRuntime(this, modules));
    await this.writer.send({ type: "ready", browserPid: pid, identity: this.identity, quota: this.monitor.receipt() });
  }
  async configureAudioPage() {
    const resources = new Map(this.begin.resources.map(resource => [resource.assetId, resource.handle]));
    const fixed = new Map(this.begin.plan.resources.map(resource => [resource.assetId, resource]));
    const voices = new Set(this.begin.plan.voices.map(voice => voice.elementId));
    await this.page.exposeFunction("__opencutAudioCallback", async (op, args) => {
      checkAbort(this.signal);
      if (!["resolveVerifiedBytes", "openVerifiedRange", "readVerifiedBytes", "releaseVerifiedBytes", "releaseDecodedSource", "admitDecode", "admitContainerDecode", "admitPcmAudioDecode", "compilePitchPreparation", "compileSourcePositions", "compileBlock"].includes(op) || !args || typeof args !== "object") reject("INVALID_HOST_CALLBACK", "Unknown audio host callback");
      if (op === "resolveVerifiedBytes" || op === "openVerifiedRange" || op === "releaseDecodedSource" || op === "admitDecode" || op === "admitContainerDecode" || op === "admitPcmAudioDecode") {
        const reference = fixed.get(args.resource?.assetId);
        if (!reference || ["contentHash", "size", "kind"].some(key => args.resource[key] !== reference[key])) reject("NOT_AUTHORIZED", "Resource does not belong to the fixed audio plan");
        if (op === "admitPcmAudioDecode" && reference.kind !== "audio") reject("INVALID_HOST_CALLBACK", "PCM window decoding requires a standalone audio resource");
        if (op === "admitDecode" || op === "admitContainerDecode" || op === "admitPcmAudioDecode") {
          const primary = args.primary;
          if (reference.kind === "audio" && op === "admitDecode") {
            if (primary !== null) reject("INVALID_HOST_CALLBACK", "Audio byte decoding does not select a container primary track");
          } else {
            if (!primary || typeof primary !== "object" || Array.isArray(primary)
              || Object.keys(primary).length !== 4
              || typeof primary.codec !== "string" || !/^[a-z0-9][a-z0-9.-]{0,127}$/.test(primary.codec)) reject("INVALID_HOST_CALLBACK", "Primary audio format is not bounded");
            integer(primary.trackId, "primary audio track ID", 0, 0xffffffff);
            integer(primary.sampleRate, "primary audio sample rate", 1, 384000);
            integer(primary.channels, "primary audio channels", 1, 64);
          }
        }
        args = ["resolveVerifiedBytes", "openVerifiedRange"].includes(op) ? { resource: reference, handle: resources.get(reference.assetId) }
          : op === "releaseDecodedSource" ? { resource: reference } : { resource: reference, primary: args.primary };
      }
      if (op === "compilePitchPreparation" || op === "compileSourcePositions") {
        if (!voices.has(args.elementId)) reject("NOT_AUTHORIZED", "Voice does not belong to the fixed audio plan");
      }
      if (op === "compilePitchPreparation" && args.residentFrameCount !== null) natural(args.residentFrameCount, "resident PCM frames");
      if (op === "compileSourcePositions" || op === "compileBlock") integer(args.frameCount, "host compile frame count", 1, 4096);
      if (op === "readVerifiedBytes") { natural(args.offset, "byte offset"); integer(args.count, "byte chunk count", 1, 65536); }
      const value = await this.broker.request(op, args, { jsonText: op === "compileBlock" });
      return op === "compileBlock" || op === "compileSourcePositions" ? compiledFactsJson(value) : value;
    });
    await this.page.exposeFunction("__opencutTerminateAudioRealm", () => this.stop());
    await this.page.exposeFunction("__opencutAudioPcm", async packet => {
      checkAbort(this.signal);
      integer(packet.channel, "PCM channel", 0, 1); integer(packet.frames, "PCM chunk frames", 1, 4096); natural(packet.offsetFrames, "PCM offset");
      if (typeof packet.bytesHex !== "string" || packet.bytesHex.length !== packet.frames * 8 || !/^(?:[0-9a-f]{2})+$/.test(packet.bytesHex)) reject("INVALID_OUTPUT_PCM", "PCM chunk is not canonical f32le bytes");
      await this.writer.send({ ...packet, type: "pcm" });
    });
  }
  async render() {
    checkAbort(this.signal); this.monitor.remainingTimeout();
    if (this.hooks.render) return this.hooks.render(this);
    await this.page.evaluate(maxPcmBytes => { globalThis.__opencutAudioAdmission = { maxPcmBytes }; }, this.begin.admission.maxPcmBytes);
    const result = await this.page.evaluate(executeAudioPage, { plan: this.begin.plan, engineIdentity: this.identity.engine, maximumEncodedBytes: this.begin.admission.maxEncodedBytes });
    if (result?.ok !== true) throw new AudioExecutionError(result?.error?.code ?? "AUDIO_BROWSER_EXECUTION_FAILED", "Browser audio execution failed");
    return result.receipt;
  }
  stop() {
    if (!this.stopping) this.stopping = (async () => {
      const server = this.server ?? await this.starting?.catch(() => undefined);
      if (!server) return;
      const child = server.process();
      let timer;
      try {
        await Promise.race([server.close(), new Promise((_, fail) => { timer = setTimeout(() => fail(new Error("Browser close timeout")), 1000); })]);
      } catch {
        try { await server.kill(); } catch {
          if (child.exitCode === null && child.signalCode === null) throw new AudioExecutionError("AUDIO_REALM_TERMINATION_FAILED", "Browser process termination was not acknowledged");
        }
      } finally { clearTimeout(timer); }
      if (child.exitCode === null && child.signalCode === null) throw new AudioExecutionError("AUDIO_REALM_TERMINATION_FAILED", "Browser process exit was not acknowledged");
    })();
    return this.stopping;
  }
}
