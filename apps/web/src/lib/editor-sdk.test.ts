import { afterEach, expect, it, vi } from "vitest";
import { createEditorClient } from "./editor-sdk";
afterEach(() => vi.unstubAllGlobals());
it.each(["", "captured-credential"])(
  "shares the captured cookie/bearer stream transport (%s)",
  async (token) => {
    const fetch = vi.fn(async () => new Response("bounded-stream"));
    vi.stubGlobal("fetch", fetch);
    const signal = new AbortController().signal;
    await createEditorClient({
      baseUrl: "http://localhost:1234/",
      token,
    }).openV4AudioPreviewStream(
      {
        session_id: "fixed",
        generation: "2",
        overlay_epoch: "3",
        owner_id: "owner",
        lease_id: "lease",
      },
      { signal },
    );
    const [url, init] = fetch.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe("http://localhost:1234/v1/preview/v4/audio/stream");
    expect(init.signal).toBe(signal);
    expect(init.redirect).toBe("error");
    expect(new Headers(init.headers).get("Authorization")).toBe(
      token ? `Bearer ${token}` : null,
    );
    expect(init.credentials).toBe(token ? undefined : "same-origin");
    expect(JSON.parse(init.body as string)).toEqual({
      session_id: "fixed",
      generation: "2",
      overlay_epoch: "3",
      owner_id: "owner",
      lease_id: "lease",
    });
  },
);
