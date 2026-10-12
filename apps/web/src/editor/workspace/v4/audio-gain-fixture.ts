import { vi } from "vitest";
import { setupSourceAudio } from "./source-audio-fixture";
import { audioGainAvailability } from "./audio-gain";
import type { V4GainRequest, V4GainPlan } from "./types";

/** Protocol fixture only; native PCM and CAS authority are separate acceptance. */
export async function setupAudioGain() {
  const f = await setupSourceAudio();
  const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
  const requests: V4GainRequest[] = [],
    plans: V4GainPlan[] = [];
  const planFor = (request: V4GainRequest): V4GainPlan => {
    const track = f.current().project.sequences[0].tracks[0];
    const element = structuredClone(track.elements[0]);
    if (element.content.kind !== "video" && element.content.kind !== "audio")
      throw new Error("Expected fixture audio");
    const audio = element.content.audio,
      old = structuredClone(audio);
    if (request.command.kind === "set_muted")
      audio.muted = request.command.muted;
    else
      audio.gain = {
        unit: "decibels",
        value: request.command.decibels_tenths / 10,
      };
    return {
      original_request_digest: "gain-digest",
      decision: {
        element_id: element.id,
        old_gain: old.gain,
        new_gain: audio.gain,
        old_element_muted: old.muted,
        element_muted: audio.muted,
        track_muted: track.muted,
        animated: audioGainAvailability(
          f.controller.getSnapshot().v4!,
          element.id,
        )!.animated,
      },
      transaction:
        JSON.stringify(old) === JSON.stringify(audio)
          ? null
          : {
              project_id: request.project_id,
              expected_revision: request.expected_revision,
              idempotency_key: request.idempotency_key,
              label: request.label,
              operations: [
                {
                  type: "element_put",
                  sequence_id: request.sequence_id,
                  track_id: request.track_id,
                  element,
                },
              ],
            },
    };
  };
  vi.mocked(f.api.sdk.call).mockImplementation(async (name, input, options) => {
    if (name === "opencut_project_v4_audio_gain_plan") {
      const request = input as V4GainRequest;
      requests.push(structuredClone(request));
      const plan = planFor(request);
      plans.push(plan);
      return plan as never;
    }
    return previous(name, input, options);
  });
  return {
    ...f,
    gainRequests: requests,
    gainPlans: plans,
    gainPlanFor: planFor,
  };
}
