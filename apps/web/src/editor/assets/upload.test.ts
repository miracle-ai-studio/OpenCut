import { createHash } from "node:crypto";
import { describe, expect, it, vi } from "vitest";
import { OpenCutClient } from "../../lib/editor-sdk";
import { uploadMedia } from "./upload";

function fixture() {
  const sdk = new OpenCutClient({ baseUrl: "http://localhost", token: "" });
  const bytes = new TextEncoder().encode("abcdefghi");
  const digest = createHash("sha256").update(bytes).digest("hex");
  const file = new File([bytes], "sample.wav");
  const events: string[] = [];
  const slice = file.slice.bind(file);
  vi.spyOn(file, "slice").mockImplementation((start, end) => {
    events.push(`read:${start}:${end}`);
    return slice(start, end);
  });
  vi.spyOn(sdk, "hostCapabilities").mockResolvedValue({
    uploads: { max_chunk_bytes: "3", max_upload_bytes: "100" },
  } as never);
  const session = {
    project_id: "p",
    upload_id: "u",
    size_bytes: "9",
    next_offset: "0",
  };
  const asset = {
    id: "independent-asset-id",
    version: { content_hash: digest },
  };
  const call = vi.spyOn(sdk, "call").mockImplementation(async (name, input) => {
    events.push(name);
    switch (name) {
      case "opencut_upload_stream_begin":
        expect(input).not.toHaveProperty("content_hash");
        return session as never;
      case "opencut_upload_stream_complete":
        expect(input).toEqual({ upload_id: "u", content_hash: digest });
        return { ...session, job_id: "j" } as never;
      case "opencut_project_media_inspect":
        return { asset, project_id: "p", media_id: "m" } as never;
      default:
        return {} as never;
    }
  });
  const upload = vi
    .spyOn(sdk, "uploadStreamChunk")
    .mockImplementation(async (_id, offset, chunk) => {
      events.push(`upload:${offset}:${chunk.byteLength}`);
      expect([...chunk]).toEqual([
        ...bytes.slice(Number(offset), Number(offset) + chunk.byteLength),
      ]);
      return {
        ...session,
        next_offset: String(Number(offset) + chunk.byteLength),
      } as never;
    });
  vi.spyOn(sdk, "waitForPublicJob").mockResolvedValue({
    state: "succeeded",
    kind: "media.import",
    result: null,
    result_unavailable: null,
    extended_result: {
      kind: "media.import",
      schema_version: 1,
      data: { media_id: "m" },
    },
  } as never);
  return { sdk, file, events, call, upload, asset };
}

describe("single-pass media import", () => {
  it("rejects an observed media identity from another project before returning it", async () => {
    const f = fixture(),
      previous = f.call.getMockImplementation()!;
    f.call.mockImplementation(async (name, input, options) => {
      if (name === "opencut_project_media_inspect")
        return { asset: f.asset, project_id: "other", media_id: "m" } as never;
      return previous(name, input, options);
    });
    await expect(
      uploadMedia(f.sdk, "p", f.file, vi.fn(), new AbortController().signal),
    ).rejects.toThrow("project resource identity");
  });
  it("reads ahead only one chunk while waiting for durable acknowledgement", async () => {
    const f = fixture();
    let acknowledge!: () => void;
    f.upload.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          acknowledge = () => resolve({ next_offset: "3" } as never);
        }),
    );
    const importing = uploadMedia(
      f.sdk,
      "p",
      f.file,
      vi.fn(),
      new AbortController().signal,
    );
    await vi.waitFor(() => expect(f.file.slice).toHaveBeenCalledTimes(2));
    expect(f.upload).toHaveBeenCalledTimes(1);
    acknowledge();
    await importing;
    expect(f.file.slice).toHaveBeenCalledTimes(3);
  });

  it("starts sending after the first chunk and reads each byte only once", async () => {
    const f = fixture();
    const result = await uploadMedia(
      f.sdk,
      "p",
      f.file,
      vi.fn(),
      new AbortController().signal,
    );
    expect(result.asset).toEqual(f.asset);
    expect(result.media_id).toBe("m");
    expect(result.media.asset.id).toBe("independent-asset-id");
    expect(result.version).toEqual(f.asset.version);
    expect(f.events).toEqual([
      "opencut_upload_stream_begin",
      "read:0:3",
      "upload:0:3",
      "read:3:6",
      "upload:3:3",
      "read:6:9",
      "upload:6:3",
      "opencut_upload_stream_complete",
      "opencut_project_media_inspect",
      "opencut_upload_release",
    ]);
  });

  it("stops reading on cancellation and releases the incomplete upload", async () => {
    const f = fixture();
    const controller = new AbortController();
    const reason = new Error("cancelled");
    f.upload.mockImplementationOnce(async () => {
      controller.abort(reason);
      throw reason;
    });
    await expect(
      uploadMedia(f.sdk, "p", f.file, vi.fn(), controller.signal),
    ).rejects.toBe(reason);
    expect(f.file.slice).toHaveBeenCalledTimes(1);
    expect(f.events).toContain("opencut_upload_stream_abort");
    expect(f.events).toContain("opencut_upload_release");
    expect(f.events).not.toContain("opencut_upload_stream_complete");
  });
  it("requests cancellation of a sealed native import instead of aborting its upload session", async () => {
    const f = fixture(),
      request = new AbortController();
    const cancel = vi
      .spyOn(f.sdk, "cancelPublicJob")
      .mockResolvedValue({} as never);
    vi.mocked(f.sdk.waitForPublicJob).mockImplementationOnce(async () => {
      request.abort();
      request.signal.throwIfAborted();
      return {} as never;
    });
    await expect(
      uploadMedia(f.sdk, "p", f.file, vi.fn(), request.signal),
    ).rejects.toThrow();
    expect(cancel).toHaveBeenCalledWith("j");
    expect(f.events).not.toContain("opencut_upload_stream_abort");
    expect(f.events).not.toContain("opencut_project_media_inspect");
  });
});
