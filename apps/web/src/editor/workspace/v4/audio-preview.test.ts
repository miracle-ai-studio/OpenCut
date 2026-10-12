import { afterEach, expect, it, vi } from "vitest";
import { OpenCutApiError } from "../../../lib/editor-sdk";
import { setupAudioPreview } from "./audio-preview-fixture";

afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });
const flush = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };

it("reports completed endpoints immediately, coalesces in-flight endpoints and ignores ordinary quantum progress", async () => {
  const f = await setupAudioPreview();
  vi.useFakeTimers();
  try {
    await f.preview.play();
    const actual = f.control.getMockImplementation()!;
    let release!: () => void;
    f.control.mockImplementationOnce(async (r, signal) => {
      const result = await actual(r, signal);
      await new Promise<void>(resolve => { release = resolve; });
      return result;
    });
    const output = f.outputs[0];
    output.progress({ consumedFrames: "128", stalled: false, contextTime: 1 });
    expect(f.controls.filter(r => r.command.kind === "report_consumed")).toHaveLength(0);
    output.progress({ consumedFrames: "4096", stalled: false, contextTime: 1 });
    output.completedBlock();
    expect(f.controls.at(-1)?.command).toMatchObject({ kind: "report_consumed", consumed_frames: "4096", sequence: "1" });
    await flush();
    for (const consumedFrames of ["8192", "12288"]) {
      output.progress({ consumedFrames, stalled: false, contextTime: 2 }); output.completedBlock();
    }
    expect(f.controls.filter(r => r.command.kind === "report_consumed")).toHaveLength(1);
    release(); await flush();
    expect(f.controls.filter(r => r.command.kind === "report_consumed").map(r => r.command)).toEqual([
      { kind: "report_consumed", consumed_frames: "4096", sequence: "1", playing: true, stalled: false },
      { kind: "report_consumed", consumed_frames: "12288", sequence: "2", playing: true, stalled: false },
    ]);
    f.control.mockImplementationOnce(async (r, signal) => {
      const result = await actual(r, signal);
      await new Promise<void>(resolve => { release = resolve; });
      return result;
    });
    output.progress({ consumedFrames: "16384", stalled: false, contextTime: 3 }); output.completedBlock();
    await flush();
    output.progress({ consumedFrames: "20480", stalled: false, contextTime: 4 }); output.completedBlock();
    f.invalidateGeneration(); release(); await flush();
    output.completedBlock();
    expect(f.controls.filter(r => r.command.kind === "report_consumed")).toHaveLength(3);
  } finally { f.cleanup(); }
});

it("releases consumption credit before heartbeat and flushes completions queued during renewal", async () => {
  const f = await setupAudioPreview();
  vi.useFakeTimers();
  try {
    await f.preview.play();
    const actual = f.control.getMockImplementation()!;
    let release!: () => void;
    f.control.mockImplementation(async (r, signal) => {
      const result = await actual(r, signal);
      if (r.command.kind === "heartbeat") await new Promise<void>(resolve => { release = resolve; });
      return result;
    });
    vi.setSystemTime(Date.now() + 501);
    const output = f.outputs[0];
    output.progress({ consumedFrames: "4096", stalled: false, contextTime: 1 }); output.completedBlock();
    await flush();
    expect(f.controls.slice(1).map(r => r.command.kind)).toEqual(["report_consumed", "heartbeat"]);
    output.progress({ consumedFrames: "8192", stalled: false, contextTime: 2 }); output.completedBlock();
    release(); await flush();
    expect(f.controls.slice(1).map(r => r.command.kind)).toEqual(["report_consumed", "heartbeat", "report_consumed"]);
    expect(f.controls.at(-1)?.command).toMatchObject({ consumed_frames: "8192", sequence: "2" });
  } finally { f.cleanup(); }
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
      await new Promise<void>(resolve => { release = resolve; });
      return result;
    });
    const output = f.outputs[0];
    output.progress({ consumedFrames: "4096", stalled: false, contextTime: 1 }); output.completedBlock();
    await flush();
    output.progress({ consumedFrames: "8192", stalled: false, contextTime: 2 }); output.completedBlock();
    output.finalFrames = "9000";
    const pause = f.preview.pause();
    output.completedBlock();
    release(); await flush(); await vi.advanceTimersByTimeAsync(5); await pause;
    expect(f.controls.filter(r => r.command.kind === "report_consumed").map(r => r.command)).toEqual([
      { kind: "report_consumed", consumed_frames: "4096", sequence: "1", playing: true, stalled: false },
      { kind: "report_consumed", consumed_frames: "9000", sequence: "2", playing: false, stalled: false },
    ]);
    expect(f.controls.at(-1)?.command.kind).toBe("pause");
    output.completedBlock(); await flush();
    expect(f.controls.at(-1)?.command.kind).toBe("pause");
  } finally { f.cleanup(); }
});

it("ignores the replaced output's endpoint after a playing seek", async () => {
  const f = await setupAudioPreview();
  vi.useFakeTimers();
  try {
    await f.preview.play();
    const old = f.outputs[0]; old.finalFrames = "128";
    f.preview.seek({ ticks: "1", timebase: 1 }); await flush();
    expect(f.outputs).toHaveLength(2);
    const reports = f.controls.filter(r => r.command.kind === "report_consumed").length;
    old.progress({ consumedFrames: "4096", stalled: false, contextTime: 3 }); old.completedBlock();
    await flush();
    expect(f.controls.filter(r => r.command.kind === "report_consumed")).toHaveLength(reports);
    await vi.advanceTimersByTimeAsync(50);
    expect(f.controls.at(-1)).toMatchObject({ generation: "2", command: { consumed_frames: "0", sequence: "1" } });
    f.outputs[1].progress({ consumedFrames: "4096", stalled: false, contextTime: 4 }); f.outputs[1].completedBlock();
    await flush();
    expect(f.controls.at(-1)).toMatchObject({ generation: "2", command: { consumed_frames: "4096", sequence: "2" } });
  } finally { f.cleanup(); }
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
