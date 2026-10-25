import { describe, expect, it } from "vitest";
import { filterCatalog, validateFlagSvg, type CatalogEntry } from "./catalog_resources";

describe("fixed catalog sources", () => {
  it("permits local SVG references, including quoted CSS fragments, without external fetches", () => {
    const svg = `<svg><defs><path id="part"/></defs><use href="#part"/><g clip-path='url("#clip")' fill="url(#paint)"/></svg>`;
    expect(new TextDecoder().decode(validateFlagSvg(svg))).toBe(svg);
    for (const source of [
      '<svg><image href="https://example.com/user.png"/></svg>',
      '<svg><use xlink:href="data:image/svg+xml,external"/></svg>',
      '<svg fill="url(https://example.com/paint.svg#paint)"/>',
      '<svg style="fill:url(\'//example.com/paint\')"/>',
      '<svg><script>fetch("https://example.com")</script></svg>',
      '<svg><foreignObject/></svg>',
      "x".repeat(524289),
    ]) expect(() => validateFlagSvg(source)).toThrow(/policy/);
  });

  it("searches full category and keyword inventory without mixing categories", () => {
    const entry = (id: string, category: CatalogEntry["category"], keywords: string[]): CatalogEntry => ({ id, category, name: id, keywords, source_url: "https://kenney.nl/assets/ui-audio", source_sha256: "a".repeat(64), license: { identifier: "CC0-1.0", source_url: null, notice: null, attribution: null }, audio: null, width: null, height: null });
    const last = entry("kenney-ui-audio:switch38", "sounds", ["switch", "interface"]);
    const values = [entry("kenney-ui-audio:click1", "sounds", ["click", "interface"]), last, entry("flags:ch", "flags", ["switch", "interface"])];
    expect(filterCatalog(values, "sounds", " SWITCH  interface ")).toEqual([last]);
    expect(filterCatalog(values, "sounds", "absent")).toEqual([]);
  });
});
