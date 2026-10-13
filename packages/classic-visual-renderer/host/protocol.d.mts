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

/** New private text-frame operation; the transport envelope remains v1. */
export type TextFrameIdentityV2 = Omit<FrameIdentity, "profile"> & { profile: "classic-v31-gpu-normal-text-frame-v2" };
export type TextTransformV1 = { centerX: number; centerY: number; scaleX: number; scaleY: number; rotationRadians: number };
export type TextRasterV1 = {
  content: string; fontSizePx: number; lineHeightPx: number; letterSpacingPx: number;
  align: "left" | "center" | "right"; decoration: "none" | "underline" | "lineThrough";
  color: [number, number, number, number]; transform: TextTransformV1;
  background: { enabled: boolean; color: [number, number, number, number]; paddingXPx: number; paddingYPx: number; offsetXPx: number; offsetYPx: number; cornerRadiusPercent: number };
};
export type FontResourceV1 = { fontId: string; contentHash: string; size: string; weight: 400 | 700; style: "normal" | "italic"; faceIndex: 0 };
export type TextPlanV2 = {
  identity: TextFrameIdentityV2; media: Plan;
  texts: { elementId: string; text: TextRasterV1; opacity: number; font: FontResourceV1 & { glyphCoverageValidated: true; validatedTextHash: string } }[];
  order: string[];
};
export type TextRectV1 = { left: number; top: number; width: number; height: number };
export type TextLayoutV1 = {
  fontId: string; fontContentHash: string; validatedTextHash: string;
  fontWeight: 400 | 700; fontStyle: "normal" | "italic"; faceIndex: 0;
  fontSizePx: number; lineHeightPx: number; letterSpacingPx: number;
  align: "left" | "center" | "right"; baseline: "middle";
  block: { maxWidth: number; height: number; visualCenterOffset: number };
  background: (TextRectV1 & { radius: number }) | null;
  lines: { content: string; baselineY: number; width: number; ascent: number; descent: number; decoration: TextRectV1 | null }[];
  transform: TextTransformV1;
};
export type TextRasterReceiptV1 = { elementId: string; width: number; height: number; layout: TextLayoutV1 };
export type TextCallbackV2 = Callback | { op: "resolveVerifiedFontBytes"; args: { font: FontResourceV1; handle: string } } | { op: "bindTextFrame"; args: { rasters: TextRasterReceiptV1[] } };
export type BeginTextFrameV2 = Omit<Begin, "type" | "plan"> & { type: "beginTextFrameV2"; plan: TextPlanV2; fonts: { fontId: string; handle: string }[] };
export function validateTextBegin(packet: unknown): BeginTextFrameV2;
