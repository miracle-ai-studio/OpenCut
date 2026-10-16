import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { EditorApi } from "../../api";
import type { V4TextFont, V4TextScope } from "./types";
import type { FontSourceMetadata } from "./font-derived";

export const FONT_SYNTHESIS_PROVIDER_V1 = "opencut-chromium-font-synthesis-v1";
const chinese: Record<string, string> = {
  "Classic Canvas italic synthesis": "Classic Canvas 斜体合成",
  "Actual font": "实际字体",
  "Requested style": "请求样式",
  "Original font bytes and license are preserved.":
    "保留原始字体字节与完整许可。",
  "Italic source": "斜体来源",
  "Real italic font": "真实斜体字体",
  "Synthesis preserves the base font bytes and full license. The base source may have been derived.":
    "合成阶段保留基础字体字节与完整许可；基础字体本身可能已经过派生。",
  "Base acquisition": "基础字体获取方式",
  "Base catalog identity": "基础字体目录标识",
  "Classic family": "Classic 字体族",
  "Native rendering profile": "原生渲染类型",
  "Static outlines": "静态轮廓",
  "COLR v0 color font": "COLR v0 彩色字体",
  "COLR v1 color font": "COLR v1 彩色字体",
  "Monochrome bitmap font": "单色位图字体",
  "Pinned static V1": "固定静态字体 V1",
  "Pinned static V3": "固定静态字体 V3",
  "Derived V1": "派生字体 V1",
  "Derived V2": "派生字体 V2",
  "Base derivation identity": "基础字体派生标识",
  "Upstream source hash": "上游源文件哈希",
  "Synthesis input hash": "合成输入文件哈希",
  "Base acquisition proof": "基础字体获取证明",
  "Upstream source": "上游来源",
  "Pinned font sources": "固定字体来源",
  "Font synthesis proof": "字体合成证明",
  "Reading font synthesis proof…": "正在读取字体合成证明…",
};
export const fontSynthesisText = (message: string, language: string) =>
  language === "zh" ? (chinese[message] ?? message) : message;
export type FontSynthesisSource =
  ToolOutput<"opencut_font_synthesis_v1_source">;
export type FontSynthesisBinding = FontSynthesisSource["binding"];
export type FontRenderProfile =
  ToolOutput<"opencut_font_catalog_v3">["fonts"][number]["render_profile"];
type BaseIdentity = {
  catalog_id: string;
  display_family: string;
  render_profile: FontRenderProfile;
};
export type FontSynthesisBase = BaseIdentity &
  (
    | {
        route: "static_v1" | "static_v3";
        source: {
          face: FontSynthesisBinding["actual"];
          license_url: string;
          license_sha256: string;
          attribution: string | null;
        };
      }
    | {
        route: "derived_v1" | "derived_v2";
        source: ToolOutput<"opencut_font_derived_v1_source">;
      }
  );
export const fontRenderProfileLabel = (profile: FontRenderProfile) =>
  ({
    static_outline: "Static outlines",
    colr_v0: "COLR v0 color font",
    colr_v1: "COLR v1 color font",
    monochrome_bitmap: "Monochrome bitmap font",
  })[profile];
export const synthesisBaseRouteLabel = (route: FontSynthesisBase["route"]) =>
  ({
    static_v1: "Pinned static V1",
    static_v3: "Pinned static V3",
    derived_v1: "Derived V1",
    derived_v2: "Derived V2",
  })[route];
const differs = () =>
  new Error(
    "The font synthesis binding differs from its fixed actual source and requested style.",
  );
function canonical(value: unknown): string {
  const sorted = (item: unknown): unknown =>
    Array.isArray(item)
      ? item.map(sorted)
      : item && typeof item === "object"
        ? Object.fromEntries(
            Object.entries(item)
              .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
              .map(([k, v]) => [k, sorted(v)]),
          )
        : item;
  return JSON.stringify(sorted(value));
}
export const synthesisBindingsEqual = (
  left: FontSynthesisBinding,
  right: FontSynthesisBinding,
) => canonical(left) === canonical(right);
export function synthesisSourceMetadata(
  pin: FontSynthesisSource,
): FontSourceMetadata {
  const { source, binding } = pin,
    face = source.face,
    actual = binding.actual;
  const proof = JSON.parse(source.proof_json) as Record<string, unknown>;
  const { derivation_id, ...original } = proof;
  const base = proof.base as FontSynthesisBase | undefined;
  if (base !== undefined) {
    if (
      !base ||
      !["static_v1", "static_v3", "derived_v1", "derived_v2"].includes(
        base.route,
      ) ||
      base.catalog_id !== actual.id ||
      typeof base.display_family !== "string" ||
      !base.display_family.trim() ||
      base.display_family.length > 1024 ||
      !["static_outline", "colr_v0", "colr_v1", "monochrome_bitmap"].includes(
        base.render_profile,
      ) ||
      !base.source ||
      !base.source.face
    )
      throw differs();
    if (base.route === "static_v1" || base.route === "static_v3") {
      if (
        canonical(base.source.face) !== canonical(actual) ||
        base.source.license_url !== source.license_url ||
        base.source.license_sha256 !== proof.license_sha256 ||
        base.source.attribution !== proof.attribution
      )
        throw differs();
    } else {
      const derived = base.source;
      // The wire route does not discriminate the source union; a derived route must carry derived source facts.
      if (!("source_url" in derived)) throw differs();
      const face = derived.face;
      if (
        face.id !== actual.id ||
        (
          ["family", "weight", "style", "content_hash", "size_bytes"] as const
        ).some((key) => face[key] !== actual[key]) ||
        derived.source_url !== source.source_url ||
        derived.source_commit !== source.source_commit ||
        derived.license_id !== source.license_id ||
        derived.license_notice !== source.license_notice ||
        derived.license_url !== source.license_url ||
        !/^[a-f0-9]{64}$/.test(derived.source_sha256) ||
        !/^\d+$/.test(derived.source_size_bytes) ||
        !Array.isArray(derived.axes) ||
        typeof derived.proof_json !== "string" ||
        !derived.proof_json ||
        typeof face.derivation_id !== "string" ||
        !face.derivation_id.startsWith(
          base.route === "derived_v2" ? "font-derived-v2_" : "font-derived_",
        ) ||
        proof.attribution !== null
      )
        throw differs();
    }
  }
  if (
    binding.schema_version !== 1 ||
    binding.profile !== "classic_chromium_italic_v1" ||
    binding.requested.style !== "italic" ||
    actual.style !== "normal" ||
    ![400, 700].includes(actual.weight) ||
    binding.requested.weight !== actual.weight ||
    !face.id.endsWith("-synthetic-italic-v1") ||
    face.id === actual.id ||
    source.axes.length !== 0 ||
    face.derivation_id !== binding.binding_id ||
    !binding.binding_id.startsWith("font-synthesis-v1_") ||
    (["family", "weight", "style", "content_hash", "size_bytes"] as const).some(
      (key) => face[key] !== actual[key],
    ) ||
    source.source_commit !== actual.source_commit ||
    source.source_url !== actual.source_url ||
    source.source_sha256 !== actual.content_hash ||
    source.source_size_bytes !== actual.size_bytes ||
    source.license_id !== actual.license_id ||
    source.license_notice !== actual.license_notice ||
    !/^https:\/\//u.test(source.source_url) ||
    !/^https:\/\//u.test(source.license_url) ||
    proof.schema_version !== 3 ||
    proof.kind !== "browser_font_synthesis" ||
    proof.provider !== FONT_SYNTHESIS_PROVIDER_V1 ||
    proof.profile !== binding.profile ||
    proof.catalog_id !== face.id ||
    proof.source_bytes_unchanged !== true ||
    canonical(proof.actual) !== canonical(actual) ||
    canonical(proof.requested) !== canonical(binding.requested) ||
    proof.license_url !== source.license_url ||
    derivation_id !== binding.binding_id ||
    proof.license_sha256 !==
      bytesToHex(sha256(new TextEncoder().encode(source.license_notice))) ||
    binding.binding_id !==
      `font-synthesis-v1_${bytesToHex(sha256(new TextEncoder().encode(canonical(original))))}`
  )
    throw differs();
  return {
    id: face.id,
    family: face.family,
    weight: face.weight,
    style: face.style,
    requested_style: binding.requested.style,
    synthesis_base: base,
    render_profile: base?.render_profile ?? "static_outline",
    synthesis: pin,
    source_commit: source.source_commit,
    source_url: source.source_url,
    content_hash: face.content_hash,
    size_bytes: face.size_bytes,
    license_id: source.license_id,
    license_notice: source.license_notice,
    license_url: source.license_url,
    license_sha256: String(proof.license_sha256),
  };
}
export function synthesisFont(font: V4TextFont): boolean {
  return (
    font.origin.kind === "generated" &&
    font.origin.provider === FONT_SYNTHESIS_PROVIDER_V1
  );
}
export const requestedFontStyle = (font: V4TextFont) =>
  font.requested_style ?? font.style;
export const displayFontFamily = (font: V4TextFont) =>
  font.display_family ?? font.family;
/** Preserve an explicit current face. Changing to italic follows Classic's
 * normal-face synthesis when that binding is already proven and imported. */
export function preferredFontVariant(
  font: V4TextFont,
  fonts: readonly V4TextFont[],
  weight: number,
  style: V4TextFont["style"],
): V4TextFont | undefined {
  if (font.weight === weight && requestedFontStyle(font) === style) return font;
  const candidates = fonts.filter(
    (item) =>
      displayFontFamily(item) === displayFontFamily(font) &&
      item.weight === weight &&
      requestedFontStyle(item) === style,
  );
  return (
    (style === "italic" &&
      candidates.find((item) => !!item.synthesis_binding)) ||
    candidates[0]
  );
}

export function checkSynthesisResource(
  font: V4TextFont,
  source: FontSynthesisSource,
): V4TextFont {
  const metadata = synthesisSourceMetadata(source);
  const { face } = source.source;
  const proof = JSON.parse(source.source.proof_json) as {
    attribution: string | null;
  };
  if (
    !synthesisFont(font) ||
    font.origin.kind !== "generated" ||
    font.origin.generation_id !== source.binding.binding_id ||
    font.face_index !== 0 ||
    font.content.id !== `av_${face.content_hash}` ||
    font.content.content_hash !== face.content_hash ||
    String(font.content.size) !== face.size_bytes ||
    font.family !== face.family ||
    font.weight !== face.weight ||
    font.style !== face.style ||
    !font.license ||
    font.license.identifier !== source.source.license_id ||
    font.license.source_url !== source.source.license_url ||
    font.license.notice !== source.source.license_notice ||
    font.license.attribution !== proof.attribution
  )
    throw differs();
  return {
    ...font,
    requested_style: source.binding.requested.style,
    synthesis_binding: source.binding,
    display_family: metadata.synthesis_base?.display_family ?? font.family,
  };
}
export async function enrichSynthesisFonts(
  api: EditorApi,
  scope: Pick<V4TextScope, "projectId" | "revision">,
  fonts: readonly V4TextFont[],
  signal: AbortSignal,
): Promise<V4TextFont[]> {
  const enriched = await Promise.all(
    fonts.map(async (font) => {
      if (!synthesisFont(font)) return font;
      const proof = await api.sdk.call(
        "opencut_font_synthesis_v1_proof",
        {
          project_id: scope.projectId,
          revision: scope.revision,
          font_id: font.id,
        },
        { signal },
      );
      signal.throwIfAborted();
      if (
        proof.schema_version !== 1 ||
        proof.project_id !== scope.projectId ||
        proof.revision !== scope.revision ||
        proof.font_id !== font.id
      )
        throw differs();
      return checkSynthesisResource(font, proof.source);
    }),
  );
  // An ordinary imported base may share exact bytes with a proven synthesis
  // source. Reuse that proof's display label without another network request.
  // Ambiguous aliases remain ungrouped instead of selecting a label arbitrarily.
  const labels = new Map<string, Set<string>>();
  const key = (
    font: Pick<V4TextFont, "family" | "weight" | "style">,
    hash: string,
    size: string,
  ) => JSON.stringify([font.family, font.weight, font.style, hash, size]);
  for (const font of enriched) {
    const actual = font.synthesis_binding?.actual;
    if (!actual || !font.display_family) continue;
    const identity = key(actual, actual.content_hash, actual.size_bytes);
    const names = labels.get(identity) ?? new Set<string>();
    names.add(font.display_family);
    labels.set(identity, names);
  }
  return enriched.map((font) => {
    if (font.synthesis_binding) return font;
    if (font.face_index !== 0) return font;
    const names = labels.get(
      key(font, font.content.content_hash, String(font.content.size)),
    );
    return names?.size === 1
      ? { ...font, display_family: [...names][0] }
      : font;
  });
}
export async function verifySynthesisFontImport(
  api: EditorApi,
  request: ToolInput<"opencut_font_synthesis_v1_import">,
  revision: string,
) {
  const [proof, snapshot] = await Promise.all([
    api.sdk.call("opencut_font_synthesis_v1_proof", {
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
    (font) => font.id === request.font_id,
  );
  if (
    proof.schema_version !== 1 ||
    proof.project_id !== request.project_id ||
    proof.revision !== revision ||
    proof.font_id !== request.font_id ||
    snapshot.project.id !== request.project_id ||
    snapshot.revision !== revision ||
    proof.source.source.face.id !== request.catalog_id ||
    !font
  )
    throw differs();
  checkSynthesisResource(font, proof.source);
  return proof;
}
