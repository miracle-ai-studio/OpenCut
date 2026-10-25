import { DEVICE_BUFFER_BLOCKS } from "./buffer-limits";
/** Device-side FIFO only. Source placement and gain are already compiled by Rust. */
export class PreviewPcmQueue {
  private blocks: Float32Array[] = [];
  private offset = 0;
  private consumed = 0n;
  private completed = 0;
  private token: string | null = null;
  reset(token: string | null) {
    this.blocks = [];
    this.offset = 0;
    this.consumed = 0n;
    this.completed = 0;
    this.token = token;
  }
  get freeBlocks() {
    return DEVICE_BUFFER_BLOCKS - this.blocks.length;
  }
  get consumedFrames() {
    return this.consumed.toString();
  }
  append(token: string, pcm: Float32Array) {
    if (token !== this.token) return false;
    if (!pcm.length || pcm.length % 2 || pcm.length > 4096 * 2)
      throw new Error("Invalid stereo audio preview block.");
    if (!pcm.every(Number.isFinite))
      throw new Error("Non-finite audio preview PCM.");
    if (!this.freeBlocks) throw new Error("Audio preview queue is full.");
    this.blocks.push(pcm);
    return true;
  }
  /** Only frames copied into the device render quantum advance the sample clock. */
  render(left: Float32Array, right: Float32Array, through?: bigint) {
    left.fill(0);
    right.fill(0);
    if (left.length !== right.length)
      throw new Error("Invalid audio output channels.");
    let copied = 0;
    const available = through === undefined ? left.length : Number(
      through <= this.consumed ? 0n : (through - this.consumed > BigInt(left.length) ? BigInt(left.length) : through - this.consumed));
    while (copied < available && this.blocks.length) {
      const block = this.blocks[0];
      const count = Math.min(
        available - copied,
        (block.length - this.offset) / 2,
      );
      for (let i = 0; i < count; i++) {
        left[copied + i] = block[this.offset + i * 2];
        right[copied + i] = block[this.offset + i * 2 + 1];
      }
      copied += count;
      this.offset += count * 2;
      if (this.offset === block.length) {
        this.blocks.shift();
        this.offset = 0;
        this.completed++;
      }
    }
    this.consumed += BigInt(copied);
    return {
      copied,
      completedBlocks: this.completed,
      consumedFrames: this.consumedFrames,
    };
  }
}
