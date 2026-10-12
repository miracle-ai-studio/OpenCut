import { vi } from "vitest";
import { setupV4 } from "./fixture";
import type {
  V4SourceAudioRequest,
  V4SourceAudioPlan,
  V4Element,
} from "./types";

/** Protocol-only fixtures. Real CAS/probe/PCM and SQLite authority are tested by
 * the root candidate; these cases prove request identity and client fences. */
export async function setupSourceAudio() {
  const fixture = await setupV4();
  const { api, controller, current, snapshots } = fixture;
  const doc = current().project;
  doc.assets[0].kind = "video";
  doc.assets[0].metadata.audio = {
    status: "present",
    metadata: { channels: 2, sample_rate: 48000 },
  };
  const source = doc.sequences[0].tracks[0].elements[0];
  source.id = "video";
  source.name = "Source video";
  source.content = {
    kind: "video",
    asset_id: "asset",
    source_audio_enabled: true,
    audio: { gain: { unit: "decibels", value: -6 }, muted: false, stream: 0 },
    retime: {
      maintain_pitch: false,
      rate: { numerator: 1, denominator: 1 },
      mapping: { kind: "classic_rounded_ticks", timebase: 120000 },
    },
    video_stream: 0,
  };
  snapshots.set("r0", structuredClone(current()));
  await controller.v4.refresh();
  controller.v4.select(source.id);
  const base = vi.mocked(api.sdk.call).getMockImplementation()!;
  const requests: V4SourceAudioRequest[] = [],
    plans: V4SourceAudioPlan[] = [];
  const planFor = (request: V4SourceAudioRequest): V4SourceAudioPlan => {
    const seq = current().project.sequences[0],
      track = seq.tracks[0];
    const video = structuredClone(
      track.elements.find((e) => e.id === request.element_id)!,
    );
    if (video.content.kind !== "video")
      throw new Error("Expected fixture video");
    const content = video.content;
    content.source_audio_enabled = request.command.kind === "restore";
    const operations: V4SourceAudioPlan["transaction"]["operations"] = [
      {
        type: "element_put",
        sequence_id: seq.id,
        track_id: track.id,
        element: video,
      },
    ];
    let detached: V4SourceAudioPlan["decision"]["detached"] = null;
    if (request.command.kind === "separate") {
      const command = request.command;
      if (command.destination.kind !== "first_available")
        throw new Error("Expected automatic destination");
      const id = command.destination.fallback_track_id;
      const audio: V4Element = {
        ...structuredClone(video),
        id: command.audio_element_id,
        name: "Separated audio",
        visual: null,
        content: {
          kind: "audio",
          asset_id: content.asset_id,
          audio: content.audio,
          retime: content.retime,
        },
      };
      operations.push({
        type: "track_put",
        sequence_id: seq.id,
        track: {
          ...structuredClone(track),
          id,
          name: "Audio track",
          kind: "audio",
          elements: [],
        },
      });
      operations.push({
        type: "element_put",
        sequence_id: seq.id,
        track_id: id,
        element: audio,
      });
      detached = {
        element_id: audio.id,
        track_id: id,
        track_created: true,
        copied_channels: [],
      };
    }
    return {
      original_request_digest: "digest",
      transaction: {
        project_id: request.project_id,
        expected_revision: request.expected_revision,
        idempotency_key: request.idempotency_key,
        label: request.label,
        operations,
      },
      decision: {
        source_element_id: video.id,
        asset_id: content.asset_id,
        source_audio_enabled: content.source_audio_enabled,
        source_request: {
          start: { ticks: "0", timebase: 1 },
          duration: { ticks: "12", timebase: 1 },
        },
        detached,
      },
    };
  };
  const commit = (transaction: V4SourceAudioPlan["transaction"]) => {
    const seq = current().project.sequences[0];
    for (const op of transaction.operations) {
      if (op.type === "track_put") seq.tracks.push(structuredClone(op.track));
      if (op.type === "element_put") {
        const track = seq.tracks.find((t) => t.id === op.track_id)!;
        track.elements = [
          ...track.elements.filter((e) => e.id !== op.element.id),
          structuredClone(op.element),
        ];
      }
    }
    current().revision = `source-${transaction.idempotency_key}`;
    snapshots.set(current().revision, structuredClone(current()));
    return {
      base_revision: transaction.expected_revision,
      revision: current().revision,
      transaction_id: `tx-${transaction.idempotency_key}`,
      changed_ids: ["video"],
      created_ids: [],
      deleted_ids: [],
    };
  };
  vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
    if (name === "opencut_project_v4_source_audio_plan") {
      const request = input as V4SourceAudioRequest;
      requests.push(structuredClone(request));
      const plan = planFor(request);
      plans.push(plan);
      return plan as never;
    }
    if (name === "opencut_edit_v4_apply")
      return commit(input as V4SourceAudioPlan["transaction"]) as never;
    return base(name, input, options);
  });
  return { ...fixture, requests, plans, planFor, commit };
}
