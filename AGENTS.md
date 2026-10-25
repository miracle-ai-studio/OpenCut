# OpenCut development boundaries

Read `design/architecture/architecture-renewal-plan.md` for the current target and
`design/architecture/headless-completion-ledger.md` for verified status. Historical
implementation notes are not evidence that the current tree is fully delivered.

- Keep domain/time/composition pure. Application owns use cases and ports; native
  adapters implement them. Construct concrete implementations in host assembly.
- CLI, HTTP, MCP, SDK and UI share application semantics. Do not call a CLI shell
  command from an internal module as a substitute for an application port.
- Use typed in-process and cross-process contracts. Keep
  public DTOs separate from internal prepared jobs and native resource paths.
- Preserve fixed revisions, atomic commits, idempotency, lease/publication fences,
  cancellation and policy propagation when extracting modules.
- `design/architecture/dependency-policy.json` is a reviewed baseline. New modules
  or dependencies need an explicit boundary rationale; do not regenerate the
  allowlist merely to make a failure green. Remove resolved migration exceptions;
  explain any extension with evidence and an exit plan.
- The user's 2026-10-18 final-delivery decision supersedes the earlier versioned
  rollout policy: consolidate business APIs, DTOs, module names and execution
  entrypoints into one implementation without development-generation suffixes.
  Retire duplicate historical operations and fallback chains; do not just rename
  colliding semantics or drop working features. Historical evidence and dependency
  identities remain exact. Generate SDK/OpenAPI/tool types from the shared
  declaration, not hand-copied DTOs.
- Rendering features need source-to-IR-to-backend coverage and independent expected
  pixels/PCM. Preview equals export is necessary but not sufficient evidence.
- Do not treat a project revision or FFmpeg hash alone as renderer identity or
  proof of exact reproducibility. Keep implemented, verified and planned distinct.
- Keep user media out of the repository and remote CI. Respect each task's local
  processing policy in every provider and child process.

For boundary changes run:

```sh
python3 -m unittest discover -s scripts/tests -p 'test_architecture.py'
python3 scripts/check-architecture.py
python3 scripts/check-contract-boundaries.py
```

For public contract changes run generated-contract and canonical-contract checks
against the same explicit integrated binary. The historical legacy-wire freeze is
superseded by the user-authorized single-contract consolidation; update its gate
and consumers together, preserving old receipts as historical evidence.
Run affected behavior tests and
update the completion ledger with actual evidence. Do not label the whole project
complete after a scoped check or a successful build.

## Change and verification discipline

- Finish one reviewable feature slice before producing another release candidate.
  Assign disjoint file ownership to agents; one integrator owns shared contracts,
  generated files, Cargo builds and stateful browser/media acceptance.
- Before building, check the complete registration path: DTO/export, tool registry,
  domain action, host dispatch, access/error policy and generated-client consumer.
  Use targeted searches; do not repeatedly read generated schemas or whole modules.
- Choose checks from the actual changed behavior. Documentation-only changes need
  reference/data consistency checks, not product builds or media runs. Reuse prior
  results only for their recorded source/candidate and scope. Re-run or broaden
  when code, dependencies, runtime identity or a demonstrated coverage gap changes.
  The boundary/protocol requirements above still apply when those boundaries change.
- Build once after integration, generate and check against that same explicit
  binary, and run affected browser suites serially in one owned synthetic workspace.
  Do not rebuild Rust for a verifier-only correction or create another fixture copy.
- Keep the migration plan forward-looking, the ledger focused on current delivery,
  and acceptance.json scoped to necessary evidence and unresolved limits. Replace
  superseded summaries; do not append per-attempt logs or duplicate file hash lists.
- Generated schemas and SDK types remain derived from Rust and reviewable. Do not
  minify, ignore or change public contracts merely to reduce the displayed diff.
  Delete a test/helper only after proving it is obsolete or its behavior is covered.
