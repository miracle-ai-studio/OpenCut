import { describe, expect, it, vi } from "vitest";
import {
  loadCurrentSourcePreview,
  prepareCurrentPreviewRequest,
  readCurrentProjectState,
  type WorkspaceIdentity,
} from "./workspace-context";

const deferred = <T>() => {
  let resolve!: (value: T) => void, reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
};

describe("workspace async response isolation", () => {
  it("keeps a current source URL and returns current snapshot/history", async () => {
    const request = {
        connection: {},
        projectId: "project-a",
        revision: "rev-1",
        generation: 1,
      },
      revoke = vi.fn();
    await expect(
      loadCurrentSourcePreview({
        request,
        current: () => request,
        load: async () => "blob:current-source",
        revoke,
      }),
    ).resolves.toEqual({ url: "blob:current-source", error: null });
    expect(revoke).not.toHaveBeenCalled();
    await expect(
      readCurrentProjectState({
        request,
        current: () => request,
        readSnapshot: async () => "current snapshot",
        readHistory: async () => "current history",
      }),
    ).resolves.toEqual({
      snapshot: "current snapshot",
      history: "current history",
    });
  });

  it("revokes a source blob completed after the user changes projects", async () => {
    const connection = {},
      request = {
        connection,
        projectId: "project-a",
        revision: "rev-1",
        generation: 1,
      },
      blob = deferred<string>(),
      revoke = vi.fn(),
      current: { value: WorkspaceIdentity<object> | null } = {
        value: request,
      },
      loading = loadCurrentSourcePreview({
        request,
        current: () => current.value,
        load: () => blob.promise,
        revoke,
      });

    current.value = { ...request, projectId: "project-b", generation: 2 };
    blob.resolve("blob:old-source");
    await expect(loading).resolves.toEqual({ url: null, error: null });
    expect(revoke).toHaveBeenCalledWith("blob:old-source");
  });

  it("does not surface stale snapshot or history readback", async () => {
    const connection = {},
      request = {
        connection,
        projectId: "project-a",
        revision: "rev-1",
        generation: 1,
      },
      snapshot = deferred<string>(),
      history = deferred<string>(),
      current: { value: WorkspaceIdentity<object> | null } = {
        value: request,
      },
      reading = readCurrentProjectState({
        request,
        current: () => current.value,
        readSnapshot: () => snapshot.promise,
        readHistory: () => history.promise,
      });

    current.value = { ...request, connection: {}, generation: 2 };
    snapshot.resolve("old snapshot");
    history.resolve("old history");
    await expect(reading).resolves.toBe(null);
  });

  it("rejects a response after leaving and returning to the same project", async () => {
    const connection = {},
      request = {
        connection,
        projectId: "project-a",
        revision: "rev-1",
        generation: 1,
      },
      blob = deferred<string>(),
      revoke = vi.fn(),
      current: { value: WorkspaceIdentity<object> | null } = {
        value: request,
      },
      loading = loadCurrentSourcePreview({
        request,
        current: () => current.value,
        load: () => blob.promise,
        revoke,
      });

    current.value = { ...request, generation: 3 };
    blob.resolve("blob:first-visit");
    await expect(loading).resolves.toEqual({ url: null, error: null });
    expect(revoke).toHaveBeenCalledWith("blob:first-visit");
  });

  it("reports a deferred preview download failure without inventing a URL", async () => {
    const request = {
        connection: {},
        projectId: "project-a",
        revision: "rev-1",
        generation: 1,
      },
      blob = deferred<string>(),
      failed = loadCurrentSourcePreview({
        request,
        current: () => request,
        load: () => blob.promise,
        revoke: vi.fn(),
      });

    blob.reject(new Error("media fetch failed"));
    await expect(failed).resolves.toMatchObject({
      url: null,
      error: { message: "media fetch failed" },
    });
  });

  it("submits only the newest consecutive preview without evicting edit readback", async () => {
    const connection = {},
      editRequest = {
        connection,
        projectId: "project-a",
        revision: "rev-1",
        generation: 40,
      },
      sourceOne = { ...editRequest, generation: 1 },
      sourceTwo = { ...editRequest, generation: 2 },
      firstPreparation = deferred<boolean>(),
      secondPreparation = deferred<boolean>(),
      editSnapshot = deferred<string>(),
      editHistory = deferred<string>(),
      editCurrent: { value: WorkspaceIdentity<object> | null } = {
        value: editRequest,
      },
      sourceCurrent: { value: WorkspaceIdentity<object> | null } = {
        value: sourceOne,
      },
      submissions: string[] = [];
    const previewFlow = async (
      name: string,
      request: WorkspaceIdentity<object>,
      preparation: Promise<boolean>,
    ) => {
      const ready = await prepareCurrentPreviewRequest({
        request,
        current: () => sourceCurrent.value,
        prepare: () => preparation,
      });
      if (ready) submissions.push(name);
    };
    const first = previewFlow("first", sourceOne, firstPreparation.promise),
      editReadback = readCurrentProjectState({
        request: editRequest,
        current: () => editCurrent.value,
        readSnapshot: () => editSnapshot.promise,
        readHistory: () => editHistory.promise,
      });

    sourceCurrent.value = sourceTwo;
    const second = previewFlow("second", sourceTwo, secondPreparation.promise);
    secondPreparation.resolve(true);
    await second;
    firstPreparation.resolve(true);
    await first;
    editSnapshot.resolve("saved snapshot");
    editHistory.resolve("saved history");

    expect(submissions).toEqual(["second"]);
    await expect(editReadback).resolves.toEqual({
      snapshot: "saved snapshot",
      history: "saved history",
    });
  });
});
