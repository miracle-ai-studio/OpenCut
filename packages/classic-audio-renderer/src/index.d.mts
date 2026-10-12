/** In-process execution ports; these are not a public API or project schema. */
export interface AudioIdentity {
  projectId: string;
  sequenceId: string;
  revision: string;
  profile: "classic-export-v31-web-audio";
  sampleRate: number;
}
export interface CasResource { assetId: string; contentHash: string; size: number; kind: "audio" | "video" }
export interface PrimaryAudioTrack { trackId: number; codec: string; sampleRate: number; channels: number }
export interface ExactSourceSample { lower: string; fractionNumerator: string; fractionDenominator: string }
export interface PitchFacts {
  mode: "unprepared-linear" | "prepared-linear" | "prepared-soundtouch";
  sourceSliceStartSample: string;
  requestedSourceFrames: string;
  actualSourceFrames: string;
  resampledFrames: string;
  outputFrames: string;
  numChannels: number;
  decodedSampleRate: number;
  targetSampleRate: number;
  tempo: { numerator: string; denominator: string };
  prefixFrames: string;
  totalChannelFrames: string;
  decodedBytes: string;
  preparedBytes: string;
  totalBytes: string;
}
export interface ClassicAudioPlan {
  identity: AudioIdentity;
  frameCount: string;
  /** Rust output selection length; frameCount remains the whole scene. */
  outputFrameCount?: string;
  range?: { startSample: string; frameCount: string } | null;
  resources: CasResource[];
  /** Stable sequence track order, then stored element order, produced by Rust. */
  voices: { elementId: string; assetId: string }[];
}
export interface SourceFormat { elementId: string; sampleRate: number; frameCount: string; channels: number; prepared: boolean }
export interface CompiledSampleBlock {
  identity: AudioIdentity;
  startSample: string;
  frameCount: number;
  voices: {
    elementId: string;
    blockOffset: number;
    frameCount: number;
    positions: ExactSourceSample[];
    /** Registry-projected amplitude, never raw dB or legacy stored gain. */
    amplitudes: number[];
  }[];
}
export interface ClassicAudioHost {
  /** Pinned actual browser build/platform/codec execution identity. */
  engineIdentity: string;
  resolveVerifiedBytes(resource: Readonly<CasResource>, signal?: AbortSignal): Promise<Uint8Array>;
  /** Must enforce hard decoder memory/time limits in the isolated host realm. */
  admitDecode(resource: CasResource, primary: PrimaryAudioTrack | null): Promise<{ maxFrames: number; maxChannels: number }>;
  /** Calls classic_audio_preparation_facts_v4 with the original authored span. */
  compilePitchPreparation(input: { elementId: string; decodedSampleRate: number; decodedFrameCount: string; decodedChannels: number }): Promise<PitchFacts>;
  /** Original source mapping, at each exact local output sample; no gain. */
  compileSourcePositions(input: { elementId: string; localStartSample: string; frameCount: number; sourceSampleRate: number }): Promise<ExactSourceSample[]>;
  /** Calls compile_audio_block_v4; prepared sources read from zero at unit rate
   * while the gain clock remains the original element's timeline-local clock.
   * frameCount is a maximum request: Rust may return a shorter positive block
   * at the exact startSample to preserve union and voice-sample admission. */
  compileBlock(input: { startSample: string; frameCount: number; sourceFormats: SourceFormat[] }): Promise<CompiledSampleBlock>;
  createAudioContext(sampleRate: number): AudioContext;
  createOfflineContext(channels: number, frames: number, sampleRate: number): OfflineAudioContext;
  /** Idempotent actual realm/process termination and acknowledgement. */
  abortRendering(): Promise<void>;
}
export class AudioExecutionError extends Error { code: string }
export function createClassicMasteringChain(context: BaseAudioContext, destination: AudioNode): { input: GainNode; disconnect(): void };
export function renderClassicExport(plan: ClassicAudioPlan, host: ClassicAudioHost, options: { signal?: AbortSignal; maxPcmBytes: number; maxEncodedBytes?: number }): Promise<{
  buffer: AudioBuffer;
  receipt: {
    identity: AudioIdentity;
    engineIdentity: string;
    decoders: { assetId: string; profile: string; sampleRate: number; frameCount: string; channels: number; primary: PrimaryAudioTrack | null }[];
    preparation: { elementId: string; facts: PitchFacts }[];
    mastering: { profile: string; wholeSceneFrames: string; inputPeak: number; applied: boolean };
    range: { startSample: string; frameCount: string } | null;
    peakAccountedBytes: number;
  };
}>;
