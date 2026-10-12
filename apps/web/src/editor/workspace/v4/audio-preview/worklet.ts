import { PreviewPcmQueue } from "./queue";

declare class AudioWorkletProcessor {
  readonly port: MessagePort;
}
declare function registerProcessor(
  name: string,
  processor: typeof AudioWorkletProcessor,
): void;
declare const currentFrame: number;

class PreviewOutputProcessor extends AudioWorkletProcessor {
  private queue = new PreviewPcmQueue();
  private token: string | null = null;
  private playing = false;
  private primed = false;
  constructor() {
    super();
    this.port.onmessage = ({ data }) => {
      if (data.kind === "reset") {
        this.token = data.token;
        this.queue.reset(this.token);
        this.playing = true;
        this.primed = false;
      } else if (data.kind === "stop") {
        this.playing = false;
        this.port.postMessage({
          kind: "stopped",
          token: this.token,
          consumedFrames: this.queue.consumedFrames,
        });
        this.queue.reset(null);
      } else if (data.kind === "block") {
        try {
          this.queue.append(data.token, new Float32Array(data.pcm));
          if (this.queue.freeBlocks === 0) this.primed = true;
        } catch (error) {
          this.playing = false;
          this.queue.reset(null);
          this.port.postMessage({
            kind: "error",
            token: this.token,
            message: String(error),
          });
        }
      } else if (data.kind === "end" && data.token === this.token) this.primed = true;
    };
  }
  process(_inputs: Float32Array[][], outputs: Float32Array[][]) {
    const output = outputs[0];
    if (!output?.[0] || !output[1]) return true;
    if (!this.playing || !this.primed) {
      output[0].fill(0);
      output[1].fill(0);
      return true;
    }
    const receipt = this.queue.render(output[0], output[1]);
    this.port.postMessage({ kind: "progress", token: this.token, ...receipt,
      renderFrame: currentFrame, quantumFrames: output[0].length });
    return true;
  }
}
registerProcessor("opencut-v4-audio-preview", PreviewOutputProcessor);
