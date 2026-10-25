# OpenCut development boundaries

Read `docs/architecture.md`, `docs/contracts.md` and `docs/status.md` for current
architecture, public interfaces, delivery scope and known limitations.

- Keep domain/time/composition pure. Application owns use cases and ports; native
  adapters implement them. Construct concrete implementations in host assembly.
- CLI, HTTP, MCP, SDK and UI share application semantics. Internal modules use
  application ports, not CLI shell commands as an internal RPC mechanism.
- Use typed contracts. Separate public DTOs from prepared jobs and native paths.
  Generate SDK/OpenAPI/tool types from the shared Rust declaration.
- Maintain one implementation per business operation. Do not add numbered
  generations, duplicate write entrypoints or silent fallback chains. Input
  format versions, dependency identities and immutable stored proofs stay exact.
- Preserve fixed revisions, atomic commits, idempotency, lease/publication fences,
  cancellation, authorization and local-processing policy through every adapter
  and child process.
- `design/architecture/dependency-policy.json` is the reviewed baseline. New
  modules or dependencies need an explicit boundary rationale. Do not regenerate
  the allowlist to make a failure green. Remove resolved exceptions; explain
  extensions with evidence and an exit plan.
- Rendering requires source-to-IR-to-backend coverage and independent expected
  pixels/PCM. Preview/export agreement alone is insufficient. A project revision
  or FFmpeg hash alone does not identify the complete renderer.
- Keep user media, databases, credentials, logs and test-run directories out of
  the repository and remote CI. Preserve required licenses and provenance.
- Finish one reviewable feature slice before another candidate. If agents are
  used, assign disjoint files; one integrator owns shared contracts, generated
  files, Cargo builds and stateful browser/media acceptance.
- Before building, check DTO/export, registry, domain action, host dispatch,
  access/error policy and generated-client consumers.
- Build once after integration, generate/check against that same explicit binary,
  then run affected browser suites serially in one owned synthetic workspace.
  Do not rebuild Rust for verifier-only changes or duplicate successful fixtures,
  jobs and media outputs.
- Choose checks from actual changed behavior. Documentation-only changes need
  reference/data checks, not product builds or media runs. Reuse results only for
  their recorded candidate and scope. Delete tests/helpers only after proving
  they are obsolete or their behavior is covered.
- Report implementation, build, scoped behavior, manual acceptance and complete
  delivery separately. Keep `docs/status.md` and `docs/status.json` accurate.
  Do not turn a limited check into a whole-product completion claim.

For boundary changes run:

```sh
python3 -m unittest discover -s scripts/tests -p 'test_architecture.py'
python3 scripts/check-architecture.py
python3 scripts/check-contract-boundaries.py
```

For public contract changes build the integrated CLI once, then use that exact
binary for generated-contract and canonical-contract checks. Run affected behavior
tests. Generated schemas and SDK types remain reviewable; do not minify, ignore
or hand-edit them to reduce a diff.
