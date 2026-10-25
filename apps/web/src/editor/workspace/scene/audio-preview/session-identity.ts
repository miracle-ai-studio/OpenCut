import type { AudioPreviewCreate, AudioPreviewSession } from "../types";
import { sameAudioIdentity } from "./transport";
export const counter = (s: unknown): s is string =>
  typeof s === "string" &&
  /^(0|[1-9][0-9]{0,19})$/.test(s) &&
  BigInt(s) <= 18446744073709551615n;
/** Same fixed-session admission for the UI shell and its consumption delegate. */
export function checkPreviewSession(
  s: AudioPreviewSession,
  expected: Pick<
    AudioPreviewCreate,
    "project_id" | "revision" | "sequence_id"
  >,
  sampleRate: number,
  previous?: AudioPreviewSession,
) {
  const i = s.identity;
  if (
    !s.session_id ||
    !s.owner_id ||
    !s.lease_id ||
    !i ||
    i.project_id !== expected.project_id ||
    i.revision !== expected.revision ||
    i.sequence_id !== expected.sequence_id ||
    i.sample_rate !== sampleRate ||
    i.channels !== 2 ||
    i.profile !== "classic-realtime-v31" ||
    !counter(s.generation) ||
    !counter(s.overlay_epoch) ||
    !counter(s.actual_start_sample) ||
    !counter(s.total_frames) ||
    !counter(s.consumed_frames) ||
    !s.submitted_time ||
    !/^-?\d+$/.test(s.submitted_time.ticks) ||
    !Number.isInteger(s.submitted_time.timebase) || s.submitted_time.timebase <= 0 ||
    (s.output && (!counter(s.output.frames) || s.output.source !== "device_timestamp" ||
      BigInt(s.output.frames) > BigInt(s.consumed_frames))) ||
    !counter(s.lease_remaining_ms) ||
    BigInt(s.actual_start_sample) + BigInt(s.consumed_frames) >
      BigInt(s.total_frames) ||
    !s.presentation_time ||
    !/^-?\d+$/.test(s.presentation_time.ticks) ||
    !Number.isInteger(s.presentation_time.timebase) ||
    s.presentation_time.timebase <= 0 ||
    (previous &&
      (s.session_id !== previous.session_id ||
        !sameAudioIdentity(i, previous.identity) ||
        s.owner_id !== previous.owner_id ||
        s.lease_id !== previous.lease_id))
  )
    throw new Error(
      "Scene audio returned an unexpected fixed session identity.",
    );
}
