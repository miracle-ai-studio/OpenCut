import { describe, expect, it } from "vitest";
import { editorProjectId, editorProjectPath, isEditorPath } from "./routes";
describe("Agent project routes", () => {
  it("round trips non UUID and encoded application IDs", () => {
    expect(isEditorPath("/")).toBe(true);
    expect(isEditorPath("/projects")).toBe(true);
    expect(isEditorPath("/blog")).toBe(false);
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
