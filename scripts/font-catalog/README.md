# Classic font catalog pins

The browser inventory derives from Classic commit
`cf5e79e919144200294fb9fed22a222592a0aeea`, file
`apps/web/public/fonts/font-atlas.json` (SHA256
`1582a3ebae342be98c3f6e7b8b7b6a4521c7fda019bca7fca33d74d7d71ade0f`).
Its 1,920 Google family entries declare weights/styles. The nine Classic system
families are visible but unavailable for redistribution; no system lookup occurs.

Sources are the unchanged Google Fonts commit
`66928797f271fed5ee428544e7229685e2d90127`. The source tree JSON must be complete
(`truncated=false`). Regeneration downloads only matching static normal/italic
400/700 faces with a supported original OFL or Apache license. It verifies the
font and license Git blob identities against that tree, inspects declared sfnt
family/weight/style, records SHA256/size, and retains each full original license.
Original B1 Lato/Ma Shan Zheng IDs and content identities remain fixed.
Discovery includes historical unnamed static files and `static/` subdirectories.
Admissible original root faces retain priority, so newly discovered files never
replace an existing catalog identity. This extension preserved all 1,797 earlier
records and added 39 faces across 35 families; 621 families still lack a pinned
400 normal face.

The current result is 1,836 pinned static faces across 1,299 families. 117
candidate static faces fail exact declared identity or native static-render support checks and remain excluded
with reasons in `catalog_source_report.json`. The actual native scan found 12
color/bitmap faces, two invalid table checksums and four nonzero italic-angle
faces mislabeled as Regular; `catalog_native_report.json` preserves those
failures and a successful complete 1,836-face native scan. The browsing inventory reports
1,490 applicable faces without a matching pinned static source, 4,348 weights
that TextV4 cannot express, and 36 unavailable system faces. Inventory presence
is not successful import/render proof. Variable fonts, UFL fonts, unpinned
families and unsupported weights do not acquire synthetic/static fallbacks.

Regenerate using a cache outside the repository; urllib retains the configured
proxy environment. Obtain the complete fixed tree from the official GitHub API:
`https://api.github.com/repos/google/fonts/git/trees/66928797f271fed5ee428544e7229685e2d90127?recursive=1`.

```sh
python3 scripts/font-catalog/generate.py \
  --tree /tmp/opencut-google-fonts-tree.json \
  --cache /tmp/opencut-b3-font-cache --workers 20
python3 scripts/font-catalog/check.py
```

Successful sources are read from cache; a rerun downloads only missing sources.
No font binaries are committed. The generated Rust catalog and notices are
reviewable text; `browser_catalog.json` is the lightweight complete inventory.

The integrator may run the offline native parser test using the same cache:

```sh
OPENCUT_FONT_CATALOG_CACHE=/tmp/opencut-b3-font-cache \
  cargo test -p opencut-native-services \
  every_pinned_classic_static_face_matches_native_inspector -- --ignored
```

This checks every actual face's hash/size and native parsed family/weight/style.
It does not prove arbitrary text coverage or independent rendered pixels; normal
import/render still rechecks coverage for the current required text.
