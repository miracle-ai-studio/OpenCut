import { expect, it, vi } from "vitest";
import type { EditorApi } from "../../api";
import {
  mergeDerivedFontCatalog,
  verifyDerivedFontImport,
} from "./font-derived";
import type { DerivedFontImport, DerivedFontSource } from "./font-derived";
function fixture() {
  const source: DerivedFontSource = {
    face: {
      id: "recipe",
      family: "Variable Only",
      weight: 700,
      style: "italic",
      derivation_id: "instance-proof",
      content_hash: "d".repeat(64),
      size_bytes: "120",
    },
    source_commit: "pinned",
    source_url: "https://example.test/variable.ttf",
    source_sha256: "e".repeat(64),
    source_size_bytes: "240",
    license_url: "https://example.test/OFL.txt",
    license_id: "OFL-1.1",
    license_notice: "Original license",
    axes: [{ tag: "wght", coordinate: "700" }],
    proof_json: "{}",
  };
  const request: DerivedFontImport = {
    project_id: "project",
    expected_revision: "before",
    idempotency_key: "original",
    catalog_id: "recipe",
    font_id: "font",
    required_text: "Title",
  };
  const proof = {
    schema_version: 1,
    project_id: "project",
    revision: "published",
    font_id: "font",
    source,
  };
  const snapshot = {
    revision: "published",
    project: {
      id: "project",
      fonts: [
        {
          id: "font",
          family: "Variable Only",
          weight: 700,
          style: "italic",
          content: { content_hash: "d".repeat(64), size: 120 },
          origin: {
            kind: "generated",
            provider: "opencut-fonttools-static-v1",
            generation_id: "instance-proof",
          },
        },
      ],
    },
  };
  const call = vi
    .fn()
    .mockResolvedValueOnce(proof)
    .mockResolvedValueOnce(snapshot);
  return {
    request,
    proof,
    snapshot,
    call,
    api: { sdk: { call } } as unknown as EditorApi,
  };
}
it("checks the original published revision and static output against its generated derivation origin", async () => {
  const f = fixture();
  await verifyDerivedFontImport(f.api, f.request, "published");
  expect(f.call.mock.calls).toEqual([
    [
      "opencut_font_derived_v1_proof",
      { project_id: "project", revision: "published", font_id: "font" },
    ],
    [
      "opencut_project_v4_inspect",
      { project_id: "project", revision: "published" },
    ],
  ]);
});
it.each(["variable input hash", "different generation", "different revision"])(
  "rejects %s instead of inferring proof from the current head",
  async (kind) => {
    const f = fixture();
    if (kind === "variable input hash")
      f.snapshot.project.fonts[0].content.content_hash =
        f.proof.source.source_sha256;
    if (kind === "different generation")
      f.snapshot.project.fonts[0].origin.generation_id = "other";
    if (kind === "different revision") f.snapshot.revision = "head";
    await expect(
      verifyDerivedFontImport(f.api, f.request, "published"),
    ).rejects.toThrow("published project font");
  },
);
it("does not substitute a derived recipe for an existing admitted static face", () => {
  const catalog = {
    families: [
      {
        family: "Variable Only",
        faces: [
          {
            weight: 700,
            style: "italic",
            catalog_id: "static",
            unavailable_reason: null,
          },
        ],
      },
    ],
  } as Parameters<typeof mergeDerivedFontCatalog>[0];
  const f = fixture();
  const merged = mergeDerivedFontCatalog(catalog, {
    schema_version: 1,
    import_available: true,
    unavailable_reason: null,
    source_commit: "pinned",
    fonts: [f.proof.source.face],
  });
  expect(merged[0].faces[0].catalog_id).toBe("static");
  expect(merged[0].faces[0].derived).toBeUndefined();
  expect(catalog.families[0].faces[0].catalog_id).toBe("static");
});

it("uses the independent V2 proof operation and rejects a V1 provider for a renamed output", async () => {
  const f = fixture();
  f.request.catalog_id = "google-example-bolditalic-derived-v2";
  f.proof.schema_version = 2;
  f.proof.source.face.id = f.request.catalog_id;
  f.proof.source.face.family = "OCDF explicit";
  f.snapshot.project.fonts[0].family = "OCDF explicit";
  f.snapshot.project.fonts[0].origin.provider =
    "opencut-fonttools-static-renamed-v2";
  await verifyDerivedFontImport(f.api, f.request, "published");
  expect(f.call.mock.calls[0][0]).toBe("opencut_font_derived_v2_proof");
  f.call
    .mockReset()
    .mockResolvedValueOnce(f.proof)
    .mockResolvedValueOnce(f.snapshot);
  f.snapshot.project.fonts[0].origin.provider = "opencut-fonttools-static-v1";
  await expect(
    verifyDerivedFontImport(f.api, f.request, "published"),
  ).rejects.toThrow("published project font");
});
