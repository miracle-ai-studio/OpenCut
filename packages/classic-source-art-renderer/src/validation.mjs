export class SourceArtError extends Error { constructor(code, message) { super(message); this.code = code; } }
export function requireFact(value, message = "Invalid source-art facts") { if (!value) throw new SourceArtError("SOURCE_ART_PROTOCOL_ERROR", message); }
export function checkAbort(signal) { if (signal?.aborted) throw new SourceArtError("CANCELLED", "Source art was cancelled"); }
export function positive(value, limit) { requireFact(Number.isSafeInteger(value) && value > 0 && value <= limit); return value; }
export const algorithm = kind => kind === "waveform" ? "source-peaks128-v1" : kind === "source_frame" ? "source-frame-at-jpeg08-v2" : "classic-thumbnail-1s-jpeg08-v1";

export const decodeProfile = "fixed-cas-streamed-concat-webaudio-canvas";
