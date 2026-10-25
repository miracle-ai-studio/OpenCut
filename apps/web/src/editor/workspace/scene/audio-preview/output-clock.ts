import type { AudioPreviewControl } from "../types";
import type { DeviceReceipt } from "./device-port";
import { counter } from "./session-identity";

export type OutputObservation = Extract<AudioPreviewControl["command"], { kind: "report_consumed" }>["output"];
type Span = { start: number; end: number; startFrame:number; endFrame:number; first: bigint; through: bigint };
type Timestamp = { contextTime: number; performanceTime: number };
/** Bounded device adapter: maps submitted content samples to output time.
 * No clip, source, gain, frame-grid or project evaluation belongs here. */
export class PreviewOutputClock {
  private spans: Span[] = [];
  private intervalError="";
  private repeatedQuanta=0;
  private submitted = 0n;
  private presented = 0n;
  private timestamp: Timestamp | null = null;
  private phase: DeviceReceipt["phase"] = "preparing";
  private offsets:number[]=[];
  private maximumOffsetStep=0;
  private rawBacksteps=0;
  private previousContext:number|null=null;
  private previousSampleAt:number|null=null;
  private maximumRefreshGap=0;
  diagnostics() {
    return {deviceRepeatedQuanta:String(this.repeatedQuanta),clockIntervalError:this.intervalError,clockOffsetRangeMs:String(this.offsets.length?(Math.max(...this.offsets)-Math.min(...this.offsets))*1000:0),
      clockOffsetStepMs:String(this.maximumOffsetStep*1000),clockRawBacksteps:String(this.rawBacksteps),clockMaxRefreshGapMs:String(this.maximumRefreshGap)};
  }
  constructor(readonly sampleRate: number, readonly graphLatencySeconds: number) {}
  observe(receipt: DeviceReceipt) {
    if (!counter(receipt.consumedFrames) || !counter(receipt.runStartConsumed) ||
        !Number.isSafeInteger(receipt.renderFrame) || receipt.renderFrame! < 0 ||
        !Number.isSafeInteger(receipt.quantumFrames) || receipt.quantumFrames! <= 0 ||
        !["preparing", "playing", "buffering", "ended"].includes(receipt.phase ?? "") ||
        receipt.copied < 0 || receipt.copied > receipt.quantumFrames!)
      throw Error("Invalid audio output clock receipt.");
    if(Number.isSafeInteger(receipt.repeatedQuanta)&&receipt.repeatedQuanta!>=0)this.repeatedQuanta=receipt.repeatedQuanta!;
    const through = BigInt(receipt.consumedFrames);
    if (through < this.submitted) throw Error("Audio submission clock moved backwards.");
    this.submitted = through;
    this.phase = receipt.phase;
    if (receipt.runStartFrame === null) {
      if (through !== 0n) throw Error("Audio output clock lost its content interval.");
      return;
    }
    if (!Number.isSafeInteger(receipt.runStartFrame) || receipt.runStartFrame! < 0)
      throw Error("Invalid audio output interval.");
    const first = BigInt(receipt.runStartConsumed!);
    if (first > through) throw Error("Invalid audio output sample interval.");
    const startFrame=receipt.runStartFrame!,endFrame=startFrame+Number(through-first);
    if(!Number.isSafeInteger(endFrame))throw Error("Invalid audio output frame interval.");
    const start = receipt.runStartFrame! / this.sampleRate;
    const end = start + Number(through - first) / this.sampleRate;
    const last = this.spans.at(-1);
    if (last && last.start === start && last.first === first) {
      if (end < last.end) throw Error("Audio output interval moved backwards.");
      last.end = end; last.endFrame=endFrame; last.through = through;
    } else {
      if (last && (startFrame < last.endFrame || first !== last.through)) {
        const cause={
          previousStart:last.startFrame,previousEnd:last.endFrame,previousFirst:String(last.first),previousThrough:String(last.through),
          startFrame,endFrame,first:String(first),through:String(through),renderFrame:receipt.renderFrame,copied:receipt.copied,
        };
        this.intervalError=JSON.stringify(cause);
        throw new Error("Audio output intervals overlap or skip content.",{cause});
      }
      if (this.spans.length >= 128) throw Error("Audio output interval budget exceeded.");
      this.spans.push({ start, end, startFrame,endFrame,first,through });
    }
  }
  sample(timestamp: Partial<Timestamp>, now: number) {
    if (typeof timestamp.contextTime !== "number" || typeof timestamp.performanceTime !== "number" ||
        !Number.isFinite(timestamp.contextTime) || !Number.isFinite(timestamp.performanceTime) ||
        timestamp.contextTime <= 0 || timestamp.performanceTime <= 0 ||
        !Number.isFinite(now) || now - timestamp.performanceTime > 1000 || timestamp.performanceTime - now > 100)
      return null;
    if (this.timestamp && (timestamp.contextTime < this.timestamp.contextTime ||
        timestamp.performanceTime < this.timestamp.performanceTime))
      throw Error("Audio output device clock changed. Resume playback to recalibrate.");
    this.timestamp = { contextTime: timestamp.contextTime, performanceTime: timestamp.performanceTime };
    const context = timestamp.contextTime + (now - timestamp.performanceTime) / 1000 - this.graphLatencySeconds;
    const first = this.spans[0];
    if (!first || context < first.start) return { output: null, started: false, stalled: true };
    const offset=timestamp.contextTime-timestamp.performanceTime/1000,previousOffset=this.offsets.at(-1);
    if(previousOffset!==undefined)this.maximumOffsetStep=Math.max(this.maximumOffsetStep,Math.abs(offset-previousOffset));
    if(this.offsets.length===4096)this.offsets.shift();this.offsets.push(offset);
    if(this.previousContext!==null&&context<this.previousContext)this.rawBacksteps++;
    this.previousContext=context;
    if(this.previousSampleAt!==null)this.maximumRefreshGap=Math.max(this.maximumRefreshGap,now-this.previousSampleAt);
    this.previousSampleAt=now;
    let frames = first.first;
    let gap = true;
    let futureRun = false;
    for (const span of this.spans) {
      if (context < span.start) { futureRun = true; break; }
      frames = span.through;
      if (context < span.end) {
        frames = span.first + BigInt(Math.max(0, Math.floor((context - span.start) * this.sampleRate + 1e-7)));
        gap = false;
        break;
      }
    }
    this.presented = frames > this.presented ? frames : this.presented;
    while (this.spans.length > 1 && this.spans[1].start <= context) this.spans.shift();
    const output: NonNullable<OutputObservation> = {
      frames: this.presented.toString(), source: "device_timestamp", uncertainty_frames: null,
    };
    return { output, started: true, stalled: gap && (futureRun || this.phase === "buffering") };
  }
  get submittedFrames() { return this.submitted.toString(); }
}
