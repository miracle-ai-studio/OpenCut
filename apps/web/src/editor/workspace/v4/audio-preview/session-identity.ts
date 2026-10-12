import type { V4AudioPreviewCreate, V4AudioPreviewSession } from "../types";
import { sameAudioIdentity } from "./transport";
export const counter = (s: unknown): s is string =>
  typeof s === "string" &&
  /^(0|[1-9][0-9]{0,19})$/.test(s) &&
  BigInt(s) <= 18446744073709551615n;
/** Same fixed-session admission for the UI shell and its consumption delegate. */
export function checkPreviewSession(
  s: V4AudioPreviewSession,
  expected: Pick<
    V4AudioPreviewCreate,
    "project_id" | "revision" | "sequence_id"
  >,
  sampleRate: number,
  previous?: V4AudioPreviewSession,
) {
  const i = s.identity;
  if (
    s.schema_version !== 1 ||
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
    !counter(s.lease_remaining_ms) ||
    BigInt(s.actual_start_sample) + BigInt(s.consumed_frames) >
      BigInt(s.total_frames) ||
    !s.authoritative_time ||
    !/^-?\d+$/.test(s.authoritative_time.ticks) ||
    !Number.isInteger(s.authoritative_time.timebase) ||
    s.authoritative_time.timebase <= 0 ||
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
