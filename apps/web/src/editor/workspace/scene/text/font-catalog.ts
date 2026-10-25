import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { FontSourceMetadata } from "./font-derived";
import { derivedSourceMetadata } from "./font-derived";
import { synthesisSourceMetadata } from "./font-synthesis";
export type UnifiedFontCatalog = ToolOutput<"opencut_font_catalog">;
export type UnifiedFontFace = UnifiedFontCatalog["fonts"][number];
export type UnifiedFontSource = ToolOutput<"opencut_font_source">;
export type DistributionFontSource = Extract<
  UnifiedFontSource["details"],
  { kind: "distribution" }
>["source"];
export type UnifiedFontSourceReader = (
  request: ToolInput<"opencut_font_source">,
  signal: AbortSignal,
) => Promise<UnifiedFontSource>;
/** Native identity, requested typography and license facts remain separate. */
export function unifiedSourceMetadata(
  value: UnifiedFontSource,
  expected: UnifiedFontFace,
): FontSourceMetadata {
  const differs = () =>
    new Error("The font source differs from the selected catalog face.");
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
    value.details.kind !== expected.kind ||
    !/^https:\/\//u.test(value.license_url) ||
    !/^[a-f0-9]{64}$/u.test(value.license_sha256)
  )
    throw differs();
  let metadata: FontSourceMetadata;
  switch (value.details.kind) {
    case "synthesis": {
      metadata = synthesisSourceMetadata(value.details.source);
      if (
        metadata.requested_style !== face.requested_style ||
        metadata.render_profile !== face.render_profile ||
        metadata.license_url !== value.license_url ||
        metadata.license_sha256 !== value.license_sha256 ||
        (metadata.synthesis_base &&
          metadata.synthesis_base.display_family !== face.display_family)
      )
        throw differs();
      break;
    }
    case "distribution": {
      const source = value.details.source;
      if (
        source.face.id !== face.id ||
        source.face.content_hash !== face.content_hash ||
        source.face.size_bytes !== face.size_bytes ||
        !source.provenance.source_url.startsWith(
          "https://fonts.gstatic.com/s/",
        ) ||
        !source.provenance.css_url.startsWith(
          "https://fonts.googleapis.com/css2?",
        ) ||
        !/^[a-f0-9]{64}$/u.test(source.provenance.css_sha256) ||
        !source.provenance.release ||
        source.license_url !== value.license_url ||
        source.license_sha256 !== value.license_sha256
      )
        throw differs();
      metadata = {
        id: face.id,
        family: face.family,
        weight: face.weight,
        style: face.style,
        content_hash: face.content_hash,
        size_bytes: face.size_bytes,
        source_url: source.provenance.source_url,
        license_id: source.license_id,
        license_notice: source.license_notice,
        distribution: source,
      };
      break;
    }
    case "derived": {
      const source = value.details.source;
      metadata = derivedSourceMetadata(source);
      const proof = JSON.parse(source.proof_json);
      // This counter identifies the retained proof format, not a public API route.
      if (
        proof.schema_version === 4 &&
        (proof.kind !== "complete_static_colr" ||
          proof.source.source_family !== face.display_family ||
          proof.output_sha256 !== face.content_hash ||
          String(proof.output_size) !== face.size_bytes ||
          proof.derivation_id !== source.face.derivation_id ||
          proof.source.license_sha256 !== value.license_sha256 ||
          proof.color_instantiation.removed_var_store !== true ||
          proof.color_instantiation.removed_var_index_map !== true)
      )
        throw differs();
      break;
    }
    case "static":
      metadata = value.details.source;
      break;
  }
  if (
    metadata.id !== face.id ||
    metadata.family !== face.family ||
    metadata.weight !== face.weight ||
    metadata.style !== face.style ||
    metadata.content_hash !== face.content_hash ||
    metadata.size_bytes !== face.size_bytes ||
    !metadata.license_notice
  )
    throw differs();
  return {
    ...metadata,
    render_profile: face.render_profile,
    license_url: value.license_url,
    license_sha256: value.license_sha256,
  };
}
