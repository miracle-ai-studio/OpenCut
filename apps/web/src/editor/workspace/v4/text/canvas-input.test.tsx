// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import { CanvasTextInput } from "./canvas-input";
import type { CanvasTextPlan } from "./canvas-types";
import type { EditorApi } from "../../api";
import { preferences } from "../../../shared/locale";
const bytes = new Uint8Array([1, 2, 3]),
  hash = bytesToHex(sha256(bytes));
const removed = vi.fn(),
  added = vi.fn();
beforeEach(() => {
  preferences({ language: "en" });
  vi.stubGlobal(
    "FontFace",
    class {
      async load() {
        return this;
      }
    },
  );
  Object.defineProperty(document, "fonts", {
    configurable: true,
    value: { add: added, delete: removed },
  });
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});
async function setup() {
  const commit = vi.fn(async () => {}),
    cancel = vi.fn(),
    shortcut = vi.fn();
  const plan = {
    scope: { projectId: "p", revision: "r", sequenceId: "s", elementId: "e" },
    width: 320,
    height: 180,
    block: { maxWidth: 80, height: 36 },
    layer: {
      elementId: "e",
      opacity: 1,
      font: {
        fontId: "f",
        contentHash: hash,
        size: "3",
        weight: 400,
        style: "normal",
        faceIndex: 0,
        glyphCoverageValidated: true,
      },
      text: {
        content: "你好",
        align: "center",
        fontSizePx: 30,
        lineHeightPx: 36,
        letterSpacingPx: 0,
        color: [1, 1, 1, 1],
        transform: {
          centerX: 160,
          centerY: 90,
          scaleX: 2,
          scaleY: 0.5,
          rotationRadians: Math.PI / 2,
        },
      },
    },
  } as CanvasTextPlan;
  const sdk = {
    call: vi.fn(async () => ({ version: { content_hash: hash, size: 3 } })),
    downloadProjectV4Resource: vi.fn(async () => new Response(bytes)),
  };
  render(
    <div onKeyDown={shortcut}>
      <CanvasTextInput
        plan={plan}
        api={{ sdk } as unknown as EditorApi}
        viewportWidth={640}
        commit={commit}
        cancel={cancel}
      />
    </div>,
  );
  const input = await screen.findByRole("textbox");
  return { input, commit, cancel, shortcut, sdk };
}
it("defers blur until IME ends, retaining line breaks and final composition text", async () => {
  const f = await setup();
  fireEvent.compositionStart(f.input);
  fireEvent.change(f.input, { target: { value: "中文\n编" } });
  fireEvent.blur(f.input);
  expect(f.commit).not.toHaveBeenCalled();
  fireEvent.compositionEnd(f.input, { target: { value: "中文\n编辑" } });
  await waitFor(() => expect(f.commit).toHaveBeenCalledWith("中文\n编辑"));
  expect(f.commit).toHaveBeenCalledTimes(1);
});
it("Escape discards during composition and stops timeline shortcuts", async () => {
  const f = await setup();
  fireEvent.compositionStart(f.input);
  fireEvent.change(f.input, { target: { value: "草稿" } });
  fireEvent.keyDown(f.input, { key: "Escape" });
  fireEvent.compositionEnd(f.input);
  fireEvent.blur(f.input);
  expect(f.cancel).toHaveBeenCalledTimes(1);
  expect(f.commit).not.toHaveBeenCalled();
  expect(f.shortcut).not.toHaveBeenCalled();
});
it("allows native selection, undo and newlines without forwarding timeline keys", async () => {
  const f = await setup();
  for (const key of ["ArrowLeft", "Backspace", " ", "z", "Enter"])
    fireEvent.keyDown(f.input, { key, metaKey: key === "z" });
  expect(f.shortcut).not.toHaveBeenCalled();
  expect(f.commit).not.toHaveBeenCalled();
  fireEvent.change(f.input, { target: { value: "AV To\n中文编辑" } });
  fireEvent.click(screen.getByRole("button", { name: "Confirm text" }));
  await waitFor(() => expect(f.commit).toHaveBeenCalledWith("AV To\n中文编辑"));
});
it("authorizes the fixed revision font and releases its isolated face", async () => {
  const f = await setup();
  expect(f.sdk.call).toHaveBeenCalledWith(
    "opencut_project_v4_resource_inspect",
    {
      project_id: "p",
      revision: "r",
      kind: "font",
      resource_id: "f",
    },
    expect.objectContaining({ signal: expect.any(AbortSignal) }),
  );
  expect(added).toHaveBeenCalledTimes(1);
  cleanup();
  expect(removed).toHaveBeenCalledTimes(1);
});
