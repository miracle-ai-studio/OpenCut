import { expect } from "@playwright/test";
import path from "node:path";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import os from "node:os";
import { fileURLToPath } from "node:url";
import { build } from "vite";

const time = (ticks, timebase = 1) => ({ ticks: String(ticks), timebase });
const toolName = "opencut_source_art_get";
const displayTool = "opencut_source_art_waveform_display_get";
const artworkTools = [toolName, displayTool];
const observationBudget = Object.freeze({ entries: 256, encoded_bytes: 16 * 1024 * 1024 });

// Independent owned-fixture oracle: the captured streamed PCM16 decoder uses
// signed /2^15 normalization, then Classic takes all-channel 128-sample maxima.
// This reads the original tiny synthetic WAV, never a service-generated peak.
async function pcm16Oracle(filename, frames = 192000) {
  const bytes = await readFile(filename);
  expect(bytes.length).toBeLessThan(2 * 1024 * 1024);
  expect(bytes.toString("ascii", 0, 4)).toBe("RIFF"); expect(bytes.toString("ascii", 8, 12)).toBe("WAVE");
  let format, data;
  for (let offset = 12; offset + 8 <= bytes.length;) {
    const kind = bytes.toString("ascii", offset, offset + 4), count = bytes.readUInt32LE(offset + 4), start = offset + 8;
    expect(start + count).toBeLessThanOrEqual(bytes.length);
    if (kind === "fmt ") { expect(count).toBeGreaterThanOrEqual(16); format = { code: bytes.readUInt16LE(start), channels: bytes.readUInt16LE(start + 2), sampleRate: bytes.readUInt32LE(start + 4), bits: bytes.readUInt16LE(start + 14) }; }
    if (kind === "data") data = bytes.subarray(start, start + count);
    offset = start + count + count % 2;
  }
  expect(format).toEqual({ code: 1, channels: 1, sampleRate: 48000, bits: 16 }); expect(data).toBeTruthy();
  expect(data.length).toBe(frames * 2);
  const summary = new Float32Array(Math.ceil(frames / 128));
  for (let sample = 0; sample < frames; sample++) {
    const raw = data.readInt16LE(sample * 2), normalized = Math.fround(raw / 32768);
    const bucket = Math.floor(sample / 128); summary[bucket] = Math.max(summary[bucket], Math.abs(normalized));
  }
  return { sampleRate: 48000, frames, summary };
}

// Isolated actual browser allocations with the production class/allocator.
// This neither exposes a production diagnostic global nor estimates the main
// workspace cache from heap usage. Vite emits only an in-memory fixture chunk.
export async function buildSourceArtCacheProbe() {
  const webRoot = fileURLToPath(new URL("../", import.meta.url));
  const entry = "virtual:source-art-cache-acceptance", absoluteEntry = path.resolve(webRoot, entry), resolved = `\0${entry}`;
  const result = await build({ configFile: false, root: webRoot, logLevel: "silent",
    plugins: [{ name: "source-art-cache-acceptance", resolveId: id => id === entry || id === absoluteEntry ? resolved : null,
      load: id => id === resolved ? `export {SourceArtCache} from ${JSON.stringify(path.join(webRoot, "src/editor/workspace/scene/source-art/cache.ts"))};\nexport {allocateArt} from ${JSON.stringify(path.join(webRoot, "src/editor/workspace/scene/source-art/service.ts"))};` : null }],
    build: { write: false, minify: false, sourcemap: false, lib: { entry, name: "SourceArtCacheAcceptance", formats: ["iife"] } } });
  const chunks = (Array.isArray(result) ? result : [result]).flatMap(value => value.output).filter(value => value.type === "chunk");
  expect(chunks).toHaveLength(1); expect(Buffer.byteLength(chunks[0].code)).toBeLessThan(1024 * 1024);
  return chunks[0].code;
}

export async function verifyCacheAllocations(browser, large, small) {
  const code = await buildSourceArtCacheProbe();
  const isolated = await browser.newPage();
  try {
    await isolated.addScriptTag({ content: code });
    return await isolated.evaluate(async ({ large, small }) => {
      const ensure = (condition, message) => { if (!condition) throw new Error(`Production cache allocation: ${message}`); };
      const { SourceArtCache, allocateArt } = globalThis.SourceArtCacheAcceptance;
      const live = new Map(), revoked = new Set(), created = [];
      const nativeCreate = URL.createObjectURL.bind(URL), nativeRevoke = URL.revokeObjectURL.bind(URL);
      URL.createObjectURL = blob => { const url = nativeCreate(blob); created.push(url); return url; };
      URL.revokeObjectURL = url => { revoked.add(url); nativeRevoke(url); };
      const cache = new SourceArtCache(); cache.activate("isolated-owned-acceptance");
      let loads = 0, peakBytes = 0, peakEntries = 0;
      const inspect = () => {
        const total = [...live.values()].reduce((sum, value) => sum + value, 0);
        ensure(cache.bytes === total, "charged bytes differ from live materialized allocations");
        ensure(cache.bytes <= 32 * 1024 * 1024 && cache.entries.size <= 256 && cache.running <= 2, "default budget exceeded");
        peakBytes = Math.max(peakBytes, cache.bytes); peakEntries = Math.max(peakEntries, cache.entries.size);
      };
      const acquire = async (key, fixture) => {
        const lease = cache.acquire(key, async () => {
          loads++; const allocation = allocateArt(fixture.response, fixture.selection);
          return { ...allocation, materialize: () => {
            const value = allocation.materialize(), originalDispose = value.dispose;
            ensure(value.value.kind === "thumbnail", "fixture is not a real JPEG thumbnail"); live.set(value.value.url, value.bytes);
            value.dispose = () => { live.delete(value.value.url); originalDispose(); };
            return value;
          } };
        }, () => {});
        try { const value = await lease.promise; inspect(); return value; } finally { lease.release(); }
      };
      try {
        const first = await acquire("large-0", large), blob = await (await fetch(first.url)).blob();
        ensure(blob.size === large.response.data.jpeg_bytes_hex.length / 2 && blob.type === "image/jpeg", "actual Blob size/type");
        const bitmap = await createImageBitmap(blob);
        const decoded = { width: bitmap.width, height: bitmap.height }; bitmap.close();
        ensure(decoded.width === 640 && decoded.height === 360, "actual JPEG decoded dimensions");
        for (let index = 1; index < 40; index++) await acquire(`large-${index}`, large);
        const afterByteEviction = { bytes: cache.bytes, entries: cache.entries.size };
        ensure(!cache.entries.has("large-0") && revoked.has(first.url), "32 MiB pressure did not evict/revoke oldest unused entry");
        const beforeRebuild = loads, rebuilt = await acquire("large-0", large);
        ensure(loads === beforeRebuild + 1 && rebuilt.url !== first.url, "evicted thumbnail was not rematerialized");
        cache.clear(); inspect(); ensure(cache.bytes === 0 && cache.entries.size === 0, "byte phase clear");
        const smallFirst = await acquire("small-0", small);
        for (let index = 1; index < 260; index++) await acquire(`small-${index}`, small);
        const afterEntryEviction = { bytes: cache.bytes, entries: cache.entries.size };
        ensure(cache.entries.size === 256 && !cache.entries.has("small-0") && revoked.has(smallFirst.url), "256-entry pressure did not evict/revoke oldest unused entry");
        const beforeSmallRebuild = loads, rebuiltSmall = await acquire("small-0", small);
        ensure(loads === beforeSmallRebuild + 1 && rebuiltSmall.url !== smallFirst.url, "entry eviction was not rebuilt");
        cache.clear(); inspect();
        ensure(cache.bytes === 0 && cache.entries.size === 0 && created.every(url => revoked.has(url)), "final allocations/ObjectURLs remain live");
        return { scope: "Isolated about:blank production SourceArtCache + allocateArt using actual authorized JPEG receipts; not main workspace/native cache occupancy.",
          defaults: { bytes: 33554432, entries: 256, concurrent: 2 }, peak_bytes: peakBytes, peak_entries: peakEntries,
          after_byte_eviction: afterByteEviction, after_entry_eviction: afterEntryEviction,
          decoded, materializations: loads, created_urls: created.length, revoked_urls: revoked.size, final_bytes: cache.bytes, final_entries: cache.entries.size };
      } finally { cache.clear(); URL.createObjectURL = nativeCreate; URL.revokeObjectURL = nativeRevoke; }
    }, { large, small });
  } finally { await isolated.close(); }
}
function assertDisplayOracle(response, request, oracle, decibels) {
  expect(response).not.toHaveProperty("schema_version");
  for (const key of ["project_id", "revision", "asset_id", "sequence_id", "track_id", "element_id", "temporary_gain", "sample_rate", "viewport"]) expect(response[key]).toEqual(request[key]);
  expect(response.display_profile).toBe("classic-waveform-display-v1"); expect(response.channels).toBe(1);
  expect(response.total_samples).toBe(String(oracle.frames)); expect(response.bucket_size).toBe(128);
  const v = request.viewport, width = v.clip_right_px - v.clip_left_px, gain = 10 ** (decibels / 20);
  const canvasWidth = Math.max(1, Math.ceil(width * v.device_pixel_ratio)), canvasHeight = Math.max(1, Math.round(v.css_height * v.device_pixel_ratio));
  expect(response.canvas).toEqual({ width: canvasWidth, height: canvasHeight, css_left: v.clip_left_px, css_width: width, css_height: v.css_height });
  expect(response.bars.length).toBe(Math.max(1, Math.floor(width / 2)));
  for (let index = 0; index < response.bars.length; index++) {
    const bar = response.bars[index], left = v.clip_left_px + index * 2, right = Math.min(v.clip_right_px, left + 2);
    const start = Math.floor(Math.min(4, left / v.pixels_per_second) * 48000);
    const end = Math.min(192000, Math.ceil(Math.min(4, right / v.pixels_per_second) * 48000));
    expect(bar.source_start_sample).toBe(String(start)); expect(bar.source_end_sample).toBe(String(end));
    let peak = 0; if (end > start) for (let bucket = Math.floor(start / 128); bucket < Math.min(1500, Math.ceil(end / 128)); bucket++) peak = Math.max(peak, oracle.summary[bucket]);
    // The public peak is f32: JSON carries its shortest round-tripping decimal.
    // Recover that exact f32 before comparing with the original-byte oracle.
    expect(Math.fround(bar.peak)).toBe(peak); expect(Math.abs(bar.gain - gain)).toBeLessThan(1e-12);
    const amplitude = peak * gain, db = amplitude > 0 ? 20 * Math.log10(amplitude) : -Infinity;
    const fraction = db <= -40 ? 0 : Math.min(1, ((db + 40) / 40) ** 1.5);
    expect(Math.abs(bar.output_amplitude - amplitude)).toBeLessThan(1e-12); expect(Math.abs(bar.height_fraction - fraction)).toBeLessThan(1e-12);
    expect(bar.burn).toBe(amplitude > 1);
    const scale = canvasHeight / v.css_height, top = fraction > 0 ? Math.round((v.css_height - Math.max(1, fraction * v.css_height)) * scale) : canvasHeight;
    expect(bar.device_left).toBe(Math.round(index * 2 * v.device_pixel_ratio));
    expect(bar.device_right).toBe(Math.max(bar.device_left + 1, Math.round((index * 2 + 1) * v.device_pixel_ratio)));
    expect(bar.device_top).toBe(top); expect(bar.device_height).toBe(fraction > 0 ? Math.max(1, canvasHeight - top) : 0);
    expect(bar.burn_height).toBe(amplitude > 1 ? Math.max(1, Math.round(scale)) : 0);
  }
}


// Uses the production upload, shared planner, source-art port and product. The
// native source-art tests own independent PCM/JPEG pixel expectations; these
// checks prove the returned facts are consumed by the actual browser surface.
export async function verifySourceArt(browser, endpoint, directory, check) {
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
  page.setDefaultTimeout(30000);
  const projects = [], pageErrors = [], failedRequests = [], cancelledRequests = [], observations = [], pending = new Set();
  let observationSequence = 0, observationBytes = 0, observationPeakBytes = 0;
  const observeArtwork = (operation, request, original) => {
    let envelope = original;
    if (operation === toolName && original.data?.data) {
      const source = original.data.data;
      // Source UI checks need identity and counts only. Do not retain JPEG hex
      // or source peaks from periodic authorization responses in Node memory.
      const { jpeg_bytes_hex, amplitudes, ...metadata } = source;
      envelope = { ...original, data: { ...original.data, data: { ...metadata,
        jpeg_bytes: typeof jpeg_bytes_hex === "string" ? jpeg_bytes_hex.length / 2 : undefined,
        amplitude_count: amplitudes?.length } } };
    }
    // Deduplicate each exact fixed element/gain/viewport. A late old-window
    // response must not overwrite the current DOM's already received receipt.
    // Hard caps cover the 100-clip/20-zoom fixture; never silently evict facts.
    const key = JSON.stringify([operation, request.project_id, request.revision, request.asset_id,
      request.sequence_id, request.track_id, request.element_id, request.temporary_gain,
      operation === toolName ? request.query : request.viewport]);
    const candidate = { operation, request, envelope, key, sequence: observationSequence + 1 };
    const bytes = Buffer.byteLength(JSON.stringify(candidate));
    const previous = observations.findIndex(value => value.key === key);
    const nextBytes = observationBytes - (previous < 0 ? 0 : observations[previous].bytes) + bytes;
    if (observations.length + (previous < 0 ? 1 : 0) > observationBudget.entries || nextBytes > observationBudget.encoded_bytes)
      throw new Error(`Source artwork observation budget exceeded: ${observations.length + (previous < 0 ? 1 : 0)} entries / ${nextBytes} encoded bytes; cap ${observationBudget.entries} / ${observationBudget.encoded_bytes}`);
    if (previous >= 0) observations.splice(previous, 1);
    observationSequence++; observationBytes = nextBytes; observationPeakBytes = Math.max(observationPeakBytes, nextBytes);
    observations.push({ ...candidate, bytes });
  };
  const writes = [], gainPlans = [];
  const sessions = new Map(), sessionFailures = [], activeSessions = new Set(), navigationAborts = new Set(), responseStatuses = new WeakMap();
  const sessionHeader = "x-opencut-source-art-witness";
  const sessionRequest = request => new URL(request.url()).origin === new URL(endpoint).origin && new URL(request.url()).pathname === "/app/session";
  await page.exposeBinding("__sourceArtSessionWitness", (_source, event) => {
    if (!event || !/^[a-f0-9-]{36}$/.test(event.id) || !["start", "settled"].includes(event.stage)) throw new Error("Invalid session witness");
    if (!sessions.has(event.id) && sessions.size >= 16) throw new Error("Session witness budget exceeded");
    const value = sessions.get(event.id) ?? {};
    if (event.stage === "start") value.started = true;
    else { value.settled = event.settled; value.status = event.status; }
    sessions.set(event.id, value);
  });
  page.on("request", request => {
    if (sessionRequest(request)) activeSessions.add(request);
    if (request.url().endsWith("/api/tools/opencut_edit_apply")) writes.push(request.postDataJSON());
    if (request.url().endsWith("/api/tools/opencut_project_audio_gain_plan")) gainPlans.push(request.postDataJSON());
  });
  page.on("requestfinished", request => activeSessions.delete(request));
  page.on("pageerror", error => pageErrors.push(error.message));
  page.on("requestfailed", request => {
    activeSessions.delete(request);
    const record = { url: new URL(request.url()).pathname, error: request.failure()?.errorText,
      method: request.method(), resource_type: request.resourceType(), navigation: request.isNavigationRequest(), response_status: responseStatuses.get(request) ?? null };
    if (sessionRequest(request)) {
      sessionFailures.push({ request, record, witness: request.headers()[sessionHeader], explicitly_navigated: navigationAborts.has(request) });
      return;
    }
    // View invalidation deliberately aborts its in-flight source-art waiter.
    if (artworkTools.some(name => record.url.endsWith(`/api/tools/${name}`)) && /ERR_ABORTED|NS_BINDING_ABORTED/.test(record.error ?? "")) cancelledRequests.push(record);
    else failedRequests.push(record);
  });
  page.on("response", response => {
    responseStatuses.set(response.request(), response.status());
    const operation = artworkTools.find(name => response.url().endsWith(`/api/tools/${name}`));
    if (!operation) return;
    const task = (async () => {
      try { observeArtwork(operation, response.request().postDataJSON(), await response.json()); }
      catch (error) {
        // A response body can disappear after an intentional waiter abort.
        if (!/aborted|closed|No resource with given identifier|No data found/i.test(String(error))) throw error;
      }
    })();
    pending.add(task);
    void task.catch(error => pageErrors.push(String(error))).finally(() => pending.delete(task));
  });
  await page.addInitScript(() => {
    const nativeFetch = globalThis.fetch.bind(globalThis);
    globalThis.fetch = (input, init) => {
      const url = new URL(input instanceof Request ? input.url : String(input), location.href);
      const method = String(init?.method ?? (input instanceof Request ? input.method : "GET")).toUpperCase();
      const headers = new Headers(init?.headers ?? (input instanceof Request ? input.headers : undefined));
      if (url.origin !== location.origin || url.pathname !== "/app/session" || method !== "POST" || headers.get("X-OpenCut-App") !== "web")
        return nativeFetch(input, init);
      // Fixture-only nonsecret correlation. No credential/header or response
      // body is exported; authorization and the production fetch stay intact.
      const id = crypto.randomUUID(); headers.set("X-OpenCut-Source-Art-Witness", id);
      void globalThis.__sourceArtSessionWitness({ id, stage: "start" }).catch(() => {});
      return nativeFetch(input, { ...init, headers }).then(async response => {
        await globalThis.__sourceArtSessionWitness({ id, stage: "settled", settled: "fulfilled", status: response.status });
        return response;
      }, async error => {
        await globalThis.__sourceArtSessionWitness({ id, stage: "settled", settled: "rejected", status: null }).catch(() => {});
        throw error;
      });
    };
    const created = [], revoked = [];
    const create = URL.createObjectURL.bind(URL), revoke = URL.revokeObjectURL.bind(URL);
    URL.createObjectURL = blob => { const url = create(blob); created.push({ url, type: blob.type }); return url; };
    URL.revokeObjectURL = url => { revoked.push(url); revoke(url); };
    globalThis.__sourceArtUrls = { created, revoked };
  });
  const tool = async (name, input) => {
    const envelope = await page.evaluate(async ({ name, input }) => {
      const response = await fetch(`/api/tools/${name}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input) });
      return response.json();
    }, { name, input });
    expect(envelope.error, `${name}: ${JSON.stringify(envelope.error)}`).toBeNull();
    return envelope.data;
  };
  const navigate = async action => {
    // Snapshot only requests already in flight. A new document's bootstrap
    // cannot inherit a broad navigation-time exemption.
    for (const request of activeSessions) navigationAborts.add(request);
    await action();
  };
  const createProject = async name => {
    const create = page.getByRole("button", { name: "New project", exact: true });
    await expect(create).toBeEnabled(); // The real HttpOnly session has bootstrapped.
    await create.click();
    const dialog = page.getByRole("dialog", { name: "New project", exact: true });
    await dialog.getByLabel("Project name", { exact: true }).fill(name);
    await dialog.getByRole("button", { name: "Create project", exact: true }).click();
    await expect(page).toHaveURL(/\/editor\/.+/);
    const id = decodeURIComponent(new URL(page.url()).pathname.split("/").at(-1)); projects.push(id);
    const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
    if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
    await expect(page.getByRole("button", { name: "Import media", exact: true })).toBeEnabled();
    return id;
  };
  const importMedia = async name => {
    const planning = page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_media_import_plan"));
    await page.getByLabel("Import media files", { exact: true }).setInputFiles(path.join(directory, name));
    const review = page.getByRole("dialog", { name: "Review media changes", exact: true });
    await expect(review).toBeVisible();
    expect((await (await planning).json()).error).toBeNull();
    const applying = page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_edit_apply"));
    await review.getByRole("button", { name: "Apply media changes", exact: true }).click();
    expect((await (await applying).json()).error).toBeNull();
    await expect(review.getByText("Media changes saved.", { exact: true })).toBeVisible();
    await review.getByRole("button", { name: "Done", exact: true }).click();
  };
  const requireArtwork = async (container, name) => {
    // A decoder may use its real 60-second admission. Observe its terminal UI
    // state, and fail immediately on a displayed error rather than retrying
    // that known failure until a larger timeout expires.
    const ready = name === "Source thumbnail" ? 'img[alt="Source thumbnail"], [role="img"][aria-label="Source thumbnail"]'
      : name === "Classic element waveform" ? 'canvas[role="img"][aria-label="Classic element waveform"]'
      : 'svg[role="img"][aria-label="Source peaks before element gain"]';
    await expect(container.locator(`${ready}, [aria-label="Source artwork unavailable"]`)).toHaveCount(1, { timeout: 65000 });
    await expect(container.getByLabel("Source artwork unavailable", { exact: true })).toHaveCount(0);
    await expect(container.getByRole("img", { name, exact: true })).toBeVisible();
  };
  let failure;
  try {
    await navigate(() => page.goto(`${endpoint}/editor`));
    const projectId = await createProject("Source artwork acceptance");
    await importMedia("sample.mp4"); await importMedia("sample.wav");
    const imported = await tool("opencut_project_inspect", { project_id: projectId, revision: null });
    const sceneId = imported.project.default_sequence_id;
    const scene = imported.project.sequences.find(value => value.id === sceneId);
    const video = imported.project.assets.find(value => value.name === "sample.mp4" && value.kind === "video");
    const audio = imported.project.assets.find(value => value.name === "sample.wav" && value.kind === "audio");
    expect(video).toBeTruthy(); expect(audio).toBeTruthy();
    for (const asset of [video, audio]) expect(BigInt(asset.duration.ticks)).toBe(4n * BigInt(asset.duration.timebase));
    const audioTrackId = crypto.randomUUID(), videoId = crypto.randomUUID(), audioId = crypto.randomUUID();
    const timing = { kind: "source_range", range: { start: time(0), duration: time(4) }, rate: { numerator: 1, denominator: 1 }, maintain_pitch: false };
    const plan = await tool("opencut_media_import_plan", {
      project_id: projectId, expected_revision: imported.revision, sequence_id: sceneId, idempotency_key: crypto.randomUUID(),
      label: "Actual source artwork placements", bindings: [], ripple: false,
      new_tracks: [{ id: audioTrackId, name: "Source artwork audio", kind: "audio" }], placements: [
        { element_id: videoId, name: "Artwork video", track_id: scene.main_track_id, asset_id: video.id, start: time(0), content: { kind: "video", timing, source_audio_enabled: false } },
        { element_id: audioId, name: "Artwork audio", track_id: audioTrackId, asset_id: audio.id, start: time(0), content: { kind: "audio", timing } },
      ],
    });
    await tool("opencut_edit_apply", plan.transaction);
    const fixed = await tool("opencut_project_inspect", { project_id: projectId, revision: null });
    await navigate(() => page.reload());
    await expect(page.getByLabel("Scene", { exact: true })).toHaveValue(sceneId);
    const videoCard = page.locator(".editor-media-card").filter({ hasText: "sample.mp4" });
    const audioCard = page.locator(".editor-media-card").filter({ hasText: "sample.wav" });
    await requireArtwork(videoCard, "Source thumbnail");
    await expect.poll(() => videoCard.locator("img").evaluate(image => image.complete && image.naturalWidth > 0)).toBe(true);
    await requireArtwork(audioCard, "Source peaks before element gain");
    await expect.poll(() => audioCard.locator("svg path").getAttribute("d")).toMatch(/M/);
    const videoClip = page.locator(`[data-element-id="${videoId}"]`), audioClip = page.locator(`[data-element-id="${audioId}"]`);
    await requireArtwork(videoClip, "Source thumbnail");
    await requireArtwork(audioClip, "Classic element waveform");
    const thumbnail = await tool(toolName, { project_id: projectId, revision: fixed.revision, asset_id: video.id,
      request_id: crypto.randomUUID(), query: { kind: "thumbnail" } });
    const source = await tool(toolName, { project_id: projectId, revision: fixed.revision, asset_id: audio.id,
      request_id: crypto.randomUUID(), query: { kind: "waveform", sample_rate: 48000, projection: { kind: "source", range: null, bins: 128 } } });
    for (const [response, asset, algorithm] of [[thumbnail, video, "classic-thumbnail-1s-jpeg08-v1"], [source, audio, "source-peaks128-v1"]]) {
      expect(response).not.toHaveProperty("schema_version"); expect(response.project_id).toBe(projectId); expect(response.revision).toBe(fixed.revision);
      expect(response.asset_id).toBe(asset.id); expect(response.version).toEqual(asset.version);
      expect(response.identity.algorithm).toBe(algorithm); expect(response.identity.browser_version).toBeTruthy();
      for (const field of ["browser_sha256", "node_sha256", "module_sha256"]) expect(response.identity[field]).toMatch(/^[a-f0-9]{64}$/);
    }
    expect(thumbnail.data.kind).toBe("thumbnail_ready"); expect(thumbnail.data.width).toBe(640); expect(thumbnail.data.height).toBe(360);
    expect(thumbnail.data.observation.kind).toBe("video"); expect(thumbnail.data.observation.requested_seconds).toBe(1);
    const jpeg = Buffer.from(thumbnail.data.jpeg_bytes_hex, "hex");
    expect(jpeg.subarray(0, 2)).toEqual(Buffer.from([255, 216]));
    expect(createHash("sha256").update(jpeg).digest("hex")).toBe(thumbnail.data.content_hash);
    expect(source.data.kind).toBe("waveform"); expect(source.data.sample_rate).toBe(48000); expect(source.data.bucket_size).toBe(128);
    expect(source.data.total_samples).toBe("192000"); expect(source.data.amplitudes).toHaveLength(128);
    expect(source.data.amplitudes.every(value => Number.isFinite(value) && value >= 0)).toBe(true);
    expect(source.data.amplitudes.some(value => value > 0)).toBe(true);
    check("source art: actual uploaded video JPEG, Media source peaks and timeline Classic display reach their distinct product surfaces", {
      source_art: { project_id: projectId, revision: fixed.revision, thumbnail: { width: thumbnail.data.width, height: thumbnail.data.height,
        content_hash: thumbnail.data.content_hash, identity: thumbnail.identity }, waveform: { sample_rate: 48000, total_samples: source.data.total_samples,
        bins: source.data.amplitudes.length, identity: source.identity }, independent_pixels_pcm: "native source-art tests; this check covers public receipts and UI consumption" },
    });
    const selectedScene = await page.getByLabel("Scene", { exact: true }).inputValue();
    const ids = await page.locator("[data-element-id]").evaluateAll(nodes => nodes.map(node => node.dataset.elementId));
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill("1.25");
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
    await expect(page.getByRole("slider", { name: "Scene playhead", exact: true })).toHaveAttribute("aria-valuenow", "1.25");
    const timeline = page.locator(".editor-timeline-panel");
    for (let count = 0; count < 5; count++) await timeline.getByRole("button", { name: "Zoom in", exact: true }).click();
    const viewport = page.locator(".editor-timeline-viewport");
    await viewport.evaluate(node => { node.scrollLeft = 200; node.dispatchEvent(new Event("scroll")); });
    await expect.poll(() => viewport.evaluate(node => node.scrollLeft)).toBeGreaterThan(0);
    await expect(audioClip.getByRole("img", { name: "Classic element waveform", exact: true })).toBeVisible();
    await viewport.evaluate(node => { node.scrollLeft = 0; node.dispatchEvent(new Event("scroll")); });
    for (let count = 0; count < 5; count++) await timeline.getByRole("button", { name: "Zoom out", exact: true }).click();
    await expect(audioClip.getByRole("img", { name: "Classic element waveform", exact: true })).toBeVisible();
    expect(await page.getByLabel("Scene", { exact: true }).inputValue()).toBe(selectedScene);
    expect(await page.locator("[data-element-id]").evaluateAll(nodes => nodes.map(node => node.dataset.elementId))).toEqual(ids);
    const afterViews = await tool("opencut_project_inspect", { project_id: projectId, revision: null });
    expect(afterViews).toEqual(fixed);
    await expect.poll(() => observations.some(({ request, envelope }) => request?.project_id === projectId && request.revision === fixed.revision &&
      request.element_id === audioId && request.temporary_gain === null &&
      envelope.error === null && envelope.data?.display_profile === "classic-waveform-display-v1")).toBe(true);
    check("source art: seek, scroll and zoom consume fixed-revision Classic display while exact head, scene and element IDs remain unchanged");
    const oracle = await pcm16Oracle(path.join(directory, "sample.wav"));
    const canvas = audioClip.getByRole("img", { name: "Classic element waveform", exact: true });
    const display = async (revision, temporaryGain, after = 0) => {
      await expect(canvas).toHaveAttribute("data-temporary-gain", temporaryGain === null ? "saved" : String(temporaryGain));
      await expect(canvas).toHaveAttribute("data-display-profile", "classic-waveform-display-v1");
      let observed;
      await expect.poll(async () => {
        const rendered = await canvas.evaluate(element => ({ width: element.width, height: element.height,
          left: Number.parseFloat(element.style.left), cssWidth: Number.parseFloat(element.style.width), cssHeight: Number.parseFloat(element.style.height) }));
        observed = observations.findLast(value => value.sequence > after && value.operation === displayTool && value.request?.project_id === projectId &&
          value.request.revision === revision && value.request.element_id === audioId && value.request.temporary_gain === temporaryGain &&
          value.envelope?.error === null && value.envelope?.data?.display_profile === "classic-waveform-display-v1" &&
          value.envelope.data.canvas.width === rendered.width && value.envelope.data.canvas.height === rendered.height &&
          value.envelope.data.canvas.css_left === rendered.left && value.envelope.data.canvas.css_width === rendered.cssWidth && value.envelope.data.canvas.css_height === rendered.cssHeight);
        return Boolean(observed);
      }).toBe(true);
      assertDisplayOracle(observed.envelope.data, observed.request, oracle, temporaryGain === null ? (revision === fixed.revision ? 0 : -12) : temporaryGain / 10);
      expect(observed.envelope.data.version).toEqual(audio.version);
      return observed;
    };
    const pixels = async (observed, target = canvas) => {
      const response = observed.envelope.data;
      await expect(target).toHaveAttribute("data-display-bars", String(response.bars.length));
      await expect(target).toHaveAttribute("data-burn-bars", String(response.bars.filter(bar => bar.burn).length));
      const bar = response.bars.find(value => value.device_height > 0 && value.device_right < response.canvas.width);
      expect(bar).toBeTruthy();
      const point = { x: bar.device_left, y: bar.device_top + Math.floor(bar.device_height / 2), gap: bar.device_right };
      await target.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      const actual = await target.evaluate((element, point) => {
        const context = element.getContext("2d");
        return { width: element.width, height: element.height, bar: [...context.getImageData(point.x, point.y, 1, 1).data], gap: [...context.getImageData(point.gap, point.y, 1, 1).data] };
      }, point);
      expect([actual.width, actual.height]).toEqual([response.canvas.width, response.canvas.height]);
      expect(actual.bar.slice(0, 3)).toEqual([255, 255, 255]); expect(actual.bar[3]).toBeGreaterThanOrEqual(170); expect(actual.bar[3]).toBeLessThanOrEqual(185);
      expect(actual.gap[3]).toBe(0);
      return { device_width: actual.width, device_height: actual.height, rgba_bar: actual.bar, gap_alpha: actual.gap[3] };
    };
    const savedDisplay = await display(fixed.revision, null); const basePixels = await pixels(savedDisplay);
    const burningRequest = { ...savedDisplay.request, request_id: crypto.randomUUID(), temporary_gain: 200 };
    const burning = await tool(displayTool, burningRequest); assertDisplayOracle(burning, burningRequest, oracle, 20);
    expect(burning.bars.some(bar => bar.burn && bar.height_fraction === 1 && bar.burn_height > 0)).toBe(true);
    expect((await tool("opencut_project_inspect", { project_id: projectId, revision: null })).revision).toBe(fixed.revision);
    check("waveform display: independent original PCM16 buckets, static gain, dB height and over-unity burn agree with public facts; real canvas paints bars and transparent gaps", {
      waveform_display_oracle: { source: "owned sample.wav mono PCM16/48000; independent original-byte conversion and 128-sample peaks", profiles: ["source-peaks128-v1", "classic-waveform-display-v1"], baseline_db: 0, readonly_burn_db: 20, baseline_pixels: basePixels,
        dense_100_clips_5_tracks_300_seconds: "separate metadata-only viewport check below; no audio-playback or native-cache-occupancy claim" },
    });
    const slider = audioClip.getByRole("slider", { name: "Volume Artwork audio", exact: true });
    const surface = audioClip.locator(".editor-gain-surface");
    const drag = async () => {
      await expect(slider).toHaveAttribute("aria-disabled", "false"); await slider.scrollIntoViewIfNeeded();
      expect(await slider.evaluate(element => { const r = element.getBoundingClientRect(), hit = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2); return hit === element || element.contains(hit); }), "The actual pointer hits the visible volume line").toBe(true);
      const line = await slider.boundingBox(); expect(line.height).toBe(14);
      await page.mouse.move(line.x + line.width / 2, line.y + line.height / 2); await page.mouse.down();
      const box = await surface.boundingBox(); const percent = 1 - Math.sqrt((10 ** (-12 / 20) - .001) / 9.999);
      await page.mouse.move(box.x + box.width / 2, box.y + box.height * percent);
      await expect(slider).toHaveAttribute("aria-valuenow", "-12");
    };
    const writeStart = writes.length, planStart = gainPlans.length, draftStart = observationSequence;
    await drag(); const draft = await display(fixed.revision, -120, draftStart); const draftPixels = await pixels(draft);
    expect(draft.envelope.data.bars[0].height_fraction).toBeLessThan(savedDisplay.envelope.data.bars[0].height_fraction);
    expect((await tool("opencut_project_inspect", { project_id: projectId, revision: null })).revision).toBe(fixed.revision);
    expect(writes.length).toBe(writeStart); expect(gainPlans.length).toBe(planStart);
    await page.keyboard.press("Escape"); await page.mouse.up();
    await expect(slider).toHaveAttribute("aria-valuenow", "0"); const restored = await display(fixed.revision, null);
    await pixels(restored); expect(writes.length).toBe(writeStart); expect(gainPlans.length).toBe(planStart);
    expect((await tool("opencut_project_inspect", { project_id: projectId, revision: null })).revision).toBe(fixed.revision);
    await drag(); await display(fixed.revision, -120);
    const planned = page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_project_audio_gain_plan") && response.request().postDataJSON().element_id === audioId);
    const applied = page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_edit_apply") && response.request().postDataJSON().project_id === projectId);
    const [planResponse, applyResponse] = await Promise.all([planned, applied, page.mouse.up()]);
    const planEnvelope = await planResponse.json(), applyEnvelope = await applyResponse.json();
    expect(planEnvelope.error).toBeNull(); expect(applyEnvelope.error).toBeNull();
    expect(applyResponse.request().postDataJSON()).toEqual(planEnvelope.data.transaction);
    expect(writes.length).toBe(writeStart + 1); expect(gainPlans.length).toBe(planStart + 1);
    expect(planResponse.request().postDataJSON().command).toEqual({ kind: "set_decibels", decibels_tenths: -120 });
    expect(planEnvelope.data.transaction.expected_revision).toBe(fixed.revision);
    const afterGain = await tool("opencut_project_inspect", { project_id: projectId, revision: null });
    expect(afterGain.revision).toBe(applyEnvelope.data.revision); expect(afterGain.revision).not.toBe(fixed.revision);
    const storedAudio = afterGain.project.sequences.find(sequence => sequence.id === sceneId).tracks.find(track => track.id === audioTrackId).elements.find(element => element.id === audioId);
    expect(storedAudio.content.audio.gain).toEqual({ unit: "decibels", value: -12 }); expect(storedAudio.content.audio.muted).toBe(false);
    const committed = await display(afterGain.revision, null); const committedPixels = await pixels(committed);
    check("waveform display: physical volume draft changes fixed-revision bars, Escape restores without writes, pointerup saves one original gain transaction and displays the new revision", {
      waveform_display_gesture: { original_revision: fixed.revision, saved_revision: afterGain.revision, temporary_gain_tenths: -120, cancelled_writes: 0, saved_writes: 1, draft_pixels: draftPixels, committed_pixels: committedPixels, scene_sound: "not playing in this waveform-only fixture" },
    });
    // A new document discards every prior Web cache/delegate. The native cache
    // may remain warm; this is not a native decoder-count/eviction assertion.
    const beforeReloadUrl = await videoCard.locator("img").getAttribute("src");
    const beforeReloadIds = await page.locator(".editor-timeline-element").evaluateAll(nodes => nodes.map(node => node.dataset.elementId));
    const reloadObservationStart = observationSequence;
    await navigate(() => page.reload());
    await expect(page.getByLabel("Scene", { exact: true })).toHaveValue(sceneId);
    await requireArtwork(videoCard, "Source thumbnail");
    await requireArtwork(audioClip, "Classic element waveform");
    const rebuiltThumbnailUrl = await videoCard.locator("img").getAttribute("src");
    expect(beforeReloadUrl).toMatch(/^blob:/); expect(rebuiltThumbnailUrl).toMatch(/^blob:/);
    expect(rebuiltThumbnailUrl).not.toBe(beforeReloadUrl);
    const rebuiltThumbnail = await tool(toolName, { project_id: projectId, revision: afterGain.revision, asset_id: video.id,
      request_id: crypto.randomUUID(), query: { kind: "thumbnail" } });
    expect(rebuiltThumbnail.data.content_hash).toBe(thumbnail.data.content_hash);
    expect(rebuiltThumbnail.version).toEqual(thumbnail.version); expect(rebuiltThumbnail.identity).toEqual(thumbnail.identity);
    const rebuiltDisplay = await display(afterGain.revision, null, reloadObservationStart);
    const rebuiltPixels = await pixels(rebuiltDisplay);
    expect(rebuiltPixels).toEqual(committedPixels);
    expect(await tool("opencut_project_inspect", { project_id: projectId, revision: null })).toEqual(afterGain);
    expect(await page.locator(".editor-timeline-element").evaluateAll(nodes => nodes.map(node => node.dataset.elementId))).toEqual(beforeReloadIds);
    expect(writes.length).toBe(writeStart + 1);
    check("source art: whole-page reload rebuilds a new JPEG ObjectURL and identical Classic display at the unchanged fixed head", {
      source_art_reload: { revision: afterGain.revision, thumbnail_hash: rebuiltThumbnail.data.content_hash,
        identity: rebuiltThumbnail.identity, rebuilt_pixels: rebuiltPixels,
        cache_scope: "Actual product Web reconstruction; native cache bytes/eviction and decode count require separate evidence." },
    });
    // Metadata-only dense source-art matrix. Two existing CAS resources are
    // reused by 100 clips; no new media or audio-playback benchmark is created.
    const denseId = crypto.randomUUID();
    const copied = await tool("opencut_project_copy", { source_project_id: projectId, source_revision: afterGain.revision,
      project_id: denseId, name: "Dense source artwork acceptance", idempotency_key: crypto.randomUUID() });
    projects.push(denseId);
    const dense = structuredClone(afterGain.project.sequences.find(value => value.id === sceneId));
    const templates = dense.tracks.flatMap(track => track.elements);
    dense.tracks = Array.from({ length: 5 }, (_, trackIndex) => {
      const kind = trackIndex < 3 ? "video" : "audio", original = dense.tracks.find(track => track.kind === kind);
      return { ...structuredClone(original), id: crypto.randomUUID(), name: `Dense artwork ${trackIndex + 1}`, enabled: true,
        muted: false, locked: false, channels: [], gain: { unit: "decibels", value: 0 },
        elements: Array.from({ length: 20 }, (_, index) => {
          const element = structuredClone(templates.find(element => element.content.kind === kind));
          element.id = crypto.randomUUID(); element.name = `Dense artwork ${trackIndex + 1}:${index + 1}`;
          element.enabled = true; element.channels = [];
          element.timeline_range = { start: time(296 * index, 19), duration: time(4) };
          // Reset every copied element explicitly; the earlier -12dB save is
          // not a default for this ordinary, unanimated 0dB display baseline.
          element.content.audio.gain = { unit: "decibels", value: 0 }; element.content.audio.muted = false;
          element.content.retime = { ...element.content.retime, rate: { numerator: 1, denominator: 1 }, maintain_pitch: false };
          if (kind === "video") element.content.source_audio_enabled = false;
          return element;
        }) };
    });
    dense.main_track_id = dense.tracks[0].id;
    const denseReceipt = await tool("opencut_edit_apply", { project_id: denseId, expected_revision: copied.revision,
      idempotency_key: crypto.randomUUID(), label: "Dense source artwork metadata baseline", operations: [{ type: "sequence_put", sequence: dense }] });
    const denseFixed = await tool("opencut_project_inspect", { project_id: denseId, revision: denseReceipt.revision });
    const denseIds = dense.tracks.flatMap(track => track.elements.map(element => element.id));
    const budget = { navigation_100_dom_ms: 2000, input_two_raf_p95_ms: 50, input_two_raf_max_ms: 100, heap_delta_bytes: 64 * 1024 * 1024 };
    const evidence = { machine: { platform: os.platform(), release: os.release(), architecture: os.arch(), cpu: os.cpus()[0]?.model, logical_cpus: os.cpus().length,
      browser_version: browser.version() }, budget, fixture: { project_id: denseId, revision: denseReceipt.revision, sequence_id: sceneId,
      tracks: 5, clips: 100, seconds: 300, start: "296*k/19, k=0..19", duration: "4/1", rate: "1/1", unique_resources: 2,
      versions: [video.version, audio.version], gain_db: 0, gain_animation: false, maintain_pitch: false }, samples: [],
      cache_decode_count: "unverified: public HTTP responses do not observe native decode count or cache occupancy",
      playback: "not measured; source-art viewport matrix only", long_source: "covered by separate real native 300-second source test, not this 4-second CAS reuse" };
    const cdp = await page.context().newCDPSession(page); await cdp.send("Performance.enable");
    const heap = async () => (await cdp.send("Performance.getMetrics")).metrics.find(metric => metric.name === "JSHeapUsedSize").value;
    const heaps = [];
    try {
      await navigate(() => page.goto(`${endpoint}/editor`));
      await expect(page.getByRole("button", { name: "New project", exact: true })).toBeEnabled();
      await cdp.send("HeapProfiler.collectGarbage");
      const heapBefore = await heap(); heaps.push(heapBefore);
      const opening = performance.now();
      await navigate(() => page.goto(`${endpoint}/editor/${encodeURIComponent(denseId)}?scene=${encodeURIComponent(sceneId)}`));
      await expect(page.locator(".editor-timeline-element")).toHaveCount(100); await expect(page.locator(".editor-track-name")).toHaveCount(5);
      evidence.navigation_100_dom_ms = performance.now() - opening;
      await requireArtwork(page.locator(`[data-element-id="${dense.tracks[0].elements[0].id}"]`), "Source thumbnail");
      evidence.first_video_art_ms = performance.now() - opening;
      // Five 80px tracks exceed the real 320px vertical viewport. Reveal the
      // first audio art before expecting its lazy visible-only decoder result.
      const firstAudio = page.locator(`[data-element-id="${dense.tracks[3].elements[0].id}"]`);
      await firstAudio.locator(".editor-timeline-art").scrollIntoViewIfNeeded();
      await requireArtwork(firstAudio, "Classic element waveform");
      evidence.first_audio_art_ms = performance.now() - opening;
      evidence.art_timing_method = "Navigation to first video art; navigation plus explicit vertical reveal to first audio art. No initial hidden-track readiness claim.";
      heaps.push(await heap());
      const denseWriteStart = writes.length;
      await page.evaluate(() => {
        const data = window.sourceArtDenseMetrics = { inputs: [], long_tasks: [], raf_gaps: [], last_raf: null, active: true };
        const observer = new PerformanceObserver(list => { for (const entry of list.getEntries()) if (data.long_tasks.length < 512) data.long_tasks.push(entry.duration); });
        observer.observe({ type: "longtask", buffered: false });
        const input = event => {
          if (!event.isTrusted || !event.target.closest?.(".editor-timeline-panel")) return;
          const at = performance.now(); requestAnimationFrame(() => requestAnimationFrame(() => {
            if (data.inputs.length < 256) data.inputs.push({ type: event.type, two_raf_ms: performance.now() - at });
          }));
        };
        for (const type of ["keydown", "pointerdown", "input"]) document.addEventListener(type, input, true);
        const tick = at => { if (!data.active) return; if (data.last_raf !== null && data.raf_gaps.length < 2048) data.raf_gaps.push(at - data.last_raf); data.last_raf = at; requestAnimationFrame(tick); };
        requestAnimationFrame(tick);
        data.stop = () => { data.active = false; observer.disconnect(); for (const type of ["keydown", "pointerdown", "input"]) document.removeEventListener(type, input, true); };
      });
      const denseZoom = page.getByRole("slider", { name: "Timeline zoom", exact: true });
      await denseZoom.focus();
      for (let index = 0; index < 20; index++) {
        await denseZoom.press(index < 10 ? "ArrowRight" : "ArrowLeft");
        await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))); heaps.push(await heap());
      }
      const denseViewport = page.locator(".editor-timeline-viewport");
      for (const index of [0, 10, 19]) {
        const track = dense.tracks[3], element = track.elements[index];
        const clip = page.locator(`[data-element-id="${element.id}"]`), select = clip.locator(".editor-element-select");
        await select.scrollIntoViewIfNeeded();
        // Scroll to an actual visible intersection, then physically select.
        const point = await select.evaluate(button => {
          const rect = button.getBoundingClientRect(), bounds = { left: Math.max(0, rect.left), right: Math.min(innerWidth, rect.right), top: Math.max(0, rect.top), bottom: Math.min(innerHeight, rect.bottom) };
          for (let parent = button.parentElement; parent; parent = parent.parentElement) {
            const style = getComputedStyle(parent), box = parent.getBoundingClientRect(), left = box.left + parent.clientLeft, top = box.top + parent.clientTop;
            if (/(auto|scroll|hidden|clip)/.test(style.overflowX)) { bounds.left = Math.max(bounds.left, left); bounds.right = Math.min(bounds.right, left + parent.clientWidth); }
            if (/(auto|scroll|hidden|clip)/.test(style.overflowY)) { bounds.top = Math.max(bounds.top, top); bounds.bottom = Math.min(bounds.bottom, top + parent.clientHeight); }
          }
          const label = button.closest(".editor-track").querySelector(".editor-track-name").getBoundingClientRect(); bounds.left = Math.max(bounds.left, label.right);
          const x = (bounds.left + bounds.right) / 2, y = (bounds.top + bounds.bottom) / 2;
          return { x, y, bounds, hit: button.contains(document.elementFromPoint(x, y)) };
        });
        expect(point.bounds.right, JSON.stringify(point)).toBeGreaterThan(point.bounds.left); expect(point.bounds.bottom, JSON.stringify(point)).toBeGreaterThan(point.bounds.top);
        expect(point.hit, JSON.stringify(point)).toBe(true);
        await page.mouse.move(point.x, point.y); await page.mouse.down(); await page.mouse.up(); await expect(select).toHaveAttribute("aria-pressed", "true");
        if (index === 10) {
          // No intermediate artwork waits: late prior windows must not win
          // over the final partially clipped display's fixed viewport.
          await denseZoom.focus();
          for (const key of ["ArrowRight", "ArrowRight", "ArrowLeft", "ArrowLeft"]) await denseZoom.press(key);
          await denseViewport.evaluate((node, id) => {
            const art = node.querySelector(`[data-element-id="${id}"] .editor-timeline-art`), rect = art.getBoundingClientRect(), viewport = node.getBoundingClientRect();
            node.scrollLeft += rect.left - viewport.left + rect.width / 2;
            node.dispatchEvent(new Event("scroll"));
          }, element.id);
        }
        await requireArtwork(clip, "Classic element waveform");
        const denseCanvas = clip.getByRole("img", { name: "Classic element waveform", exact: true });
        let observed;
        await expect.poll(async () => {
          // Match the LAST rendered viewport, not any earlier successful HTTP.
          const actual = await clip.locator(".editor-timeline-art").evaluate(node => {
            const rect = node.getBoundingClientRect(), viewport = node.closest(".editor-timeline-viewport").getBoundingClientRect();
            return { clip_left_px: Math.max(0, viewport.left - rect.left), clip_right_px: Math.min(rect.width, viewport.right - rect.left),
              pixels_per_second: rect.width / 4, css_height: rect.height, device_pixel_ratio: devicePixelRatio };
          });
          const canvasActual = await denseCanvas.evaluate(node => {
            const rect = node.getBoundingClientRect(), parent = node.closest(".editor-timeline-art").getBoundingClientRect();
            return { width: node.width, height: node.height, temporary_gain: node.dataset.temporaryGain,
              inline: { left: node.style.left, width: node.style.width, height: node.style.height },
              layout: { left: rect.left - parent.left, width: rect.width, height: rect.height } };
          });
          // Retain only this poll's small geometry snapshot; no bars/media or
          // attempt history. This distinguishes raw scale, layout and CSSOM
          // serialization without hiding a mismatch behind a wider tolerance.
          evidence.viewport_poll = { index, element_id: element.id, actual, canvas: canvasActual,
            recent: observations.filter(value => value.operation === displayTool && value.request?.project_id === denseId &&
              value.request.revision === denseReceipt.revision && value.request.element_id === element.id).slice(-3).map(value => ({
                sequence: value.sequence, viewport: value.request.viewport, canvas: value.envelope?.data?.canvas,
                error_code: value.envelope?.error?.code ?? null })) };
          observed = observations.findLast(value => value.operation === displayTool && value.request?.project_id === denseId && value.request.revision === denseReceipt.revision &&
            value.request.sequence_id === sceneId && value.request.track_id === track.id && value.request.element_id === element.id && value.request.asset_id === audio.id &&
            value.request.temporary_gain === null && value.envelope?.error === null && Object.entries(actual).every(([key, value2]) => Math.abs(value.request.viewport[key] - value2) < 1e-7));
          evidence.viewport_poll.matched_response = !!observed;
          if (!observed) return false;
          const expected = observed.envelope.data.canvas;
          // CSSOM strings serialize fractional positions with rounded decimal
          // digits. Actual layout is the product geometry being verified.
          return canvasActual.width === expected.width && canvasActual.height === expected.height &&
            Math.abs(canvasActual.layout.left - expected.css_left) < 1e-7 && Math.abs(canvasActual.layout.width - expected.css_width) < 1e-7 &&
            Math.abs(canvasActual.layout.height - expected.css_height) < 1e-7 && canvasActual.temporary_gain === "saved";
        }).toBe(true);
        if (index === 10) {
          expect(observed.request.viewport.clip_left_px).toBeGreaterThan(0);
          expect(observed.request.viewport.clip_right_px - observed.request.viewport.clip_left_px)
            .toBeLessThan(4 * observed.request.viewport.pixels_per_second);
        }
        assertDisplayOracle(observed.envelope.data, observed.request, oracle, 0); expect(observed.envelope.data.version).toEqual(audio.version);
        evidence.samples.push({ index, element_id: element.id, viewport: observed.request.viewport, bars: observed.envelope.data.bars.length,
          pixels: await pixels(observed, denseCanvas), scroll_left: await denseViewport.evaluate(node => node.scrollLeft) });
        heaps.push(await heap());
      }
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      const metrics = await page.evaluate(() => { const data = window.sourceArtDenseMetrics; data.stop(); return { inputs: data.inputs, long_tasks: data.long_tasks, raf_gaps: data.raf_gaps }; });
      const latencies = metrics.inputs.map(value => value.two_raf_ms).sort((a, b) => a - b);
      expect(metrics.inputs.filter(value => value.type === "keydown").length).toBeGreaterThanOrEqual(20);
      expect(metrics.inputs.filter(value => value.type === "pointerdown").length).toBeGreaterThanOrEqual(3);
      evidence.input_count = latencies.length; evidence.input_two_raf_p95_ms = latencies[Math.ceil(latencies.length * .95) - 1]; evidence.input_two_raf_max_ms = latencies.at(-1);
      evidence.long_tasks_ms = metrics.long_tasks; evidence.raf_gap_max_ms = Math.max(0, ...metrics.raf_gaps);
      heaps.push(await heap()); evidence.heap_baseline_bytes = heapBefore; evidence.heap_peak_bytes = Math.max(...heaps); evidence.heap_delta_bytes = evidence.heap_peak_bytes - heapBefore;
      evidence.heap_method = "CDP JSHeapUsedSize, baseline GC before navigation only; raw sampled peak after open, each zoom and each viewport. No retained-leak claim.";
      expect(await page.getByLabel("Scene", { exact: true }).inputValue()).toBe(sceneId);
      expect(await page.locator(".editor-timeline-element").evaluateAll(nodes => nodes.map(node => node.dataset.elementId))).toEqual(denseIds);
      expect(await tool("opencut_project_inspect", { project_id: denseId, revision: null })).toEqual(denseFixed);
      expect(writes.length).toBe(denseWriteStart);
      expect(evidence.navigation_100_dom_ms, JSON.stringify(evidence)).toBeLessThanOrEqual(budget.navigation_100_dom_ms);
      expect(evidence.input_two_raf_p95_ms, JSON.stringify(evidence)).toBeLessThanOrEqual(budget.input_two_raf_p95_ms);
      expect(evidence.input_two_raf_max_ms, JSON.stringify(evidence)).toBeLessThanOrEqual(budget.input_two_raf_max_ms);
      expect(evidence.heap_delta_bytes, JSON.stringify(evidence)).toBeLessThanOrEqual(budget.heap_delta_bytes);
      evidence.rapid_partial_clip = { index: 10, zoom_inputs: 4, intermediate_artwork_waits: 0 };
      check("source art dense: 100 clips / 5 tracks / 300 seconds, 20 zoom inputs plus rapid partially-clipped zoom and first/middle/last actual display viewports meet fixed UI/heap budgets without changing head or IDs", { source_art_dense: evidence });
    } catch (error) {
      if (error instanceof Error) error.message += `\nDense source artwork evidence: ${JSON.stringify(evidence).slice(0, 12000)}`;
      throw error;
    } finally {
      try { await page.evaluate(() => window.sourceArtDenseMetrics?.stop()); } catch {}
      await cdp.detach();
    }
    // The unified workspace owns these small fixtures. No new source media is
    // made by this verifier; all four follow the real upload/CAS/product path.
    await navigate(() => page.goto(`${endpoint}/editor/${encodeURIComponent(projectId)}?scene=${encodeURIComponent(sceneId)}`));
    await expect(page.getByRole("button", { name: "Import media", exact: true })).toBeEnabled();
    for (const name of ["tiny.png", "tiny.jpg", "short.mp4", "long.wav"]) await importMedia(name);
    const varied = await tool("opencut_project_inspect", { project_id: projectId, revision: null });
    const variedEvidence = [];
    let smallThumbnail, smallSelection;
    for (const name of ["tiny.png", "tiny.jpg", "short.mp4", "long.wav"]) {
      const asset = varied.project.assets.find(value => value.name === name);
      expect(asset, name).toBeTruthy();
      const card = page.locator(".editor-media-card").filter({ hasText: name });
      await card.scrollIntoViewIfNeeded();
      await requireArtwork(card, name === "long.wav" ? "Source peaks before element gain" : "Source thumbnail");
      const query = name === "long.wav" ? { kind: "waveform", sample_rate: 48000, projection: { kind: "source", range: null, bins: 128 } }
        : { kind: "thumbnail" };
      const response = await tool(toolName, { project_id: projectId, revision: varied.revision, asset_id: asset.id,
        request_id: crypto.randomUUID(), query });
      expect(response).not.toHaveProperty("schema_version"); expect(response.project_id).toBe(projectId); expect(response.revision).toBe(varied.revision);
      expect(response.asset_id).toBe(asset.id); expect(response.version).toEqual(asset.version);
      for (const field of ["browser_sha256", "node_sha256", "module_sha256"]) expect(response.identity[field]).toMatch(/^[a-f0-9]{64}$/);
      if (name === "long.wav") {
        expect(response.identity.algorithm).toBe("source-peaks128-v1");
        expect(response.identity.decode_profile).toBe("fixed-cas-streamed-concat-webaudio-canvas");
        expect(response.data.kind).toBe("waveform"); expect(response.data.total_samples).toBe("576000");
        expect(response.data.sample_rate).toBe(48000); expect(response.data.channels).toBe(1); expect(response.data.bucket_size).toBe(128);
        expect(response.data.amplitudes).toHaveLength(128);
        const original = await pcm16Oracle(path.join(directory, name), 576000);
        for (let index = 0; index < 128; index++) {
          const start = Math.floor(index * original.frames / 128), end = Math.ceil((index + 1) * original.frames / 128);
          let peak = 0;
          for (let bucket = Math.floor(start / 128); bucket < Math.ceil(end / 128); bucket++) peak = Math.max(peak, original.summary[bucket]);
          expect(Math.fround(response.data.amplitudes[index])).toBe(peak);
        }
        variedEvidence.push({ source: name, total_samples: response.data.total_samples, bins: 128,
          oracle: "Independent original PCM16 bytes with complete 128-sample source bucket projection", identity: response.identity });
      } else {
        expect(response.identity.algorithm).toBe("classic-thumbnail-1s-jpeg08-v1");
        expect(response.data.kind).toBe("thumbnail_ready");
        const encoded = Buffer.from(response.data.jpeg_bytes_hex, "hex");
        expect(encoded.length).toBeLessThanOrEqual(1024 * 1024);
        expect(createHash("sha256").update(encoded).digest("hex")).toBe(response.data.content_hash);
        await expect.poll(() => card.locator("img").evaluate(node => node.complete && node.naturalWidth > 0)).toBe(true);
        if (name === "short.mp4") {
          // Classic/MB getSample selects the last start <= 1s, even after EOF;
          // null means before the first sample, not a guessed post-end hold gate.
          expect(response.data.width).toBe(640); expect(response.data.height).toBe(360);
          expect(response.data.observation.kind).toBe("video"); expect(response.data.observation.requested_seconds).toBe(1);
          expect(response.data.observation.actual_timestamp_seconds).toBeGreaterThanOrEqual(0);
          expect(response.data.observation.actual_timestamp_seconds).toBeLessThan(.5);
          expect(response.data.observation.frame_duration_seconds).toBeGreaterThan(0);
          variedEvidence.push({ source: name, observation: response.data.observation, content_hash: response.data.content_hash,
            semantics: "Classic 1-second last-start sample; explicit NoFrame-before-first-PTS remains independently native-tested", identity: response.identity });
        } else {
          expect(response.data.width).toBe(32); expect(response.data.height).toBe(24);
          expect(response.data.observation).toEqual({ kind: "static_image", source_width: 32, source_height: 24 });
          const rgba = await page.evaluate(async hex => {
            const bytes = Uint8Array.from(hex.match(/../g), pair => Number.parseInt(pair, 16));
            const bitmap = await createImageBitmap(new Blob([bytes], { type: "image/jpeg" }));
            const canvas = document.createElement("canvas"); canvas.width = bitmap.width; canvas.height = bitmap.height;
            try { const context = canvas.getContext("2d"); context.drawImage(bitmap, 0, 0); return [...context.getImageData(16, 12, 1, 1).data]; }
            finally { bitmap.close(); canvas.width = 0; canvas.height = 0; }
          }, response.data.jpeg_bytes_hex);
          for (let index = 0; index < 3; index++) expect(Math.abs(rgba[index] - [255, 0, 0][index])).toBeLessThanOrEqual(2);
          expect(rgba[3]).toBe(255);
          variedEvidence.push({ source: name, dimensions: [32, 24], rgba, expected_rgb: [255, 0, 0], jpeg_rgb_tolerance: 2,
            content_hash: response.data.content_hash, identity: response.identity });
          if (name === "tiny.png") { smallThumbnail = response; smallSelection = { projectId, revision: varied.revision, asset, query }; }
        }
      }
    }
    expect(await tool("opencut_project_inspect", { project_id: projectId, revision: null })).toEqual(varied);
    check("source art: static PNG/JPEG retain tiny dimensions and independent red pixels, short video preserves Classic 1-second last-sample selection, and 12-second PCM peaks match original source bytes", {
      source_art_varied: { revision: varied.revision, sources: variedEvidence },
    });
    const cacheEvidence = await verifyCacheAllocations(browser,
      { response: thumbnail, selection: { projectId, revision: fixed.revision, asset: video, query: { kind: "thumbnail" } } },
      { response: smallThumbnail, selection: smallSelection });
    check("source art cache: isolated production browser allocations hit real 32 MiB and 256-entry eviction, rebuild evicted JPEG URLs and clear all charged bytes", {
      source_art_cache_allocations: cacheEvidence,
    });
    const oldUrls = await page.evaluate(() => globalThis.__sourceArtUrls.created.filter(value => value.type === "image/jpeg").map(value => value.url));
    expect(oldUrls.length).toBeGreaterThan(0);
    await page.getByRole("button", { name: "Back to projects", exact: true }).click();
    const emptyId = await createProject("Source artwork empty scope");
    await expect(page.locator(".editor-media-card")).toHaveCount(0);
    await expect(page.getByRole("img", { name: "Source thumbnail", exact: true })).toHaveCount(0);
    await expect(page.getByRole("img", { name: "Source peaks before element gain", exact: true })).toHaveCount(0);
    await expect(page.getByRole("img", { name: "Classic element waveform", exact: true })).toHaveCount(0);
    await expect.poll(() => page.evaluate(urls => urls.every(url => globalThis.__sourceArtUrls.revoked.includes(url)), oldUrls)).toBe(true);
    expect(new URL(page.url()).pathname.endsWith(`/${emptyId}`)).toBe(true);
    check("source art: opening another project releases old JPEG ObjectURLs and leaves no prior artwork mounted", {
      source_art_scope: { released_jpeg_urls: oldUrls.length, credential_revocation: "covered separately by native/host lifecycle tests; not claimed by this project-switch fixture" },
    });
    await Promise.allSettled([...pending]);
    const sessionAbortEvidence = sessionFailures.map(({ request, record, witness, explicitly_navigated }) => {
      record.response_status = responseStatuses.get(request) ?? record.response_status;
      const settled = sessions.get(witness);
      const aborted = /ERR_ABORTED|NS_BINDING_ABORTED/.test(record.error ?? "");
      const fulfilledNoContent = record.method === "POST" && !record.navigation && record.response_status === 204 &&
        settled?.started === true && settled.settled === "fulfilled" && settled.status === 204;
      const classification = aborted && fulfilledNoContent ? "fulfilled_204_body_abort"
        : aborted && explicitly_navigated ? "explicit_navigation_of_active_request" : "unexpected";
      if (classification === "unexpected") failedRequests.push(record);
      return { ...record, classification, settled: settled?.settled ?? "unobserved", fetch_status: settled?.status ?? null };
    });
    expect(pageErrors).toEqual([]); expect(failedRequests).toEqual([]);
    check("source art: no page errors or unexpected failed requests", { source_art_browser: { page_errors: 0, failed_requests: 0,
      expected_waiter_aborts: cancelledRequests.length, session_abort_evidence: sessionAbortEvidence,
      observation_budget: { ...observationBudget, retained_entries: observations.length, peak_encoded_bytes: observationPeakBytes,
        responses_observed: observationSequence, method: "Source metadata only; exact fixed element/temporary-gain/viewport duplicates replace in place, distinct viewports retain complete bars. Exceeding either cap fails acceptance; no silent eviction." } } });
  } catch (error) {
    failure = error;
    // Keep failure evidence in the existing runner report only. Never include
    // encoded media or large waveform arrays in diagnostic output.
    let mountedErrors = [];
    try { mountedErrors = await page.getByLabel("Source artwork unavailable", { exact: true }).evaluateAll(nodes => nodes.slice(0, 8).map(node => ({
      title: (node.getAttribute("title") ?? "").slice(0, 400),
      surface: node.closest(".editor-media-card") ? "media" : node.closest(".editor-timeline-art") ? "timeline" : "other",
    }))); } catch {}
    const responses = observations.slice(-12).map(({ operation, request, envelope }) => ({
      operation,
      asset_id: request?.asset_id, revision: request?.revision, query: request?.query, element_id: request?.element_id, temporary_gain: request?.temporary_gain,
      error: envelope?.error ? { code: envelope.error.code, message: String(envelope.error.message ?? "").slice(0, 400) } : null,
      result: envelope?.data ? { kind: envelope.data.data?.kind, version: envelope.data.version, identity: envelope.data.identity,
        width: envelope.data.data?.width, height: envelope.data.data?.height,
        jpeg_bytes: envelope.data.data?.jpeg_bytes,
        amplitude_count: envelope.data.data?.amplitude_count, display_profile: envelope.data.display_profile, display_bars: envelope.data.bars?.length } : null,
    }));
    if (failure instanceof Error) failure.message += `\nSource artwork diagnostics: ${JSON.stringify({ mounted_errors: mountedErrors,
      page_errors: pageErrors.slice(0, 8).map(value => String(value).slice(0, 1000)),
      observation_budget: { ...observationBudget, retained_entries: observations.length, retained_encoded_bytes: observationBytes },
      failed_requests: failedRequests.slice(0, 8), session_failures: sessionFailures.slice(0, 8).map(({ record, witness, explicitly_navigated }) => ({
        ...record, explicitly_navigated, fetch: sessions.get(witness) ?? null })), responses, pending_responses: pending.size }).slice(0, 16000)}`;
    throw error;
  }
  finally {
    const cleanup = [];
    try { await navigate(() => page.goto(`${endpoint}/editor`)); } catch (error) { cleanup.push(String(error)); }
    for (const projectId of projects.reverse()) try {
      const head = await tool("opencut_project_inspect", { project_id: projectId, revision: null });
      await tool("opencut_project_delete", { idempotency_key: crypto.randomUUID(), projects: [{ project_id: projectId, expected_revision: head.revision }] });
    } catch (error) { cleanup.push(String(error)); }
    await page.close();
    if (cleanup.length) {
      if (failure instanceof Error) failure.message += `\nSource artwork cleanup: ${cleanup.join("\n")}`;
      else throw new Error(cleanup.join("\n"));
    }
  }
}
