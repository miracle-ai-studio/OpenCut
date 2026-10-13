import { afterEach, expect, it, vi } from "vitest";
import { OpenCutApiError } from "../../../lib/editor-sdk";
import { setupAudioPreview } from "./audio-preview-fixture";

const gainDraft = (
  f: Awaited<ReturnType<typeof setupAudioPreview>>,
  tenths = -120,
) => ({
  id: "overlap-gesture",
  projectId: f.state().head.project.id,
  revision: f.state().head.revision,
  sequenceId: "scene",
  trackId: "track",
  elementId: "audio",
  originalDb: 0,
  tenths,
  changed: true,
});

it.each(["seek", "set_temporary_gain"] as const)(
  "applies the latest gain after an accepted %s without streaming its superseded overlay",
  async (kind) => {
    const f = await setupAudioPreview();
    try {
      await f.preview.play(48000);
      f.outputs[0].finalFrames = "256";
      const actual = f.control.getMockImplementation()!;
      let release!: () => void, entered!: () => void;
      const began = new Promise<void>((r) => {
        entered = r;
      });
      let held = false;
      f.control.mockImplementation(async (...args) => {
        const result = await actual(...args);
        if (!held && args[0].command.kind === kind) {
          held = true;
          entered();
          await new Promise<void>((r) => {
            release = r;
          });
        }
        return result;
      });
      if (kind === "seek") f.preview.seek({ ticks: "1", timebase: 1 });
      else f.preview.draft(gainDraft(f, -60));
      await began;
      f.preview.draft(gainDraft(f, -180));
      release();
      await vi.waitFor(() =>
        expect(
          f.state().audioPreview.session?.temporary_gain?.decibels_tenths,
        ).toBe(-180),
      );
      await flush();
      expect(f.stream.mock.calls.map(([s]) => s.generation)).toEqual([
        "1",
        "3",
      ]);
      expect(
        f.controls.filter((r) => r.command.kind === "report_consumed"),
      ).toHaveLength(1);
      expect(f.controls.some((r) => r.command.kind === "stop")).toBe(false);
    } finally {
      f.cleanup();
    }
  },
);

it.each([false, true])(
  "serializes Escape, seek and pause=%s behind the accepted gain",
  async (pause) => {
    const f = await setupAudioPreview();
    try {
      const before = structuredClone(f.state().head);
      await f.preview.play(48000);
      f.outputs[0].finalFrames = "256";
      const actual = f.control.getMockImplementation()!;
      let release!: () => void, entered!: () => void;
      const began = new Promise<void>((r) => {
        entered = r;
      });
      let active = 0,
        maximum = 0;
      f.control.mockImplementation(async (...args) => {
        maximum = Math.max(maximum, ++active);
        try {
          const result = await actual(...args);
          if (args[0].command.kind === "set_temporary_gain") {
            entered();
            await new Promise<void>((r) => {
              release = r;
            });
          }
          return result;
        } finally {
          active--;
        }
      });
      f.preview.draft(gainDraft(f));
      await began;
      expect(f.preview.seek({ ticks: "3", timebase: 2 })).toBe(true);
      f.preview.draft(null);
      const paused = pause ? f.preview.pause() : null;
      release();
      if (paused) await paused;
      else
        await vi.waitFor(() =>
          expect(f.state().audioPreview.session?.generation).toBe("4"),
        );
      await flush();
      expect(maximum).toBe(1);
      expect(
        f.controls
          .filter((r) => r.command.kind !== "report_consumed")
          .map((r) => [r.generation, r.command.kind]),
      ).toEqual([
        ["1", "play"],
        ["1", "set_temporary_gain"],
        ["2", "seek"],
        ["3", "clear_temporary_gain"],
        ...(pause ? [["4", "pause"]] : []),
      ]);
      expect(f.stream.mock.calls.map(([s]) => s.generation)).toEqual(
        pause ? ["1"] : ["1", "4"],
      );
      expect(f.state().audioPreview.phase).toBe(pause ? "paused" : "loading");
      expect(f.state().audioPreview.session?.temporary_gain).toBeNull();
      expect(f.state().head).toEqual(before);
    } finally {
      f.cleanup();
    }
  },
);

it("changes and cancels paused gain without reporting the retired device or restarting sound", async () => {
  const f = await setupAudioPreview();
  try {
    await f.preview.play(48000);
    f.outputs[0].finalFrames = "256";
    await f.preview.pause();
    f.preview.draft(gainDraft(f));
    await vi.waitFor(() =>
      expect(
        f.state().audioPreview.session?.temporary_gain?.decibels_tenths,
      ).toBe(-120),
    );
    f.preview.draft(null);
    await vi.waitFor(() =>
      expect(f.state().audioPreview.session?.temporary_gain).toBeNull(),
    );
    expect(f.state().audioPreview.phase).toBe("paused");
    expect(f.state().playhead).toEqual({ ticks: "256", timebase: 48000 });
    expect(
      f.controls.filter((r) => r.command.kind === "report_consumed"),
    ).toHaveLength(1);
    expect(f.outputs).toHaveLength(1);
    expect(f.stream).toHaveBeenCalledTimes(1);
  } finally {
    f.cleanup();
  }
});

it("retires a late accepted gain generation after departure using original authority", async () => {
  const f = await setupAudioPreview();
  try {
    await f.preview.play(48000);
    const actual = f.control.getMockImplementation()!;
    let release!: () => void, entered!: () => void;
    const began = new Promise<void>((r) => {
      entered = r;
    });
    f.control.mockImplementation(async (...args) => {
      const result = await actual(...args);
      if (args[0].command.kind === "set_temporary_gain") {
        entered();
        await new Promise<void>((r) => {
          release = r;
        });
      }
      return result;
    });
    f.preview.draft(gainDraft(f));
    await began;
    f.preview.retireDocument();
    release();
    await flush();
    const stops = f.control.mock.calls.filter(
      ([r]) => r.command.kind === "stop",
    );
    expect(stops.map(([r]) => r.generation)).toEqual(["1", "2"]);
    expect(
      stops.every(
        ([r, , options]) =>
          r.owner_id === "owner" &&
          r.lease_id === "lease" &&
          options?.keepalive,
      ),
    ).toBe(true);
    expect(f.stream).toHaveBeenCalledTimes(1);
    expect(f.state().audioPreview.session).toBeNull();
  } finally {
    f.cleanup();
  }
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});
const flush = async () => {
  for (let i = 0; i < 20; i++) await Promise.resolve();
};

it.each(["playing", "paused"])(
  "retires a known %s document with original authority and keepalive, without inventing a stopped cursor",
  async (phase) => {
    const f = await setupAudioPreview();
    try {
      await f.preview.play();
      if (phase === "paused") {
        f.outputs[0].finalFrames = "128";
        await f.preview.pause();
      }
      const session = f.state().audioPreview.session!;
      const reports = f.controls.filter(
        (r) => r.command.kind === "report_consumed",
      ).length;
      f.outputs[0].finalFrames = "256";
      f.preview.retireDocument();
      f.preview.retireDocument();
      await flush();
      const stops = f.control.mock.calls.filter(
        ([r]) => r.command.kind === "stop",
      );
      expect(stops).toHaveLength(1);
      expect(stops[0][0]).toMatchObject({
        session_id: session.session_id,
        generation: session.generation,
        owner_id: session.owner_id,
        lease_id: session.lease_id,
        command: { kind: "stop" },
      });
      expect(stops[0][1]).toBeUndefined();
      expect(stops[0][2]).toEqual({ keepalive: true });
      expect(
        f.controls.filter((r) => r.command.kind === "report_consumed"),
      ).toHaveLength(reports);
      expect(f.outputs[0].silenced).toBe(true);
      expect(f.state().audioPreview).toMatchObject({
        phase: "paused",
        session: null,
        request: null,
      });
    } finally {
      f.cleanup();
    }
  },
);
it("retires a late accepted create using its validated identity and keeps the departed document silent", async () => {
  const f = await setupAudioPreview();
  let release!: () => void;
  const original = f.create.getMockImplementation()!;
  f.create.mockImplementationOnce(async (...args) => {
    const session = await original(...args);
    await new Promise<void>((r) => {
      release = r;
    });
    return session;
  });
  try {
    const play = f.preview.play();
    await flush();
    f.preview.retireDocument();
    release();
    await play;
    await flush();
    expect(f.stream).not.toHaveBeenCalled();
    const stops = f.control.mock.calls.filter(
      ([r]) => r.command.kind === "stop",
    );
    expect(stops).toHaveLength(1);
    expect(stops[0][2]).toEqual({ keepalive: true });
    expect(f.state().audioPreview.session).toBeNull();
  } finally {
    f.cleanup();
  }
});
it("stops a known device failure once and ignores the retired output after a replacement starts", async () => {
  const f = await setupAudioPreview();
  try {
    await f.preview.play();
    const first = f.state().audioPreview.session!;
    f.outputs[0].failure(new Error("device failed"));
    await flush();
    expect(
      f.controls
        .filter((r) => r.command.kind === "stop")
        .map((r) => r.session_id),
    ).toEqual([first.session_id]);
    expect(f.state().audioPreview.phase).toBe("failed");
    await f.preview.play();
    const next = f.state().audioPreview.session!;
    expect(next.session_id).not.toBe(first.session_id);
    f.outputs[0].failure(new Error("late device failure"));
    await flush();
    expect(f.controls.filter((r) => r.command.kind === "stop")).toHaveLength(1);
    expect(f.state().audioPreview.session!.session_id).toBe(next.session_id);
  } finally {
    f.cleanup();
  }
});
it("a late resumed Play receipt retires its new generation even after departure sent an old-generation Stop", async () => {
  const f = await setupAudioPreview();
  try {
    await f.preview.play();
    await f.preview.pause();
    const original = f.control.getMockImplementation()!;
    let release!: () => void;
    f.control.mockImplementationOnce(async (...args) => {
      const result = await original(...args);
      await new Promise<void>((r) => {
        release = r;
      });
      return result;
    });
    const resumed = f.preview.play();
    await flush();
    f.preview.retireDocument();
    await flush();
    release();
    await resumed;
    await flush();
    const stops = f.control.mock.calls.filter(
      ([r]) => r.command.kind === "stop",
    );
    expect(stops).toHaveLength(2);
    expect(stops.map(([r]) => r.generation)).toEqual(["1", "2"]);
    expect(stops.map(([, , options]) => options)).toEqual([
      { keepalive: true },
      { keepalive: true },
    ]);
    expect(f.state().audioPreview.session).toBeNull();
  } finally {
    f.cleanup();
  }
});
it.each(["create", "play"])(
  "BFCache retirement preserves an unknown %s request for original-key recovery",
  async (kind) => {
    const f = await setupAudioPreview();
    try {
      if (kind === "create") {
        const original = f.create.getMockImplementation()!;
        f.create.mockImplementationOnce(async (...args) => {
          await original(...args);
          throw new Error("lost original create");
        });
      } else {
        await f.preview.play();
        await f.preview.pause();
        const original = f.control.getMockImplementation()!;
        f.control.mockImplementationOnce(async (...args) => {
          await original(...args);
          throw new Error("lost original play");
        });
      }
      await expect(f.preview.play()).rejects.toThrow("lost original");
      const request = kind === "create" ? f.requests.at(-1) : f.controls.at(-1);
      f.preview.retireDocument();
      await flush();
      expect(f.state().audioPreview.phase).toBe("unconfirmed");
      expect(f.controls.filter((r) => r.command.kind === "stop")).toHaveLength(
        0,
      );
      await f.preview.retry();
      expect(kind === "create" ? f.requests.at(-1) : f.controls.at(-1)).toBe(
        request,
      );
      expect(f.state().audioPreview.session).not.toBeNull();
    } finally {
      f.cleanup();
    }
  },
);

it("reports completed endpoints immediately, coalesces in-flight endpoints and ignores ordinary quantum progress", async () => {
  const f = await setupAudioPreview();
  vi.useFakeTimers();
  try {
    await f.preview.play();
    const actual = f.control.getMockImplementation()!;
    let release!: () => void;
    f.control.mockImplementationOnce(async (r, signal) => {
      const result = await actual(r, signal);
      await new Promise<void>((resolve) => {
        release = resolve;
      });
      return result;
    });
    const output = f.outputs[0];
    output.progress({ consumedFrames: "128", stalled: false, contextTime: 1 });
    expect(
      f.controls.filter((r) => r.command.kind === "report_consumed"),
    ).toHaveLength(0);
    output.progress({ consumedFrames: "4096", stalled: false, contextTime: 1 });
    output.completedBlock();
    expect(f.controls.at(-1)?.command).toMatchObject({
      kind: "report_consumed",
      consumed_frames: "4096",
      sequence: "1",
    });
    await flush();
    for (const consumedFrames of ["8192", "12288"]) {
      output.progress({ consumedFrames, stalled: false, contextTime: 2 });
      output.completedBlock();
    }
    expect(
      f.controls.filter((r) => r.command.kind === "report_consumed"),
    ).toHaveLength(1);
    release();
    await flush();
    expect(
      f.controls
        .filter((r) => r.command.kind === "report_consumed")
        .map((r) => r.command),
    ).toEqual([
      {
        kind: "report_consumed",
        consumed_frames: "4096",
        sequence: "1",
        playing: true,
        stalled: false,
      },
      {
        kind: "report_consumed",
        consumed_frames: "12288",
        sequence: "2",
        playing: true,
        stalled: false,
      },
    ]);
    f.control.mockImplementationOnce(async (r, signal) => {
      const result = await actual(r, signal);
      await new Promise<void>((resolve) => {
        release = resolve;
      });
      return result;
    });
    output.progress({
      consumedFrames: "16384",
      stalled: false,
      contextTime: 3,
    });
    output.completedBlock();
    await flush();
    output.progress({
      consumedFrames: "20480",
      stalled: false,
      contextTime: 4,
    });
    output.completedBlock();
    f.invalidateGeneration();
    release();
    await flush();
    output.completedBlock();
    expect(
      f.controls.filter((r) => r.command.kind === "report_consumed"),
    ).toHaveLength(3);
  } finally {
    f.cleanup();
  }
});

it("releases consumption credit before heartbeat and flushes completions queued during renewal", async () => {
  const f = await setupAudioPreview();
  vi.useFakeTimers();
  let monotonic = 0;
  vi.spyOn(performance, "now").mockImplementation(() => monotonic);
  try {
    await f.preview.play();
    const actual = f.control.getMockImplementation()!;
    let release!: () => void;
    f.control.mockImplementation(async (r, signal) => {
      const result = await actual(r, signal);
      if (r.command.kind === "heartbeat")
        await new Promise<void>((resolve) => {
          release = resolve;
        });
      return result;
    });
    monotonic = 501;
    const output = f.outputs[0];
    output.progress({ consumedFrames: "4096", stalled: false, contextTime: 1 });
    output.completedBlock();
    await flush();
    expect(f.controls.slice(1).map((r) => r.command.kind)).toEqual([
      "report_consumed",
      "heartbeat",
    ]);
    output.progress({ consumedFrames: "8192", stalled: false, contextTime: 2 });
    output.completedBlock();
    release();
    await flush();
    expect(f.controls.slice(1).map((r) => r.command.kind)).toEqual([
      "report_consumed",
      "heartbeat",
      "report_consumed",
    ]);
    expect(f.controls.at(-1)?.command).toMatchObject({
      consumed_frames: "8192",
      sequence: "2",
    });
  } finally {
    f.cleanup();
  }
});

it("discards a queued endpoint when draining and reports the final stopped cursor before pause", async () => {
  const f = await setupAudioPreview();
  vi.useFakeTimers();
  try {
    await f.preview.play();
    const actual = f.control.getMockImplementation()!;
    let release!: () => void;
    f.control.mockImplementationOnce(async (r, signal) => {
      const result = await actual(r, signal);
      await new Promise<void>((resolve) => {
        release = resolve;
      });
      return result;
    });
    const output = f.outputs[0];
    output.progress({ consumedFrames: "4096", stalled: false, contextTime: 1 });
    output.completedBlock();
    await flush();
    output.progress({ consumedFrames: "8192", stalled: false, contextTime: 2 });
    output.completedBlock();
    output.finalFrames = "9000";
    const pause = f.preview.pause();
    output.completedBlock();
    release();
    await flush();
    await vi.advanceTimersByTimeAsync(5);
    await pause;
    expect(
      f.controls
        .filter((r) => r.command.kind === "report_consumed")
        .map((r) => r.command),
    ).toEqual([
      {
        kind: "report_consumed",
        consumed_frames: "4096",
        sequence: "1",
        playing: true,
        stalled: false,
      },
      {
        kind: "report_consumed",
        consumed_frames: "9000",
        sequence: "2",
        playing: false,
        stalled: false,
      },
    ]);
    expect(f.controls.at(-1)?.command.kind).toBe("pause");
    output.completedBlock();
    await flush();
    expect(f.controls.at(-1)?.command.kind).toBe("pause");
  } finally {
    f.cleanup();
  }
});

it("ignores the replaced output's endpoint after a playing seek", async () => {
  const f = await setupAudioPreview();
  vi.useFakeTimers();
  try {
    await f.preview.play();
    const old = f.outputs[0];
    old.finalFrames = "128";
    f.preview.seek({ ticks: "1", timebase: 1 });
    await flush();
    expect(f.outputs).toHaveLength(2);
    const reports = f.controls.filter(
      (r) => r.command.kind === "report_consumed",
    ).length;
    old.progress({ consumedFrames: "4096", stalled: false, contextTime: 3 });
    old.completedBlock();
    await flush();
    expect(
      f.controls.filter((r) => r.command.kind === "report_consumed"),
    ).toHaveLength(reports);
    await vi.advanceTimersByTimeAsync(50);
    expect(f.controls.at(-1)).toMatchObject({
      generation: "2",
      command: { consumed_frames: "0", sequence: "1" },
    });
    f.outputs[1].progress({
      consumedFrames: "4096",
      stalled: false,
      contextTime: 4,
    });
    f.outputs[1].completedBlock();
    await flush();
    expect(f.controls.at(-1)).toMatchObject({
      generation: "2",
      command: { consumed_frames: "4096", sequence: "2" },
    });
  } finally {
    f.cleanup();
  }
});
it("binds historical revision and publishes only service consumption Time, with no clock movement while stalled", async () => {
  const f = await setupAudioPreview();
  try {
    f.update({
      historical: {
        ...structuredClone(f.state().head),
        revision: "fixed-history",
      },
      playhead: { ticks: "1", timebase: 3 },
    });
    await f.preview.play(48000);
    expect(f.requests[0]).toMatchObject({
      revision: "fixed-history",
      start_time: { ticks: "1", timebase: 3 },
    });
    f.outputs[0].progress({
      consumedFrames: "128",
      stalled: false,
      contextTime: 98.7,
    });
    await vi.waitFor(() =>
      expect(f.state().playhead).toEqual({ ticks: "128", timebase: 48000 }),
    );
    f.outputs[0].progress({
      consumedFrames: "128",
      stalled: true,
      contextTime: 200,
    });
    await vi.waitFor(() =>
      expect(f.state().audioPreview.phase).toBe("stalled"),
    );
    expect(f.state().playhead.ticks).toBe("128");
  } finally {
    f.cleanup();
  }
});
it("silences immediately, confirms the final device cursor, then pauses; seek uses the returned authoritative grid", async () => {
  const f = await setupAudioPreview();
  try {
    await f.preview.play(48000);
    f.outputs[0].finalFrames = "256";
    const pause = f.preview.pause();
    expect(f.outputs[0].silenced).toBe(true);
    await pause;
    const final = f.controls.findIndex(
      (r) =>
        r.command.kind === "report_consumed" &&
        r.command.consumed_frames === "256",
    );
    expect(final).toBeGreaterThan(-1);
    expect(f.controls[final + 1].command.kind).toBe("pause");
    expect(f.state().playhead.ticks).toBe("256");
    await f.preview.play(48000);
    expect(f.preview.seek({ ticks: "257", timebase: 1000 })).toBe(true);
    expect(f.outputs[1].silenced).toBe(true);
    await vi.waitFor(() =>
      expect(f.state().playhead).toEqual({ ticks: "12345", timebase: 48000 }),
    );
    expect(f.controls.find((r) => r.command.kind === "seek")?.command).toEqual({
      kind: "seek",
      sequence_time: { ticks: "257", timebase: 1000 },
    });
  } finally {
    f.cleanup();
  }
});
it("sends only an element override and clears it on cancellation without an edit or project mirror", async () => {
  const f = await setupAudioPreview();
  try {
    const before = structuredClone(f.state().head);
    await f.preview.play();
    f.outputs[0].finalFrames = "128";
    f.preview.draft({
      id: "gesture",
      projectId: before.project.id,
      revision: before.revision,
      sequenceId: "scene",
      trackId: "track",
      elementId: "audio",
      originalDb: 0,
      tenths: -60,
      changed: true,
    });
    expect(f.outputs[0].silenced).toBe(true);
    await vi.waitFor(() =>
      expect(
        f.controls.some((r) => r.command.kind === "set_temporary_gain"),
      ).toBe(true),
    );
    await vi.waitFor(() => expect(f.outputs).toHaveLength(2));
    f.preview.draft(null);
    await vi.waitFor(() =>
      expect(
        f.controls.some((r) => r.command.kind === "clear_temporary_gain"),
      ).toBe(true),
    );
    expect(f.state().head).toEqual(before);
    const set = f.controls.find(
      (r) => r.command.kind === "set_temporary_gain",
    )!;
    expect(set.command).toMatchObject({
      gain: {
        track_id: "track",
        element_id: "audio",
        decibels_tenths: -60,
        gesture_id: "gesture",
      },
    });
  } finally {
    f.cleanup();
  }
});
it("replays the original unknown create key and stops immediately on revoked controls", async () => {
  const f = await setupAudioPreview();
  try {
    const create = f.create.getMockImplementation()!;
    f.create.mockImplementationOnce(async (r, signal) => {
      await create(r, signal);
      throw new Error("lost response");
    });
    await expect(f.preview.play()).rejects.toThrow("lost response");
    expect(f.state().audioPreview.phase).toBe("unconfirmed");
    expect(f.outputs[0].silenced).toBe(true);
    await f.preview.retry();
    expect(f.requests[1]).toBe(f.requests[0]);
    f.control.mockRejectedValueOnce(
      new OpenCutApiError("NOT_AUTHORIZED", "revoked", null, 403, "req", []),
    );
    await vi.waitFor(() => expect(f.state().audioPreview.phase).toBe("failed"));
    expect(f.outputs[1].silenced).toBe(true);
  } finally {
    f.cleanup();
  }
});
it("rejects a delayed session after project/generation change and never starts its stream", async () => {
  const f = await setupAudioPreview();
  try {
    let resolve!: (s: ReturnType<typeof f.sessionFor>) => void;
    f.create.mockImplementationOnce(
      () =>
        new Promise((r) => {
          resolve = r;
        }),
    );
    const play = f.preview.play();
    await vi.waitFor(() => expect(resolve).toBeTypeOf("function"));
    f.invalidateGeneration();
    expect(f.outputs[0].silenced).toBe(true);
    resolve(f.sessionFor(f.state().audioPreview.request!));
    await play;
    expect(f.stream).not.toHaveBeenCalled();
  } finally {
    f.cleanup();
  }
});
it("cancels a late old-generation body rather than pushing it into a restarted output", async () => {
  const f = await setupAudioPreview();
  try {
    let resolve!: (r: Response) => void;
    const cancelled = vi.fn();
    f.stream.mockImplementationOnce(
      () =>
        new Promise((r) => {
          resolve = r;
        }),
    );
    await f.preview.play();
    f.preview.seek();
    await f.preview.play();
    resolve(
      new Response(new ReadableStream({ cancel: cancelled }), {
        headers: { "content-type": "application/vnd.opencut.audio-preview-v4" },
      }),
    );
    await vi.waitFor(() => expect(cancelled).toHaveBeenCalledOnce());
    expect(f.outputs.every((output) => output.pushed.length === 0)).toBe(true);
  } finally {
    f.cleanup();
  }
});
it("resumes a paused fixed session without recreating native preparation and replays a lost Play control unchanged", async () => {
  const f = await setupAudioPreview();
  try {
    await f.preview.play(48000);
    const first = f.state().audioPreview.session!;
    f.outputs[0].finalFrames = "256";
    await f.preview.pause();
    await f.preview.play(48000);
    expect(f.create).toHaveBeenCalledOnce();
    expect(f.state().audioPreview.session!.session_id).toBe(first.session_id);
    expect(f.state().audioPreview.session!.generation).not.toBe(
      first.generation,
    );
    expect(f.controls.some((r) => r.command.kind === "stop")).toBe(false);
    expect(f.stream.mock.calls.map(([r]) => r.session_id)).toEqual([
      first.session_id,
      first.session_id,
    ]);
    f.outputs[1].finalFrames = "128";
    await f.preview.pause();
    const actual = f.control.getMockImplementation()!;
    f.control.mockImplementationOnce(async (r, signal) => {
      await actual(r, signal);
      throw new Error("lost resumed Play");
    });
    await expect(f.preview.play(48000)).rejects.toThrow("lost resumed Play");
    const original = f.controls.at(-1)!;
    expect(original.command.kind).toBe("play");
    expect(f.state().audioPreview.phase).toBe("unconfirmed");
    await f.preview.retry();
    expect(f.controls.at(-1)).toBe(original);
    expect(f.create).toHaveBeenCalledOnce();
    expect(f.state().audioPreview.session!.session_id).toBe(first.session_id);
  } finally {
    f.cleanup();
  }
});

it("gates PCM behind first picture and reuses the same picture port after seek", async () => {
  const f = await setupAudioPreview();
  const prepare = vi.fn(async () => {}),
    observe = vi.fn(),
    suspend = vi.fn(),
    close = vi.fn();
  try {
    let release!: () => void;
    prepare.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
    );
    const playing = f.preview.play(undefined, {
      prepare,
      observe,
      suspend,
      close,
    });
    await flush();
    expect(prepare).toHaveBeenCalledOnce();
    expect(f.stream).not.toHaveBeenCalled();
    release();
    await playing;
    expect(f.controls.some((r) => r.command.kind === "play")).toBe(true);
    f.preview.seek({ ticks: "1", timebase: 2 });
    await flush();
    expect(suspend).toHaveBeenCalled();
    expect(prepare).toHaveBeenCalledTimes(2);
  } finally {
    f.preview.invalidate();
  }
});

it("known first-picture failure stops its created session and never opens PCM", async () => {
  const f = await setupAudioPreview();
  try {
    await expect(
      f.preview.play(undefined, {
        prepare: async () => {
          throw new Error("Unsupported picture");
        },
        observe: () => {},
        suspend: () => {},
        close: () => {},
      }),
    ).rejects.toThrow("Unsupported picture");
    expect(f.stream).not.toHaveBeenCalled();
    expect(f.controls.some((r) => r.command.kind === "stop")).toBe(true);
    expect(f.state().audioPreview.phase).toBe("failed");
  } finally {
    f.preview.invalidate();
  }
});

it("producer EOF during pause cannot replace its final confirmed pause receipt with failure", async () => {
  const f = await setupAudioPreview();
  let producer!: ReadableStreamDefaultController<Uint8Array>;
  f.stream.mockResolvedValue(
    new Response(
      new ReadableStream<Uint8Array>({
        start(controller) {
          producer = controller;
        },
      }),
      {
        headers: { "content-type": "application/vnd.opencut.audio-preview-v4" },
      },
    ),
  );
  try {
    await f.preview.play();
    await flush();
    const actual = f.control.getMockImplementation()!;
    f.control.mockImplementation(async (request, signal) => {
      if (request.command.kind === "pause") {
        producer.close();
        await flush();
      }
      return actual(request, signal);
    });
    await f.preview.pause();
    expect(f.state().audioPreview).toMatchObject({
      phase: "paused",
      error: null,
    });
    expect(f.controls.filter((r) => r.command.kind === "stop")).toHaveLength(0);
  } finally {
    f.cleanup();
  }
});

it("coalesces rapid seeks during final consumption without stopping or inventing a local clock", async () => {
  const f = await setupAudioPreview();
  try {
    await f.preview.play(48000);
    f.outputs[0].finalFrames = "256";
    const actual = f.control.getMockImplementation()!;
    let release!: () => void;
    f.control.mockImplementationOnce(async (r, signal) => {
      const result = await actual(r, signal);
      await new Promise<void>((resolve) => {
        release = resolve;
      });
      return result;
    });
    expect(f.preview.seek({ ticks: "1", timebase: 1 })).toBe(true);
    await flush();
    expect(f.preview.seek({ ticks: "2", timebase: 1 })).toBe(true);
    expect(f.preview.seek({ ticks: "5", timebase: 2 })).toBe(true);
    release();
    await vi.waitFor(() => expect(f.outputs).toHaveLength(2));
    await flush();
    const seeks = f.controls.filter((r) => r.command.kind === "seek");
    expect(seeks).toHaveLength(1);
    expect(seeks[0].command).toEqual({
      kind: "seek",
      sequence_time: { ticks: "5", timebase: 2 },
    });
    expect(
      f.controls.filter((r) => r.command.kind === "report_consumed"),
    ).toHaveLength(1);
    expect(f.controls.some((r) => r.command.kind === "stop")).toBe(false);
    expect(f.state().playhead).toEqual({ ticks: "12345", timebase: 48000 });
    expect(f.requests).toHaveLength(1);
  } finally {
    f.cleanup();
  }
});

it("serializes rapid seeks while an accepted generation is in flight and skips its superseded output", async () => {
  const f = await setupAudioPreview();
  try {
    await f.preview.play();
    f.outputs[0].finalFrames = "128";
    const actual = f.control.getMockImplementation()!;
    let release!: () => void, entered!: () => void;
    const began = new Promise<void>((resolve) => {
      entered = resolve;
    });
    let active = 0,
      maximum = 0;
    f.control.mockImplementation(async (r, signal) => {
      active++;
      maximum = Math.max(maximum, active);
      try {
        const result = await actual(r, signal);
        if (r.command.kind === "seek" && r.generation === "1") {
          entered();
          await new Promise<void>((resolve) => {
            release = resolve;
          });
        }
        return result;
      } finally {
        active--;
      }
    });
    f.preview.seek({ ticks: "1", timebase: 1 });
    await began;
    expect(f.preview.seek({ ticks: "2", timebase: 1 })).toBe(true);
    expect(f.preview.seek({ ticks: "3", timebase: 1 })).toBe(true);
    release();
    await vi.waitFor(() =>
      expect(f.state().audioPreview.session?.generation).toBe("3"),
    );
    expect(f.outputs).toHaveLength(2);
    expect(maximum).toBe(1);
    expect(
      f.controls
        .filter((r) => r.command.kind === "seek")
        .map((r) => [r.generation, r.command]),
    ).toEqual([
      ["1", { kind: "seek", sequence_time: { ticks: "1", timebase: 1 } }],
      ["2", { kind: "seek", sequence_time: { ticks: "3", timebase: 1 } }],
    ]);
    expect(f.controls.some((r) => r.command.kind === "stop")).toBe(false);
    expect(f.stream.mock.calls.map(([s]) => s.generation)).toEqual(["1", "3"]);
  } finally {
    f.cleanup();
  }
});

it("pause during a rapid seek waits for its accepted generation and never restarts sound", async () => {
  const f = await setupAudioPreview();
  try {
    await f.preview.play();
    f.outputs[0].finalFrames = "128";
    const actual = f.control.getMockImplementation()!;
    let release!: () => void, entered!: () => void;
    const began = new Promise<void>((resolve) => {
      entered = resolve;
    });
    f.control.mockImplementation(async (r, signal) => {
      const result = await actual(r, signal);
      if (r.command.kind === "seek") {
        entered();
        await new Promise<void>((resolve) => {
          release = resolve;
        });
      }
      return result;
    });
    f.preview.seek({ ticks: "1", timebase: 1 });
    await began;
    const paused = f.preview.pause();
    release();
    await paused;
    expect(f.state().audioPreview.phase).toBe("paused");
    expect(f.controls.at(-1)).toMatchObject({
      generation: "2",
      command: { kind: "pause" },
    });
    expect(f.outputs).toHaveLength(1);
    expect(f.stream).toHaveBeenCalledTimes(1);
    expect(
      f.controls.filter((r) => r.command.kind === "report_consumed"),
    ).toHaveLength(1);
  } finally {
    f.cleanup();
  }
});

it("retires a late accepted seek generation after document departure without reviving output", async () => {
  const f = await setupAudioPreview();
  try {
    await f.preview.play();
    const actual = f.control.getMockImplementation()!;
    let release!: () => void, entered!: () => void;
    const began = new Promise<void>((resolve) => {
      entered = resolve;
    });
    f.control.mockImplementation(async (r, signal) => {
      const result = await actual(r, signal);
      if (r.command.kind === "seek") {
        entered();
        await new Promise<void>((resolve) => {
          release = resolve;
        });
      }
      return result;
    });
    f.preview.seek({ ticks: "1", timebase: 1 });
    await began;
    f.preview.retireDocument();
    release();
    await flush();
    expect(
      f.controls
        .filter((r) => r.command.kind === "stop")
        .map((r) => r.generation),
    ).toEqual(["1", "2"]);
    expect(
      f.control.mock.calls
        .filter(([r]) => r.command.kind === "stop")
        .at(-1)?.[2],
    ).toEqual({ keepalive: true });
    expect(f.outputs).toHaveLength(1);
    expect(f.state().audioPreview.session).toBeNull();
  } finally {
    f.cleanup();
  }
});

it("cancels a superseded seek picture preparation before starting its PCM and preserves only the last target", async () => {
  const f = await setupAudioPreview();
  let entered!: () => void;
  const began = new Promise<void>((resolve) => {
    entered = resolve;
  });
  const generations: string[] = [];
  const picture = {
    suspend: vi.fn(),
    close: vi.fn(),
    observe: vi.fn(),
    prepare: vi.fn(async (s: { generation: string }, signal: AbortSignal) => {
      generations.push(s.generation);
      if (s.generation === "2") {
        entered();
        await new Promise<void>((_, reject) =>
          signal.addEventListener(
            "abort",
            () => reject(new DOMException("Replaced", "AbortError")),
            { once: true },
          ),
        );
      }
    }),
  };
  try {
    await f.preview.play(48000, picture);
    f.outputs[0].finalFrames = "256";
    f.preview.seek({ ticks: "1", timebase: 1 });
    await began;
    expect(f.preview.seek({ ticks: "7", timebase: 2 })).toBe(true);
    await vi.waitFor(() =>
      expect(f.state().audioPreview.session?.generation).toBe("3"),
    );
    expect(generations).toEqual(["1", "2", "3"]);
    expect(f.outputs).toHaveLength(3);
    expect(f.outputs[1].silenced).toBe(true);
    expect(f.stream.mock.calls.map(([s]) => s.generation)).toEqual(["1", "3"]);
    expect(
      f.controls
        .filter((r) => r.command.kind === "report_consumed")
        .map((r) => [r.generation, r.command]),
    ).toEqual([
      [
        "1",
        {
          kind: "report_consumed",
          consumed_frames: "256",
          sequence: "1",
          playing: false,
          stalled: false,
        },
      ],
    ]);
    expect(
      f.controls.filter((r) => r.command.kind === "seek").at(-1)?.command,
    ).toEqual({ kind: "seek", sequence_time: { ticks: "7", timebase: 2 } });
    expect(f.state().audioPreview.phase).toBe("loading");
    expect(picture.close).not.toHaveBeenCalled();
  } finally {
    f.cleanup();
  }
});

it("Escape cancels obsolete gain picture preparation before any intermediate PCM starts", async () => {
  const f = await setupAudioPreview();
  let entered!: () => void;
  const began = new Promise<void>((r) => {
    entered = r;
  });
  const prepared: string[] = [];
  const picture = {
    suspend: vi.fn(),
    close: vi.fn(),
    observe: vi.fn(),
    prepare: vi.fn(async (s: { generation: string }, signal: AbortSignal) => {
      prepared.push(s.generation);
      if (s.generation === "2") {
        entered();
        await new Promise<void>((_, reject) =>
          signal.addEventListener(
            "abort",
            () => reject(new DOMException("Replaced", "AbortError")),
            { once: true },
          ),
        );
      }
    }),
  };
  try {
    await f.preview.play(48000, picture);
    f.outputs[0].finalFrames = "256";
    f.preview.draft(gainDraft(f));
    await began;
    f.preview.draft(null);
    await vi.waitFor(() =>
      expect(f.state().audioPreview.session?.generation).toBe("3"),
    );
    expect(prepared).toEqual(["1", "2", "3"]);
    expect(f.outputs[1].silenced).toBe(true);
    expect(f.stream.mock.calls.map(([s]) => s.generation)).toEqual(["1", "3"]);
    expect(
      f.controls.filter((r) => r.command.kind === "report_consumed"),
    ).toHaveLength(1);
    expect(f.state().audioPreview.session?.temporary_gain).toBeNull();
  } finally {
    f.cleanup();
  }
});

it("preserves pause caller errors while publishing failure and retiring original authority", async () => {
  const f = await setupAudioPreview();
  try {
    await f.preview.play(48000);
    const actual = f.control.getMockImplementation()!;
    f.control.mockImplementation(async (...args) => {
      if (args[0].command.kind === "pause")
        throw new Error("Pause response unavailable");
      return actual(...args);
    });
    await expect(f.preview.pause()).rejects.toThrow(
      "Pause response unavailable",
    );
    expect(f.state().audioPreview.phase).toBe("failed");
    expect(
      f.controls
        .filter((r) => r.command.kind === "stop")
        .map((r) => r.generation),
    ).toEqual(["1"]);
    expect(f.outputs[0].silenced).toBe(true);
  } finally {
    f.cleanup();
  }
});

it.each(["seek", "set_temporary_gain", "pause"] as const)(
  "retires an accepted %s whose response was aborted by leaving the view, replaying only its original request",
  async (kind) => {
    const f = await setupAudioPreview();
    try {
      await f.preview.play(48000);
      const actual = f.control.getMockImplementation()!;
      let entered!: () => void;
      const began = new Promise<void>((r) => {
        entered = r;
      });
      let held = false;
      f.control.mockImplementation(async (...args) => {
        const result = await actual(...args);
        if (!held && args[0].command.kind === kind) {
          held = true;
          entered();
          await new Promise<void>((_, reject) =>
            args[1]!.addEventListener(
              "abort",
              () => reject(new DOMException("View left", "AbortError")),
              { once: true },
            ),
          );
        }
        return result;
      });
      let pending: Promise<void> | null = null;
      if (kind === "seek") f.preview.seek({ ticks: "1", timebase: 1 });
      else if (kind === "set_temporary_gain") f.preview.draft(gainDraft(f));
      else pending = f.preview.pause();
      const rejected = pending
        ? expect(pending).rejects.toThrow("View left")
        : null;
      await began;
      const original = f.control.mock.calls.find(
        ([r]) => r.command.kind === kind,
      )![0];
      f.preview.seek();
      if (rejected) await rejected;
      await flush();
      const recovery = f.control.mock.calls.filter(
        ([r]) => r.command.kind === kind,
      );
      expect(recovery).toHaveLength(2);
      expect(recovery[1][0]).toBe(original);
      expect(recovery[1][1]?.aborted).toBe(false);
      const stops = f.controls.filter((r) => r.command.kind === "stop");
      expect(stops.map((r) => r.generation)).toEqual(
        kind === "pause" ? ["1"] : ["1", "2"],
      );
      expect(
        stops.every((r) => r.owner_id === "owner" && r.lease_id === "lease"),
      ).toBe(true);
      expect(f.outputs).toHaveLength(1);
      expect(f.stream).toHaveBeenCalledTimes(1);
      expect(f.state().audioPreview.session).toBeNull();
    } finally {
      f.cleanup();
    }
  },
);

it("failed original-key retirement cannot disturb a newer active preview or force a changed owner", async () => {
  const f = await setupAudioPreview();
  try {
    await f.preview.play(48000);
    const actual = f.control.getMockImplementation()!;
    let entered!: () => void;
    const began = new Promise<void>((r) => {
      entered = r;
    });
    let seeks = 0;
    f.control.mockImplementation(async (...args) => {
      if (args[0].command.kind === "seek") {
        seeks++;
        if (seeks === 1) {
          await actual(...args);
          entered();
          await new Promise<void>((_, reject) =>
            args[1]!.addEventListener(
              "abort",
              () => reject(new DOMException("View left", "AbortError")),
              { once: true },
            ),
          );
        }
        throw new OpenCutApiError(
          "PREVIEW_OWNER_CONFLICT",
          "changed owner",
          null,
          403,
          "req",
          [],
        );
      }
      return actual(...args);
    });
    f.preview.seek({ ticks: "1", timebase: 1 });
    await began;
    f.preview.seek();
    await f.preview.play(48000);
    await flush();
    expect(seeks).toBe(2);
    expect(f.state().audioPreview.session?.session_id).toBe("preview-1");
    expect(f.state().audioPreview.phase).toBe("loading");
    expect(
      f.controls
        .filter((r) => r.command.kind === "stop")
        .map((r) => r.session_id),
    ).toEqual(["preview-0"]);
  } finally {
    f.cleanup();
  }
});

it("retired unknown gain replays and stops with original document keepalive authority", async () => {
  const f = await setupAudioPreview();
  try {
    await f.preview.play(48000);
    const actual = f.control.getMockImplementation()!;
    let entered!: () => void;
    const began = new Promise<void>((r) => {
      entered = r;
    });
    let first = true;
    f.control.mockImplementation(async (...args) => {
      const result = await actual(...args);
      if (first && args[0].command.kind === "set_temporary_gain") {
        first = false;
        entered();
        await new Promise<void>((_, reject) =>
          args[1]!.addEventListener(
            "abort",
            () => reject(new DOMException("Document left", "AbortError")),
            { once: true },
          ),
        );
      }
      return result;
    });
    f.preview.draft(gainDraft(f));
    await began;
    f.preview.retireDocument();
    await flush();
    const calls = f.control.mock.calls.filter(
      ([r]) => r.command.kind === "set_temporary_gain",
    );
    expect(calls).toHaveLength(2);
    expect(calls[1][0]).toBe(calls[0][0]);
    expect(calls[1][2]).toEqual({ keepalive: true });
    const stops = f.control.mock.calls.filter(
      ([r]) => r.command.kind === "stop",
    );
    expect(stops.map(([r]) => r.generation)).toEqual(["1", "2"]);
    expect(stops.map(([, , options]) => options)).toEqual([
      { keepalive: true },
      { keepalive: true },
    ]);
    expect(f.state().audioPreview.session).toBeNull();
  } finally {
    f.cleanup();
  }
});

it("rejects a recovery receipt carrying a different owner before any new-generation Stop", async () => {
  const f = await setupAudioPreview();
  try {
    await f.preview.play(48000);
    const actual = f.control.getMockImplementation()!;
    let entered!: () => void;
    const began = new Promise<void>((r) => {
      entered = r;
    });
    let seeks = 0;
    f.control.mockImplementation(async (...args) => {
      const result = await actual(...args);
      if (args[0].command.kind === "seek") {
        if (++seeks === 1) {
          entered();
          await new Promise<void>((_, reject) =>
            args[1]!.addEventListener(
              "abort",
              () => reject(new DOMException("View left", "AbortError")),
              { once: true },
            ),
          );
        } else return { ...result, owner_id: "other-owner" };
      }
      return result;
    });
    f.preview.seek({ ticks: "1", timebase: 1 });
    await began;
    f.preview.seek();
    await flush();
    expect(seeks).toBe(2);
    expect(
      f.controls
        .filter((r) => r.command.kind === "stop")
        .map((r) => r.generation),
    ).toEqual(["1"]);
    expect(f.state().audioPreview.session).toBeNull();
  } finally {
    f.cleanup();
  }
});

it.each(["NOT_AUTHORIZED", "PREVIEW_GENERATION_CONFLICT"])(
  "does not replay an authoritative %s rejection",
  async (code) => {
    const f = await setupAudioPreview();
    try {
      await f.preview.play(48000);
      const actual = f.control.getMockImplementation()!;
      let rejected = 0;
      f.control.mockImplementation(async (...args) => {
        if (args[0].command.kind === "seek") {
          rejected++;
          throw new OpenCutApiError(code, "rejected", null, 403, "req", []);
        }
        return actual(...args);
      });
      f.preview.seek({ ticks: "1", timebase: 1 });
      await flush();
      expect(rejected).toBe(1);
      expect(f.state().audioPreview.phase).toBe("failed");
      expect(f.outputs[0].silenced).toBe(true);
    } finally {
      f.cleanup();
    }
  },
);
