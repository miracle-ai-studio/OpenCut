import workletUrl from "./worklet.ts?worker&url";
import { createClassicMasteringChain } from "../audio-preview-mastering";
import { PreviewDevicePort } from "./device-port";
import { WorkerPreviewPump } from "./worker-pump";
import type { DeviceReceipt } from "./device-port";
import type { EditorConnection } from "../../../../lib/editor-sdk";
import type { V4AudioPreviewSession } from "../types";
export type AudioOutputProgress = {
  consumedFrames: string;
  stalled: boolean;
  contextTime: number;
};
/** Device/master chain assembled during Play; worker owns the data-plane port. */
export class PreviewAudioOutput {
  readonly ready: Promise<void>;
  readonly sampleRate: number;
  private context: AudioContext;
  private node: AudioWorkletNode | null = null;
  private chain: ReturnType<typeof createClassicMasteringChain> | null = null;
  private device: PreviewDevicePort | null = null;
  private pump: WorkerPreviewPump | null = null;
  private stopped = false;
  private closing: Promise<string> | null = null;
  constructor(
    readonly token: string,
    sampleRate: number,
    private readonly progress: (value: AudioOutputProgress) => void,
    private readonly failure: (error: Error) => void,
    private readonly completedBlock: () => void = () => {},
  ) {
    this.context = new AudioContext({ sampleRate });
    this.sampleRate = this.context.sampleRate;
    const activation = this.context.resume();
    this.ready = this.initialize(sampleRate, activation);
  }
  private observed(data: DeviceReceipt) {
    if (!this.stopped)
      this.progress({
        consumedFrames: data.consumedFrames,
        stalled: data.copied === 0,
        contextTime: this.context.currentTime,
      });
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
      this.node = new AudioWorkletNode(
        this.context,
        "opencut-v4-audio-preview",
        { numberOfInputs: 0, numberOfOutputs: 1, outputChannelCount: [2] },
      );
      this.chain = createClassicMasteringChain(
        this.context,
        this.context.destination,
      );
      this.node.connect(this.chain.input);
      this.device = new PreviewDevicePort(
        this.node.port,
        this.token,
        (data) => this.observed(data),
        this.failure,
        this.completedBlock,
      );
      this.node.onprocessorerror = () =>
        this.failure(new Error("Scene audio output failed."));
      this.context.onstatechange = () => {
        if (!this.stopped && this.context.state !== "running")
          this.failure(
            new Error(
              "Scene audio playback was suspended. Press Play to resume.",
            ),
          );
      };
    } catch (error) {
      this.silence();
      throw error;
    }
  }
  async createPump(
    connection: EditorConnection,
    updated: (s: V4AudioPreviewSession, p: AudioOutputProgress) => void,
    failure: (error: unknown) => void,
  ) {
    await this.ready;
    this.pump = new WorkerPreviewPump(
      this.token,
      connection,
      () => this.device!.detach(),
      (data) => this.observed(data),
      updated,
      failure,
    );
    await this.pump.ready;
    return this.pump;
  }
  async waitForCredit(signal: AbortSignal) {
    await this.ready;
    await this.device!.waitForCredit(signal);
  }
  async push(pcm: Float32Array, signal: AbortSignal) {
    await this.ready;
    await this.device!.push(pcm, signal);
  }
  finish() {
    this.device?.finish();
  }
  silence() {
    if (!this.stopped) {
      if (this.pump?.ownsDevice) this.pump.freeze();
      else this.device?.silence();
    }
    this.stopped = true;
    this.node?.disconnect();
    this.chain?.disconnect();
  }
  stop() {
    return (this.closing ??= this.close());
  }
  private async close() {
    this.silence();
    try {
      return this.pump?.ownsDevice
        ? await this.pump.stopOutput()
        : this.device
          ? await this.device.stop()
          : "0";
    } finally {
      await this.context.close();
    }
  }
}
