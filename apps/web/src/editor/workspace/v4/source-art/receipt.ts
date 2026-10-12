import type { ToolOutput } from "../../../../lib/editor-sdk";
import type { V4Asset } from "../types";

export const canonicalArt = (value: unknown): string => JSON.stringify(ordered(value));
function ordered(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(ordered);
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)).map(([key, child]) => [key, ordered(child)]));
  return value;
}
function invalid(): never { throw new Error("The service returned invalid source artwork."); }
const hash = (value: unknown) => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);

export function validateArtScope(response: Pick<ToolOutput<"opencut_source_art_v1_get">, "schema_version" | "project_id" | "revision" | "asset_id" | "version" | "identity">,
  selected: { projectId: string; revision: string; asset: V4Asset }) {
  if (!response || response.schema_version !== 1 || response.project_id !== selected.projectId ||
      response.revision !== selected.revision || response.asset_id !== selected.asset.id ||
      canonicalArt(response.version) !== canonicalArt(selected.asset.version) || !response.identity ||
      !response.identity.browser_version || !hash(response.identity.browser_sha256) ||
      !hash(response.identity.node_sha256) || !hash(response.identity.module_sha256)) invalid();
}
