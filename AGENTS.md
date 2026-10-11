# OpenCut development boundaries

Read `design/architecture/architecture-renewal-plan.md` for the current target and
`design/architecture/headless-completion-ledger.md` for verified status. Historical
implementation notes are not evidence that the current tree is fully delivered.

- Keep domain/time/composition pure. Application owns use cases and ports; native
  adapters implement them. Construct concrete implementations in host assembly.
- CLI, HTTP, MCP, SDK and UI share application semantics. Do not call a CLI shell
  command from an internal module as a substitute for an application port.
- Use typed in-process contracts and versioned cross-process contracts. Keep
  public DTOs separate from internal prepared jobs and native resource paths.
- Preserve fixed revisions, atomic commits, idempotency, lease/publication fences,
  cancellation and policy propagation when extracting modules.
- `design/architecture/dependency-policy.json` is a reviewed baseline. New modules
  or dependencies need an explicit boundary rationale; do not regenerate the
  allowlist merely to make a failure green. Remove resolved migration exceptions;
  explain any extension with evidence and an exit plan.
- Existing wire schemas remain frozen. Prefer a new versioned operation when a
  schema change is incompatible with the repository's compatibility gate. Generate
  SDK/OpenAPI/tool types from the shared declaration, not hand-copied DTOs.
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

For public protocol changes also run the generated-contract and wire-compatibility
checks documented in `crates/sdk-rust/moon.yml`. Run affected behavior tests and
update the completion ledger with actual evidence. Do not label the whole project
complete after a scoped check or a successful build.
