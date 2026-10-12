import type { ClassicAudioPlan, PitchFacts, ExactSourceSample, CompiledSampleBlock, SourceFormat, CasResource, PrimaryAudioTrack } from "../src/index.d.mts";

/** Private process NDJSON protocol v1; no existing public editor wire changes.
 * All byte strings are lowercase hex. PCM is always stereo f32le-planar. */
export interface HostAdmission {
  maxPcmBytes: number;
  maxEncodedBytes: number; // <= 32 MiB per immutable resource
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
interface Envelope { protocolVersion: 1; executionId: string }
export interface Begin extends Envelope { type: "begin"; plan: ClassicAudioPlan; admission: HostAdmission; resources: { assetId: string; handle: string }[]; runtime: RuntimeConfiguration }
export interface Cancel extends Envelope { type: "cancel" }
export interface Reply extends Envelope { type: "reply"; id: string; ok: boolean; value?: unknown; error?: { code: string; message: string } }
/** Ordered parts of one UTF-8 JSON value, maximum 16 MiB after decoding. */
export interface ReplyChunk extends Envelope { type: "reply_chunk"; id: string; sequence: number; bytesHex: string; final: boolean }
export type InputPacket = Begin | Cancel | Reply | ReplyChunk;
export interface CallbackMap {
  resolveVerifiedBytes: { input: { handle: string; resource: CasResource }; output: { transferId: string; byteLength: string } };
  readVerifiedBytes: { input: { transferId: string; offset: string; count: number }; output: { offset: string; bytesHex: string; eof: boolean } };
  releaseVerifiedBytes: { input: { transferId: string }; output: { released: true } };
  admitDecode: { input: { resource: CasResource; primary: PrimaryAudioTrack | null }; output: { maxFrames: number; maxChannels: number } };
  compilePitchPreparation: { input: { elementId: string; decodedSampleRate: number; decodedFrameCount: string; decodedChannels: number }; output: PitchFacts };
  compileSourcePositions: { input: { elementId: string; localStartSample: string; frameCount: number; sourceSampleRate: number }; output: ExactSourceSample[] };
  compileBlock: { input: { startSample: string; frameCount: number; sourceFormats: SourceFormat[] }; output: CompiledSampleBlock };
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
