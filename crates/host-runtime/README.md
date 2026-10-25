# Native host assembly

`opencut-host-runtime` constructs SQLite, CAS, native media, plugins and Worker
adapters. CLI/HTTP/MCP consume this assembly; they must not reconstruct editing,
authorization, storage or media semantics in transport code.

Native embedders use `opencut_host_runtime::Automation`. Portable integrations
implement application `ToolExecution` and use `opencut_automation::dispatch` with
the registry-backed `ToolRequest`. Authorization, refusal auditing, typed input
and atomic/idempotent application behavior remain enforced at their boundaries.

Workers inject `JobStoreFactory`, `JobStore` and `ExecutorPort` into the scheduler.
Publication requires atomic attempt/lease fences; prior scheduler checks are not
a substitute. Native execution owns bounded resources, heartbeat, cancellation,
progress and attempt-local outputs.

`workspace::Workspace` exposes `WorkspacePort` through an opaque native handle.
`transport_io` owns bounded streams and upload/preview transport. Neither exposes
the concrete SQLite application for transport-side use cases.

Changes to this assembly require architecture and contract-boundary checks.
