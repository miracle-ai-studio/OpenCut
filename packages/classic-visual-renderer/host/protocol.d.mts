/** Private visual-v1 wire; Rust application session owns these facts. */
export type ExactTime = { ticks: string; timebase: number };
export type Resource = { assetId: string; contentHash: string; size: string; kind: "image" | "video" };
export type FrameIdentity = { projectId: string; revision: string; sequenceId: string; time: ExactTime; width: number; height: number; profile: "classic-v31-gpu-normal-frame" };
export type Decode = { kind: "image" } | { kind: "video"; requestedTime: ExactTime; requestedTimeSeconds: number; pointProfile: "classic_video_rounded_ticks"; selectedVideoStream: null; noFrameReason: null | "outside_read_window" };
export type Plan = { identity: FrameIdentity; resources: Resource[]; layers: { elementId: string; resource: Resource; decode: Decode }[] };
export type Decoded = { kind: "readyImage"; elementId: string; width: number; height: number } | { kind: "readyVideo"; elementId: string; width: number; height: number; requestedTime: ExactTime; selectedVideoStream: null; actualTimestampSeconds: number; frameDurationSeconds: number } | { kind: "noFrame"; elementId: string; requestedTime: ExactTime; reason: "outside_read_window" | "eof" | "timestamp_gap" };
export type Callback = { op: "resolveVerifiedBytes"; args: { resource: Resource; handle: string } } | { op: "readVerifiedBytes"; args: { transferId: string; offset: string; count: number } } | { op: "releaseVerifiedBytes"; args: { transferId: string } } | { op: "admitDecode"; args: { elementId: string } } | { op: "bindFrame"; args: { decoded: Decoded[] } };
export type Admission = { maxFramePixels: string; maxDecodedPixels: string; maxGpuBytes: string; maxRgbaBytes: string; maxEncodedBytes: number; maxOutputBytes: string; maxRssBytes: string; deadlineMs: number; quotaProfile: "monitored-terminate" | "strict-hard"; cgroupDirectory?: string | null; monitorIntervalMs?: number };
export type Begin = { protocolVersion: 1; executionId: string; type: "begin"; plan: Plan; resources: { assetId: string; handle: string }[]; admission: Admission; runtime: { chromiumExecutable: string; chromiumSha256?: string } };
export type RgbaPacket = { protocolVersion: 1; executionId: string; type: "rgba"; offsetBytes: string; bytesHex: string };
export function validateBegin(packet: unknown): Begin;
export function validateAdmission(value: unknown): Admission;
