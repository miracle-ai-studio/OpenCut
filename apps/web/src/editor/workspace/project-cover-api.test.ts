import { afterEach, expect, it, vi } from "vitest";
import { EditorApi } from "./api";
import { WorkspaceController } from "./controller";
import { OpenCutApiError } from "../../lib/editor-sdk";
import type { ProjectRef } from "../model/types";
import { initialSnapshot } from "./scene/fixture";
import type { Snapshot as EditorSnapshot } from "./scene/types";
import { sourceThumbnail } from "./source-thumbnail";
vi.mock("./source-thumbnail", () => ({ sourceThumbnail: vi.fn(async () => new Blob(["thumbnail"], {type:"image/png"})) }));
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
type Snapshot = EditorSnapshot & { format: "editor" | "base" | "visual" };
const projectRef = (snapshot: Snapshot): ProjectRef => ({id:snapshot.project.id,name:snapshot.project.name,revision:snapshot.revision,format:snapshot.format});
function fixture(format: Snapshot["format"] = "editor") {
  vi.stubGlobal("location", {origin:"http://localhost:8788"});
  const api = new EditorApi("http://localhost:8788", "test"), base = initialSnapshot();
  const s: Snapshot = {...base, format, revision:"fixed", project:{...base.project,id:"p",name:"P"}};
  const inspect = vi.spyOn(api.sdk,"inspectEditorProject").mockResolvedValue(s);
  const resource = vi.spyOn(api.scene,"resource").mockResolvedValue({project_id:"p",revision:"fixed",resource_id:"asset",version:s.project.assets[0].version} as never);
  vi.spyOn(api.sdk,"projectResourceUrl").mockReturnValue("http://localhost:8788/resource");
  const submit = vi.spyOn(api.sdk,"callWithAdmissionRetry");
  const cancel = vi.spyOn(api.sdk,"cancelJob").mockResolvedValue({} as never);
  return {api,s,inspect,resource,submit,cancel};
}
it("loads fixed media for library artwork without submitting or cancelling render jobs", async () => {
  const f=fixture(); const signal=new AbortController().signal;
  expect((await f.api.covers.request(projectRef(f.s),signal))!.type).toBe("image/png");
  expect(f.resource).toHaveBeenCalledWith("p","fixed","asset",expect.any(AbortSignal));
  expect(sourceThumbnail).toHaveBeenCalledWith("http://localhost:8788/resource","image",expect.any(AbortSignal));
  expect(f.submit).not.toHaveBeenCalled(); expect(f.cancel).not.toHaveBeenCalled();
});
it("a saved video cover uses its source time and overrides the browser's selected scene", async () => {
  const f = fixture();
  f.s.project.assets[0].kind = "video";
  f.s.project.cover = { kind: "video_frame", asset_id: "asset", time: { ticks: "75", timebase: 25 } };
  f.s.project.sequences.push({ ...structuredClone(f.s.project.sequences[0]), id: "empty", tracks: [] });
  await f.api.covers.request(projectRef(f.s), new AbortController().signal, "empty");
  expect(sourceThumbnail).toHaveBeenLastCalledWith("http://localhost:8788/resource", "video", expect.any(AbortSignal), 3);
  expect(f.resource).toHaveBeenCalledWith("p", "fixed", "asset", expect.any(AbortSignal));
  expect(f.submit).not.toHaveBeenCalled();
});
it("a saved image cover need not be placed on a timeline and still requires the exact resource version", async () => {
  const f = fixture();
  f.s.project.sequences[0].tracks = [];
  f.s.project.cover = { kind: "image", asset_id: "asset" };
  expect(await f.api.covers.request(projectRef(f.s), new AbortController().signal)).toBeTruthy();
  expect(sourceThumbnail).toHaveBeenLastCalledWith("http://localhost:8788/resource", "image", expect.any(AbortSignal));
  f.api.covers.invalidate("p");
  f.resource.mockResolvedValue({ project_id: "another", revision: "fixed", resource_id: "asset", version: f.s.project.assets[0].version } as never);
  await expect(f.api.covers.request(projectRef(f.s), new AbortController().signal)).rejects.toThrow("resource changed");
});
it("refuses mismatched project and resource versions before decoding media", async () => {
  const f=fixture(); f.inspect.mockResolvedValue({...f.s,revision:"wrong"});
  await expect(f.api.covers.request(projectRef(f.s),new AbortController().signal)).rejects.toThrow("revision changed");
  f.inspect.mockResolvedValue(f.s); f.resource.mockResolvedValue({project_id:"p",revision:"fixed",resource_id:"asset",version:{...f.s.project.assets[0].version,content_hash:"wrong"}} as never);
  await expect(f.api.covers.request(projectRef(f.s),new AbortController().signal)).rejects.toThrow("resource changed");
  expect(f.submit).not.toHaveBeenCalled();
});
it("empty scenes and older project formats use placeholders; selected scenes fall back to the main scene", async () => {
  const f=fixture(); const empty={...structuredClone(f.s.project.sequences[0]),id:"empty",tracks:[]}; f.s.project.sequences.push(empty);
  expect(await f.api.covers.request(projectRef(f.s),new AbortController().signal,"empty")).toBeNull();
  expect(await f.api.covers.request(projectRef(f.s),new AbortController().signal,"deleted")).toBeTruthy();
  for (const format of ["base","visual"] as const) { const old=fixture(format); expect(await old.api.covers.request(projectRef(old.s),new AbortController().signal)).toBeNull(); expect(old.inspect).not.toHaveBeenCalled(); }
});
it("navigation aborts thumbnail decoding without creating background jobs", async () => {
  const f=fixture(); vi.mocked(sourceThumbnail).mockImplementationOnce((_url,_kind,signal)=>new Promise((_resolve,reject)=>signal.addEventListener("abort",()=>reject(signal.reason),{once:true})));
  const abort=new AbortController(); const result=f.api.covers.request(projectRef(f.s),abort.signal).catch(e=>e);
  await vi.waitFor(()=>expect(f.resource).toHaveBeenCalled()); await Promise.resolve(); abort.abort("navigation");
  expect(await result).toBe("navigation"); await f.api.covers.quiesce(["p"]); expect(f.submit).not.toHaveBeenCalled(); expect(f.cancel).not.toHaveBeenCalled();
});
it("project deletion waits for owned-cover quiescence and does not cancel unrelated jobs", async () => {
  const { api, s, cancel } = fixture();
  vi.spyOn(api.sdk, "hostCapabilities").mockResolvedValue({} as never);
  vi.spyOn(api, "authenticateConnection").mockResolvedValue(undefined);
  vi.spyOn(api, "projects").mockResolvedValue([projectRef(s)]);
  const controller = new WorkspaceController(() => api);
  await controller.connect("http://localhost:8788", "test");
  const call = vi.spyOn(api.sdk, "call").mockResolvedValue({
    receipt: { projects: [] },
    cleanup_pending: [],
  } as never);
  let resolve!: () => void;
  vi.spyOn(api.covers, "quiesce").mockImplementation(
    () => new Promise((r) => (resolve = r)),
  );
  const pending = controller.deleteProjects(
    [projectRef(s)],
    "fixed-deletion-key",
  );
  expect(call).not.toHaveBeenCalled();
  resolve();
  await pending;
  expect(call).toHaveBeenCalledWith("opencut_project_delete", {
    idempotency_key: "fixed-deletion-key",
    projects: [{ project_id: "p", expected_revision: "fixed" }],
  });
  expect(cancel).not.toHaveBeenCalled();
});

const rejection = (code: string) =>
  new OpenCutApiError(code, code, null, 409, "request", []);
async function deletionFixture() {
  const { api, s, cancel } = fixture();
  vi.spyOn(api.sdk, "hostCapabilities").mockResolvedValue({} as never);
  vi.spyOn(api, "authenticateConnection").mockResolvedValue(undefined);
  vi.spyOn(api, "projects").mockResolvedValue([projectRef(s)]);
  const controller = new WorkspaceController(() => api);
  await controller.connect("http://localhost:8788", "test");
  const call = vi.spyOn(api.sdk, "call");
  return { api, s, cancel, controller, call };
}
it("a definitive busy deletion retries the identical fixed request without cancelling user jobs", async () => {
  const { s, cancel, controller, call } = await deletionFixture();
  call.mockRejectedValueOnce(rejection("PROJECT_BUSY")).mockResolvedValue({
    receipt: { projects: [] },
    cleanup_pending: [],
  } as never);
  await controller.deleteProjects([projectRef(s)], "same-key");
  expect(call).toHaveBeenCalledTimes(2);
  expect(call.mock.calls[1]).toEqual(call.mock.calls[0]);
  expect(call.mock.calls[0][1]).toEqual({
    idempotency_key: "same-key",
    projects: [{ project_id: "p", expected_revision: "fixed" }],
  });
  expect(cancel).not.toHaveBeenCalled();
});
it("deletion does not retry revision conflicts or unknown transport outcomes", async () => {
  for (const error of [
    rejection("REVISION_CONFLICT"),
    new TypeError("network"),
  ]) {
    const { s, controller, call } = await deletionFixture();
    call.mockRejectedValue(error);
    await expect(
      controller.deleteProjects([projectRef(s)], "key"),
    ).rejects.toBe(error);
    expect(call).toHaveBeenCalledTimes(1);
  }
});
it("persistent project busy stops after ten seconds and releases the cover hold", async () => {
  const { api, s, controller, call } = await deletionFixture();
  const busy = rejection("PROJECT_BUSY");
  call.mockRejectedValue(busy);
  vi.useFakeTimers();
  try {
    const pending = controller
      .deleteProjects([projectRef(s)], "key")
      .catch((e) => e);
    await vi.advanceTimersByTimeAsync(10000);
    expect(await pending).toBe(busy);
    expect(call).toHaveBeenCalledTimes(51);
    expect(
      await api.covers.request(projectRef(s), new AbortController().signal),
    ).toBeInstanceOf(Blob);
  } finally {
    vi.useRealTimers();
  }
});
