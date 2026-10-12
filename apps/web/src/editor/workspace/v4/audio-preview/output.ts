import workletUrl from "./worklet.ts?worker&url";
import { createClassicMasteringChain } from "../audio-preview-mastering";

export type AudioOutputProgress = {
  consumedFrames: string;
  stalled: boolean;
  contextTime: number;
};

/** No source clocks or gain changes here: Rust sends the pre-master stereo mix. */
export class PreviewAudioOutput {
  readonly ready: Promise<void>;
  readonly sampleRate: number;
  private node: AudioWorkletNode | null = null;
  private chain: ReturnType<typeof createClassicMasteringChain> | null = null;
  private sent = 0;
  private completed = 0;
  private consumedFrames = "0";
  private stopped = false;
  private waiters: Array<() => void> = [];
  private context: AudioContext;
  private stopReceipt: ((value: string) => void) | null = null;
  private finalReceipt: Promise<string> | null = null;
  private closing: Promise<string> | null = null;

  constructor(
    readonly token: string,
    sampleRate: number,
    private readonly progress: (value: AudioOutputProgress) => void,
    private readonly failure: (error: Error) => void,
    private readonly completedBlock: () => void = () => {},
  ) {
    // Construct and resume synchronously inside the user's Play activation.
    this.context = new AudioContext({ sampleRate });
    this.sampleRate = this.context.sampleRate;
    const activation = this.context.resume();
    this.ready = this.initialize(sampleRate, activation);
  }
  private async initialize(sampleRate: number, activation: Promise<void>) {
    try {
      if (
        this.sampleRate !== sampleRate ||
        ![44100, 48000].includes(sampleRate)
      )
        throw new Error(
          "The audio device sample rate does not match this service.",
        );
      await activation;
      if (this.context.state !== "running")
        throw new Error("Allow audio playback before starting scene sound.");
      await this.context.audioWorklet.addModule(workletUrl);
      if (this.stopped) return;
      const node = new AudioWorkletNode(
        this.context,
        "opencut-v4-audio-preview",
        {
          numberOfInputs: 0,
          numberOfOutputs: 1,
          outputChannelCount: [2],
        },
      );
      this.node = node;
      this.chain = createClassicMasteringChain(
        this.context,
        this.context.destination,
      );
      node.connect(this.chain.input);
      node.port.onmessage = ({ data }) => {
        if (data.token !== this.token) return;
        if (data.kind === "stopped") {
          this.stopReceipt?.(data.consumedFrames);
          this.stopReceipt = null;
          return;
        }
        if (this.stopped) return;
        if (data.kind === "error") {
          this.failure(new Error(data.message));
          return;
        }
        if (data.kind !== "progress") return;
        const completed = Math.max(this.completed, data.completedBlocks);
        const advanced = completed > this.completed;
        this.completed = completed;
        this.consumedFrames = data.consumedFrames;
        this.progress({
          consumedFrames: this.consumedFrames,
          stalled: data.copied === 0,
          contextTime: this.context.currentTime,
        });
        if (this.sent - this.completed < 2) this.wake();
        if (advanced) this.completedBlock();
      };
      node.onprocessorerror = () =>
        this.failure(new Error("Scene audio output failed."));
      this.context.onstatechange = () => {
        if (!this.stopped && this.context.state !== "running")
          this.failure(
            new Error(
              "Scene audio playback was suspended. Press Play to resume.",
            ),
          );
      };
      node.port.postMessage({ kind: "reset", token: this.token });
    } catch (error) {
      this.silence();
      throw error;
    }
  }
  private wake() {
    this.waiters.splice(0).forEach((resolve) => resolve());
  }
  async waitForCredit(signal: AbortSignal) {
    await this.ready;
    while (
      !this.stopped &&
      !signal.aborted &&
      this.sent - this.completed >= 2
    ) {
      await new Promise<void>((resolve) => {
        const done = () => {
          signal.removeEventListener("abort", done);
          resolve();
        };
        this.waiters.push(done);
        signal.addEventListener("abort", done, { once: true });
      });
    }
    // Hold the reader until the controller reports the final device cursor and
    // changes the server generation. Closing it early would invalidate that fence.
    if (this.stopped && !signal.aborted)
      await new Promise<void>((resolve) =>
        signal.addEventListener("abort", () => resolve(), { once: true }),
      );
    if (this.stopped || signal.aborted)
      throw new DOMException("Stopped", "AbortError");
  }
  async push(pcm: Float32Array, signal: AbortSignal) {
    await this.waitForCredit(signal);
    if (
      !pcm.length ||
      pcm.length % 2 ||
      pcm.length > 8192 ||
      !pcm.every(Number.isFinite)
    )
      throw new Error("Invalid scene audio PCM block.");
    if (!(pcm.buffer instanceof ArrayBuffer))
      throw new Error("Invalid scene audio transfer buffer.");
    this.sent++;
    this.node!.port.postMessage(
      { kind: "block", token: this.token, pcm: pcm.buffer },
      [pcm.buffer],
    );
  }
  /** Disconnect before any asynchronous server stop, including limiter release tails. */
  silence() {
    if (!this.stopped && this.node) {
      this.finalReceipt = new Promise<string>((resolve) => {
        this.stopReceipt = resolve;
      });
      this.node.port.postMessage({ kind: "stop" });
    }
    this.stopped = true;
    this.node?.disconnect();
    this.chain?.disconnect();
    this.wake();
  }
  stop(): Promise<string> {
    return (this.closing ??= this.closeOutput());
  }
  private async closeOutput(): Promise<string> {
    this.silence();
    const node = this.node;
    if (node) {
      const value = await Promise.race([
        this.finalReceipt!,
        new Promise<null>((resolve) => setTimeout(() => resolve(null), 500)),
      ]);
      // A missing device receipt is not evidence of a final consumed cursor.
      this.stopReceipt = null;
      node.port.close();
      await this.context.close();
      if (value === null)
        throw new Error(
          "The audio device did not confirm its stopped sample cursor.",
        );
      return value;
    }
    await this.context.close();
    return this.consumedFrames;
  }
}
