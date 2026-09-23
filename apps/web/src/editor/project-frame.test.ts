import { afterEach, describe, expect, it, vi } from "vitest";
import { EditorClient, timeFromDecimalSeconds, type Snapshot } from "../lib/editor-client";
import { isCurrentProjectFrameRequest } from "./project-frame";

const request = { projectId: "project-a", revision: "rev-4", generation: 9 };

describe("isCurrentProjectFrameRequest", () => {
  it("accepts only the selected project, fixed revision, and newest request", () => {
    expect(isCurrentProjectFrameRequest(request, { ...request })).toBe(true);
    expect(
      isCurrentProjectFrameRequest(request, {
        ...request,
        projectId: "project-b",
      }),
    ).toBe(false);
    expect(
      isCurrentProjectFrameRequest(request, { ...request, revision: "rev-5" }),
    ).toBe(false);
    expect(
      isCurrentProjectFrameRequest(request, { ...request, generation: 10 }),
    ).toBe(false);
  });
});

describe("project frame time input", () => {
  it("keeps decimal seconds exact and rejects negative input", () => {
    expect(timeFromDecimalSeconds("12.125")).toEqual({
      ticks: "97",
      timebase: 8,
    });
    expect(() => timeFromDecimalSeconds("-0.1")).toThrow(
      "non-negative decimal",
    );
  });
});

describe("project frame artifact requests", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("keeps encoded preview as the default and explicitly opts into direct composition", async () => {
    const fetchMock = vi.fn().mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ data: { job_id: "frame" }, error: null }))),
    );
    vi.stubGlobal("fetch", fetchMock);
    const client = new EditorClient("http://localhost:8788", "secret");
    const snapshot: Snapshot = {
      revision: "fixed-revision",
      project: {
        id: "project-a", name: "Project", schema_version: 2,
        default_sequence_id: "main", sequences: [], assets: [],
      },
    };
    const time = timeFromDecimalSeconds("0.125");
    await client.previewProjectFrame(snapshot, time);
    await client.previewProjectFrame(snapshot, time, "preview.composited_frame");
    const [encoded, direct] = fetchMock.mock.calls.map((call) => JSON.parse(call[1].body));
    expect(encoded.kind).toBe("preview.project_frame");
    expect(direct.kind).toBe("preview.composited_frame");
    expect(direct.input).toEqual({
      project_id: "project-a", revision: "fixed-revision", sequence_id: "main",
      requested_time: { ticks: "1", timebase: 8 }, alignment: "nearest",
    });
    expect(encoded.input).toEqual(direct.input);
    expect(encoded.idempotency_key).not.toBe(direct.idempotency_key);
  });

  it("aborts a stale download and permits a clean retry", async () => {
    const fetchMock = vi
      .fn()
      .mockImplementationOnce(
        (_: string, init?: RequestInit) =>
          new Promise((_, reject) =>
            init?.signal?.addEventListener("abort", () =>
              reject(new DOMException("Aborted", "AbortError")),
            ),
          ),
      )
      .mockResolvedValueOnce(new Response("frame", { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const client = new EditorClient("http://localhost:8788", "secret"),
      controller = new AbortController(),
      stale = client.artifact("old-job", controller.signal);

    controller.abort();
    await expect(stale).rejects.toMatchObject({ name: "AbortError" });
    await expect(client.artifact("new-job")).resolves.toBeInstanceOf(Blob);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("submits a fixed-revision range without converting exact times to floats", async () => {
    const fetchMock = vi.fn().mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ data: { job_id: "range" }, error: null }))),
    );
    vi.stubGlobal("fetch", fetchMock);
    const client = new EditorClient("http://localhost:8788", "secret");
    const snapshot: Snapshot = {
      revision: "fixed-range-revision",
      project: {
        id: "project-range", name: "Range", schema_version: 2,
        default_sequence_id: "main", sequences: [], assets: [],
      },
    };
    const requestedRange = {
      start: { ticks: "1001", timebase: 30000 },
      duration: { ticks: "1001", timebase: 1000 },
    };
    await client.previewProjectRange(snapshot, requestedRange);
    const request = JSON.parse(fetchMock.mock.calls[0][1].body);
    expect(request.kind).toBe("preview.project_range");
    expect(request.input).toEqual({
      project_id: "project-range", revision: "fixed-range-revision", sequence_id: "main",
      requested_range: requestedRange, alignment: "nearest",
    });
    expect(request.input).not.toHaveProperty("container");
  });
});
