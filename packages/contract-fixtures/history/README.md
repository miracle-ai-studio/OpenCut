# Database and stored-result compatibility fixtures

SQL files freeze supported database-format increments. `project.json` and
`results.json` contain synthetic projects, requests, receipts and stored results
for compatibility readers. These fixtures contain no user media.

Keep exact format versions, IDs, hashes and default-field behavior. Do not derive
expected historical data from the current production schema during verification.
Unknown or corrupt databases must fail without modifying the original files;
upgrades must be atomic, and immutable receipts must retain their identity.

These are reconstructed format fixtures, not samples collected from released old
binaries. Successful stored media records without media bytes prove reader
compatibility only, not artifact availability or rendering correctness.
