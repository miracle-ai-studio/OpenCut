import type { OpenCutClient, ToolOutput } from "../../lib/editor-sdk";
import type { Snapshot, Time } from "../model/types";
import { addTime, fromSeconds } from "../model/time";
type Session = ToolOutput<"opencut_preview_create">;
const wait = (ms: number, signal: AbortSignal) =>
  new Promise<void>((resolve, reject) => {
    signal.throwIfAborted();
    const abort = () => {
      clearTimeout(timer);
      reject(signal.reason);
    };
    const timer = setTimeout(() => {
      signal.removeEventListener("abort", abort);
      resolve();
    }, ms);
    signal.addEventListener("abort", abort, { once: true });
  });
function event(target: EventTarget, name: string, signal: AbortSignal) {
  return new Promise<void>((resolve, reject) => {
    signal.throwIfAborted();
    const done = () => {
        clean();
        resolve();
      },
      fail = () => {
        clean();
        reject(new Error(`Media ${name} failed`));
      },
      abort = () => {
        clean();
        reject(signal.reason);
      },
      clean = () => {
        target.removeEventListener(name, done);
        target.removeEventListener("error", fail);
        signal.removeEventListener("abort", abort);
      };
    target.addEventListener(name, done, { once: true });
    target.addEventListener("error", fail, { once: true });
    signal.addEventListener("abort", abort, { once: true });
  });
}
export class Playback {
  private abort = new AbortController();
  private session: Session | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;
  private url: string | null = null;
  private reporting = false;
  private sequence = 0;
  constructor(
    private sdk: OpenCutClient,
    private video: HTMLVideoElement,
    private onError: (error: unknown) => void,
  ) {}
  async start(snapshot: Snapshot, sequenceId: string, time: Time) {
    const signal = this.abort.signal;
    if (!window.MediaSource)
      throw new Error(
        "This browser does not support continuous preview. Use frame preview.",
      );
    const mime = 'video/mp4; codecs="avc1.64001f,mp4a.40.2"';
    if (!MediaSource.isTypeSupported(mime))
      throw new Error(
        "This browser cannot play the service's H.264/AAC preview.",
      );
    const input = {
      project_id: snapshot.project.id,
      revision: snapshot.revision,
      sequence_id: sequenceId,
      start_time: time,
      alignment: "nearest" as const,
      audio_enabled: true,
      idempotency_key: crypto.randomUUID(),
    };
    const created = await this.sdk.call("opencut_preview_create", input);
    this.session = created;
    if (signal.aborted) {
      await this.stopSession();
      return;
    }
    const session = await this.sdk.call("opencut_preview_control", {
      ...this.identity(),
      command: { kind: "play" },
    });
    this.session = session;
    signal.throwIfAborted();
    const media = new MediaSource();
    this.url = URL.createObjectURL(media);
    const opened = event(media, "sourceopen", signal);
    this.video.src = this.url;
    await opened;
    const buffer = media.addSourceBuffer(mime);
    const response = await this.sdk.openPreviewStream(session, { signal });
    if (!response.body) throw new Error("Preview stream is empty.");
    this.timer = setInterval(() => void this.report(), 1000);
    const reader = response.body.getReader();
    let began = false;
    try {
      for (;;) {
        signal.throwIfAborted();
        while (
          buffer.buffered.length &&
          buffer.buffered.end(buffer.buffered.length - 1) -
            this.video.currentTime >
            12
        )
          await wait(200, signal);
        const { done, value } = await reader.read();
        if (done) break;
        if (
          this.video.currentTime > 25 &&
          buffer.buffered.length &&
          buffer.buffered.start(0) < this.video.currentTime - 15
        ) {
          const removed = event(buffer, "updateend", signal);
          buffer.remove(0, this.video.currentTime - 10);
          await removed;
        }
        const appended = event(buffer, "updateend", signal);
        buffer.appendBuffer(new Uint8Array(value));
        await appended;
        if (!began && buffer.buffered.length) {
          began = true;
          void this.video.play().catch((error) => {
            if (!signal.aborted) this.onError(error);
          });
        }
      }
      if (media.readyState === "open") media.endOfStream();
    } finally {
      await reader.cancel().catch(() => {});
      reader.releaseLock();
    }
  }
  private identity() {
    if (!this.session) throw new Error("Preview is not ready");
    const { session_id, generation, owner_id, lease_id } = this.session;
    return { session_id, generation, owner_id, lease_id };
  }
  private async report() {
    if (!this.session || this.abort.signal.aborted || this.reporting) return;
    this.reporting = true;
    try {
      await this.sdk.call("opencut_preview_control", {
        ...this.identity(),
        command: {
          kind: "report_presentation",
          telemetry: {
            sequence: String(++this.sequence),
            presented_time: addTime(
              this.session.actual_start,
              fromSeconds(this.video.currentTime),
            ),
            media_clock_time: fromSeconds(this.video.currentTime),
            playing: !this.video.paused,
            stalled: this.video.readyState < 3,
            client_observed_at_ms: String(Date.now()),
          },
        },
      });
    } catch (error) {
      if (!this.abort.signal.aborted) this.onError(error);
    } finally {
      this.reporting = false;
    }
  }
  position() {
    return this.session
      ? addTime(this.session.actual_start, fromSeconds(this.video.currentTime))
      : null;
  }
  private async stopSession() {
    if (!this.session) return;
    try {
      await this.sdk.call("opencut_preview_control", {
        ...this.identity(),
        command: { kind: "stop" },
      });
    } catch {
      /* Expired sessions are already cleaned by the host. */
    }
  }
  stop() {
    this.abort.abort();
    if (this.timer) clearInterval(this.timer);
    this.video.pause();
    this.video.removeAttribute("src");
    this.video.load();
    if (this.url) URL.revokeObjectURL(this.url);
    void this.stopSession();
  }
}
