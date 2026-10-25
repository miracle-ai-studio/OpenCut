// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { OpenCutClient } from "../../../../lib/editor-sdk";
import type { EditorApi } from "../../api";
import { canvasTextPlanner } from "./canvas-plan";
import { synthesisFixture } from "./font-synthesis-fixture";
import { measureClassicText } from "../visual-preview-renderer";
vi.mock("../visual-preview-renderer", () => ({
  measureClassicText: vi.fn(() => ({ block: { maxWidth: 32, height: 20 } })),
}));
const added = vi.fn(),
  removed = vi.fn(),
  descriptors: FontFaceDescriptors[] = [];
beforeEach(() => {
  descriptors.length = 0;
  vi.stubGlobal(
    "FontFace",
    class {
      constructor(
        _name: string,
        _bytes: ArrayBuffer,
        descriptor: FontFaceDescriptors,
      ) {
        descriptors.push(descriptor);
      }
      async load() {
        return this;
      }
    },
  );
  Object.defineProperty(document, "fonts", {
    configurable: true,
    value: { add: added, delete: removed },
  });
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
    {} as never,
  );
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});
function fixture() {
  const f = synthesisFixture(),
    scope = { projectId: "p", revision: "r", sequenceId: "s", elementId: "e" },
    time = { ticks: "0", timebase: 1 };
  const result = {
    identity: {
      projectId: "p",
      revision: "r",
      sequenceId: "s",
      time,
      width: 320,
      height: 180,
      profile: "classic-canvas-text",
    },
    layer: {
      elementId: "e",
      font: {
        fontId: f.font.id,
        contentHash: f.hash,
        size: "3",
        weight: 700,
        style: "normal",
        faceIndex: 0,
        glyphCoverageValidated: true,
      },
      text: { content: "AV" },
    },
    font_synthesis: f.source.binding,
  };
  const call = vi.fn(async (name: string) => {
    if (name === "opencut_project_inspect")
      return { kind: "editor", snapshot: { revision: "r", project: { id: "p", fonts: [f.font] } } };
    if (name === "opencut_text_canvas_plan") return result;
    if (name === "opencut_font_synthesis_proof")
      return {
        
        project_id: "p",
        revision: "r",
        font_id: f.font.id,
        source: f.source,
      };
    if (name === "opencut_project_resource_inspect")
      return { version: { content_hash: f.hash, size: 3 } };
    throw new Error(`Unexpected ${name}`);
  });
  const api = {
    connectionIdentity: "connection",
    sdk: {
      call,
      inspectEditorProject: OpenCutClient.prototype.inspectEditorProject,
      hostCapabilities: vi.fn(async () => ({
        tools: ["opencut_text_canvas_plan"],
      })),
      downloadProjectResource: vi.fn(async () => new Response(f.bytes)),
    },
  } as unknown as EditorApi;
  return { f, scope, time, result, api, call };
}
it("uses actual FontFace metadata and requested italic only for Canvas measurement/input", async () => {
  const f = fixture(),
    plan = await canvasTextPlanner(f.api)(
      f.scope,
      f.time,
      new AbortController().signal,
    );
  expect(descriptors[0].style).toBe("normal");
  expect(plan.actualFont?.style).toBe("normal");
  expect(plan.layer.font.style).toBe("italic");
  expect(f.result.layer.font.style).toBe("normal");
  expect(vi.mocked(measureClassicText).mock.calls[0][3].style).toBe("italic");
  expect(removed).toHaveBeenCalledTimes(1);
  expect(f.call.mock.calls.map(([name]) => name)).toContain("opencut_text_canvas_plan");
});
it("refuses an omitted synthesis binding from the sole Canvas interface", async () => {
  const g = fixture();
  g.result.font_synthesis = null as never;
  await expect(
    canvasTextPlanner(g.api)(g.scope, g.time, new AbortController().signal),
  ).rejects.toThrow("omitted");
  expect(descriptors).toHaveLength(0);
});
