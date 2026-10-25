import type { ToolOutput } from "../../../lib/editor-sdk";
export type CatalogEntry = ToolOutput<"opencut_catalog_resources_list">["entries"][number];
const flags = import.meta.glob<string>("./catalog-flags/*.svg", { query: "?raw", import: "default" });
export function filterCatalog(entries: readonly CatalogEntry[], category: CatalogEntry["category"], query: string, displayName: (name: string) => string = name => name) {
  const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  return entries.filter(entry => entry.category === category && terms.every(term => [entry.name, displayName(entry.name), entry.id, ...entry.keywords].join(" ").toLocaleLowerCase().includes(term)));
}
export function validateFlagSvg(svg: string) {
  const bytes = new TextEncoder().encode(svg);
  const references = [...svg.matchAll(/(?:href|src)\s*=\s*(["'])(.*?)\1/gi)].map(match => match[2]);
  const urls = [...svg.matchAll(/url\(\s*([^)]*)\)/gi)].map(match => match[1].trim().replace(/^(["'])(.*)\1$/, "$2"));
  if (bytes.length > 524288 || [...references, ...urls].some(value => !/^#[A-Za-z_][\w:.-]*$/.test(value)) || /<(?:script|foreignObject)\b/i.test(svg)) throw Error("Flag source exceeds the bundled image policy.");
  return bytes;
}
export async function flagPng(entry: CatalogEntry, signal: AbortSignal): Promise<File> {
  if (entry.category !== "flags" || !/^flags:[a-z]{2}(?:-[a-z]{3})?$/.test(entry.id) || !entry.width || !entry.height || entry.width > 640 || entry.height > 640 || !entry.source_sha256) throw Error("Invalid fixed flag catalog entry.");
  signal.throwIfAborted();
  const load = flags[`./catalog-flags/${entry.id.slice(6)}.svg`]; if (!load) throw Error("This flag source is missing from the installed application.");
  const svg = await load(), bytes = validateFlagSvg(svg);
  const hash = Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)), b => b.toString(16).padStart(2, "0")).join("");
  if (hash !== entry.source_sha256) throw Error("Installed flag source differs from the catalog identity.");
  signal.throwIfAborted();
  const url = URL.createObjectURL(new Blob([bytes], { type: "image/svg+xml" })), img = new Image();
  try {
    await new Promise<void>((resolve, reject) => {
      const aborted = () => { cleanup(); reject(new DOMException("Flag acquisition cancelled", "AbortError")); };
      const cleanup = () => { signal.removeEventListener("abort", aborted); img.onload = null; img.onerror = null; };
      img.onload = () => { cleanup(); resolve(); }; img.onerror = () => { cleanup(); reject(Error("Bundled flag could not be decoded.")); };
      signal.addEventListener("abort", aborted, { once: true }); if (signal.aborted) return aborted(); img.src = url;
    });
    signal.throwIfAborted();
    const canvas = document.createElement("canvas"); canvas.width = entry.width; canvas.height = entry.height;
    const ctx = canvas.getContext("2d"); if (!ctx) throw Error("Flag image conversion is unavailable.");
    ctx.clearRect(0, 0, canvas.width, canvas.height); ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
    const blob = await new Promise<Blob>((resolve, reject) => canvas.toBlob(b => b ? resolve(b) : reject(Error("Flag PNG encoding failed.")), "image/png"));
    signal.throwIfAborted(); if (!blob.size || blob.size > 1048576) throw Error("Flag PNG exceeds its resource byte budget.");
    return new File([blob], `${entry.id.slice(6)}.png`, { type: "image/png" });
  } finally { img.src = ""; URL.revokeObjectURL(url); }
}
