# Fixed color and large-source inputs

These files record implementation inputs. They are not native admission,
Chromium rendering, independent pixel or end-to-end import acceptance results.
The integration owner runs those checks after all consumers are assembled.

`generate_color_sources_v3.py` reads only the previously recorded color/bitmap
exclusions from the static and derived source reports. It does not rescan the
catalog, download files or rewrite fonts. With the existing external cache and
the existing pinned fontTools environment:

```sh
/private/tmp/opencut-font-derive-venv/bin/python scripts/font-catalog/generate_color_sources_v3.py \
  --tree /private/tmp/opencut-google-fonts-tree.json \
  --cache /private/tmp/opencut-b3-font-cache \
  --output crates/editor-application/src/font_catalog
```

The reviewed Google Fonts commit is
`66928797f271fed5ee428544e7229685e2d90127`. Every source and original OFL notice
is matched to that complete tree's Git blob and size before SHA-256 pins are
written. `color_source_pins.json` contains 12 original static sources: eight
COLR sources and four Anonymous Pro one-bit bitmap sources. The 21 entries in
`color_variable_inputs.json` are variable **inputs**. Their admitted output
bytes, names and derivation proofs belong to the separate V2 derivation pipeline.
Neither source bytes nor a variable source digest can stand in for an output pin.

`CatalogFontInspector` only enables a declared rendering profile for a compiled
SHA-256 and exact size, then checks actual family, weight and style. Unknown
fonts continue through the unchanged V1 inspector. SFNT checksums, directory
bounds, Unicode coverage, glyph metrics, shaping-table parsing and cancellation
remain required. COLRv0/v1 and CPAL are parsed; referenced COLRv1 paint graphs
are independently walked because the pinned ttf-parser painter may ignore a
nested parse failure. Fully static output cannot retain variable paint records.

The monochrome profile currently admits the compiled Anonymous Pro
EBLC/EBDT format: one-bit strikes, index format 3, image format 1, with valid
outline fallback. Other bitmap formats require a separately reviewed profile.
An original font containing both COLR and SVG retains both tables; declared SVG
table references are bounded and checked. The renderer profile consumes COLR.
SVG-only, CBDT/CBLC, sbix and Apple bitmap fonts are outside these profiles. This
is an explicit admission boundary, **not** a claim about all Chromium versions'
font support. No table is deleted to make an unsupported source appear static.

`prepare_large_source_v3.py` fetches only the exact Chiron TTF, METADATA and OFL
paths, preserving the configured HTTPS proxy. It streams 64 KiB chunks, checks
the fixed size and Git blob before atomic cache publication, and records SHA-256.
The source cap is 64 MiB; metadata and license each remain at most 64 KiB.

```sh
/private/tmp/opencut-font-derive-venv/bin/python scripts/font-catalog/prepare_large_source_v3.py \
  --tree /private/tmp/opencut-google-fonts-tree.json \
  --cache /private/tmp/opencut-b3-font-cache \
  --output crates/editor-application/src/font_catalog/large_variable_inputs.json \
  --download
```

The two 400/700 targets share `ofl/chirongoroundtc/ChironGoRoundTC[wght].ttf`:
51,170,304 bytes, Git blob `19c23f315935741f8e2079a6dc430162dd6f5826`, SHA-256
`47cd5ff5153799d4c0d05bcc64da849e13b8eb842e46728a66bf2d52c480231f`.
The original source, METADATA and full OFL notice were acquired into the existing
external cache. No font binary was added to this repository. The V2 recipe
pipeline owns the output identities and their independent byte limit. V1 keeps
its 32 MiB contract. A V3 output above that limit requires both an explicitly
larger caller budget and an exact compiled output hash/size; no arbitrary upload
or project resource receives the larger allowance.

Native tests are in `font_inspection/tests.rs`. The ignored fixed-cache tests use
`OPENCUT_FONT_CATALOG_CACHE=/private/tmp/opencut-b3-font-cache` and do not fetch
or derive. The synthetic red-square color glyph has an independently authored
geometry/palette reference; it is not added to the production approved catalog.
