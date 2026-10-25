import type { Job } from "../../../lib/editor-sdk";

/** Presentation of public receipts only. Never guess missing fields from the
 * current project: the job can refer to an older revision or a cropped range. */
export function isUserExport(job: Job) {
  return job.purpose !== "playback_optimization" && ["render.video", "render.video_range", "render.audio", "render.audio_range"].includes(job.kind);
}
export function publishedArtifacts(job: Job) {
  return job.state === "succeeded" && job.result && !job.result_unavailable && !job.failure
    ? job.artifacts.filter(a => a.job_id === job.job_id && a.role === "primary" && /^(video\/(mp4|webm)|audio\/wav)$/.test(a.mime_type)) : [];
}
export function exportRecord(job: Job) {
  const result = job.result;
  const video = result?.kind === "render.video" ? result.data : result?.kind === "render.video_range" ? result.data.output : null;
  const audio = result?.kind === "render.audio" ? result.data : null;
  const identity = video?.identity;
  return {
    format: identity?.options.format.toUpperCase() ?? (audio ? "WAV" : null),
    size: identity ? `${identity.width} × ${identity.height}` : null,
    fps: identity ? identity.fps.numerator / identity.fps.denominator : null,
    seconds: identity ? Number(identity.video_duration.ticks) / identity.video_duration.timebase
      : audio ? Number(audio.identity.frame_count) / audio.identity.sample_rate : null,
    rate: audio?.identity.sample_rate ?? null,
    channels: audio?.identity.channels ?? null,
  };
}
