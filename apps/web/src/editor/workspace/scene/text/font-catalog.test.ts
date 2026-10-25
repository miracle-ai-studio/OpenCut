import { expect, it } from "vitest";
import { unifiedSourceMetadata } from "./font-catalog";
import type { UnifiedFontSource } from "./font-catalog";
import { mergeFontCatalog } from "./font-derived";
import type { FontBrowserCatalog } from "./font-browser";

const face: UnifiedFontSource["face"] = {
  id: "google-fjordone-regular-alias-v3",
  display_family: "Fjord One",
  family: "Fjord",
  weight: 400,
  style: "normal",
  requested_style: null,
  kind: "static",
  content_hash: "a".repeat(64),
  size_bytes: "128",
  render_profile: "static_outline",
  alias_reason: "Explicit pinned alias",
  import_available: true,
  unavailable_reason: null,
};
function source(): UnifiedFontSource {
  return {
    license_url: "https://example.test/OFL.txt",
    license_sha256: "c".repeat(64),
    face: { ...face },
    details: {
      kind: "static",
      source: {
        id: face.id,
        family: face.family,
        weight: 400,
        style: "normal",
        source_commit: "fixed",
        source_url: "https://example.test/font.ttf",
        content_hash: face.content_hash,
        size_bytes: "128",
        license_id: "OFL-1.1",
        license_notice: "Original notice",
      },
    },
  };
}
it("separates the Classic display family from the actual pinned native family", () => {
  const value = source();
  // Runtime availability can change between discovery and source inspection.
  value.face.import_available = false;
  value.face.unavailable_reason = "Offline";
  expect(unifiedSourceMetadata(value, face).family).toBe("Fjord");
});
it("merges official distribution faces while preserving native identity and non-Git provenance", () => {
  const distribution = {
    ...face,
    id: "google-pushster-regular-distribution-v1",
    kind: "distribution" as const,
    display_family: "Pushster",
    family: "Lobster",
    alias_reason: "Official CSS label; original native name preserved",
  };
  const value = {
    face: distribution,
    provenance: {
      css_url: "https://fonts.googleapis.com/css2?family=Pushster",
      css_sha256: "d".repeat(64),
      source_url: "https://fonts.gstatic.com/s/pushster/v23/font.ttf",
      release: "v23",
      copyright: "Original copyright",
    },
    license_id: "OFL-1.1",
    license_notice: "Complete notice",
    license_url: "https://example.test/OFL.txt",
    license_sha256: "c".repeat(64),
  };
  const envelope: UnifiedFontSource = {
    face: distribution,
    details: { kind: "distribution", source: value },
    license_url: value.license_url,
    license_sha256: value.license_sha256,
  };
  const metadata = unifiedSourceMetadata(envelope, distribution);
  expect(metadata.family).toBe("Lobster");
  expect(metadata.source_commit).toBeUndefined();
  expect(metadata.distribution?.provenance.release).toBe("v23");
  const merged = mergeFontCatalog({
    families: [
      {
        family: "Pushster",
        category: "google",
        faces: [
          {
            weight: 400,
            style: "normal",
            catalog_id: null,
            unavailable_reason: "no_pinned_static_source",
          },
        ],
      },
    ],
    fonts: [distribution],
  } as FontBrowserCatalog);
  expect(merged[0].faces[0].unified).toEqual(distribution);
  expect(() =>
    unifiedSourceMetadata(
      {
        ...envelope,
        details: {
          kind: "distribution",
          source: {
            ...value,
            provenance: {
              ...value.provenance,
              source_url: "https://example.test/other.ttf",
            },
          },
        },
      },
      distribution,
    ),
  ).toThrow("selected catalog face");
});
it.each([
  "display_family",
  "family",
  "content_hash",
  "render_profile",
] as const)(
  "rejects changed %s instead of substituting another source",
  (key) => {
    const value = source();
    (value.face as unknown as Record<string, unknown>)[key] = "different";
    expect(() => unifiedSourceMetadata(value, face)).toThrow(
      "selected catalog face",
    );
  },
);
it("rejects a mismatched source kind and altered native bytes", () => {
  const value = source();
  value.details.kind = "derived";
  expect(() => unifiedSourceMetadata(value, face)).toThrow(
    "selected catalog face",
  );
  value.details.kind = "static";
  if ("content_hash" in value.details.source)
    value.details.source.content_hash = "b".repeat(64);
  expect(() => unifiedSourceMetadata(value, face)).toThrow(
    "selected catalog face",
  );
});
it("fills an unavailable Classic family without changing an existing admitted face", () => {
  const catalog = {
    families: [
      {
        family: "Fjord One",
        faces: [
          {
            weight: 400,
            style: "normal",
            catalog_id: null,
            unavailable_reason: "no_pinned_static_source",
          },
        ],
      },
      {
        family: "Other",
        faces: [
          {
            weight: 400,
            style: "normal",
            catalog_id: "original",
            unavailable_reason: null,
          },
        ],
      },
    ],
  } as FontBrowserCatalog;
  const unified = {
    download_available: true,
    unavailable_reason: null,
    source_commit: "fixed",
    classic_atlas_sha256: "b".repeat(64),
    classic_family_count: 2,
    fonts: [
      face,
      { ...face, id: "replacement", display_family: "Other", family: "Other" },
    ],
  };
  const merged = mergeFontCatalog({ ...unified, ...catalog });
  expect(merged[0].faces[0].unified?.family).toBe("Fjord");
  expect(merged[1].faces[0].catalog_id).toBe("original");
  expect(catalog.families[0].faces[0].catalog_id).toBeNull();
});
