import { describe, expect, it } from "vitest";
import { editorProjectId, editorProjectPath } from "./routes";
describe("Agent project routes", () => {
  it("round trips non UUID and encoded application IDs", () => {
    for (const id of ["speech", "剪辑 / 100%", "project?#one", "asset%2Fraw"])
      expect(editorProjectId(editorProjectPath(id))).toBe(id);
  });
  it("rejects malformed segments without crashing navigation", () => {
    for (const path of [
      "/projects",
      "/editor/",
      "/editor/a/b",
      "/editor/%",
      "/editor/%ff",
      "/editor/%00",
      "/editor/%2e%2e",
    ])
      expect(editorProjectId(path)).toBeNull();
  });
});
