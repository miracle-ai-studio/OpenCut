import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { EditorApi } from "../../api";
import type { FontBrowserCatalog, FontBrowserSource } from "./font-browser";
export type DerivedFontCatalog = ToolOutput<"opencut_font_derived_v1_catalog">;
export type DerivedFontSource = ToolOutput<"opencut_font_derived_v1_source">;
export type DerivedFontImport = ToolInput<"opencut_font_derived_v1_import">;
export type DerivedSourceReader = (
  id: string,
  signal: AbortSignal,
) => Promise<DerivedFontSource>;
/** UI-only overlay. Static faces always win; metadata remains in the admitted atlas. */
export function mergeDerivedFontCatalog(
  catalog: FontBrowserCatalog,
  derived?: DerivedFontCatalog | null,
) {
  return catalog.families.map((family) => ({
    ...family,
    faces: family.faces.map((face) => {
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
          }
        : { ...face, derived: undefined };
    }),
  }));
}
export type FontSourceMetadata = FontBrowserSource & {
  derived?: DerivedFontSource;
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
  const [proof, snapshot] = await Promise.all([
    api.sdk.call("opencut_font_derived_v1_proof", {
      project_id: request.project_id,
      revision,
      font_id: request.font_id,
    }),
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
    proof.schema_version !== 1 ||
    proof.project_id !== request.project_id ||
    proof.revision !== revision ||
    proof.font_id !== request.font_id ||
    snapshot.revision !== revision ||
    snapshot.project.id !== request.project_id ||
    !font ||
    source.face.id !== request.catalog_id ||
    font.origin.kind !== "generated" ||
    font.origin.provider !== "opencut-fonttools-static-v1" ||
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
