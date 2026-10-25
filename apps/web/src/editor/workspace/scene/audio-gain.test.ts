import { afterEach, expect, it, vi } from "vitest";
import { OpenCutApiError } from "../../../lib/editor-sdk";
import { setupAudioGain } from "./audio-gain-fixture";
import { audioGainAvailability } from "./audio-gain";
import { gainLinePercent, gainLineTenths } from "./audio-gain-display";
import type { GainRequest, EditReceipt } from "./types";
afterEach(() => vi.restoreAllMocks());
it("uses the Classic nonlinear presentation with 0.1 dB commands and distinct quiet endpoint", () => {
  expect(gainLinePercent(-60)).toBe(100);
  expect(gainLinePercent(20)).toBe(0);
  expect(gainLineTenths(100)).toBe(-600);
  expect(gainLineTenths(0)).toBe(200);
  expect(gainLinePercent(0)).toBeCloseTo(68.391);
  expect(gainLineTenths(gainLinePercent(-6))).toBe(-60);
});
it("keeps gesture drafts out of the document; cancellation and unchanged values never plan", async () => {
  const f = await setupAudioGain();
  try {
    const original = structuredClone(f.controller.getSnapshot().scene!.head);
    const g = f.controller.scene.audioGain,
      id = g.begin("video");
    g.update(id, -123);
    expect(f.controller.getSnapshot().scene!.gainDraft!.tenths).toBe(-123);
    expect(f.controller.getSnapshot().scene!.head).toEqual(original);
    g.cancel(id);
    await g.commit(id);
    const same = g.begin("video");
    g.update(same, -60);
    await g.commit(same);
    expect(f.gainRequests).toHaveLength(0);
  } finally {
    f.controller.dispose();
  }
});
it("commits one exact planner transaction and mute preserves gain and all unrelated fields", async () => {
  const f = await setupAudioGain();
  try {
    const g = f.controller.scene.audioGain,
      id = g.begin("video");
    g.update(id, -125);
    await g.commit(id);
    await g.commit(id);
    expect(f.gainRequests).toHaveLength(1);
    expect(f.gainRequests[0].command).toEqual({
      kind: "set_decibels",
      decibels_tenths: -125,
    });
    expect(
      f.call.mock.calls.find(([n]) => n === "opencut_edit_apply")![1],
    ).toEqual(f.gainPlans[0].transaction);
    const before = structuredClone(
      f.current().project.sequences[0].tracks[0].elements[0],
    );
    await g.mute("video", true);
    const next = f.current().project.sequences[0].tracks[0].elements[0];
    expect(next).toEqual({
      ...before,
      content: {
        ...before.content,
        audio: {
          ...(before.content.kind === "video" ? before.content.audio : {}),
          muted: true,
        },
      },
    });
    const drag = g.begin("video");
    g.update(drag, -200);
    await g.commit(drag);
    expect(next.content.kind === "video" && next.content.audio.muted).toBe(
      true,
    );
  } finally {
    f.controller.dispose();
  }
});
it("external heads preserve volume drafts without dispatch; navigation discards pointer drafts", async () => {
  const f = await setupAudioGain();
  try {
    const g = f.controller.scene.audioGain;
    let id = g.begin("video");
    g.update(id, 10);
    f.current().revision = "external";
    await f.controller.scene.refresh();
    await expect(g.commit(id)).rejects.toThrow("another client");
    expect(f.controller.getSnapshot().scene!.gainDraft?.id).toBe(id);
    expect(f.gainRequests).toHaveLength(0);
    g.cancel(id);
    await f.controller.scene.refresh();
    expect(f.controller.getSnapshot().scene!.gainDraft).toBeNull();
    id = g.begin("video");
    g.update(id, 10);
    f.controller.scene.chooseSequence("scene");
    await g.commit(id);
    expect(f.gainRequests).toHaveLength(0);
    f.controller.scene.select("video");
    id = g.begin("video");
    g.update(id, 10);
    await f.controller.scene.inspectRevision("r0");
    await g.commit(id);
    expect(f.gainRequests).toHaveLength(0);
    expect(() => g.begin("video")).toThrow("current revision");
    f.controller.scene.returnToHead();
    f.controller.scene.select("video");
    id = g.begin("video");
    g.update(id, 10);
    await f.controller.open(f.ref());
    await g.commit(id);
    expect(f.gainRequests).toHaveLength(0);
  } finally {
    f.controller.dispose();
  }
});
it("fences stale plans before apply, retaining the captured transaction without replanning", async () => {
  const f = await setupAudioGain();
  try {
    let resolve!: (p: ReturnType<typeof f.gainPlanFor>) => void,
      request!: GainRequest;
    const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
    vi.mocked(f.api.sdk.call).mockImplementation(
      async (name, input, options) => {
        if (name === "opencut_project_audio_gain_plan") {
          request = input as GainRequest;
          return (await new Promise<ReturnType<typeof f.gainPlanFor>>((r) => {
            resolve = r;
          })) as never;
        }
        return previous(name, input, options);
      },
    );
    const g = f.controller.scene.audioGain,
      id = g.begin("video");
    g.update(id, -100);
    const pending = g.commit(id);
    await f.controller.scene.inspectRevision("r0");
    resolve(f.gainPlanFor(request));
    await expect(pending).rejects.toThrow("view changed");
    expect(
      f.call.mock.calls.filter(([n]) => n === "opencut_edit_apply"),
    ).toHaveLength(0);
    f.controller.scene.returnToHead();
    f.controller.scene.select("video");
    await g.retry();
    expect(
      f.call.mock.calls.filter(
        ([n]) => n === "opencut_project_audio_gain_plan",
      ),
    ).toHaveLength(1);
  } finally {
    f.controller.dispose();
  }
});
it("lost commit outcomes replay the unchanged original request after reopen and retain unknown on revocation", async () => {
  const f = await setupAudioGain();
  try {
    const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
    let mode = "lost";
    vi.mocked(f.api.sdk.call).mockImplementation(async (n, input, options) => {
      if (n === "opencut_edit_apply" && mode !== "ok") {
        if (mode === "lost") throw new Error("response lost");
        throw new OpenCutApiError(
          "ACCESS_DENIED",
          "Revoked",
          null,
          403,
          "request",
          [],
        );
      }
      return previous(n, input, options);
    });
    const g = f.controller.scene.audioGain,
      id = g.begin("video");
    g.update(id, -110);
    await expect(g.commit(id)).rejects.toThrow("lost");
    const attempt = f.controller.getSnapshot().scene!.audioGain!;
    await f.controller.open(f.ref());
    f.controller.scene.select("video");
    mode = "denied";
    await expect(g.retry()).rejects.toThrow("Revoked");
    expect(f.controller.getSnapshot().scene!.audioGain!.phase).toBe("unconfirmed");
    mode = "ok";
    await g.retry();
    const applies = f.call.mock.calls.filter(
      ([n]) => n === "opencut_edit_apply",
    );
    expect(applies).toHaveLength(3);
    for (const [, tx] of applies) expect(tx).toEqual(attempt.plan!.transaction);
    expect(f.gainRequests).toHaveLength(1);
  } finally {
    f.controller.dispose();
  }
});
it("only identical unknown replay revision conflict resolves the pending operation without automatic replan", async () => {
  const f = await setupAudioGain();
  try {
    const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
    let conflict = false;
    vi.mocked(f.api.sdk.call).mockImplementation(async (n, input, options) => {
      if (n === "opencut_edit_apply") {
        if (conflict)
          throw new OpenCutApiError(
            "REVISION_CONFLICT",
            "Conflict",
            null,
            409,
            "request",
            [],
          );
        throw new Error("lost");
      }
      return previous(n, input, options);
    });
    const g = f.controller.scene.audioGain,
      id = g.begin("video");
    g.update(id, -110);
    await expect(g.commit(id)).rejects.toThrow("lost");
    conflict = true;
    await expect(g.retry()).rejects.toThrow("Dismiss it");
    expect(f.controller.getSnapshot().scene!.audioGain!.phase).toBe("retryable");
    expect(f.controller.getSnapshot().scene!.audioGain!.plan).toBe(f.gainPlans[0]);
    expect(f.gainRequests).toHaveLength(1);
    g.dismiss();
    expect(f.controller.getSnapshot().scene!.audioGain).toBeNull();
  } finally {
    f.controller.dispose();
  }
});
it("a successful commit with a lost response advances the head but still replays its original receipt", async () => {
  const f = await setupAudioGain();
  try {
    const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
    let receipt: EditReceipt | null = null;
    vi.mocked(f.api.sdk.call).mockImplementation(async (n, input, options) => {
      if (n === "opencut_edit_apply") {
        if (receipt) return receipt as never;
        receipt = (await previous(n, input, options)) as EditReceipt;
        throw new Error("saved response lost");
      }
      return previous(n, input, options);
    });
    const g = f.controller.scene.audioGain,
      id = g.begin("video");
    g.update(id, -115);
    await expect(g.commit(id)).rejects.toThrow("lost");
    const unknown = f.controller.getSnapshot().scene!.audioGain!;
    expect(f.controller.getSnapshot().scene!.head.revision).toBe(
      unknown.request.expected_revision,
    );
    expect(f.controller.getSnapshot().scene!.externalRevision).toBe(f.current().revision);
    await g.retry();
    expect(f.controller.getSnapshot().scene!.audioGain!.receipt).toEqual(receipt);
    expect(f.gainRequests).toHaveLength(1);
    const applies = f.call.mock.calls.filter(
      ([n]) => n === "opencut_edit_apply",
    );
    expect(applies).toHaveLength(2);
    expect(applies[1][1]).toEqual(applies[0][1]);
  } finally {
    f.controller.dispose();
  }
});
it("hides keyed and legacy static lines, while separate mute preserves automation and enforces actual video presence/lock", async () => {
  const f = await setupAudioGain();
  try {
    const e = f.current().project.sequences[0].tracks[0].elements[0];
    e.channels = [
      {
        kind: "scalar",
        id: "vol",
        target: { kind: "element", element_id: "video" },
        binding: { kind: "property", parameter: "gain_decibels" },
        extrapolation: { before: "hold", after: "hold" },
        keys: [
          {
            id: "k",
            time: { ticks: "0", timebase: 1 },
            value: -8,
            left_handle: null,
            right_handle: null,
            segment_to_next: { kind: "linear" },
            tangent_mode: "auto",
          },
        ],
      },
    ];
    await f.controller.scene.refresh();
    const g = f.controller.scene.audioGain;
    expect(
      audioGainAvailability(f.controller.getSnapshot().scene!, "video")!.animated,
    ).toBe(true);
    expect(() => g.begin("video")).toThrow("automation");
    await g.mute("video", true);
    expect(
      f.current().project.sequences[0].tracks[0].elements[0].channels,
    ).toEqual(e.channels);
    const next = f.current().project.sequences[0].tracks[0].elements[0];
    next.channels = [];
    if (next.content.kind === "video")
      next.content.audio.gain = { unit: "linear_legacy", value: 0.5 };
    await f.controller.scene.refresh();
    expect(() => g.begin("video")).toThrow("Legacy");
    await g.mute("video", false);
    f.current().project.assets[0].metadata.audio = { status: "unknown" };
    await f.controller.scene.refresh();
    await expect(g.mute("video", true)).rejects.toThrow("known source audio");
    f.current().project.sequences[0].tracks[0].locked = true;
    await f.controller.scene.refresh();
    expect(() => g.begin("video")).toThrow("locked");
  } finally {
    f.controller.dispose();
  }
});
it("rejects wrong-target and unrelated command proposals before apply", async () => {
  const f = await setupAudioGain();
  try {
    const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
    vi.mocked(f.api.sdk.call).mockImplementation(async (n, input, options) => {
      if (n === "opencut_project_audio_gain_plan") {
        const plan = f.gainPlanFor(input as GainRequest);
        if (plan.transaction?.operations[0].type === "element_put")
          plan.transaction.operations[0].track_id = "wrong-track";
        return plan as never;
      }
      return previous(n, input, options);
    });
    const g = f.controller.scene.audioGain,
      id = g.begin("video");
    g.update(id, -130);
    await expect(g.commit(id)).rejects.toThrow("does not match");
    expect(
      f.call.mock.calls.filter(([n]) => n === "opencut_edit_apply"),
    ).toHaveLength(0);
    g.dismiss();
    vi.mocked(f.api.sdk.call).mockImplementation(async (n, input, options) => {
      if (n === "opencut_project_audio_gain_plan") {
        const plan = f.gainPlanFor(input as GainRequest);
        plan.decision.new_gain.value = 17;
        return plan as never;
      }
      return previous(n, input, options);
    });
    const next = g.begin("video");
    g.update(next, -130);
    await expect(g.commit(next)).rejects.toThrow("does not match");
    expect(
      f.call.mock.calls.filter(([n]) => n === "opencut_edit_apply"),
    ).toHaveLength(0);
    g.dismiss();
    vi.mocked(f.api.sdk.call).mockImplementation(async (n, input, options) => {
      if (n === "opencut_project_audio_gain_plan") {
        const plan = f.gainPlanFor(input as GainRequest);
        const operation = plan.transaction?.operations[0];
        if (operation?.type === "element_put") {
          // Deliberately malformed transport data must not match an empty array.
          operation.element.channels =
            {} as unknown as typeof operation.element.channels;
        }
        return plan as never;
      }
      return previous(n, input, options);
    });
    const malformed = g.begin("video");
    g.update(malformed, -130);
    await expect(g.commit(malformed)).rejects.toThrow("does not match");
    expect(
      f.call.mock.calls.filter(([n]) => n === "opencut_edit_apply"),
    ).toHaveLength(0);
    g.dismiss();
    vi.mocked(f.api.sdk.call).mockImplementation(async (n, input, options) => {
      if (n === "opencut_project_audio_gain_plan") {
        const plan = f.gainPlanFor(input as GainRequest);
        const operation = plan.transaction?.operations[0];
        if (operation?.type === "element_put") {
          const { channels, ...rest } = operation.element;
          operation.element = { channels, ...rest };
        }
        return plan as never;
      }
      return previous(n, input, options);
    });
    const reordered = g.begin("video");
    g.update(reordered, -130);
    await g.commit(reordered);
    expect(
      f.call.mock.calls.filter(([n]) => n === "opencut_edit_apply"),
    ).toHaveLength(1);
  } finally {
    f.controller.dispose();
  }
});
it("independent audio is eligible and empty element keys or track automation do not hide its line", async () => {
  const f = await setupAudioGain();
  try {
    const track = f.current().project.sequences[0].tracks[0],
      e = track.elements[0];
    if (e.content.kind !== "video") throw new Error("fixture video missing");
    e.content = {
      kind: "audio",
      asset_id: e.content.asset_id,
      retime: e.content.retime,
      audio: e.content.audio,
    };
    e.visual = null;
    track.kind = "audio";
    const channel = {
      kind: "scalar" as const,
      id: "empty",
      target: { kind: "element" as const, element_id: e.id },
      binding: {
        kind: "property" as const,
        parameter: "gain_decibels" as const,
      },
      extrapolation: { before: "hold" as const, after: "hold" as const },
      keys: [],
    };
    e.channels = [channel];
    track.channels = [
      {
        ...channel,
        id: "track-gain",
        target: { kind: "track", track_id: track.id },
        keys: [
          {
            id: "tk",
            time: { ticks: "0", timebase: 1 },
            value: 4,
            left_handle: null,
            right_handle: null,
            segment_to_next: { kind: "linear" },
            tangent_mode: "auto",
          },
        ],
      },
    ];
    await f.controller.scene.refresh();
    expect(
      audioGainAvailability(f.controller.getSnapshot().scene!, "video")!
        .lineReason,
    ).toBeNull();
    const g = f.controller.scene.audioGain,
      id = g.begin("video");
    g.update(id, -150);
    await g.commit(id);
    expect(f.gainRequests).toHaveLength(1);
    expect(f.current().project.sequences[0].tracks[0].channels).toEqual(
      track.channels,
    );
  } finally {
    f.controller.dispose();
  }
});
