import { afterEach, describe, expect, it, vi } from "vitest";
import { EditorClient, parseSseStream } from "../lib/editor-client";

const stream = (chunks: string[]) =>
  new ReadableStream<Uint8Array>({
    start(controller) {
      for (const chunk of chunks)
        controller.enqueue(new TextEncoder().encode(chunk));
      controller.close();
    },
  });

describe("parseSseStream", () => {
  it("preserves event ids and data across arbitrary chunk boundaries", async () => {
    const values = [];
    for await (const item of parseSseStream(
      stream([
        "id: 4\r",
        '\nevent: job\r\ndata: {"seq',
        'uence":4}\r\n\r\nid: 5\ndata: first\ndata: second\n\n',
      ]),
    ))
      values.push(item);
    expect(values).toEqual([
      { id: "4", event: "job", data: '{"sequence":4}' },
      { id: "5", event: null, data: "first\nsecond" },
    ]);
  });
  it("ignores heartbeat comments", async () => {
    const values = [];
    for await (const item of parseSseStream(
      stream([": keepalive\n\nid: 9\ndata: {}\n\n"]),
    ))
      values.push(item);
    expect(values).toHaveLength(1);
    expect(values[0]?.id).toBe("9");
  });
});

describe("EditorClient.streamJobEvents", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("resumes with Last-Event-ID and applies each sequence once", async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(() =>
        Promise.resolve(
          new Response(
            stream([
              'id: 7\nevent: progress\ndata: {"sequence":7}\n\n',
              'id: 8\nevent: progress\ndata: {"sequence":8}\n\n',
              'id: 8\nevent: progress\ndata: {"sequence":8}\n\n',
            ]),
            { status: 200 },
          ),
        ),
      );
    vi.stubGlobal("fetch", fetchMock);
    const seen: number[] = [];
    const cursor = await new EditorClient(
      "http://localhost:8788",
      "secret",
    ).streamJobEvents(7, null, new AbortController().signal, (_, sequence) =>
      seen.push(sequence),
    );

    expect(cursor).toBe(8);
    expect(seen).toEqual([8]);
    expect(fetchMock).toHaveBeenCalledWith(
      "http://localhost:8788/v1/job-events/stream?after_sequence=7",
      expect.objectContaining({
        headers: expect.objectContaining({ "Last-Event-ID": "7" }),
      }),
    );
  });

  it("throws on server error events without advancing the caller cursor", async () => {
    const cancellations: Array<ReturnType<typeof vi.fn>> = [];
    const fetchMock = vi.fn().mockImplementation(() => {
      const cancel = vi.fn();
      cancellations.push(cancel);
      return Promise.resolve(
        new Response(
          new ReadableStream<Uint8Array>({
            start(controller) {
              controller.enqueue(
                new TextEncoder().encode(
                  'id: 12\nevent: error\ndata: {"code":"JOB_EVENT_READ_FAILED","retryable":true}\n\n',
                ),
              );
            },
            cancel,
          }),
          { status: 200 },
        ),
      );
    });
    vi.stubGlobal("fetch", fetchMock);
    const client = new EditorClient("http://localhost:8788", "secret");
    const cursor = 11;

    await expect(
      client.streamJobEvents(cursor, null, new AbortController().signal, () => {
        throw new Error("error events must not be applied");
      }),
    ).rejects.toThrow("JOB_EVENT_READ_FAILED");
    expect(cancellations[0]).toHaveBeenCalledOnce();

    await expect(
      client.streamJobEvents(
        cursor,
        null,
        new AbortController().signal,
        () => {},
      ),
    ).rejects.toThrow("JOB_EVENT_READ_FAILED");
    expect(cancellations[1]).toHaveBeenCalledOnce();
    expect(fetchMock.mock.calls[1]?.[1]).toEqual(
      expect.objectContaining({
        headers: expect.objectContaining({ "Last-Event-ID": "11" }),
      }),
    );
  });
});
