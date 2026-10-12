import { vi } from "vitest";
import { setupV4 } from "./fixture";
import { V4AudioPreview } from "./audio-preview";
import type { AudioOutputProgress } from "./audio-preview/output";
import type {
  V4AudioPreviewSession,
  V4AudioPreviewControl,
  V4AudioPreviewCreate,
} from "./types";

/** Protocol/device-consumer fixture. Actual PCM, WebAudio and permissions are
 * independently exercised by the real browser acceptance owned by the host. */
export async function setupAudioPreview() {
  const f = await setupV4();
  let state = f.controller.getSnapshot().v4!,
    generation = 1;
  const sessionFor = (
    request: V4AudioPreviewCreate,
  ): V4AudioPreviewSession => ({
    schema_version: 1,
    session_id: "preview",
    generation: "1",
    overlay_epoch: "0",
    identity: {
      project_id: request.project_id,
      revision: request.revision,
      sequence_id: request.sequence_id,
      sample_rate: request.sample_rate,
      channels: 2,
      profile: "classic-realtime-v31",
    },
    owner_id: "owner",
    lease_id: "lease",
    state: "ready",
    actual_start_sample: "0",
    total_frames: "48000",
    authoritative_time: { ticks: "0", timebase: 48000 },
    consumed_frames: "0",
    produced_through_sample: "8192",
    buffered_frames: "8192",
    presentation_fresh: true,
    stalled: false,
    lease_remaining_ms: "30000",
    expires_in_ms: "120000",
    temporary_gain: null,
    failure_code: null,
    next_event_cursor: "0",
  });
  let session: V4AudioPreviewSession | null = null;
  const created = new Map<string, V4AudioPreviewSession>();
  const sessions = new Map<string, V4AudioPreviewSession>();
  const controls: V4AudioPreviewControl[] = [],
    requests: V4AudioPreviewCreate[] = [];
  const receipts = new Map<string, V4AudioPreviewSession>();
  vi.spyOn(f.api.v4, "audioPreviewCapability").mockResolvedValue({
    schema_version: 1,
    available: true,
    unavailable_reason: null,
    profile: "classic-realtime-v31",
    sample_rates: [44100, 48000],
    stream_path: "/v1/preview/v4/audio/stream",
    max_sessions_per_database: 4,
    max_block_frames: 4096,
    max_buffered_frames: 8192,
    owner_lease_ms: 30000,
    authorization_heartbeat_ms: 1000,
    session_ttl_ms: 120000,
  });
  const create = vi
    .spyOn(f.api.v4, "audioPreviewCreate")
    .mockImplementation(async (r) => {
      requests.push(r);
      session = created.get(r.idempotency_key) ?? {
        ...sessionFor(r),
        session_id: `preview-${created.size}`,
      };
      created.set(r.idempotency_key, session);
      sessions.set(session.session_id, session);
      return structuredClone(session);
    });
  const control = vi
    .spyOn(f.api.v4, "audioPreviewControl")
    .mockImplementation(async (r) => {
      controls.push(r);
      if (receipts.has(r.control_id))
        return structuredClone(receipts.get(r.control_id)!);
      session = sessions.get(r.session_id) ?? null;
      if (!session) throw new Error("Expected session");
      const command = r.command;
      if (command.kind === "play") {
        if (session.state === "paused") {
          session.generation = (BigInt(session.generation) + 1n).toString();
          session.actual_start_sample = session.authoritative_time.ticks;
          session.consumed_frames = "0";
        }
        session.state = "playing";
      } else if (command.kind === "pause" || command.kind === "stop")
        session.state = command.kind === "pause" ? "paused" : "stopped";
      else if (command.kind === "report_consumed") {
        session.consumed_frames = command.consumed_frames;
        session.authoritative_time = {
          ticks: (
            BigInt(session.actual_start_sample) +
            BigInt(command.consumed_frames)
          ).toString(),
          timebase: session.identity.sample_rate,
        };
        session.stalled = command.stalled;
      } else if (
        command.kind === "set_temporary_gain" ||
        command.kind === "clear_temporary_gain" ||
        command.kind === "seek"
      ) {
        session.generation = (BigInt(session.generation) + 1n).toString();
        session.actual_start_sample = session.authoritative_time.ticks;
        session.consumed_frames = "0";
        if (command.kind === "set_temporary_gain")
          session.temporary_gain = command.gain;
        else if (command.kind === "clear_temporary_gain")
          session.temporary_gain = null;
        else {
          // This fixture's returned sample was deliberately chosen by the service.
          session.actual_start_sample = "12345";
          session.authoritative_time = {
            ticks: "12345",
            timebase: session.identity.sample_rate,
          };
        }
        if (command.kind !== "seek")
          session.overlay_epoch = (
            BigInt(session.overlay_epoch) + 1n
          ).toString();
      }
      receipts.set(r.control_id, structuredClone(session));
      return structuredClone(session);
    });
  const stream = vi
    .spyOn(f.api.sdk, "openV4AudioPreviewStream")
    .mockImplementation(async (_input, { signal } = {}) => {
      return new Response(
        new ReadableStream({
          start(controller) {
            signal?.addEventListener("abort", () => {
              try {
                controller.close();
              } catch {
                /* already cancelled */
              }
            });
          },
        }),
        {
          headers: {
            "content-type": "application/vnd.opencut.audio-preview-v4",
          },
        },
      );
    });
  const outputs: {
    silenced: boolean;
    finalFrames: string;
    pushed: Float32Array[];
    progress(p: AudioOutputProgress): void;
    completedBlock(): void;
    failure(error: Error): void;
  }[] = [];
  const preview = new V4AudioPreview(
    {
      api: () => f.api,
      state: () => state,
      generation: () => generation,
      viewGeneration: () => 0,
      changeView: () => 0,
      publish: (s) => {
        state = s;
      },
      acquire: () => () => {},
      refreshTasks: async () => {},
    },
    (time) => {
      state = { ...state, playhead: time };
    },
    (_token, rate, progress, failure, completedBlock) => {
      const output = {
        silenced: false,
        finalFrames: "0",
        pushed: [] as Float32Array[],
        progress,
        completedBlock,
        failure,
      };
      outputs.push(output);
      return {
        sampleRate: rate,
        ready: Promise.resolve(),
        waitForCredit: async () => {},
        push: async (pcm) => {
          output.pushed.push(pcm);
        },
        silence: () => {
          output.silenced = true;
        },
        stop: async () => {
          output.silenced = true;
          return output.finalFrames;
        },
      };
    },
  );
  await preview.capability();
  return {
    ...f,
    preview,
    controls,
    requests,
    outputs,
    create,
    control,
    stream,
    sessionFor,
    state: () => state,
    update: (patch: Partial<typeof state>) => {
      state = { ...state, ...patch };
    },
    invalidateGeneration: () => {
      generation++;
      preview.invalidate();
    },
    cleanup: () => {
      preview.invalidate();
      f.controller.dispose();
    },
  };
}
