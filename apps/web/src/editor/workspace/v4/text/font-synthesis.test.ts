import { expect, it, vi } from "vitest";
import type { EditorApi } from "../../api";
import {
  checkSynthesisResource,
  enrichSynthesisFonts,
  requestedFontStyle,
  synthesisSourceMetadata,
  preferredFontVariant,
} from "./font-synthesis";
import {
  synthesisFixture,
  derivedSynthesisFixture,
} from "./font-synthesis-fixture";
import {
  verifyDerivedFontImport,
  mergeDerivedFontCatalog,
} from "./font-derived";
import { textDraft, textDraftValue, editableTextFonts } from "./draft";
import { textInitial } from "./fixture";
import { createMask } from "../scene-stack/model";
import type { FontBrowserCatalog } from "./font-browser";
import type { UnifiedFontCatalog } from "./font-catalog";
it("retains actual normal bytes while exposing a typed italic request for text and TextMask", () => {
  const f = synthesisFixture(),
    metadata = synthesisSourceMetadata(f.source),
    font = checkSynthesisResource(f.font, f.source);
  expect(metadata.style).toBe("normal");
  expect(metadata.requested_style).toBe("italic");
  expect(font.style).toBe("normal");
  expect(requestedFontStyle(font)).toBe("italic");
  expect(font.content).toEqual(f.font.content);
  expect(font.license).toEqual(f.font.license);
  const initial = textInitial(),
    draft = textDraft(initial);
  draft.fontId = font.id;
  expect(textDraftValue(initial, draft, [font]).text).toMatchObject({
    font_style: "italic",
    font_weight: "bold",
    font: { font_id: font.id },
  });
  const mask = createMask("text", null, font);
  expect(mask.shape.kind).toBe("text");
  if (mask.shape.kind === "text")
    expect(mask.shape.mask.font_style).toBe("italic");
  expect(editableTextFonts([f.font])).toEqual([]);
  expect(() => createMask("text", null, f.font)).toThrow();
});
it.each(["requested", "actual", "resource", "license"])(
  "rejects an altered %s identity",
  (field) => {
    const f = synthesisFixture();
    if (field === "requested") f.source.binding.requested.style = "normal";
    if (field === "actual") f.source.binding.actual.style = "italic";
    if (field === "resource") f.font.style = "italic";
    if (field === "license") f.font.license!.attribution = "Different author";
    expect(() => checkSynthesisResource(f.font, f.source)).toThrow(
      "synthesis binding",
    );
  },
);
it("reads existing synthesis resources only from the requested fixed revision", async () => {
  const f = synthesisFixture(),
    call = vi.fn(async () => ({
      schema_version: 1,
      project_id: "p",
      revision: "r",
      font_id: f.font.id,
      source: f.source,
    }));
  const fonts = await enrichSynthesisFonts(
    { sdk: { call } } as unknown as EditorApi,
    { projectId: "p", revision: "r" },
    [f.font],
    new AbortController().signal,
  );
  expect(fonts[0].requested_style).toBe("italic");
  expect(call.mock.calls[0]).toMatchObject([
    "opencut_font_synthesis_v1_proof",
    { project_id: "p", revision: "r", font_id: f.font.id },
  ]);
});
it("verifies a synthesis import through its new proof tool without calling a V1 or V2 proof", async () => {
  const f = synthesisFixture(),
    request = {
      project_id: "p",
      expected_revision: "before",
      idempotency_key: "same",
      catalog_id: f.source.source.face.id,
      font_id: f.font.id,
      required_text: "AV",
    };
  const call = vi.fn(async (name: string) =>
    name === "opencut_font_synthesis_v1_proof"
      ? {
          schema_version: 1,
          project_id: "p",
          revision: "saved",
          font_id: f.font.id,
          source: f.source,
        }
      : { project: { id: "p", fonts: [f.font] }, revision: "saved" },
  );
  await verifyDerivedFontImport(
    { sdk: { call } } as unknown as EditorApi,
    request,
    "saved",
  );
  expect(call.mock.calls.map(([name]) => name)).toEqual([
    "opencut_font_synthesis_v1_proof",
    "opencut_project_v4_inspect",
  ]);
});
it("matches synthesis discovery to the requested italic atlas face while preserving actual normal metadata", () => {
  const f = synthesisFixture(),
    catalog = {
      families: [
        {
          family: "Example",
          faces: [{ weight: 700, style: "italic", catalog_id: null }],
        },
      ],
    } as unknown as FontBrowserCatalog;
  const unified = {
    fonts: [
      {
        id: f.source.source.face.id,
        display_family: "Example",
        family: "Example",
        weight: 700,
        style: "normal",
        requested_style: "italic",
        kind: "synthesis_v1",
      },
    ],
  } as unknown as UnifiedFontCatalog;
  const merged = mergeDerivedFontCatalog(catalog, null, unified);
  expect(merged[0].faces[0].unified?.style).toBe("normal");
  expect(merged[0].faces[0].style).toBe("italic");
  expect(merged[0].faces[0].catalog_id).toBe(f.source.source.face.id);
});
it("retains a 400 normal OCDF derived base, upstream proof and native color profile separately from synthesis input bytes", () => {
  const f = derivedSynthesisFixture(),
    metadata = synthesisSourceMetadata(f.source),
    font = checkSynthesisResource(f.font, f.source);
  expect(metadata.synthesis_base).toEqual(f.base);
  expect(metadata.render_profile).toBe("colr_v1");
  expect(metadata.family).toBe("OCDF fixture");
  expect(metadata.weight).toBe(400);
  expect(metadata.requested_style).toBe("italic");
  expect(metadata.content_hash).not.toBe(f.base.source.source_sha256);
  expect(font.style).toBe("normal");
  expect(requestedFontStyle(font)).toBe("italic");
  const initial = textInitial(),
    draft = textDraft(initial);
  draft.fontId = font.id;
  expect(textDraftValue(initial, draft, [font]).text).toMatchObject({
    font_style: "italic",
    font_weight: "normal",
  });
});
it("rejects a base route/source substitution and output/upstream hash confusion", () => {
  const f = derivedSynthesisFixture();
  f.source.source.source_sha256 = f.base.source.source_sha256;
  expect(() => synthesisSourceMetadata(f.source)).toThrow("synthesis binding");
  const g = derivedSynthesisFixture(),
    proof = JSON.parse(g.source.source.proof_json);
  proof.base.catalog_id = "other";
  g.source.source.proof_json = JSON.stringify(proof);
  expect(() => synthesisSourceMetadata(g.source)).toThrow("synthesis binding");
});
it("prefers synthesis over an admitted real italic while preserving the exact real source for explicit selection", () => {
  const f = synthesisFixture(),
    real = "google-example-bolditalic-v1",
    catalog = {
      families: [
        {
          family: "Example",
          faces: [
            {
              weight: 700,
              style: "italic",
              catalog_id: real,
              unavailable_reason: null,
            },
          ],
        },
      ],
    } as unknown as FontBrowserCatalog;
  const synthetic = {
    id: f.source.source.face.id,
    display_family: "Example",
    family: "Example",
    weight: 700,
    style: "normal",
    requested_style: "italic",
    kind: "synthesis_v1",
  };
  const realFace = {
    ...synthetic,
    id: real,
    style: "italic",
    requested_style: null,
    kind: "static_v1",
  };
  const unified = {
    fonts: [realFace, synthetic],
  } as unknown as UnifiedFontCatalog;
  const result = mergeDerivedFontCatalog(catalog, null, unified)[0].faces[0];
  expect(result.catalog_id).toBe(synthetic.id);
  expect(result.realItalic?.catalog_id).toBe(real);
  expect(result.realItalic?.unified).toEqual(realFace);
  expect(catalog.families[0].faces[0].catalog_id).toBe(real);
});
it("prefers a proven imported synthesis binding when toggling italic, but leaves an explicitly selected real italic unchanged", () => {
  const f = synthesisFixture(),
    synthesized = checkSynthesisResource(f.font, f.source),
    normal = { ...f.font, id: "normal", origin: { kind: "legacy" as const } },
    real = { ...normal, id: "real", style: "italic" as const };
  const fonts = [real, normal, synthesized];
  expect(preferredFontVariant(normal, fonts, 700, "italic")?.id).toBe(
    synthesized.id,
  );
  expect(preferredFontVariant(real, fonts, 700, "italic")?.id).toBe(real.id);
  expect(preferredFontVariant(synthesized, fonts, 700, "normal")?.id).toBe(
    normal.id,
  );
});
it("groups separate OCDF weights only from validated proof labels and exact ordinary base bytes, without additional reads or identity changes", async () => {
  const regular = derivedSynthesisFixture(400),
    bold = derivedSynthesisFixture(700);
  const base = (f: typeof regular) => ({
    ...structuredClone(f.font),
    id: `base-${f.font.weight}`,
    origin: {
      kind: "generated" as const,
      provider: "opencut-fonttools-static-renamed-v2",
      generation_id: f.base.source.face.derivation_id,
    },
  });
  const r = base(regular),
    b = base(bold),
    wrong = { ...r, id: "different-bytes", content: { ...r.content, size: 4 } };
  const originals = [regular.font, bold.font, r, b, wrong],
    before = structuredClone(originals);
  const call = vi.fn(async (_name, request: { font_id: string }) => ({
    schema_version: 1,
    project_id: "p",
    revision: "r",
    font_id: request.font_id,
    source: request.font_id === regular.font.id ? regular.source : bold.source,
  }));
  const fonts = await enrichSynthesisFonts(
    { sdk: { call } } as unknown as EditorApi,
    { projectId: "p", revision: "r" },
    originals,
    new AbortController().signal,
  );
  expect(call).toHaveBeenCalledTimes(2);
  expect(fonts.slice(0, 4).map((font) => font.display_family)).toEqual([
    "Example",
    "Example",
    "Example",
    "Example",
  ]);
  expect(fonts[4].display_family).toBeUndefined();
  expect(fonts[2].family).not.toBe(fonts[3].family);
  expect(preferredFontVariant(fonts[2], fonts, 700, "normal")?.id).toBe(b.id);
  expect(preferredFontVariant(fonts[2], fonts, 700, "italic")?.id).toBe(
    bold.font.id,
  );
  expect(originals).toEqual(before);
  expect(
    fonts.map((font) => ({
      id: font.id,
      family: font.family,
      content: font.content,
    })),
  ).toEqual(
    before.map((font) => ({
      id: font.id,
      family: font.family,
      content: font.content,
    })),
  );
});
