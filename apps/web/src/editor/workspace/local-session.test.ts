// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { WorkspaceController } from "./controller";
import { EditorApi } from "./api";

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
const deferred = <T>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
};
const empty = () => new Response(null, { status: 204 });
function setup(endpoint = location.origin, token = "") {
  const api = new EditorApi(endpoint, token);
  vi.spyOn(api.sdk, "hostCapabilities").mockResolvedValue({} as never);
  vi.spyOn(api, "authenticateConnection").mockResolvedValue();
  vi.spyOn(api, "projects").mockResolvedValue([]);
  const controller = new WorkspaceController(() => api);
  return { controller, api };
}

describe("local browser session lifecycle", () => {
  it("sends logout after an in-flight bootstrap so its late cookie cannot revive the session", async () => {
    const { controller } = setup();
    const bootstrap = deferred<Response>();
    const fetcher = vi
      .fn()
      .mockImplementationOnce(() => bootstrap.promise)
      .mockResolvedValue(empty());
    vi.stubGlobal("fetch", fetcher);
    const connect = vi.spyOn(controller, "connect").mockResolvedValue();
    const start = controller.start();
    await Promise.resolve();
    expect(fetcher).toHaveBeenCalledTimes(1);
    const logout = controller.logoutLocalSession();
    await Promise.resolve();
    expect(fetcher).toHaveBeenCalledTimes(1);
    bootstrap.resolve(empty());
    await Promise.all([start, logout]);
    expect(fetcher.mock.calls.map(([, init]) => init.method)).toEqual([
      "POST",
      "DELETE",
    ]);
    for (const [path, init] of fetcher.mock.calls) {
      expect(path).toBe("/app/session");
      expect(init.credentials).toBe("same-origin");
      expect(init.headers).toEqual({ "X-OpenCut-App": "web" });
    }
    expect(connect).not.toHaveBeenCalled();
    expect(controller.getSnapshot().api).toBeNull();
    expect(controller.getSnapshot().message).toBe("Local session ended.");
  });

  it("disconnect alone keeps the grant, while failed explicit logout stays disconnected and can be retried", async () => {
    const { controller } = setup();
    const fetcher = vi
      .fn()
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValue(empty());
    vi.stubGlobal("fetch", fetcher);
    await controller.connect(location.origin, "");
    controller.disconnect();
    expect(fetcher).not.toHaveBeenCalled();
    await controller.connect(location.origin, "");
    await expect(controller.logoutLocalSession()).rejects.toThrow("offline");
    expect(controller.getSnapshot().connection).toBe("disconnected");
    expect(controller.getSnapshot().api).toBeNull();
    await controller.logoutLocalSession();
    expect(fetcher).toHaveBeenCalledTimes(2);
    expect(controller.getSnapshot().message).toBe("Local session ended.");
  });

  it("deduplicates logout and waits for it before an explicitly requested new session", async () => {
    const { controller } = setup();
    const ending = deferred<Response>();
    const fetcher = vi
      .fn()
      .mockImplementationOnce(() => ending.promise)
      .mockResolvedValue(empty());
    vi.stubGlobal("fetch", fetcher);
    const connect = vi.spyOn(controller, "connect").mockResolvedValue();
    const first = controller.logoutLocalSession(),
      second = controller.logoutLocalSession();
    const restart = controller.start();
    await Promise.resolve();
    expect(fetcher).toHaveBeenCalledTimes(1);
    ending.resolve(empty());
    await Promise.all([first, second, restart]);
    expect(fetcher.mock.calls.map(([, init]) => init.method)).toEqual([
      "DELETE",
      "POST",
    ]);
    expect(connect).toHaveBeenCalledExactlyOnceWith(location.origin, "");
  });

  it.each([
    [location.origin, "explicit-token"],
    ["http://other-service.example", ""],
  ])(
    "does not revoke or disconnect an explicit bearer or remote connection (%s)",
    async (endpoint, token) => {
      const { controller, api } = setup(endpoint, token);
      const fetcher = vi.fn();
      vi.stubGlobal("fetch", fetcher);
      await controller.connect(endpoint, token);
      await expect(controller.logoutLocalSession()).rejects.toThrow(
        "Only the local browser session",
      );
      expect(controller.getSnapshot().api).toBe(api);
      expect(fetcher).not.toHaveBeenCalled();
    },
  );
});
