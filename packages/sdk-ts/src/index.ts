export { requestEnvelope, OpenCutApiError, OpenCutProtocolError, OpenCutTransportError, type Envelope } from "./transport.js";
import { requestEnvelope, OpenCutApiError, OpenCutProtocolError, OpenCutTransportError, type Envelope } from "./transport.js";
export * from "./contracts.generated.js";
import { toolNames, domainNames, domainActions, type ToolMap, type DomainMap, type JobResultMap, type AudioPreviewPcmV4, type AudioPreviewStreamV4 } from "./contracts.generated.js";

export type AudioPreviewPcmHeaderV4 = AudioPreviewPcmV4.AudioPreviewPcmHeaderV4;
export type AudioPreviewStreamInputV4 = AudioPreviewStreamV4.AudioPreviewStreamV4;
export type ToolName = keyof ToolMap;
export type ToolInput<N extends ToolName> = ToolMap[N]["input"];
export type ToolOutput<N extends ToolName> = ToolMap[N]["output"];
export type DomainName = keyof DomainMap & string;
export type DomainAction<D extends DomainName> = keyof DomainMap[D] & string;
export type DomainInput<D extends DomainName, A extends DomainAction<D>> =
  DomainMap[D][A] extends { input: infer I } ? I : never;
export type DomainOutput<D extends DomainName, A extends DomainAction<D>> =
  DomainMap[D][A] extends { output: infer O } ? O : never;
export type Job = ToolOutput<"opencut_job_get">;
export type PublicJob = ToolOutput<"opencut_job_view">;
export type HostCapabilities = Extract<ToolOutput<"opencut_resource_read">["payload"], {kind: "capabilities"}>["data"];
export type ArtifactReference = PublicJob["artifacts"][number];
export type PublicJobResult = NonNullable<PublicJob["result"]>;
type LegacyPublicResultMap = { [R in PublicJobResult as R["kind"]]: R["data"] };
export type PublicResultMap = LegacyPublicResultMap & Pick<JobResultMap,
  "media.import" | "analysis.transcribe" | "analysis.silence" | "analysis.shots" | "analysis.waveform" | "analysis.speaker_turns" | "render.project_audio.v4" | "render.project_audio_range.v4" | "render.project_frame.v4" | "render.project_text_frame.v4" | "render.project_caption_frame.v4" | "analysis.sequence_transcribe.v4" | "render.project_video.v4"> & {
  "render.project.v3": LegacyPublicResultMap["render.project"];
  "preview.project_frame.v3": LegacyPublicResultMap["preview.composited_frame"];
  "preview.project_range.v3": LegacyPublicResultMap["preview.project_range"];
};
const extendedPublicKinds = new Set<string>([
  "media.import", "analysis.transcribe", "analysis.silence", "analysis.shots", "analysis.waveform", "analysis.speaker_turns",
  "render.project.v3", "preview.project_frame.v3", "preview.project_range.v3",
  "render.project_audio.v4", "render.project_audio_range.v4", "render.project_frame.v4", "render.project_text_frame.v4", "render.project_caption_frame.v4", "analysis.sequence_transcribe.v4", "render.project_video.v4",
]);
export function publicJobResult<K extends keyof PublicResultMap>(job: PublicJob, kind: K): PublicResultMap[K] {
  if (job.state === "succeeded" && job.kind === kind && !job.result_unavailable) {
    if (job.result?.kind === kind && !job.extended_result) return job.result.data as PublicResultMap[K];
    const result = job.extended_result;
    if (extendedPublicKinds.has(kind) && !job.result && result?.kind === kind &&
      result.schema_version === 1 && object(result.data)) return result.data as PublicResultMap[K];
  }
  throw new TypeError(`Expected an available successful ${kind} result.`);
}
/** Narrow a successful job result by its registered kind; rejects a wrong kind or unfinished job. */
export function jobResult<K extends keyof JobResultMap>(job: Job, kind: K): JobResultMap[K] {
  if (job.state !== "succeeded" || job.kind !== kind || job.result === null || job.result === undefined) {
    throw new TypeError(`Expected a successful ${kind} job.`);
  }
  return job.result as JobResultMap[K];
}
export class JobWaitTimeoutError extends Error {
  readonly name = "JobWaitTimeoutError";
  readonly code = "JOB_WAIT_TIMEOUT";
  constructor(readonly jobId: string) { super(`Stopped waiting for ${jobId}; the job has not been cancelled.`); }
}

export class JobCompletionError<J extends Job | PublicJob = Job> extends Error {
  readonly name = "JobCompletionError";
  readonly code: "JOB_FAILED" | "JOB_CANCELLED";
  constructor(readonly job: J) {
    super(`Job ${job.job_id} ended with state ${job.state}.`);
    this.code = job.state === "cancelled" ? "JOB_CANCELLED" : "JOB_FAILED";
  }
}

export interface ClientOptions {
  baseUrl: string;
  token: string;
  /** Standard fetch by default; injectable for a host's network policy. */
  fetch?: typeof globalThis.fetch;
}
export interface CallOptions {
  signal?: AbortSignal;
  /** Standard fetch best-effort delivery during document departure. */
  keepalive?: boolean;
}
export interface AdmissionRetryOptions extends CallOptions {
  /** Opt-in deferral of explicit budget-lock contention; never retries an unknown outcome. */
  admissionTimeoutMs?: number;
  beforeAttempt?: () => void;
}
export interface WaitOptions extends CallOptions {
  /** Local deadline, including network time. Does not cancel the durable job. */
  timeoutMs?: number;
  pollIntervalMs?: number;
}

function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function pause(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    signal.throwIfAborted();
    const abort = () => { clearTimeout(timer); reject(signal.reason); };
    const timer = setTimeout(() => { signal.removeEventListener("abort", abort); resolve(); }, ms);
    signal.addEventListener("abort", abort, { once: true });
  });
}

/** Calls are one-shot unless admission deferral is explicitly requested. No revision refreshes, implicit job cancellation or UI state. */

export class OpenCutClient {
  readonly baseUrl: string;
  #token: string;
  #fetch: typeof globalThis.fetch;

  constructor(options: ClientOptions) {
    const url = new URL(options.baseUrl);
    if (!["http:", "https:"].includes(url.protocol) || url.username || url.password || url.search || url.hash) {
      throw new TypeError("baseUrl must be an HTTP(S) URL without credentials, query or fragment.");
    }
    this.baseUrl = url.href.replace(/\/$/, "");
    this.#token = options.token;
    this.#fetch = options.fetch ?? globalThis.fetch.bind(globalThis);
  }

  async call<N extends ToolName>(name: N, input: ToolInput<N>, options: CallOptions = {}): Promise<ToolOutput<N>> {
    return (await this.callEnvelope(name, input, options)).data;
  }

  /** Repeat only explicit STORAGE_BUDGET_BUSY rejection, with the same captured input.
   * Network/protocol errors, capacity faults and revision conflicts remain one-shot. */
  async callWithAdmissionRetry<N extends ToolName>(name: N, input: ToolInput<N>, options: AdmissionRetryOptions = {}): Promise<ToolOutput<N>> {
    const timeout = options.admissionTimeoutMs ?? 5000;
    if (!Number.isSafeInteger(timeout) || timeout < 1 || timeout > 30000) throw new TypeError("Admission retry deadline must be 1 to 30000 milliseconds.");
    const captured = JSON.parse(JSON.stringify(input)) as ToolInput<N>;
    const deadline = performance.now() + timeout;
    const signal = options.signal ?? new AbortController().signal;
    for (;;) {
      signal.throwIfAborted();
      options.beforeAttempt?.();
      try { return await this.call(name, captured, { signal, keepalive: options.keepalive }); }
      catch (error) {
        if (!(error instanceof OpenCutApiError) || error.code !== "STORAGE_BUDGET_BUSY" || performance.now() >= deadline) throw error;
        await pause(Math.min(100, Math.max(1, deadline - performance.now())), signal);
        if (performance.now() >= deadline) throw error;
      }
    }
  }

  /** Call one typed action through the compact domain surface. */
  async callDomain<D extends DomainName, A extends DomainAction<D>>(
    domain: D, action: A, input: DomainInput<D, A>, options: CallOptions = {},
  ): Promise<DomainOutput<D, A>> {
    return (await this.callDomainEnvelope(domain, action, input, options)).data;
  }

  async callDomainEnvelope<D extends DomainName, A extends DomainAction<D>>(
    domain: D, action: A, input: DomainInput<D, A>, options: CallOptions = {},
  ): Promise<Envelope<DomainOutput<D, A>>> {
    if (!(domainNames as readonly string[]).includes(domain) ||
      !((domainActions as Record<string, readonly string[]>)[domain] ?? []).includes(action)) {
      throw new TypeError(`Unknown OpenCut domain action: ${domain}.${action}`);
    }
    const body = JSON.stringify({ action, input });
    return this.#request<DomainOutput<D, A>>(`/v1/tools/opencut_${domain}`, "POST", body, "application/json", options);
  }

  /** Typed additive discovery; the original v1 capabilities tool schema stays frozen. */
  async hostCapabilities(options: CallOptions = {}): Promise<HostCapabilities> {
    const resource = await this.call("opencut_resource_read", { uri: "opencut://system/capabilities" }, options);
    if (resource.payload.kind !== "capabilities") throw new OpenCutProtocolError("Expected the host capabilities resource.", 200);
    return resource.payload.data;
  }

  async callEnvelope<N extends ToolName>(name: N, input: ToolInput<N>, options: CallOptions = {}): Promise<Envelope<ToolOutput<N>>> {
    if (!(toolNames as readonly string[]).includes(name)) throw new TypeError(`Unknown OpenCut tool: ${name}`);
    options.signal?.throwIfAborted();
    // Serialize once before awaiting. Caller mutations cannot change an in-flight write.
    const body = JSON.stringify(input);
    return this.#request<ToolOutput<N>>(`/v1/tools/${name}`, "POST", body, "application/json", options);
  }

  /** Send one raw chunk. On an unknown outcome, inspect the confirmed offset before retrying. */
  uploadChunk(uploadId: string, offset: string, bytes: Uint8Array, options: CallOptions = {}): Promise<ToolOutput<"opencut_upload_inspect">> {
    return this.#putUploadChunk("uploads", uploadId, offset, bytes, options);
  }

  /** Same durable chunk protocol; the complete digest may still be undeclared. */
  uploadStreamChunk(uploadId: string, offset: string, bytes: Uint8Array, options: CallOptions = {}): Promise<ToolOutput<"opencut_upload_stream_inspect">> {
    return this.#putUploadChunk("stream-uploads", uploadId, offset, bytes, options);
  }

  async #putUploadChunk<T>(route: "uploads" | "stream-uploads", uploadId: string, offset: string, bytes: Uint8Array, options: CallOptions): Promise<T> {
    options.signal?.throwIfAborted();
    if (!/^upload_[a-f0-9]{32}$/.test(uploadId) || !/^(0|[1-9][0-9]{0,10})$/.test(offset) ||
      bytes.byteLength < 1 || bytes.byteLength > 4194304 ||
      BigInt(offset) + BigInt(bytes.byteLength) > 17179869184n) throw new TypeError("Invalid upload identity or chunk range.");
    const body = new Uint8Array(bytes);
    const hash = Array.from(new Uint8Array(await globalThis.crypto.subtle.digest("SHA-256", body)),
      b => b.toString(16).padStart(2, "0")).join("");
    const query = new URLSearchParams({ size_bytes: String(body.byteLength), content_hash: hash });
    return (await this.#request<T>(
      `/v1/${route}/${uploadId}/chunks/${offset}?${query}`, "PUT", body, "application/octet-stream", options,
    )).data;
  }

  #request<T>(path: string, method: string, body: BodyInit, contentType: string,
    options: CallOptions): Promise<Envelope<T>> {
    return requestEnvelope(this.#fetch, this.baseUrl, this.#token, path, {
      method, body, signal: options.signal, headers: { "Content-Type": contentType },
      ...(options.keepalive === undefined ? {} : { keepalive: options.keepalive }),
    });
  }

  /** Explicit durable cancellation; terminal state may still require waiting. */
  cancelJob(jobId: string, options: CallOptions = {}): Promise<Job> {
    return this.call("opencut_job_cancel", { job_id: jobId }, options);
  }

  /** Preferred task submission returns the public projection, with no execution inputs. */
  submitJob(input: ToolInput<"opencut_job_submit">, options: CallOptions = {}): Promise<PublicJob> {
    return this.call("opencut_job_submit", input, options);
  }
  cancelPublicJob(jobId: string, options: CallOptions = {}): Promise<PublicJob> {
    return this.call("opencut_job_request_cancel", { job_id: jobId }, options);
  }
  waitForPublicJob(jobId: string, options: WaitOptions = {}): Promise<PublicJob> {
    return this.wait(jobId, signal => this.call("opencut_job_view", { job_id: jobId }, { signal }), options);
  }
  waitForJob(jobId: string, options: WaitOptions = {}): Promise<Job> {
    return this.wait(jobId, signal => this.call("opencut_job_get", { job_id: jobId }, { signal }), options);
  }
  artifactUrl(reference: ArtifactReference): string {
    const id = `artifact:${reference.job_id}:${reference.content_hash}`;
    if (reference.artifact_id !== id || !/^artifact:job_[a-f0-9]{32}:[a-f0-9]{64}$/.test(id)) throw new TypeError("Invalid artifact reference");
    return `${this.baseUrl}/v1/artifacts/${encodeURIComponent(id)}`;
  }
  projectMediaUrl(resource: ToolOutput<"opencut_project_media_inspect">): string {
    if (!resource.project_id || !/^media_[a-f0-9]{32}$/.test(resource.media_id) ||
      resource.asset.uri !== `opencut-media://${resource.media_id}`) {
      throw new TypeError("Invalid project media resource");
    }
    return `${this.baseUrl}/v1/projects/${encodeURIComponent(resource.project_id)}/media/${resource.media_id}/content`;
  }
  /** Return authorized, hash-verified project media as a streaming response. */
  downloadProjectMedia(resource: ToolOutput<"opencut_project_media_inspect">, options: CallOptions = {}): Promise<Response> {
    return this.#downloadBytes(this.projectMediaUrl(resource), "project media", options);
  }
  /** Recompute the scoped URL; a descriptor cannot select an external URL or global CAS path. */
  projectV4ResourceUrl(resource: ToolOutput<"opencut_project_v4_resource_inspect">): string {
    const ids = [resource.project_id, resource.revision, resource.resource_id];
    if (ids.some(id => !id.trim() || new TextEncoder().encode(id).length > 1024 || /[\u0000-\u001f\u007f-\u009f]/u.test(id)) ||
      !["asset", "font"].includes(resource.kind) ||
      !/^[a-f0-9]{64}$/.test(resource.version.content_hash) ||
      resource.version.id !== `av_${resource.version.content_hash}`) throw new TypeError("Invalid V4 resource descriptor");
    const query = (id: string) => encodeURIComponent(id).replace(/[!'()*]/g, c => `%${c.charCodeAt(0).toString(16).toUpperCase()}`);
    const path = `/v1/project-v4-resources/content?project_id=${query(resource.project_id)}&revision=${query(resource.revision)}&kind=${resource.kind}&resource_id=${query(resource.resource_id)}`;
    if (resource.content_path !== path) throw new TypeError("V4 resource path differs from its scoped identity");
    return `${this.baseUrl}${path}`;
  }
  downloadProjectV4Resource(resource: ToolOutput<"opencut_project_v4_resource_inspect">, options: CallOptions = {}): Promise<Response> {
    return this.#downloadBytes(this.projectV4ResourceUrl(resource), "V4 resource", options);
  }
  /** Return a streaming response. The server verifies the published hash before serving bytes. */
  downloadArtifact(reference: ArtifactReference, options: CallOptions = {}): Promise<Response> {
    return this.#downloadBytes(this.artifactUrl(reference), "artifact", options);
  }
  /** Claim the current owner generation's continuous fMP4 stream. Cancel the response body to disconnect. */
  openPreviewStream(session: Pick<ToolOutput<"opencut_preview_create">, "session_id" | "generation" | "owner_id" | "lease_id">, options: CallOptions = {}): Promise<Response> {
    const { session_id, generation, owner_id, lease_id } = session;
    return this.#downloadBytes(`${this.baseUrl}/v1/preview/stream`, "preview stream", options,
      JSON.stringify({ session_id, generation, owner_id, lease_id }));
  }
  /** Open the bounded pre-master PCM stream for one fixed V4 audio generation.
   * Consumer reports return server-authored Time; queued frames are not playback. */
  openV4AudioPreviewStream(session: Pick<ToolOutput<"opencut_audio_preview_v4_create">, "session_id" | "generation" | "overlay_epoch" | "owner_id" | "lease_id">, options: CallOptions = {}): Promise<Response> {
    const { session_id, generation, overlay_epoch, owner_id, lease_id } = session;
    return this.#downloadBytes(`${this.baseUrl}/v1/preview/v4/audio/stream`, "V4 audio preview stream", options,
      JSON.stringify({ session_id, generation, overlay_epoch, owner_id, lease_id }));
  }
  /** Download a locked font authorized by the current V4 preview owner and generation. */
  downloadVisualPreviewFontV2(input: ToolInput<"opencut_visual_preview_v4_font_authorize">, options: CallOptions = {}): Promise<Response> {
    return this.#downloadBytes(`${this.baseUrl}/v1/preview/v4/visual-font/stream`, "V4 visual preview font", options, JSON.stringify(input));
  }
  async #downloadBytes(url: string, kind: string, options: CallOptions, body?: string): Promise<Response> {
    options.signal?.throwIfAborted();
    let response: Response;
    try {
      response = await this.#fetch(url, { redirect: "error", signal: options.signal,
        ...(options.keepalive === undefined ? {} : { keepalive: options.keepalive }),
        method: body === undefined ? "GET" : "POST", body,
        headers: { Authorization: `Bearer ${this.#token}`, ...(body === undefined ? {} : { "Content-Type": "application/json" }) } });
    } catch (cause) {
      if (options.signal?.aborted) throw options.signal.reason;
      throw new OpenCutTransportError(cause);
    }
    if (!response.ok) {
      let value: unknown;
      try { value = await response.json(); }
      catch (cause) {
        if (options.signal?.aborted) throw options.signal.reason;
        if (!(cause instanceof SyntaxError)) throw new OpenCutTransportError(cause);
        throw new OpenCutProtocolError(`Invalid ${kind} error response.`, response.status);
      }
      if (object(value) && value.api_version === "1" && typeof value.request_id === "string" && object(value.error) && typeof value.error.code === "string" && typeof value.error.message === "string" && Array.isArray(value.warnings) && value.warnings.every(w => typeof w === "string")) {
        throw new OpenCutApiError(value.error.code, value.error.message, value.error.details, response.status, value.request_id, value.warnings);
      }
      throw new OpenCutProtocolError(`Invalid ${kind} response.`, response.status);
    }
    return response;
  }
  private async wait<J extends Job | PublicJob>(jobId: string, read: (signal: AbortSignal) => Promise<J>, options: WaitOptions): Promise<J> {
    const timeout = options.timeoutMs ?? 30_000;
    const interval = options.pollIntervalMs ?? 200;
    if (!Number.isFinite(timeout) || timeout < 0 || timeout > 2_147_483_647
      || !Number.isFinite(interval) || interval < 1 || interval > 2_147_483_647) {
      throw new RangeError("Invalid job wait timeout or polling interval.");
    }
    options.signal?.throwIfAborted();
    if (timeout === 0) throw new JobWaitTimeoutError(jobId);
    const local = new AbortController();
    const signal = options.signal ? AbortSignal.any([options.signal, local.signal]) : local.signal;
    const timer = setTimeout(() => local.abort(new JobWaitTimeoutError(jobId)), timeout);
    try {
      for (;;) {
        const job = await read(signal);
        if (job.state === "succeeded") return job;
        if (job.state === "failed" || job.state === "cancelled") throw new JobCompletionError(job);
        await pause(interval, signal);
      }
    } finally { clearTimeout(timer); }
  }
}
