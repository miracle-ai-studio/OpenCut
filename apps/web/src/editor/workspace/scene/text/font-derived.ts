import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { UnifiedFontCatalog, UnifiedFontFace } from "./font-catalog";
import type { EditorApi } from "../../api";
import type { FontBrowserCatalog } from "./font-browser";
import { verifySynthesisFontImport } from "./font-synthesis";
import type {
  FontSynthesisSource,
  FontSynthesisBase,
  FontRenderProfile,
} from "./font-synthesis";
export type DerivedFontSource = Extract<
  ToolOutput<"opencut_font_source">["details"],
  { kind: "derived" }
>["source"];
export type DerivedFontImport = ToolInput<"opencut_font_import">;
export type DerivedSourceReader = (
  id: string,
  signal: AbortSignal,
) => Promise<DerivedFontSource>;
type ResolvedFontFace =
  FontBrowserCatalog["families"][number]["faces"][number] & {
    unified?: UnifiedFontFace;
  };
export type MergedFontFace = ResolvedFontFace & {
  realItalic?: ResolvedFontFace;
};
function classicFacesWithSynthesis(
  family: FontBrowserCatalog["families"][number],
  unified?: UnifiedFontCatalog | null,
) {
  const faces = [...family.faces];
  for (const weight of [400, 700]) {
    const normal = faces.find(
      (face) => face.weight === weight && face.style === "normal",
    );
    if (
      normal &&
      !faces.some(
        (face) => face.weight === weight && face.style === "italic",
      ) &&
      unified?.fonts.some(
        (face) =>
          face.kind === "synthesis" &&
          face.display_family === family.family &&
          face.weight === weight &&
          face.style === "normal" &&
          face.requested_style === "italic",
      )
    )
      faces.push({
        ...normal,
        style: "italic",
        catalog_id: null,
        unavailable_reason: null,
      });
  }
  return faces;
}
/** Classic loaded 400/700 normal sources and requested italic from Canvas.
 * Prefer the explicit synthesis binding for italic slots while retaining any
 * real italic face as a separately selectable source. */
export function mergeFontCatalog(catalog: UnifiedFontCatalog) {
  return catalog.families.map((family) => ({
    ...family,
    faces: classicFacesWithSynthesis(family, catalog).map(
      (face): MergedFontFace => {
        const matching = catalog.fonts.filter(
          (item) =>
            item.kind !== "synthesis" &&
            item.display_family === family.family &&
            item.weight === face.weight &&
            (item.requested_style ?? item.style) === face.style &&
            (!face.catalog_id || item.id === face.catalog_id),
        );
        const unifiedFace =
          matching?.find((item) => item.import_available) ?? matching?.[0];
        const resolved: ResolvedFontFace = unifiedFace
          ? {
              ...face,
              catalog_id: unifiedFace.id,
              unavailable_reason: null,
              unified: unifiedFace,
            }
          : { ...face, unified: undefined };
        const synthesis =
          face.style === "italic" &&
          (face.weight === 400 || face.weight === 700)
            ? (() => {
                const matches = catalog.fonts.filter(
                  (item) =>
                    item.kind === "synthesis" &&
                    item.display_family === family.family &&
                    item.weight === face.weight &&
                    item.style === "normal" &&
                    item.requested_style === "italic",
                );
                return (
                  matches?.find((item) => item.import_available) ?? matches?.[0]
                );
              })()
            : undefined;
        return synthesis
          ? {
              ...face,
              catalog_id: synthesis.id,
              unavailable_reason: null,
              unified: synthesis,
              realItalic:
                resolved.catalog_id && resolved.unified?.kind !== "synthesis"
                  ? resolved
                  : undefined,
            }
          : resolved;
      },
    ),
  }));
}
export type FontSourceMetadata = Omit<
  Extract<
    ToolOutput<"opencut_font_source">["details"],
    { kind: "static" }
  >["source"],
  "source_commit"
> & {
  source_commit?: string;
  distribution?: import("./font-catalog").DistributionFontSource;
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
export async function verifyFontImport(
  api: EditorApi,
  request: DerivedFontImport,
  revision: string,
) {
  const selected = await api.sdk.call("opencut_font_source", {
    catalog_id: request.catalog_id,
  });
  if (selected.face.id !== request.catalog_id)
    throw new Error("The font source differs from its original import.");
  if (
    selected.details.kind === "static" ||
    selected.details.kind === "distribution"
  )
    return null;
  if (selected.details.kind === "synthesis")
    return verifySynthesisFontImport(api, request, revision);
  const [proof, snapshot] = await Promise.all([
    api.sdk.call("opencut_font_derived_proof", {
      project_id: request.project_id,
      revision,
      font_id: request.font_id,
    }),
    api.sdk.inspectEditorProject({
      project_id: request.project_id,
      revision,
    }),
  ]);
  const font = snapshot.project.fonts.find(
      (item) => item.id === request.font_id,
    ),
    source = proof.source;
  const retained = JSON.parse(source.proof_json);
  if (
    proof.project_id !== request.project_id ||
    proof.revision !== revision ||
    proof.font_id !== request.font_id ||
    snapshot.project.id !== request.project_id ||
    snapshot.revision !== revision ||
    !font ||
    source.face.id !== request.catalog_id ||
    font.origin.kind !== "generated" ||
    font.origin.provider !== retained.provider ||
    font.origin.generation_id !== source.face.derivation_id ||
    font.content.content_hash !== source.face.content_hash ||
    String(font.content.size) !== source.face.size_bytes ||
    font.family !== source.face.family ||
    font.weight !== source.face.weight ||
    font.style !== source.face.style ||
    !source.license_notice ||
    JSON.stringify(source) !== JSON.stringify(selected.details.source)
  )
    throw new Error(
      "The derived font proof differs from the published project font.",
    );
  return proof;
}
