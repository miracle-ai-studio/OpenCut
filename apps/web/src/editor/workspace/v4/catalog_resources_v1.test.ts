import { describe, expect, it } from "vitest";
import { filterCatalogV1, validateFlagSvgV1, type CatalogEntryV1 } from "./catalog_resources_v1";

describe("fixed catalog sources", () => {
  it("permits local SVG references, including quoted CSS fragments, without external fetches", () => {
    const svg = `<svg><defs><path id="part"/></defs><use href="#part"/><g clip-path='url("#clip")' fill="url(#paint)"/></svg>`;
    expect(new TextDecoder().decode(validateFlagSvgV1(svg))).toBe(svg);
    for (const source of [
      '<svg><image href="https://example.com/user.png"/></svg>',
      '<svg><use xlink:href="data:image/svg+xml,external"/></svg>',
      '<svg fill="url(https://example.com/paint.svg#paint)"/>',
      '<svg style="fill:url(\'//example.com/paint\')"/>',
      '<svg><script>fetch("https://example.com")</script></svg>',
      '<svg><foreignObject/></svg>',
      "x".repeat(524289),
    ]) expect(() => validateFlagSvgV1(source)).toThrow(/policy/);
  });

  it("searches full category and keyword inventory without mixing categories", () => {
    const entry = (id: string, category: CatalogEntryV1["category"], keywords: string[]): CatalogEntryV1 => ({ id, category, name: id, keywords, source_url: "https://kenney.nl/assets/ui-audio", source_sha256: "a".repeat(64), license: { identifier: "CC0-1.0", source_url: null, notice: null, attribution: null }, width: null, height: null });
    const last = entry("kenney-ui-audio:switch38", "sounds", ["switch", "interface"]);
    const values = [entry("kenney-ui-audio:click1", "sounds", ["click", "interface"]), last, entry("flags:ch", "flags", ["switch", "interface"])];
    expect(filterCatalogV1(values, "sounds", " SWITCH  interface ")).toEqual([last]);
    expect(filterCatalogV1(values, "sounds", "absent")).toEqual([]);
  });
});
