# Native host assembly

`opencut-host-runtime` constructs SQLite, CAS, native media, plugin and worker
adapters. CLI/HTTP/MCP choose transport inputs and trusted identities, then use
this host; they must not reconstruct database or media implementations.

## Rust embedding migration

Existing native embedders change `opencut_automation::Automation::open(path)` to
`opencut_host_runtime::Automation::open(path)`. `call`, caller context and public
wire envelopes retain their behavior. This is a Rust source API migration,
not a promise that the old Rust import path remains compatible.

Portable embedders implement application `ToolExecution`, then call
`opencut_automation::dispatch`. Inputs cross that boundary as the registry-backed
`ToolRequest` enum. The dispatcher enforces context, authorization, decoding and
refusal auditing; implementations must recheck credentials in `begin` and retain
application transaction/idempotency semantics.

Custom workers inject `JobStoreFactory`, `JobStore` and `ExecutorPort` into
`opencut_job_runtime::run`. Execution never publishes directly: final state
changes remain fenced by attempt and lease. The native executor owns heartbeat,
progress, cancellation, admission and attempt-local outputs. Storage adapters
must enforce fences atomically, not only trust the scheduler's preceding read.

`workspace::Workspace` exposes `WorkspacePort` through an opaque native handle.
`transport_io` owns bounded native streams and upload/preview operations. Neither
interface exposes the concrete SQLite application for transport-side use cases.
The host retains the legacy native facade internally while further per-use-case
splitting remains possible; this is not a new domain layer.

Run the repository architecture and contract-boundary gates for changes to this
assembly. The dependency policy deliberately allows native edges here and
forbids them in portable dispatch/scheduling crates.
