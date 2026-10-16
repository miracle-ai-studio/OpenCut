import type { ToolInput, ToolOutput } from "../../../lib/editor-sdk";
export type BrandEntryV1 = ToolOutput<"opencut_brand_resources_v1_list">["entries"][number];
export type BrandSourceV1 = ToolOutput<"opencut_brand_resources_v1_source">;
export type BrandImportV1 = ToolInput<"opencut_brand_resources_v1_import">;
export const brandProviderV1 = "simple-icons-browser-raster-v1";
export function validateBrandSvgV1(source: BrandSourceV1, entry: BrandEntryV1) {
  if (source.schema_version !== 1 || !entry.import_available || !/^simple-icons:16\.35\.0:[a-z0-9_]+$/.test(entry.id) || source.collection.release !== "16.35.0" || source.collection.commit !== "ed9a5192b24ad2f169bf1009931da0df27d5a49a" || JSON.stringify(source.entry) !== JSON.stringify(entry) || source.raster_width !== 512 || source.raster_height !== 512 || source.license.identifier !== entry.license_identifier || !source.license.notice || !source.license.attribution) throw Error("Brand source differs from the selected fixed catalog entry.");
  const bytes = new TextEncoder().encode(source.svg);
  if (!bytes.length || bytes.length > 262144 || bytes.length !== entry.svg_size || /<!DOCTYPE|<!ENTITY|<\?/i.test(source.svg)) throw Error("Brand SVG exceeds the pinned path-only image policy.");
  const doc = new DOMParser().parseFromString(source.svg, "image/svg+xml"), root = doc.documentElement;
  if (doc.querySelector("parsererror") || root.localName !== "svg" || root.namespaceURI !== "http://www.w3.org/2000/svg" || root.getAttribute("viewBox") !== "0 0 24 24" || [...root.attributes].some(a => !["role", "viewBox", "xmlns"].includes(a.name))) throw Error("Brand SVG has an unsupported root.");
  let paths = 0;
  for (const child of root.children) {
    if (child.namespaceURI !== root.namespaceURI || child.children.length || !["title", "path"].includes(child.localName)) throw Error("Brand SVG contains unsupported content.");
    if (child.localName === "title") { if (child.attributes.length) throw Error("Brand SVG title contains attributes."); }
    else { if (child.attributes.length !== 1 || !child.hasAttribute("d") || !/^[MmZzLlHhVvCcSsQqTtAaEe0-9+.,\s-]+$/.test(child.getAttribute("d")!)) throw Error("Brand SVG path contains unsupported attributes."); ++paths; }
  }
  if (!paths) throw Error("Brand SVG has no path.");
  return bytes;
}
export async function brandPngV1(source: BrandSourceV1, entry: BrandEntryV1, signal: AbortSignal): Promise<File> {
  signal.throwIfAborted();
  const bytes = validateBrandSvgV1(source, entry);
  const hash = Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)), b => b.toString(16).padStart(2, "0")).join("");
  if (hash !== entry.svg_sha256) throw Error("Brand SVG differs from its pinned source hash.");
  signal.throwIfAborted();
  const url = URL.createObjectURL(new Blob([bytes], { type: "image/svg+xml" })), image = new Image();
  try {
    await new Promise<void>((resolve, reject) => {
      const clean = () => { signal.removeEventListener("abort", aborted); image.onload = null; image.onerror = null; };
      const aborted = () => { clean(); reject(new DOMException("Brand acquisition cancelled", "AbortError")); };
      image.onload = () => { clean(); resolve(); }; image.onerror = () => { clean(); reject(Error("Pinned brand image could not be decoded.")); };
      signal.addEventListener("abort", aborted, { once: true }); if (signal.aborted) return aborted(); image.src = url;
    });
    signal.throwIfAborted();
    const canvas = document.createElement("canvas"); canvas.width = 512; canvas.height = 512;
    const ctx = canvas.getContext("2d"); if (!ctx) throw Error("Brand rasterization is unavailable.");
    ctx.clearRect(0, 0, 512, 512); ctx.drawImage(image, 0, 0, 512, 512);
    const png = await new Promise<Blob>((resolve, reject) => canvas.toBlob(value => value ? resolve(value) : reject(Error("Brand PNG encoding failed.")), "image/png"));
    signal.throwIfAborted(); if (!png.size || png.size > 1048576) throw Error("Brand PNG exceeds the upload budget.");
    return new File([png], `${entry.slug}.png`, { type: "image/png" });
  } finally { image.src = ""; URL.revokeObjectURL(url); }
}
