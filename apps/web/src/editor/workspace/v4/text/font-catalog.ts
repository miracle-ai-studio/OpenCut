import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { FontSourceMetadata } from "./font-derived";
import { derivedSourceMetadata } from "./font-derived";
import { synthesisSourceMetadata } from "./font-synthesis";

export type UnifiedFontCatalog = ToolOutput<"opencut_font_catalog_v3">;
export type UnifiedFontFace = UnifiedFontCatalog["fonts"][number];
export type UnifiedFontSource = ToolOutput<"opencut_font_source_v3">;
export type UnifiedFontSourceReader = (
  request: ToolInput<"opencut_font_source_v3">,
  signal: AbortSignal,
) => Promise<UnifiedFontSource>;
export type FontImportProtocol = "derived" | "static_v3" | "synthesis";

/** Display names belong to discovery. Native family, bytes and proof must all
 * remain the exact source selected by the versioned catalog. */
export function unifiedSourceMetadata(
  value: UnifiedFontSource,
  expected: UnifiedFontFace,
): FontSourceMetadata {
  const face = value.face;
  if (
    (
      [
        "id",
        "display_family",
        "family",
        "weight",
        "style",
        "requested_style",
        "kind",
        "content_hash",
        "size_bytes",
        "render_profile",
        "alias_reason",
      ] as const
    ).some((key) => face[key] !== expected[key]) ||
    value.details.kind !== expected.kind
  )
    throw new Error("The font source differs from the selected catalog face.");
  if (value.details.kind === "synthesis_v1") {
    const metadata = synthesisSourceMetadata(value.details.source);
    if (
      expected.kind !== "synthesis_v1" ||
      expected.requested_style !== metadata.requested_style ||
      metadata.id !== face.id ||
      metadata.family !== face.family ||
      metadata.weight !== face.weight ||
      metadata.style !== face.style ||
      metadata.content_hash !== face.content_hash ||
      metadata.size_bytes !== face.size_bytes ||
      metadata.render_profile !== face.render_profile ||
      (metadata.synthesis_base &&
        metadata.synthesis_base.display_family !== face.display_family) ||
      metadata.license_url !== value.license_url ||
      metadata.license_sha256 !== value.license_sha256
    )
      throw new Error(
        "The font source differs from the selected catalog face.",
      );
    return metadata;
  }
  const source = value.details.source;
  const derived = "face" in source;
  if (
    derived !==
    (expected.kind === "derived_v1" || expected.kind === "derived_v2")
  )
    throw new Error("The font source differs from the selected catalog face.");
  const metadata = "face" in source ? derivedSourceMetadata(source) : source;
  if (
    metadata.id !== face.id ||
    metadata.family !== face.family ||
    metadata.weight !== face.weight ||
    metadata.style !== face.style ||
    metadata.content_hash !== face.content_hash ||
    metadata.size_bytes !== face.size_bytes ||
    !metadata.license_notice
  )
    throw new Error("The font source differs from the selected catalog face.");
  if (
    !/^https:\/\//u.test(value.license_url) ||
    !/^[a-f0-9]{64}$/u.test(value.license_sha256)
  )
    throw new Error("The font source differs from the selected catalog face.");
  return {
    ...metadata,
    render_profile: face.render_profile,
    license_url: value.license_url,
    license_sha256: value.license_sha256,
  };
}
