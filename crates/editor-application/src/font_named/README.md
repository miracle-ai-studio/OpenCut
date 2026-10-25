# Fixed derived fonts V2

This module preserves the frozen V1 source/import/proof DTO shapes. New public
operations and provider `opencut-fonttools-static-renamed-v2` select canonical
proof version 2; original V1 operations and the V1 RFN rejection stay unchanged.

Application code owns only reviewed recipes, exact identities and atomic import
semantics. It has no native paths, network, runtime processes or system fonts.
`approved_recipes.json` is a separate immutable input allowlist so source-pin
initialization does not call CatalogV3 or recurse through its native lookup.
Actual source and licence repositories, commits, paths and hashes are independent
of the Classic atlas commit and bind each public source URL.

OFL and Apache outputs use deterministic `OCDF <hash>` family names. Ubuntu Font
Licence 1.0 instances follow its modified-version naming clause 2(c), retaining
the original family followed by `derivative OCDF <hash>`. The original full
licence, copyright/name records and trademark records are retained. Catalog
display names remain the original Classic labels; imported resource family names
always match actual font bytes.

Only the eighteen records in `static_correction_inputs.json` may use static
metadata/checksum correction kinds. Their proof records original/target metadata,
the exact changed-table allowlist and unchanged outline/cmap/metric table hashes.
Static inputs have no invented variation axes. Instantiated color outputs retain
their original color tables and carry an explicit rendering profile.

The two approved Chiron targets may acquire their single 51,170,304-byte fixed
source under a 64 MiB source budget. Output inspection remains independently
bounded to 32 MiB. The V2 child has a 2 GiB memory ceiling, 90-second CPU ceiling,
32 MiB output limit, 64 file descriptors, and the existing host's absolute
deadline/cancellation checks. Native IO and child supervision reuse the V1 engine
through an explicit V2 profile with separately embedded worker/algorithm/lock.

Configure the existing Python/cache paths plus
`OPENCUT_FONT_NAMED_SCRIPT=/absolute/path/scripts/font-catalog/run_derived_v2.py`.
When omitted, the host resolves `run_derived_v2.py` beside the configured V1
worker. Availability hints only stat cache files; imports rehash actual bytes.
Without explicit runtime paths, the host reads the bundled runtime manifest;
malformed manifests fail explicitly and never fall back to PATH discovery.

Generated pin/output hashes are implementation data. They do not replace native
parser, independent glyph/color pixel, runtime budget, recovery, history or
package acceptance. No font binaries belong in this repository.
