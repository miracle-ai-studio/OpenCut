import type { CasResource, PrimaryAudioTrack, PitchFacts } from "../src/index.d.mts";
import type { HostAdmission, RuntimeConfiguration, Ready, Started, Failed, Cancelled, Reply, ReplyChunk, CallbackMap, QuotaReceipt } from "./protocol.d.mts";

/** Separate private preview-v1 protocol. Export WAV packets remain frozen. */
interface Envelope { protocolVersion: 1; executionId: string }
export interface PreviewIdentity { projectId: string; revision: string; sequenceId: string; profile: "classic-realtime-v31-web-audio"; sampleRate: number; channels: 2 }
export interface PreviewPlan { identity: PreviewIdentity; frameCount: string; resources: CasResource[]; voices: { elementId: string; assetId: string; sourceMode: "preserve-pts" | "prepared-concat" }[] }
export interface PreviewBegin extends Envelope { type: "begin"; plan: PreviewPlan; admission: HostAdmission & { deadlineMs: 3600000 }; preparationDeadlineMs: number; runtime: RuntimeConfiguration; resources: { assetId: string; handle: string }[] }
export interface PreviewNext extends Envelope { type: "command"; op: "next"; id: string; generation: string; overlayEpoch: string; startSample: string; frameCount: number }
export interface PreviewStop extends Envelope { type: "command"; op: "stop" }
export interface PreviewSegment { segmentId: number; timestampSeconds: number; frameCount: string }
export interface PreviewSourceFormat { elementId: string; sampleRate: number; frameCount: string; channels: number; prepared: boolean; segments: PreviewSegment[] }
export interface SharedPreviewBindingV1 { schemaVersion: 1; ordinarySources: { assetId: string; segments: PreviewSegment[] }[]; sourceFormats: { elementId: string; sampleRate: number; frameCount: string; channels: number; prepared: boolean; sourceRef: string | null }[] }
export type PreviewPrivateResult<T> = { ok: true; value?: T } | { ok: false; error: { code: string } };
export interface PreviewPosition { segmentId: number; lower: string; upper: string; fraction: number }
export interface PreviewCompiledBlock { identity: PreviewIdentity; startSample: string; frameCount: number; voices: { elementId: string; blockOffset: number; frameCount: number; positions: (PreviewPosition | null)[]; amplitudes: number[] }[]; receipts: unknown[] }
export interface PreviewCallbackMap extends Omit<CallbackMap, "compileBlock" | "admitDecode"> {
  admitDecode: { input: { resource: CasResource; primary: PrimaryAudioTrack }; output: { maxFrames: number; maxChannels: number } };
  observePreviewDecoded: { input: { resource: CasResource; sampleRate: number; frameCount: string; channels: number }; output: unknown };
  bindPreviewSourcesSharedV1: { input: SharedPreviewBindingV1; output: unknown };
  compilePreviewBlock: { input: { startSample: string; frameCount: number }; output: PreviewCompiledBlock };
}
export interface PreviewPreparedReceipt { identity: PreviewIdentity; engineIdentity: string; sourceBindingSha256: string; decoders: { assetId: string; primary: PrimaryAudioTrack; sampleRate: number; frameCount: string; channels: number }[]; sourceBinding: SharedPreviewBindingV1; preparation: { elementId: string; facts: PitchFacts }[]; peakAccountedBytes: number }
export interface PreviewPrepared extends Envelope { type: "prepared"; receipt: PreviewPreparedReceipt }
export interface PreviewPcm extends Envelope { type: "pcm"; commandId: string; generation: string; overlayEpoch: string; startSample: string; channel: 0 | 1; frames: number; bytesHex: string }
export interface PreviewBlock extends Envelope { type: "block"; commandId: string; generation: string; overlayEpoch: string; startSample: string; frameCount: number; receipt: Pick<PreviewPreparedReceipt, "identity" | "engineIdentity" | "sourceBindingSha256" | "peakAccountedBytes"> & { compilation: { startSample: string; frameCount: number; receipts: unknown[] } } }
export interface PreviewRequest extends Envelope { type: "request"; id: string; op: keyof PreviewCallbackMap; args: PreviewCallbackMap[keyof PreviewCallbackMap]["input"] }
export interface PreviewStopped extends Envelope { type: "stopped"; identity: Ready["identity"]; quota: QuotaReceipt }
export type PreviewInput = PreviewBegin | PreviewNext | PreviewStop | Reply | ReplyChunk | (Envelope & { type: "cancel" });
export type PreviewOutput = Started | Ready | PreviewRequest | PreviewPrepared | PreviewPcm | PreviewBlock | PreviewStopped | Failed | Cancelled;
