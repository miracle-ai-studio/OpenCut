/** Synthetic metadata fixture; no native glyph/render acceptance is implied. */
import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import type { FontSynthesisSource } from "./font-synthesis";
import { FONT_SYNTHESIS_PROVIDER_V1 } from "./font-synthesis";
import type { V4TextFont } from "./types";
export const fixtureCanonical = (value: unknown): string => {
  const sorted = (item: unknown): unknown =>
    Array.isArray(item)
      ? item.map(sorted)
      : item && typeof item === "object"
        ? Object.fromEntries(
            Object.entries(item)
              .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
              .map(([key, value]) => [key, sorted(value)]),
          )
        : item;
  return JSON.stringify(sorted(value));
};
export function synthesisFixture() {
  const bytes = new Uint8Array([1, 2, 3]),
    hash = bytesToHex(sha256(bytes)),
    license = "Original copyright and full fixture OFL notice";
  const actual = {
    id: "google-example-bold-v1",
    family: "Example",
    weight: 700,
    style: "normal" as const,
    source_url: "https://example.test/source.ttf",
    source_commit: "a".repeat(40),
    content_hash: hash,
    size_bytes: "3",
    license_id: "OFL-1.1",
    license_notice: license,
  };
  const id = "google-example-bold-synthetic-italic-v1",
    requested = { weight: 700, style: "italic" as const },
    profile = "classic_chromium_italic_v1" as const;
  const canonical = (value: Record<string, unknown>): string =>
    JSON.stringify(
      Object.fromEntries(
        Object.keys(value)
          .sort()
          .map((key) => [
            key,
            value[key] && typeof value[key] === "object"
              ? JSON.parse(canonical(value[key] as Record<string, unknown>))
              : value[key],
          ]),
      ),
    );
  const proof = {
    schema_version: 3,
    kind: "browser_font_synthesis",
    provider: FONT_SYNTHESIS_PROVIDER_V1,
    profile,
    catalog_id: id,
    actual,
    requested,
    license_url: "https://example.test/OFL.txt",
    license_sha256: bytesToHex(sha256(new TextEncoder().encode(license))),
    attribution: "Original author",
    source_bytes_unchanged: true,
  };
  const binding_id = `font-synthesis-v1_${bytesToHex(sha256(new TextEncoder().encode(canonical(proof))))}`;
  const source: FontSynthesisSource = {
    source: {
      face: {
        id,
        family: actual.family,
        weight: 700,
        style: "normal",
        derivation_id: binding_id,
        content_hash: hash,
        size_bytes: "3",
      },
      source_commit: actual.source_commit,
      source_url: actual.source_url,
      source_sha256: hash,
      source_size_bytes: "3",
      license_url: proof.license_url,
      license_id: actual.license_id,
      license_notice: license,
      axes: [],
      proof_json: canonical({ ...proof, derivation_id: binding_id }),
    },
    binding: { schema_version: 1, binding_id, profile, actual, requested },
  };
  const font: V4TextFont = {
    id: "font-synthesis",
    family: actual.family,
    weight: 700,
    style: "normal",
    face_index: 0,
    content: { id: `av_${hash}`, content_hash: hash, size: 3 },
    origin: {
      kind: "generated",
      provider: FONT_SYNTHESIS_PROVIDER_V1,
      generation_id: binding_id,
    },
    license: {
      identifier: actual.license_id,
      source_url: proof.license_url,
      notice: license,
      attribution: proof.attribution,
    },
  };
  return { source, font, bytes, hash };
}

/** Models the new acquisition envelope only; these bytes are not a native font. */
export function derivedSynthesisFixture(weight: 400 | 700 = 400) {
  const f = synthesisFixture(),
    actual = f.source.binding.actual;
  actual.id = `google-example-${weight === 400 ? "regular" : "bold"}-derived-v2`;
  actual.family = weight === 400 ? "OCDF fixture" : "OCDF fixture bold";
  actual.weight = weight;
  f.font.id = `font-synthesis-${weight}`;
  f.font.family = actual.family;
  f.font.weight = weight;
  f.font.license!.attribution = null;
  f.source.binding.requested.weight = weight;
  Object.assign(f.source.source.face, {
    id: `${actual.id}-derived-v2-synthetic-italic-v1`,
    family: actual.family,
    weight,
  });
  const base = {
    route: "derived_v2" as const,
    catalog_id: actual.id,
    display_family: "Example",
    render_profile: "colr_v1" as const,
    source: {
      ...structuredClone(f.source.source),
      face: {
        ...structuredClone(f.source.source.face),
        id: actual.id,
        derivation_id: `font-derived-v2_${"b".repeat(64)}`,
      },
      source_sha256: "e".repeat(64),
      source_size_bytes: "6",
      axes: [{ tag: "wght", coordinate: String(weight) }],
      proof_json: '{"schema_version":2,"fixture_only":true}',
    },
  };
  const proof = {
    ...JSON.parse(f.source.source.proof_json),
    actual,
    requested: f.source.binding.requested,
    catalog_id: f.source.source.face.id,
    attribution: null,
    base,
  };
  delete proof.derivation_id;
  const binding_id = `font-synthesis-v1_${bytesToHex(sha256(new TextEncoder().encode(fixtureCanonical(proof))))}`;
  f.source.binding.binding_id = binding_id;
  f.source.source.face.derivation_id = binding_id;
  f.font.origin = {
    kind: "generated",
    provider: FONT_SYNTHESIS_PROVIDER_V1,
    generation_id: binding_id,
  };
  f.source.source.proof_json = fixtureCanonical({
    ...proof,
    derivation_id: binding_id,
  });
  return { ...f, base };
}
