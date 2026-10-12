import type { EditorConnection } from "../../../../lib/editor-sdk";
import type { V4AudioPreviewSession } from "../types";
import type { AudioOutputProgress } from "./output";
import type { DeviceReceipt } from "./device-port";
import type { PreviewPump } from "./pump";
import { counter } from "./session-identity";
import { PUMP_CHANNEL } from "./worker-protocol";
import type { PumpCommand, PumpEvent } from "./worker-protocol";

type CommandBody = PumpCommand extends infer C
  ? C extends PumpCommand
    ? Omit<C, "channel" | "version" | "token">
    : never
  : never;
/** Private worker bridge, fixed to one device token and one stream generation. */
export class WorkerPreviewPump implements PreviewPump {
  readonly ready: Promise<void>;
  ownsDevice = false;
  private readonly worker: Worker;
  private alive = true;
  private initialized = false;
  private sequence = 0;
  private stopping: Promise<string> | null = null;
  private pending = new Map<
    number,
    {
      resolve: (event: Extract<PumpEvent, { kind: "reply" }>) => void;
      reject: (error: Error) => void;
      timer: ReturnType<typeof setTimeout>;
    }
  >();
  private resolveReady!: () => void;
  private rejectReady!: (error: Error) => void;
  constructor(
    private readonly token: string,
    connection: EditorConnection,
    private readonly takeDevice: () => MessagePort,
    private readonly progress: (receipt: DeviceReceipt) => void,
    private readonly updated: (
      session: V4AudioPreviewSession,
      progress: AudioOutputProgress,
    ) => void,
    private readonly failure: (error: Error) => void,
  ) {
    this.ready = new Promise((resolve, reject) => {
      this.resolveReady = resolve;
      this.rejectReady = reject;
    });
    this.worker = new Worker(new URL("./pump-worker.ts", import.meta.url), {
      type: "module",
      name: "opencut-audio-pump",
    });
    const readyTimer = setTimeout(
      () => this.fail(new Error("Scene audio data pump did not initialize.")),
      5000,
    );
    void this.ready.then(
      () => clearTimeout(readyTimer),
      () => clearTimeout(readyTimer),
    );
    this.worker.onmessage = ({ data }: MessageEvent<PumpEvent>) => {
      try {
        if (!this.alive) return;
        if (
          data.channel !== PUMP_CHANNEL ||
          data.version !== 1 ||
          data.token !== token
        ) {
          this.fail(new Error("Invalid scene audio data pump reply."));
          return;
        }
        if (data.kind === "ready") {
          if (this.initialized)
            throw new Error("Duplicate scene audio data pump initialization.");
          this.initialized = true;
          this.resolveReady();
        } else if (data.kind === "progress") this.progress(data.receipt);
        else if (data.kind === "session")
          this.updated(data.session, data.progress);
        else if (data.kind === "error") this.fail(new Error(data.message));
        else if (data.kind === "reply") {
          const pending = this.pending.get(data.id);
          if (!pending) {
            this.fail(new Error("Unexpected scene audio data pump reply."));
            return;
          }
          clearTimeout(pending.timer);
          this.pending.delete(data.id);
          pending.resolve(data);
        }
      } catch (error) {
        this.fail(error instanceof Error ? error : new Error(String(error)));
      }
    };
    this.worker.onerror = () =>
      this.fail(new Error("Scene audio data pump failed."));
    this.send({ kind: "init", connection });
  }
  private send(body: CommandBody, transfer: Transferable[] = []) {
    if (!this.alive) throw new Error("Scene audio data pump has stopped.");
    this.worker.postMessage(
      { channel: PUMP_CHANNEL, version: 1, token: this.token, ...body },
      transfer,
    );
  }
  start(session: V4AudioPreviewSession) {
    if (!this.alive || !this.initialized)
      throw new Error("Scene audio data pump is not ready.");
    if (this.ownsDevice)
      throw new Error("Scene audio data pump already owns its device.");
    const port = this.takeDevice();
    this.send({ kind: "start", session, port }, [port]);
    this.ownsDevice = true;
  }
  observe(_progress: AudioOutputProgress) {} // The worker owns the actual device receipt.
  completed() {} // Completion credit is released by the same worker owner.
  freeze() {
    if (this.alive) this.send({ kind: "freeze" });
  }
  private request(
    body: { kind: "stop" } | { kind: "reportStopped"; frames: string },
  ) {
    if (!this.alive)
      return Promise.reject(new Error("Scene audio data pump stopped."));
    if (this.pending.size >= 2)
      return Promise.reject(
        new Error("Scene audio pump request budget exceeded."),
      );
    const id = ++this.sequence;
    return new Promise<Extract<PumpEvent, { kind: "reply" }>>(
      (resolve, reject) => {
        const timer = setTimeout(
          () =>
            this.fail(new Error("Scene audio data pump request timed out.")),
          4000,
        );
        this.pending.set(id, { resolve, reject, timer });
        try {
          this.send({ ...body, id });
        } catch (error) {
          this.fail(error instanceof Error ? error : new Error(String(error)));
        }
      },
    );
  }
  stopOutput() {
    return (this.stopping ??= this.request({ kind: "stop" }).then((event) => {
      if (!counter(event.frames))
        throw new Error("Scene audio device returned no final cursor.");
      return event.frames;
    }));
  }
  reportStopped(frames: string) {
    return this.request({ kind: "reportStopped", frames }).then(
      (event) => event.session,
    );
  }
  private fail(error: Error) {
    if (!this.alive) return;
    this.rejectReady(error);
    this.close(error);
    this.failure(error);
  }
  private close(error: Error) {
    this.alive = false;
    for (const p of this.pending.values()) {
      clearTimeout(p.timer);
      p.reject(error);
    }
    this.pending.clear();
    this.worker.terminate();
  }
  abort() {
    if (!this.alive) return;
    this.send({ kind: "abort" });
    this.rejectReady(new Error("Scene audio data pump stopped."));
    this.close(new Error("Scene audio data pump stopped."));
  }
}
