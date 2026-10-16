import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { UnifiedFontCatalog, UnifiedFontFace } from "./font-catalog";
import type { EditorApi } from "../../api";
import type { FontBrowserCatalog, FontBrowserSource } from "./font-browser";
import { verifySynthesisFontImport } from "./font-synthesis";
import type {
  FontSynthesisSource,
  FontSynthesisBase,
  FontRenderProfile,
} from "./font-synthesis";
export type DerivedFontCatalog = ToolOutput<"opencut_font_derived_v1_catalog">;
export type DerivedFontSource = ToolOutput<"opencut_font_derived_v1_source">;
export type DerivedFontImport = ToolInput<"opencut_font_derived_v1_import">;
export type DerivedSourceReader = (
  id: string,
  signal: AbortSignal,
) => Promise<DerivedFontSource>;
type ResolvedFontFace =
  FontBrowserCatalog["families"][number]["faces"][number] & {
    derived?: DerivedFontCatalog["fonts"][number];
    unified?: UnifiedFontFace;
  };
export type MergedFontFace = ResolvedFontFace & {
  realItalic?: ResolvedFontFace;
};
/** Classic loaded 400/700 normal sources and requested italic from Canvas.
 * Prefer the explicit synthesis binding for italic slots while retaining any
 * real italic face as a separately selectable source. */
export function mergeDerivedFontCatalog(
  catalog: FontBrowserCatalog,
  derived?: DerivedFontCatalog | null,
  unified?: UnifiedFontCatalog | null,
) {
  return catalog.families.map((family) => ({
    ...family,
    faces: family.faces.map((face): MergedFontFace => {
      const unifiedFace = unified?.fonts.find(
        (item) =>
          item.kind !== "synthesis_v1" &&
          item.display_family === family.family &&
          item.weight === face.weight &&
          (item.requested_style ?? item.style) === face.style &&
          (!face.catalog_id || item.id === face.catalog_id),
      );
      const resolved: ResolvedFontFace = unifiedFace
        ? {
            ...face,
            catalog_id: unifiedFace.id,
            unavailable_reason: null,
            unified: unifiedFace,
            derived: undefined,
          }
        : (() => {
            const source =
              !face.catalog_id &&
              derived?.fonts.find(
                (item) =>
                  item.family === family.family &&
                  item.weight === face.weight &&
                  item.style === face.style,
              );
            return source
              ? {
                  ...face,
                  catalog_id: source.id,
                  unavailable_reason: null,
                  derived: source,
                  unified: undefined,
                }
              : { ...face, derived: undefined, unified: undefined };
          })();
      const synthesis =
        face.style === "italic" && (face.weight === 400 || face.weight === 700)
          ? unified?.fonts.find(
              (item) =>
                item.kind === "synthesis_v1" &&
                item.display_family === family.family &&
                item.weight === face.weight &&
                item.style === "normal" &&
                item.requested_style === "italic",
            )
          : undefined;
      return synthesis
        ? {
            ...face,
            catalog_id: synthesis.id,
            unavailable_reason: null,
            unified: synthesis,
            derived: undefined,
            realItalic:
              resolved.catalog_id && resolved.unified?.kind !== "synthesis_v1"
                ? resolved
                : undefined,
          }
        : resolved;
    }),
  }));
}
export type FontSourceMetadata = FontBrowserSource & {
  derived?: DerivedFontSource;
  synthesis?: FontSynthesisSource;
  synthesis_base?: FontSynthesisBase;
  render_profile?: FontRenderProfile;
  requested_style?: "normal" | "italic";
  license_url?: string;
  license_sha256?: string;
};
export function derivedSourceMetadata(
  source: DerivedFontSource,
): FontSourceMetadata {
  return {
    id: source.face.id,
    family: source.face.family,
    weight: source.face.weight,
    style: source.face.style,
    source_commit: source.source_commit,
    source_url: source.source_url,
    content_hash: source.face.content_hash,
    size_bytes: source.face.size_bytes,
    license_id: source.license_id,
    license_notice: source.license_notice,
    derived: source,
  };
}
/** Re-read the published fixed revision. The variable input hash cannot identify
 * the static output stored as a project font. */
export async function verifyDerivedFontImport(
  api: EditorApi,
  request: DerivedFontImport,
  revision: string,
) {
  if (request.catalog_id.endsWith("-synthetic-italic-v1"))
    return verifySynthesisFontImport(api, request, revision);
  const v2 = request.catalog_id.endsWith("-derived-v2");
  const [proof, snapshot] = await Promise.all([
    api.sdk.call(
      v2 ? "opencut_font_derived_v2_proof" : "opencut_font_derived_v1_proof",
      {
        project_id: request.project_id,
        revision,
        font_id: request.font_id,
      },
    ),
    api.sdk.call("opencut_project_v4_inspect", {
      project_id: request.project_id,
      revision,
    }),
  ]);
  const font = snapshot.project.fonts.find(
      (item) => item.id === request.font_id,
    ),
    source = proof.source;
  if (
    proof.schema_version !== (v2 ? 2 : 1) ||
    proof.project_id !== request.project_id ||
    proof.revision !== revision ||
    proof.font_id !== request.font_id ||
    snapshot.revision !== revision ||
    snapshot.project.id !== request.project_id ||
    !font ||
    source.face.id !== request.catalog_id ||
    font.origin.kind !== "generated" ||
    font.origin.provider !==
      (v2
        ? "opencut-fonttools-static-renamed-v2"
        : "opencut-fonttools-static-v1") ||
    font.origin.generation_id !== source.face.derivation_id ||
    font.content.content_hash !== source.face.content_hash ||
    String(font.content.size) !== source.face.size_bytes ||
    font.family !== source.face.family ||
    font.weight !== source.face.weight ||
    font.style !== source.face.style ||
    !source.license_notice ||
    !source.proof_json
  )
    throw new Error(
      "The derived font proof differs from the published project font.",
    );
  return proof;
}
