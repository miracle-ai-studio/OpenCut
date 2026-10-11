# OpenCut development guidance

Read [AGENTS.md](../AGENTS.md) for module boundaries and required checks.

- OpenCut supplies editing and media tools to external agents. Keep model calls,
  conversations and creative orchestration outside the editing engine.
- CLI, HTTP, MCP and clients consume shared application behavior.
- TypeScript clients use generated contracts and the SDK public entry point.
  Do not copy response parsing or import private implementation files across apps.
- Treat unknown write outcomes as unresolved. Preserve idempotency, cancellation,
  caller identity and structured errors.
- Keep accessibility and keyboard behavior intact in UI changes.
- Run affected behavior tests and boundary checks. Keep build output, local media,
  credentials and test-run directories out of source and delivery packages.
- Report implemented code, verified behavior and unfinished work separately.
