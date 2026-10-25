import type { ClassicAudioPlan, ClassicAudioHost, PitchFacts, ExactSourceSample, CompiledSampleBlock, SourceFormat, CasResource, PrimaryAudioTrack } from "../src/index.d.mts";

/** Private process NDJSON transport; typed content and fixed execution identity.
 * All byte strings are lowercase hex. PCM is always stereo f32le-planar. */
export interface HostAdmission {
  maxPcmBytes: number;
  maxEncodedBytes: number; // Resident range-cache and transport budget; source length is independent.
  maxRssBytes: string;
  deadlineMs: number;
  quotaProfile: "monitored-terminate" | "strict-hard";
  monitorIntervalMs?: number; // default 100, maximum 250
  cgroupDirectory?: string; // strict-hard existing Linux cgroup v2 only
}
export interface RuntimeConfiguration {
  chromiumExecutable: string;
  chromiumSha256?: string;
  playwrightRoot?: string; // default exact installed same-package dependency
  mediabunnyRoot?: string; // default exact installed same-package dependency
}
interface Envelope { executionId: string }
export interface Begin extends Envelope { type: "begin"; plan: ClassicAudioPlan; admission: HostAdmission; resources: { assetId: string; handle: string }[]; runtime: RuntimeConfiguration }
export interface Cancel extends Envelope { type: "cancel" }
export interface Reply extends Envelope { type: "reply"; id: string; ok: boolean; value?: unknown; error?: { code: string; message: string } }
/** Ordered parts of one UTF-8 JSON value, maximum 16 MiB after decoding. */
export type ReplyChunk = Envelope & { type: "reply_chunk"; id: string; sequence: number; bytesHex: string; final: boolean } & (
  { encoding?: "identity"; decodedBytes?: never } | { encoding: "zlib"; decodedBytes: number }
);
export type InputPacket = Begin | Cancel | Reply | ReplyChunk;
/** Private lossless projection. Logical host/mixer DTOs remain unchanged. */
export interface PackedExportBlock {
  block: Omit<CompiledSampleBlock, "voices"> & { voices: {
    elementId: string; blockOffset: number; frameCount: number;
    positionRef: number; amplitudeRef: number;
  }[] };
  positionTables: [lower: string, numerator: string, denominator: string][][];
  amplitudeTables: number[][];
}
export interface CallbackMap {
  resolveVerifiedBytes: { input: { handle: string; resource: CasResource }; output: { transferId: string; byteLength: string } };
  openVerifiedRange: CallbackMap["resolveVerifiedBytes"];
  readVerifiedBytes: { input: { transferId: string; offset: string; count: number }; output: { offset: string; bytesHex: string; eof: boolean } };
  releaseVerifiedBytes: { input: { transferId: string }; output: { released: true } };
  releaseDecodedSource: { input: { resource: CasResource }; output: Awaited<ReturnType<ClassicAudioHost["releaseDecodedSource"]>> };
  admitDecode: { input: { resource: CasResource; primary: PrimaryAudioTrack | null }; output: { maxFrames: number; maxChannels: number } };
  admitContainerDecode: { input: { resource: CasResource; primary: PrimaryAudioTrack }; output: Awaited<ReturnType<ClassicAudioHost["admitContainerDecode"]>> };
  admitPcmAudioDecode: { input: { resource: CasResource; primary: PrimaryAudioTrack }; output: Awaited<ReturnType<ClassicAudioHost["admitPcmAudioDecode"]>> };
  compilePitchPreparation: { input: Parameters<ClassicAudioHost["compilePitchPreparation"]>[0]; output: PitchFacts };
  compileSourcePositions: { input: { elementId: string; localStartSample: string; frameCount: number; sourceSampleRate: number }; output: ExactSourceSample[] };
  compileBlock: { input: { startSample: string; frameCount: number; sourceFormats: SourceFormat[] }; output: PackedExportBlock };
}
export interface Request extends Envelope { type: "request"; id: string; op: keyof CallbackMap; args: CallbackMap[keyof CallbackMap]["input"] }
export interface Started extends Envelope { type: "started"; browserPid: number }
export interface Ready extends Envelope { type: "ready"; browserPid: number; identity: { engine: string; browserVersion: string; browserSha256: string; moduleSha256: string; playwrightVersion: "1.63.0"; mediabunnyVersion: "1.41.0" }; quota: QuotaReceipt }
export interface Pcm extends Envelope { type: "pcm"; channel: 0 | 1; offsetFrames: string; frames: number; bytesHex: string }
export interface QuotaReceipt { profile: "monitored-terminate" | "strict-hard"; maxRssBytes: string; observedPeakRssBytes: string; monitorIntervalMs: number; elapsedMs: number; hardMemoryEnforced: boolean }
export interface Complete extends Envelope { type: "complete"; receipt: unknown; quota: QuotaReceipt }
export interface Failed extends Envelope { type: "failed"; error: { code: string; message: string }; quota?: QuotaReceipt }
/** Terminal packets follow actual browser process close/kill acknowledgement. */
export interface Cancelled extends Envelope { type: "cancelled"; quota?: QuotaReceipt }
export type OutputPacket = Request | Started | Ready | Pcm | Complete | Failed | Cancelled;
