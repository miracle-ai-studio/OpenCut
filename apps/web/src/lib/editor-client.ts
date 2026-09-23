import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import type {
  JobMediaImportResult,
  JobArtifactVerifyResult,
  JobMediaProbeResult,
  JobPreviewFrameResult,
  JobPreviewCompositedFrameResult,
  JobPreviewProjectFrameResult,
  JobPreviewProjectRangeResult,
  JobRenderExportResult,
  JobRenderProjectResult,
  OpencutCapabilitiesOutput,
  OpencutEditApplyInput,
  OpencutJobEventsOutput,
  OpencutJobListOutput,
  OpencutJobStartInput,
  OpencutProjectHistoryOutput,
  OpencutProjectListOutput,
  OpencutRenderPreflightInput,
  OpencutRenderPreflightOutput,
  OpencutUploadCreateOutput,
  OpencutUploadReleaseOutput,
  OpencutJobViewOutput,
} from "./editor-contracts.generated";

export type Time = OpencutProjectListOutput.Time;
export type Range = OpencutProjectListOutput.TimeRange;
export type Clip = OpencutProjectListOutput.Clip;
export type Track = OpencutProjectListOutput.Track;
export type Cue = OpencutProjectListOutput.CaptionCue;
export type Sequence = OpencutProjectListOutput.Sequence;
export type Asset = OpencutProjectListOutput.Asset;
export type Operation = OpencutEditApplyInput.Operation;
export type Snapshot = OpencutProjectListOutput.ProjectSnapshot;
export type Envelope<T> = {
  api_version: string;
  request_id: string;
  data: T | null;
  error: { code: string; message: string; details: unknown } | null;
  warnings: string[];
};
export type Job = OpencutJobListOutput.JobRecord;
export type JobEvent = OpencutJobEventsOutput.JobEvent;
export type MediaProbeJobResult = JobMediaProbeResult.MediaMetadata;
export type MediaFrameJobResult = JobPreviewFrameResult.MediaFrameJobResult;
export type ProjectFrameJobResult =
  JobPreviewProjectFrameResult.ProjectFrameJobResult;
export type CompositedProjectFrameJobResult =
  JobPreviewCompositedFrameResult.CompositedProjectFrameJobResult;
export type ProjectRangeJobResult = JobPreviewProjectRangeResult.ProjectRangeJobResult;
export type MediaExportJobResult = JobRenderExportResult.MediaExportJobResult;
export type ProjectRenderJobResult =
  JobRenderProjectResult.ProjectRenderJobResult;
export type ArtifactVerifyJobResult =
  JobArtifactVerifyResult.VerificationReport;
export type CaptionStyle = OpencutProjectListOutput.CaptionStyle;
export type SseMessage = {
  id: string | null;
  event: string | null;
  data: string;
};

export async function* parseSseStream(
  stream: ReadableStream<Uint8Array>,
): AsyncGenerator<SseMessage> {
  const reader = stream.getReader(),
    decoder = new TextDecoder();
  let buffer = "";
  try {
    for (;;) {
      const { done, value } = await reader.read();
      buffer += decoder.decode(value ?? new Uint8Array(), { stream: !done });
      // Normalize after appending so a CRLF pair split across chunks is handled.
      buffer = buffer.replaceAll("\r\n", "\n");
      let boundary: number;
      while ((boundary = buffer.indexOf("\n\n")) >= 0) {
        const frame = buffer.slice(0, boundary);
        buffer = buffer.slice(boundary + 2);
        let id: null | string = null,
          event: null | string = null;
        const data: string[] = [];
        for (const line of frame.split("\n")) {
          if (line.startsWith(":")) continue;
          const colon = line.indexOf(":"),
            field = colon < 0 ? line : line.slice(0, colon),
            raw = colon < 0 ? "" : line.slice(colon + 1).replace(/^ /, "");
          if (field === "id") id = raw;
          else if (field === "event") event = raw;
          else if (field === "data") data.push(raw);
        }
        if (data.length) yield { id, event, data: data.join("\n") };
      }
      if (done) break;
    }
  } finally {
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}
export type RevisionSummary = OpencutProjectHistoryOutput.RevisionSummary;
export type RevisionPage = OpencutProjectHistoryOutput.RevisionPage;
export type ParameterDescriptor = OpencutCapabilitiesOutput.ParameterDescriptor;
export type Capabilities = OpencutCapabilitiesOutput.Capabilities;
export type RenderPreflightInput =
  OpencutRenderPreflightInput.RenderPreflightInput;
export type RenderPreflightReport =
  OpencutRenderPreflightOutput.ProjectPreflightReport;

const gcd = (a: bigint, b: bigint): bigint => {
  a = a < 0n ? -a : a;
  while (b) {
    const r = a % b;
    a = b;
    b = r;
  }
  return a;
};
const exact = (ticks: bigint, timebase: bigint): Time => {
  const d = gcd(ticks, timebase),
    n = ticks / d,
    b = timebase / d;
  if (b < 1n || b > 4294967295n || n < -(2n ** 63n) || n > 2n ** 63n - 1n)
    throw new Error("Exact time exceeds public representation");
  return { ticks: n.toString(), timebase: Number(b) };
};
export const secondsForDisplay = (value: Time) =>
  Number.parseFloat(value.ticks) / value.timebase;
export const addTime = (a: Time, b: Time) =>
  exact(
    BigInt(a.ticks) * BigInt(b.timebase) + BigInt(b.ticks) * BigInt(a.timebase),
    BigInt(a.timebase) * BigInt(b.timebase),
  );
export const scaleTime = (value: Time, numerator: Time, denominator: Time) =>
  exact(
    BigInt(value.ticks) *
      BigInt(numerator.ticks) *
      BigInt(denominator.timebase),
    BigInt(value.timebase) *
      BigInt(numerator.timebase) *
      BigInt(denominator.ticks),
  );
export const subtractWholeSeconds = (value: Time, seconds: bigint) =>
  exact(
    BigInt(value.ticks) - seconds * BigInt(value.timebase),
    BigInt(value.timebase),
  );
export const halfTime = (value: Time) =>
  exact(BigInt(value.ticks), BigInt(value.timebase) * 2n);
export const compareTime = (a: Time, b: Time) => {
  const left = BigInt(a.ticks) * BigInt(b.timebase);
  const right = BigInt(b.ticks) * BigInt(a.timebase);
  return left < right ? -1 : left > right ? 1 : 0;
};
export const timeFromDecimalSeconds = (value: string): Time => {
  const match = /^(\d+)(?:\.(\d{0,6}))?$/.exec(value.trim());
  if (!match) throw new Error("Time must be a non-negative decimal");
  const fraction = match[2] ?? "",
    timebase = 10n ** BigInt(fraction.length),
    ticks = BigInt(match[1]) * timebase + BigInt(fraction || "0");
  return exact(ticks, timebase);
};

const UPLOAD_CHUNK_BYTES = 4 * 1024 * 1024;
const MAX_UPLOAD_BYTES = 1024 * 1024 * 1024;

async function fileHash(file: File): Promise<string> {
  const digest = sha256.create();
  for (let offset = 0; offset < file.size; offset += UPLOAD_CHUNK_BYTES) {
    digest.update(new Uint8Array(await file.slice(offset, offset + UPLOAD_CHUNK_BYTES).arrayBuffer()));
  }
  return bytesToHex(digest.digest());
}

export class EditorClient {
  constructor(
    public readonly baseUrl: string,
    private readonly token: string,
  ) {}
  private async request<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await fetch(`${this.baseUrl.replace(/\/$/, "")}${path}`, {
      ...init,
      headers: {
        Authorization: `Bearer ${this.token}`,
        ...(init?.body instanceof File
          ? {}
          : { "Content-Type": "application/json" }),
        ...init?.headers,
      },
    });
    const envelope = (await response.json()) as Envelope<T>;
    if (envelope.error)
      throw Object.assign(new Error(envelope.error.message), envelope.error);
    if (!response.ok || envelope.data === null)
      throw new Error(`Request failed (${response.status})`);
    return envelope.data;
  }
  capabilities() {
    return this.request<Capabilities>("/v1/capabilities");
  }
  projects() {
    return this.request<Snapshot[]>("/v1/projects");
  }
  snapshot(id: string) {
    return this.request<Snapshot>(
      `/v1/projects/${encodeURIComponent(id)}/snapshot`,
    );
  }
  history(projectId: string, fromRevision: string | null = null) {
    return this.tool<RevisionPage>("opencut_project_history", {
      project_id: projectId,
      from_revision: fromRevision,
      limit: 100,
    });
  }
  revision(projectId: string, revision: string) {
    return this.tool<Snapshot>("opencut_revision_inspect", {
      project_id: projectId,
      revision,
    });
  }
  restore(
    projectId: string,
    expectedRevision: string,
    targetRevision: string,
    label: string,
  ) {
    return this.tool("opencut_revision_restore", {
      project_id: projectId,
      request: {
        expected_revision: expectedRevision,
        target_revision: targetRevision,
        idempotency_key: crypto.randomUUID(),
        label,
      },
    });
  }
  create(project: {
    id: string;
    name: string;
    sequence_id: string;
    width: number;
    height: number;
    fps: { numerator: number; denominator: number };
  }) {
    return this.request<Snapshot>("/v1/projects", {
      method: "POST",
      body: JSON.stringify({ project, idempotency_key: crypto.randomUUID() }),
    });
  }
  tool<T>(name: string, args: unknown) {
    return this.request<T>(`/v1/tools/${name}`, {
      method: "POST",
      body: JSON.stringify(args),
    });
  }
  apply(
    projectId: string,
    revision: string,
    operations: Operation[],
    label: string,
  ) {
    return this.tool("opencut_edit_apply", {
      project_id: projectId,
      request: {
        expected_revision: revision,
        idempotency_key: crypto.randomUUID(),
        label,
        operations,
      },
    });
  }
  plan(projectId: string, revision: string, operations: Operation[]) {
    return this.tool("opencut_edit_plan", {
      project_id: projectId,
      request: {
        expected_revision: revision,
        idempotency_key: crypto.randomUUID(),
        label: "Raw edit plan",
        operations,
      },
    });
  }
  async upload(projectId: string, file: File): Promise<{
    asset: Snapshot["project"]["assets"][number];
    media_url: string;
    stagingWarning: string | null;
  }> {
    if (file.size < 1 || file.size > MAX_UPLOAD_BYTES)
      throw new Error("Media file must be between 1 byte and 1 GiB");
    const contentHash = await fileHash(file);
    let uploadId: string | null = null;
    let terminal = false;
    const release = async (id: string): Promise<string | null> => {
      try {
        const retention = await this.tool<OpencutUploadReleaseOutput.UploadRetentionView>(
          "opencut_upload_release", { upload_id: id },
        );
        if (retention.state !== "released" || retention.reserved_bytes !== "0")
          throw new Error("staging remains reserved");
        return null;
      } catch (error) {
        return `upload ${id} staging release needs retry: ${(error as Error).message}`;
      }
    };
    try {
      const created = await this.tool<OpencutUploadCreateOutput.UploadView>(
        "opencut_upload_create", {
          project_id: projectId,
          idempotency_key: crypto.randomUUID(),
          name: file.name,
          size_bytes: String(file.size),
          content_hash: contentHash,
        },
      );
      uploadId = created.upload_id;
      if (created.project_id !== projectId || created.content_hash !== contentHash ||
          created.size_bytes !== String(file.size) || created.state !== "receiving")
        throw new Error("Upload session does not match the selected project or file");
      let offset = Number(created.next_offset);
      if (!Number.isSafeInteger(offset) || offset < 0 || offset > file.size)
        throw new Error("Upload session returned an invalid offset");
      while (offset < file.size) {
        const chunk = new Uint8Array(await file.slice(offset, offset + UPLOAD_CHUNK_BYTES).arrayBuffer());
        const query = new URLSearchParams({
          size_bytes: String(chunk.byteLength),
          content_hash: bytesToHex(sha256(chunk)),
        });
        const accepted: OpencutUploadCreateOutput.UploadView = await this.request<OpencutUploadCreateOutput.UploadView>(
          `/v1/uploads/${encodeURIComponent(uploadId)}/chunks/${offset}?${query}`,
          { method: "PUT", body: new Blob([chunk]),
            headers: { "Content-Type": "application/octet-stream" } },
        );
        if (accepted.upload_id !== uploadId || accepted.next_offset !== String(offset + chunk.byteLength))
          throw new Error("Upload chunk was not acknowledged at the expected offset");
        offset += chunk.byteLength;
      }
      const sealed = await this.tool<OpencutUploadCreateOutput.UploadView>(
        "opencut_upload_seal", { upload_id: uploadId },
      );
      if (sealed.state !== "sealed" || !sealed.job_id)
        throw new Error("Upload was not sealed into a media import job");
      const deadline = Date.now() + 5 * 60_000;
      let job: OpencutJobViewOutput.JobView;
      for (;;) {
        job = await this.tool<OpencutJobViewOutput.JobView>(
          "opencut_job_view", { job_id: sealed.job_id },
        );
        if (job.state === "succeeded" || job.state === "failed" || job.state === "cancelled") {
          terminal = true;
          break;
        }
        if (Date.now() >= deadline)
          throw new Error(`Media import is still running (job ${sealed.job_id}); staging remains retained`);
        await new Promise((resolve) => setTimeout(resolve, 250));
      }
      if (job.state !== "succeeded")
        throw new Error(job.failure?.message ?? `Media import ${job.state}`);
      const result = job.extended_result;
      if (job.kind !== "media.import" || result?.kind !== "media.import" ||
          result.schema_version !== 1 || job.result_unavailable || !result.data)
        throw new Error("Media import result is unavailable");
      const imported = result.data as JobMediaImportResult.MediaImportJobResult;
      if (imported.project_id !== projectId || imported.upload_id !== uploadId ||
          imported.job_id !== sealed.job_id)
        throw new Error("Media import result does not match this upload");
      const verified = await this.tool<JobMediaImportResult.MediaImportJobResult>(
        "opencut_project_media_inspect", {
          project_id: projectId, media_id: imported.media_id,
        },
      );
      if (verified.job_id !== imported.job_id || verified.asset.id !== imported.asset.id ||
          verified.asset.version?.content_hash !== contentHash)
        throw new Error("Project media resource does not match the import job");
      const mediaUrl = `/v1/projects/${encodeURIComponent(projectId)}/media/${encodeURIComponent(imported.media_id)}/content`;
      const stagingWarning = await release(uploadId);
      return { asset: verified.asset, media_url: mediaUrl, stagingWarning };
    } catch (error) {
      if (uploadId) {
        try {
          const current = await this.tool<OpencutUploadCreateOutput.UploadView>(
            "opencut_upload_inspect", { upload_id: uploadId },
          );
          if (current.state === "receiving")
            await this.tool("opencut_upload_abort", { upload_id: uploadId });
          if (current.state === "receiving" || current.state === "aborted" || terminal)
            await release(uploadId);
        } catch {
          // Leave the original failure visible. Retained staging is discoverable by upload ID.
        }
      }
      throw error;
    }
  }
  async mediaBlob(path: string) {
    const response = await fetch(`${this.baseUrl.replace(/\/$/, "")}${path}`, {
      headers: { Authorization: `Bearer ${this.token}` },
    });
    if (!response.ok)
      throw new Error(`Media fetch failed (${response.status})`);
    return URL.createObjectURL(await response.blob());
  }
  jobs() {
    return this.tool<Job[]>("opencut_job_list", {});
  }
  job(id: string) {
    return this.tool<Job>("opencut_job_get", { job_id: id });
  }
  cancelJob(id: string) {
    return this.tool<Job>("opencut_job_cancel", { job_id: id });
  }
  exportProject(snapshot: Snapshot) {
    const sequence_id = snapshot.project.default_sequence_id;
    return this.tool<Job>("opencut_job_start", {
      kind: "render.project",
      idempotency_key: crypto.randomUUID(),
      input: {
        project_id: snapshot.project.id,
        revision: snapshot.revision,
        sequence_id,
        container: "mp4",
        video_codec: "h264",
      },
    });
  }
  previewProjectFrame(
    snapshot: Snapshot,
    requestedTime: Time,
    kind: "preview.project_frame" | "preview.composited_frame" = "preview.project_frame",
  ) {
    const request: OpencutJobStartInput.JobStartInput = {
      kind,
      idempotency_key: crypto.randomUUID(),
      input: {
        project_id: snapshot.project.id,
        revision: snapshot.revision,
        sequence_id: snapshot.project.default_sequence_id,
        requested_time: requestedTime,
        alignment: "nearest",
      },
    };
    return this.tool<Job>("opencut_job_start", request);
  }
  renderPreflight(input: RenderPreflightInput, signal?: AbortSignal) {
    return this.request<RenderPreflightReport>("/v1/render/preflight", {
      method: "POST",
      body: JSON.stringify(input),
      signal,
    });
  }
  previewProjectRange(snapshot: Snapshot, requestedRange: Range) {
    const request: OpencutJobStartInput.JobStartInput = {
      kind: "preview.project_range",
      idempotency_key: crypto.randomUUID(),
      input: {
        project_id: snapshot.project.id,
        revision: snapshot.revision,
        sequence_id: snapshot.project.default_sequence_id,
        requested_range: requestedRange,
        alignment: "nearest",
      },
    };
    return this.tool<Job>("opencut_job_start", request);
  }
  async artifact(jobId: string, signal?: AbortSignal) {
    const response = await fetch(
      `${this.baseUrl.replace(/\/$/, "")}/v1/jobs/${encodeURIComponent(jobId)}/artifact`,
      { headers: { Authorization: `Bearer ${this.token}` }, signal },
    );
    if (!response.ok)
      throw new Error(`Artifact download failed (${response.status})`);
    return response.blob();
  }
  async streamJobEvents(
    afterSequence: number,
    jobId: string | null,
    signal: AbortSignal,
    onEvent: (value: JobEvent, sequence: number) => void,
  ) {
    const query = new URLSearchParams({
      after_sequence: String(afterSequence),
    });
    if (jobId) query.set("job_id", jobId);
    const response = await fetch(
      `${this.baseUrl.replace(/\/$/, "")}/v1/job-events/stream?${query}`,
      {
        headers: {
          Authorization: `Bearer ${this.token}`,
          "Last-Event-ID": String(afterSequence),
        },
        signal,
      },
    );
    if (!response.ok || !response.body)
      throw new Error(`Job event stream failed (${response.status})`);
    let cursor = afterSequence;
    for await (const message of parseSseStream(response.body)) {
      const value = JSON.parse(message.data) as {
        sequence?: number;
        code?: string;
      };
      if (message.event === "error") {
        throw new Error(value.code ?? "Job event stream reported an error");
      }
      const sequence = value.sequence ?? Number(message.id);
      if (Number.isSafeInteger(sequence) && sequence > cursor) {
        cursor = sequence;
        onEvent(value as JobEvent, sequence);
      }
    }
    return cursor;
  }
}
