import type { CasResource, PrimaryAudioTrack, PitchFacts, ContainerDecodeAllocation } from "../src/index.d.mts";
import type { HostAdmission, RuntimeConfiguration, Ready, Started, Failed, Cancelled, Reply, ReplyChunk, CallbackMap, QuotaReceipt } from "./protocol.d.mts";

/** One private continuous preview transport, with fixed primary identity and command-scoped PCM facts. */
interface Envelope { executionId: string }
export interface PreviewIdentity { projectId: string; revision: string; sequenceId: string; profile: "classic-realtime-v31-web-audio"; sampleRate: number; channels: 2 }
export interface PreviewPlan { identity: PreviewIdentity; frameCount: string; encodedSource: { mode: "verified-range"; cacheBytes: number }; resources: CasResource[]; voices: { elementId: string; assetId: string; sourceMode: "preserve-pts" | "prepared-concat" }[] }
export interface PreviewBegin extends Envelope { type: "begin"; plan: PreviewPlan; admission: HostAdmission & { deadlineMs: 3600000 }; preparationDeadlineMs: number; runtime: RuntimeConfiguration; resources: { assetId: string; handle: string }[] }
export interface PreviewNext extends Envelope { type: "command"; op: "next"; id: string; generation: string; overlayEpoch: string; startSample: string; frameCount: number; sourceWindows?: { assetId: string; startSeconds: number; endSeconds: number }[] }
export interface PreviewStop extends Envelope { type: "command"; op: "stop" }
export interface PreviewSegment { segmentId: number; timestampSeconds: number; frameCount: string }
export interface PreviewSourceFormat { elementId: string; sampleRate: number; frameCount: string; channels: number; prepared: boolean; segments: PreviewSegment[] }
export interface SharedPreviewBinding { ordinarySources: { assetId: string; segments: PreviewSegment[] }[]; sourceFormats: { elementId: string; sampleRate: number; frameCount: string; channels: number; prepared: boolean; sourceRef: string | null }[] }
export type PreviewPrivateResult<T> = { ok: true; value?: T } | { ok: false; error: { code: string } };
export interface PreviewPosition { segmentId: number; lower: string; upper: string; fraction: number }
export interface PreviewCompiledBlock { identity: PreviewIdentity; startSample: string; frameCount: number; voices: { elementId: string; blockOffset: number; frameCount: number; positions: (PreviewPosition | null)[]; amplitudes: number[] }[]; receipts: unknown[] }
export interface PreviewCallbackMap extends Omit<CallbackMap, "compileBlock" | "admitDecode" | "admitContainerDecode" | "admitPcmAudioDecode" | "resolveVerifiedBytes" | "releaseDecodedSource"> {
  observePreviewDecoded: { input: { resource: CasResource; sampleRate: number; frameCount: string; channels: number }; output: unknown };
  observeWindowedPreviewDecoded: { input: { decoded: PreviewCallbackMap["observePreviewDecoded"]["input"]; residentFrameCount: string }; output: unknown };
  bindAndCompilePreviewBlockPacked: { input: { binding: SharedPreviewBinding; window: PreviewWindowRequest }; output: string };
  bindPreviewSourcesShared: { input: { binding: SharedPreviewBinding; window: PreviewWindowRequest | null }; output: unknown };
  previewSourceWindows: { input: PreviewWindowRequest; output: PreviewSourceWindow[] };
  compilePreviewBlockPacked: { input: PreviewWindowRequest; output: string };
  admitPreviewDecode: { input: { resource: CasResource; primary: PrimaryAudioTrack }; output: { stream: RollingPreviewAdmission | null; preparation: ContainerDecodeAllocation | null } };
  compilePreviewBlock: { input: { startSample: string; frameCount: number }; output: PreviewCompiledBlock };
}
export interface PreviewWindowRequest { startSample: string; frameCount: number }
export interface PreviewSourceWindow { assetId: string; startSeconds: number; endSeconds: number }
export interface RollingPreviewAdmission { workingFrames: number; maxChannels: number; maxChunkFrames: number; sourceSampleRate: number; targetSampleRate: number }
export interface PreviewPreparedReceipt { identity: PreviewIdentity; engineIdentity: string; sourceBindingSha256: string; decoders: { assetId: string; primary: PrimaryAudioTrack; sampleRate: number; frameCount: string | null; channels: number }[]; sourceBinding: SharedPreviewBinding; preparation: { elementId: string; facts: PitchFacts }[]; peakAccountedBytes: number }
export interface PreviewPrepared extends Envelope { type: "prepared"; receipt: PreviewPreparedReceipt }
export interface PreviewPcm extends Envelope { type: "pcm"; commandId: string; generation: string; overlayEpoch: string; startSample: string; channel: 0 | 1; frames: number; bytesHex: string }
export interface PreviewBlock extends Envelope { type: "block"; commandId: string; generation: string; overlayEpoch: string; startSample: string; frameCount: number; receipt: Pick<PreviewPreparedReceipt, "identity" | "engineIdentity" | "sourceBindingSha256" | "peakAccountedBytes"> & { compilation: { startSample: string; frameCount: number; receipts: unknown[] } } }
export interface PreviewRequest extends Envelope { type: "request"; id: string; op: keyof PreviewCallbackMap; args: PreviewCallbackMap[keyof PreviewCallbackMap]["input"] }
export interface PreviewStopped extends Envelope { type: "stopped"; identity: Ready["identity"]; quota: QuotaReceipt }
export type PreviewInput = PreviewBegin | PreviewNext | PreviewStop | Reply | ReplyChunk | (Envelope & { type: "cancel" });
export type PreviewOutput = Started | Ready | PreviewRequest | PreviewPrepared | PreviewPcm | PreviewBlock | PreviewStopped | Failed | Cancelled;
