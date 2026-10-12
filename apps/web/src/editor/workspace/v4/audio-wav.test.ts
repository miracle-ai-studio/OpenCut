import { afterEach, expect, it, vi } from "vitest";
import {
  OpenCutApiError,
  OpenCutTransportError,
} from "../../../lib/editor-sdk";
import { setupAudioWav } from "./audio-wav-fixture";
import type {
  V4AudioWavRequest,
  V4AudioRangeWavRequest,
  V4AudioRangeWavCapability,
} from "./types";
import { audioSampleRange } from "./audio-wav";
afterEach(() => vi.restoreAllMocks());

it("sample range submission keeps its exact integers, historical revision and the shared task authority", async () => {
  const { controller, current, rangeRequests, requests } =
    await setupAudioWav();
  try {
    current().revision = "later";
    await controller.v4.refresh();
    await controller.v4.inspectRevision("r0");
    const fixed = structuredClone(current());
    const id = await controller.v4.audioWav.start(48000, {
      start_sample: "24000",
      frame_count: "64000",
    });
    expect(rangeRequests[0]).toMatchObject({
      revision: "r0",
      sequence_id: "scene",
      sample_rate: 48000,
      range: { start_sample: "24000", frame_count: "64000" },
    });
    expect(requests).toHaveLength(0);
    expect(current()).toEqual(fixed);
    expect(controller.getSnapshot().jobs).toHaveLength(1);
    expect(controller.getSnapshot().jobs[0]).toMatchObject({
      job_id: id,
      kind: "render.project_audio_range.v4",
      target: { revision: "r0" },
    });
    expect(audioSampleRange("9007199254740993", "1").start_sample).toBe(
      "9007199254740993",
    );
  } finally {
    controller.dispose();
  }
});
it("range integers reject noncanonical spelling without changing a project or sending a job", async () => {
  const { controller, rangeRequests, current } = await setupAudioWav();
  try {
    const fixed = structuredClone(current());
    for (const range of [
      { start_sample: "01", frame_count: "1" },
      { start_sample: "-1", frame_count: "1" },
      { start_sample: "1.0", frame_count: "1" },
      { start_sample: "0", frame_count: "0" },
      { start_sample: "0", frame_count: "" },
      { start_sample: " 1", frame_count: "1" },
      { start_sample: "18446744073709551616", frame_count: "1" },
    ])
      await expect(controller.v4.audioWav.start(48000, range)).rejects.toThrow(
        "canonical",
      );
    expect(rangeRequests).toHaveLength(0);
    expect(current()).toEqual(fixed);
  } finally {
    controller.dispose();
  }
});
it("missing range capability leaves whole-scene WAV available and range uses only its own sample rates", async () => {
  const { controller, setRangeCapability, requests, rangeRequests } =
    await setupAudioWav();
  try {
    setRangeCapability({
      schema_version: 1,
      available: true,
      submit_tool: "opencut_job_v4_audio_range_wav",
      sample_rates: [44100],
      unavailable_reason: null,
    });
    await controller.v4.audioWav.refreshCapability("range");
    await expect(
      controller.v4.audioWav.start(48000, {
        start_sample: "0",
        frame_count: "1",
      }),
    ).rejects.toThrow("available audio sample");
    setRangeCapability({
      schema_version: 1,
      available: false,
      submit_tool: "opencut_job_v4_audio_range_wav",
      sample_rates: [],
      unavailable_reason: "Range audio is unavailable.",
    });
    await controller.v4.audioWav.refreshCapability("range");
    await expect(
      controller.v4.audioWav.start(44100, {
        start_sample: "0",
        frame_count: "1",
      }),
    ).rejects.toThrow("Range audio is unavailable");
    await controller.v4.audioWav.start(48000);
    expect(requests).toHaveLength(1);
    expect(requests[0]).not.toHaveProperty("range");
    expect(rangeRequests).toHaveLength(0);
  } finally {
    controller.dispose();
  }
});
it("an unknown accepted range task replays the same request after navigation and retains it on revocation", async () => {
  const f = await setupAudioWav();
  try {
    const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
    const submitted: V4AudioRangeWavRequest[] = [];
    let mode = "lost";
    vi.mocked(f.api.sdk.call).mockImplementation(
      async (name, input, options) => {
        if (name === "opencut_job_v4_audio_range_wav") {
          const request = structuredClone(input as V4AudioRangeWavRequest);
          submitted.push(request);
          if (mode === "lost") {
            f.rows.push(f.jobFor(request));
            throw new OpenCutTransportError(new Error("Lost response"));
          }
          if (mode === "denied")
            throw new OpenCutApiError(
              "PERMISSION_DENIED",
              "Revoked",
              null,
              403,
              "request",
              [],
            );
          return structuredClone(f.rows[0]) as never;
        }
        return previous(name, input, options);
      },
    );
    await expect(
      f.controller.v4.audioWav.start(48000, {
        start_sample: "7",
        frame_count: "64000",
      }),
    ).rejects.toThrow("request could not");
    f.current().revision = "changed-head";
    await f.controller.open(f.ref());
    mode = "denied";
    await expect(f.controller.v4.audioWav.retry()).rejects.toThrow("Revoked");
    expect(f.controller.getSnapshot().v4!.audioWav!.phase).toBe("unconfirmed");
    expect(() => f.controller.v4.audioWav.dismiss()).toThrow("Confirm");
    mode = "ok";
    await f.controller.v4.audioWav.retry();
    expect(submitted).toHaveLength(3);
    for (const request of submitted) expect(request).toEqual(submitted[0]);
    expect(f.rows).toHaveLength(1);
    expect(f.controller.getSnapshot().jobs[0].kind).toBe(
      "render.project_audio_range.v4",
    );
  } finally {
    f.controller.dispose();
  }
});
it("an older range availability response cannot replace a newer unavailable observation", async () => {
  const { controller, api } = await setupAudioWav();
  try {
    const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
    let release!: (capability: V4AudioRangeWavCapability) => void,
      first = true;
    vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
      if (name === "opencut_project_v4_audio_range_capability") {
        if (first) {
          first = false;
          return (await new Promise<V4AudioRangeWavCapability>((resolve) => {
            release = resolve;
          })) as never;
        }
        throw new Error("Latest range capability is unavailable.");
      }
      return previous(name, input, options);
    });
    const older = controller.v4.audioWav.refreshCapability("range");
    await controller.v4.audioWav.refreshCapability("range");
    release({
      schema_version: 1,
      available: true,
      submit_tool: "opencut_job_v4_audio_range_wav",
      sample_rates: [48000],
      unavailable_reason: null,
    });
    await older;
    expect(controller.getSnapshot().v4!.audioRangeWavCapability).toBeNull();
    expect(controller.getSnapshot().v4!.audioRangeWavCapabilityError).toBe(
      "Latest range capability is unavailable.",
    );
    expect(controller.getSnapshot().v4!.audioWavCapability!.available).toBe(
      true,
    );
  } finally {
    controller.dispose();
  }
});

it("submits the historical revision and selected scene unchanged, and publishes only the existing workspace job rows", async () => {
  const { controller, requests, current, api } = await setupAudioWav();
  try {
    current().revision = "later-head";
    await controller.v4.refresh();
    await controller.v4.inspectRevision("r0");
    const before = structuredClone(current());
    const jobId = await controller.v4.audioWav.start(44100);
    expect(requests[0]).toMatchObject({
      project_id: "v4-project",
      revision: "r0",
      sequence_id: "scene",
      sample_rate: 44100,
    });
    expect(current()).toEqual(before);
    const state = controller.getSnapshot();
    expect(state.snapshot).toBeNull();
    expect(state.v4!.head.revision).toBe("later-head");
    expect(state.v4!.audioWav).toMatchObject({ phase: "accepted", jobId });
    expect(state.jobs.map((job) => job.job_id)).toEqual([jobId]);
    expect(state.jobs[0].target!.revision).toBe("r0");
    expect(
      vi
        .mocked(api.sdk.call)
        .mock.calls.some(
          ([name]) =>
            name === "opencut_job_v3_render" || name === "opencut_job_submit",
        ),
    ).toBe(false);
  } finally {
    controller.dispose();
  }
});
it("unknown submission survives route reopen and explicitly replays the original key and fixed target after a head change", async () => {
  const { controller, api, ref, current, rows, jobFor } = await setupAudioWav();
  try {
    const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
    let first = true;
    const submissions: V4AudioWavRequest[] = [];
    vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
      if (name === "opencut_job_v4_audio_wav") {
        const request = structuredClone(input as V4AudioWavRequest);
        submissions.push(request);
        if (first) {
          first = false;
          rows.push(jobFor(request));
          throw new OpenCutTransportError(new Error("Lost accepted response"));
        }
        return structuredClone(rows[0]) as never;
      }
      return previous(name, input, options);
    });
    await expect(controller.v4.audioWav.start(48000)).rejects.toThrow(
      "request could not",
    );
    expect(controller.getSnapshot().v4!.audioWav!.phase).toBe("unconfirmed");
    expect(() => controller.v4.audioWav.dismiss()).toThrow("Confirm");
    current().revision = "new-head";
    await controller.open(ref());
    expect(controller.getSnapshot().v4!.audioWav!.request).toEqual(
      submissions[0],
    );
    await expect(controller.v4.audioWav.start(44100)).rejects.toThrow(
      "previous audio",
    );
    await controller.v4.audioWav.retry();
    expect(submissions[1]).toEqual(submissions[0]);
    expect(rows).toHaveLength(1);
    expect(controller.getSnapshot().jobs[0].target!.revision).toBe("r0");
  } finally {
    controller.dispose();
  }
});
it("only actual capability sample rates open export and unavailable services keep the editor usable", async () => {
  const { controller, setCapability, requests } = await setupAudioWav();
  try {
    setCapability({
      schema_version: 1,
      available: true,
      submit_tool: "opencut_job_v4_audio_wav",
      sample_rates: [44100],
      unavailable_reason: null,
    });
    await controller.v4.audioWav.refreshCapability();
    await expect(controller.v4.audioWav.start(48000)).rejects.toThrow(
      "available audio sample",
    );
    setCapability({
      schema_version: 1,
      available: false,
      submit_tool: "opencut_job_v4_audio_wav",
      sample_rates: [],
      unavailable_reason: "The audio worker is unavailable.",
    });
    await controller.v4.audioWav.refreshCapability();
    await expect(controller.v4.audioWav.start(44100)).rejects.toThrow(
      "worker is unavailable",
    );
    expect(requests).toHaveLength(0);
    await controller.v4.bookmark({
      kind: "put",
      bookmark: {
        id: "still-editable",
        sequence_id: "scene",
        time: { ticks: "0", timebase: 1 },
        note: "Still editable",
      },
    });
    expect(
      controller.getSnapshot().v4!.head.project.sequences[0].bookmarks,
    ).toHaveLength(1);
  } finally {
    controller.dispose();
  }
});
it("a denied replay retains the unknown accepted task identity rather than claiming non-acceptance", async () => {
  const { controller, api } = await setupAudioWav();
  try {
    const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
    let first = true;
    vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
      if (name === "opencut_job_v4_audio_wav") {
        if (first) {
          first = false;
          throw new OpenCutTransportError(new Error("Lost response"));
        }
        throw new OpenCutApiError(
          "PERMISSION_DENIED",
          "Job permission revoked.",
          null,
          403,
          "req",
          [],
        );
      }
      return previous(name, input, options);
    });
    await expect(controller.v4.audioWav.start(48000)).rejects.toThrow(
      "request could not",
    );
    const request = controller.getSnapshot().v4!.audioWav!.request;
    await expect(controller.v4.audioWav.retry()).rejects.toThrow("revoked");
    expect(controller.getSnapshot().v4!.audioWav).toMatchObject({
      phase: "unconfirmed",
      request,
      error: "Job permission revoked.",
    });
    expect(() => controller.v4.audioWav.dismiss()).toThrow("Confirm");
  } finally {
    controller.dispose();
  }
});
it("task polling continues during V4 editing and a project switch fences old task observations", async () => {
  const { controller, api, rows, ref, current } = await setupAudioWav();
  try {
    await controller.v4.audioWav.start(48000);
    rows[0].state = "running";
    const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
    let releasePlan!: (value: unknown) => void;
    vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
      if (name === "opencut_edit_v4_plan")
        return (await new Promise<unknown>((resolve) => {
          releasePlan = resolve;
        })) as never;
      return previous(name, input, options);
    });
    const edit = controller.v4.write("Held edit", []);
    await controller.sync();
    expect(controller.getSnapshot().jobs[0].state).toBe("running");
    releasePlan({ project: current().project });
    await edit;
    let releaseTasks!: (value: Awaited<ReturnType<typeof api.tasks>>) => void;
    vi.mocked(api.tasks).mockImplementation(
      () =>
        new Promise((resolve) => {
          releaseTasks = resolve;
        }),
    );
    const oldSync = controller.sync();
    await controller.open(ref());
    releaseTasks({ jobs: structuredClone(rows), next_cursor: null });
    await oldSync;
    expect(controller.getSnapshot().jobs).toEqual([]);
  } finally {
    controller.dispose();
  }
});
