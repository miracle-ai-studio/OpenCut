# Local transcription contract

`analysis.transcribe` is a durable Job for one audio or video asset in an immutable project revision. The public tool is `opencut_analysis_transcribe`; the compact media domain exposes action `analysis.transcribe`. Both submit through the existing Job queue, idempotency receipt, worker recovery, wait/view and cancellation paths. The legacy `opencut_job_start` input schema is unchanged.

The request contains only `idempotency_key`, `project_id`, `revision`, `asset_id`, and optional `language`. Omitted `language` or `auto` asks the engine to detect it; an explicit value is two or three lowercase ASCII letters, such as `zh` or `en`. Unknown fields are rejected. In particular, callers cannot supply an executable, model, source path or output path. The host reads `OPENCUT_WHISPER_MODEL` for its local model; `OPENCUT_WHISPER_CLI` and `OPENCUT_FFMPEG` optionally select host executables, defaulting to `whisper-cli` and `ffmpeg`.

A scoped caller needs `run_jobs` on the project to submit and `read` to inspect its Job. The host loads the requested revision, confirms the asset belongs to it and has audio or video kind, then captures exactly that asset in the content addressed store. The accepted Job records the fixed snapshot and an asset lock with content hash and size. A retry with the same key and request returns the original Job, including after the source URI changes or disappears; reusing the key with different input fails. The worker materializes and verifies the locked bytes before running Whisper. It never falls back to a later version of the asset.

Submission returns a public `JobView` in `queued` state. A successful `opencut_job_view` has `state: "succeeded"`, `result: null`, and `extended_result` with `kind: "analysis.transcribe"`, `schema_version: 1`, and `data` shaped as follows:

```json
{
  "project_id": "project-1",
  "revision": "rev-1",
  "asset_id": "speech",
  "asset_version": {"asset_id": "speech", "content_hash": "<64 lowercase hex digits>", "size": 12345},
  "transcription": {
    "language": "zh",
    "provider": "whisper.cpp",
    "model": "ggml-tiny.bin",
    "segments": [
      {"start": {"ticks": "120", "timebase": 1000}, "end": {"ticks": "780", "timebase": 1000}, "text": "你好", "words": []}
    ]
  }
}
```

The public projection checks the stored result against the prepared project, revision, asset ID and locked version before returning it. `asset_version.content_hash` identifies the locked source bytes. `model` is a safe filename label, never a model path or model content hash; the Job result does not pin the model version or promise identical output across hosts. Segment times are **source-media milliseconds**, not timeline positions. For a clip with an offset, trim or changed speed, callers must map the source interval through that clip before creating a caption cue. `caption.insert` is a separate edit transaction; its `source_anchor` can retain `{asset_id, source_range}`. Transcription itself does not modify the project.

The engine returns segment intervals from Whisper JSON. `word_timestamps: true` requests a second real word-split decoding pass against the same privately pinned model and audio. English output is published only when every interval and text reconstruction validates; `word_timing_status` is `available`, `unavailable`, or `not_requested`. Other languages currently retain segment evidence with unavailable word timing. The exact private model bytes produce `model_sha256`; older results may omit it. The engine does not assign speaker identities. Publication accepts at most 10,000 nonoverlapping segments, each with nonempty text up to 4,000 UTF-8 bytes; combined text is at most 1,000,000 bytes. Times use a 1,000-tick timebase, remain within one hour, and each end exceeds its start. The conversion WAV and Whisper JSON have 120 MB and 16 MB ceilings. On Unix, both temporary outputs use the Job attempt directory and the existing output admission budget; private inputs and temporary outputs are cleaned with attempt lifecycle.

Cancellation is requested through the existing Job cancel operation and observed through Job view/wait; an in-flight process is interrupted and no successful transcript is published for a cancelled attempt. Missing host model or tool capability, unavailable or corrupted locked content, invalid engine output, and storage limits become failed Jobs with public failure codes. Detailed local paths and process diagnostics stay out of the public Job view. A successful Job whose stored result fails projection remains succeeded with `result_unavailable` instead of exposing unverified data.

CLI example (with a running worker or `serve` host and host model configured):

```sh
opencut --database workspace.sqlite domain media analysis.transcribe --input request.json
opencut --database workspace.sqlite call opencut_job_view --input job.json
```

`request.json` contains the request fields above; `job.json` contains `{"job_id":"<returned job_id>"}`. A TypeScript client can use the same public result:

```ts
import { OpenCutClient, publicJobResult } from "@opencut/sdk";

const client = new OpenCutClient({ baseUrl: "http://127.0.0.1:8788", token });
const accepted = await client.callDomain("media", "analysis.transcribe", {
  idempotency_key: "transcribe-speech-v1",
  project_id: "project-1",
  revision: "rev-1",
  asset_id: "speech",
  language: "zh",
});
const done = await client.waitForPublicJob(accepted.job_id);
const transcript = publicJobResult(done, "analysis.transcribe");
```

The validated local journey exercised a real Chinese speech file and model, four returned segments, source-anchored caption insertion, human correction, render and reopened persisted state. This contract covers local transcription Job plumbing. English word timing is additionally covered by the public HTTP journey in `r5-word-timestamps-2026-09-24.json`. This does not establish multilingual alignment quality, speaker identification, a transcription search UI, or completion of every R5 product requirement.
