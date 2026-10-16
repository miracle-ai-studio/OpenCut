import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import type { EditorApi } from "../../api";
import { readPreviewBytes } from "../visual-preview-bytes";
import type { CanvasTextLayer } from "./canvas-types";
import type { V4TextScope } from "./types";

/** Scoped authorization is repeated for each edit, never inferred from a
 * previously loaded face. This volatile face cannot outlive its owner view. */
export async function loadCanvasFont(
  api: EditorApi,
  scope: V4TextScope,
  font: CanvasTextLayer["font"],
  signal: AbortSignal,
  actualFont: CanvasTextLayer["font"] = font,
) {
  const connection = api.connectionIdentity;
  const checkConnection = () => {
    signal.throwIfAborted();
    if (api.connectionIdentity !== connection)
      throw new Error("The locked font identity changed.");
  };
  if (
    font.fontId !== actualFont.fontId ||
    font.contentHash !== actualFont.contentHash ||
    font.size !== actualFont.size ||
    font.weight !== actualFont.weight
  )
    throw new Error("The locked font identity changed.");
  if (!font.glyphCoverageValidated || font.faceIndex !== 0)
    throw new Error("Canvas text editing unavailable");
  const resource = await api.sdk.call(
    "opencut_project_v4_resource_inspect",
    {
      project_id: scope.projectId,
      revision: scope.revision,
      kind: "font",
      resource_id: font.fontId,
    },
    { signal },
  );
  if (
    resource.version.content_hash !== font.contentHash ||
    String(resource.version.size) !== font.size
  )
    throw new Error("The locked font identity changed.");
  const response = await api.sdk.downloadProjectV4Resource(resource, {
    signal,
  });
  const bytes = await readPreviewBytes(response, font.size, signal);
  checkConnection();
  if (bytesToHex(sha256(bytes)) !== font.contentHash)
    throw new Error("The locked font identity changed.");
  const family = `OpenCutCanvas_${font.contentHash}`;
  const face = new FontFace(family, bytes.slice().buffer as ArrayBuffer, {
    weight: String(actualFont.weight),
    style: actualFont.style,
  });
  await face.load();
  checkConnection();
  document.fonts.add(face);
  return {
    family,
    release() {
      document.fonts.delete(face);
    },
  };
}
