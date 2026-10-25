// Keep callers on the existing typed ASR-intent API; persistence is confined to
// one bounded control-state adapter shared with caption protocol locators.
export {
  readTranscriptionRequest,
  writeTranscriptionRequest,
  clearTranscriptionRequest,
} from "./client-state";
export type { TranscriptionRequest } from "./client-state";
