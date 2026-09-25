<table width="100%">
  <tr>
    <td align="left" width="120">
      <img src="https://assets.opencut.app/branding/symbol.svg" alt="OpenCut Logo" width="100" />
    </td>
    <td align="right">
      <h1>OpenCut</h1>
      <h3 style="margin-top: -10px;">A free and open source video editor for web, desktop, and mobile.</h3>
    </td>
  </tr>
</table>

[![Discord](https://img.shields.io/discord/1386309140057690133?label=Discord&logo=discord&logoColor=fff&color=5865F2&style=flat)](https://discord.gg/zmR9N35cjK)
[![X](https://img.shields.io/badge/follow-%40opencutapp-000?logo=x&logoColor=fff&style=flat)](https://x.com/opencutapp)
[![License: MIT](https://img.shields.io/badge/license-MIT-green?style=flat)](LICENSE)

## Status

**OpenCut is being rewritten from the ground up.** What's coming:

- An Editor API
- First-class third party plugins (made possible by a plugin-first architecture)
- Desktop, mobile, and browser from one codebase (Rust core)
- MCP server (for AI agents)
- Headless mode (automation, batch rendering)
- A scripting tab directly in the editor

You can still find the previous version at [opencut-app/opencut-classic](https://github.com/opencut-app/opencut-classic), which is the one to reach for today. [opencut.app](https://opencut.app) still runs the classic version. The rewrite will live at [new.opencut.app](https://new.opencut.app) until it's ready to take over.

## Local editor and Agent host (in development)

The working implementation shares project transactions, revision history, jobs,
and schemas across the CLI, HTTP API, and stdio/Streamable HTTP MCP server. The Web editor connects
to the local HTTP host; the GPUI shell is not connected to these services yet.

```sh
cargo build -p opencut-cli
target/debug/opencut --json system domains
target/debug/opencut --json system domains --domain edit --schemas
target/debug/opencut --database .opencut/workspace.sqlite serve \
  --listen 127.0.0.1:8788 \
  --token-file .opencut/api-token \
  --allow-origin http://localhost:5173
```

Start the Web app with `moon run web:dev`, open `/editor`, and connect using the
host URL and the token in `.opencut/api-token`. Media jobs require `ffmpeg` and
`ffprobe`. The HTTP host starts a worker; standalone CLI/MCP users can run
`opencut worker run` against the same database. All processes stopping also stops
computation; a subsequent worker recovers unfinished jobs after their lease expires.

```sh
target/debug/opencut --database .opencut/workspace.sqlite mcp serve
target/debug/opencut --database .opencut/workspace.sqlite --json job events --limit 100
```

Local speech transcription is available through `opencut_analysis_transcribe` (media action `analysis.transcribe`). Configure `OPENCUT_WHISPER_MODEL` on the worker host and install `whisper-cli`; clients reference a fixed project revision and asset, and receive segment timings through the existing public Job view. See the [transcription contract](design/architecture/transcription-contract.md) and [SDK example](packages/sdk-ts/README.md). Word alignment and speaker identification are not implemented.

MCP advertises nine domain tools by default (`system`, `project`, `edit`,
`media`, `task`, `observe`, `plugin`, `access`, `workspace`). Read
`opencut://schemas/domains` and then `opencut://schemas/domains/{domain}`
for the exact action contracts. For example, the CLI form is
`opencut --json domain edit plan --input request.json`; the MCP form calls
`opencut_edit` with `{"action":"plan","input":{...}}`. The underlying
61 versioned operations remain available through `opencut call` and the
HTTP API. Existing MCP clients may call their old tool names directly, or
start the host with `--tool-surface legacy` to advertise the old list.
Domain calls resolve to the same leaf operation before authorization,
validation and execution. Media bytes use the separate upload data endpoint.

The HTTP host also serves Streamable HTTP MCP at `/mcp`; `mcp serve --transport
streamable-http` starts the same host. Network clients use the host bearer token,
then retain the negotiated `Mcp-Session-Id` and `Mcp-Protocol-Version: 2025-11-25`.
Tools, resources, prompts and subscriptions share the stdio implementation.
See the [MCP transport contract](design/architecture/mcp-http-contract.md) for
session limits, bounded replay, expiry and disconnect behavior.

`opencut --json system capabilities` includes `job_kinds`: nine native job names
with their public input payload and successful result JSON Schemas. The Web
contract generator emits their concrete TypeScript input and result types. Automation
and workers share typed task contracts; stored job records retain their JSON
extension boundary. Legacy results without media-lock fields remain readable as
unlocked results.

See the [architecture and contracts](design/architecture/agent-friendly-editor-v1.md)
and [implementation and verification status](design/architecture/implementation-status.md)
for the supported scope, reproducible checks, and remaining work. Source previews,
rendered project artifacts, and job-backed project frame previews use distinct
interfaces. A full interactive project preview and complete Classic feature parity
are still in development.

`preview.project_frame` accepts a fixed revision and a sequence time through the
same job API as exports:

```json
{
  "kind": "preview.project_frame",
  "idempotency_key": "preview-a-1",
  "input": {
    "project_id": "project-a",
    "revision": "<revision from project inspect>",
    "sequence_id": "main",
    "requested_time": { "ticks": "125", "timebase": 100 },
    "alignment": "nearest"
  }
}
```

Save the request as `frame.json` and run `opencut job start --input frame.json`
with the same `--database` used by the host. A successful job returns a PNG
artifact, its fixed revision, and `actual_sequence_time`. This mode renders the
whole sequence before extracting the frame; `quality: encoded_project_frame`
identifies that cost and fidelity boundary. It is not low-latency timeline playback.

`preview.composited_frame` accepts the same public input and outputs a PNG directly,
with `quality: composited_project_frame`. It resolves visual media only and does
not encode an intermediate movie or process audio. Audio clips still contribute
to the sequence duration, so an audio-only tail can produce a black frame. The
current implementation may decode the source prefix to reach the requested frame;
it is not continuous or real-time playback. Direct PNG pixels can differ from
frames decoded from a lossy export. The Web editor exposes both preview modes.

`preview.project_range` previews a fixed audiovisual sequence interval as an
H.264/MP4 artifact. Its input contains `project_id`, `revision`, `sequence_id`,
`requested_range: {start, duration}`, and `alignment`; times use the same exact
`{ticks, timebase}` representation. The result returns the requested range and
`actual_sequence_range`, plus `quality: encoded_project_range`. Both interval
endpoints are aligned on the sequence frame grid; collapsed or out-of-bounds
intervals are rejected. The output plays from zero while its content retains the
absolute sequence animation and audio timing. This path encodes the selected
interval but may decode the prefix, and currently locks all required audiovisual
assets in the sequence. It does not provide continuous timeline playback.
The [continuous-preview plan](design/architecture/continuous-preview-plan.md)
defines the next session, playback-clock, cancellation, and observation contracts;
those session APIs are not implemented yet.

Before submitting a project render, `opencut render preflight --input render.json`
checks a fixed revision. Its input contains `project_id`, `revision`, `sequence_id`,
`container`, and `video_codec`; HTTP exposes the same check at
`POST /v1/render/preflight`. The report lists object-linked issues and creates no
job. Readiness describes the checked environment, not a guarantee of later success.

New `asset.bind` and `asset.replace` transactions pin media into the local
content-addressed store and save its SHA-256 and size as `Asset.version` in the
revision. Replacement keeps the logical asset ID and existing clip references;
historical revisions keep their old content. `edit.plan` remains read-only and
reports `unresolved_asset_ids` for bindings without a version.

Project export and project-frame jobs lock their required inputs before
acceptance. Their `asset_lock` records hashes, sizes, revision, and sequence.
Workers verify and privately copy those bytes. New jobs from a versioned revision
reuse its pinned content even after the provenance file changes or disappears;
missing or corrupt CAS objects fail without falling back to that file. Retries
with the same idempotency key return the accepted result.

Legacy unversioned assets are still supported and are captured when a project
job is accepted; their historical documents are not silently upgraded. Persisted
older jobs without a media lock explicitly report `legacy_unlocked`.
Font/plugin/tool dependencies, source-only jobs, and CAS garbage collection still
need further work. See the implementation status for verification of the current
asset-version integration; the full reproducibility contract is not complete.

## Development

The standalone [TypeScript SDK](packages/sdk-ts/README.md) exposes the registered
automation tools without depending on the Web application. Its types, tool schemas,
and [OpenAPI document](packages/contract-fixtures/openapi.generated.json) are generated
from the Rust registry. Run `npm ci` in `packages/sdk-ts`, then
`node scripts/verify-sdk.mjs` from the repository root to build a package, install it
in an external consumer, and verify real HTTP calls with independent CLI readback.
This does not replace the remaining R2 architecture or full-product acceptance work.

The [Rust SDK](crates/sdk-rust/README.md) uses the same typed tool and job registry.
Run `python3 scripts/verify-rust-sdk.py` to compile it outside the repository and
verify real calls, including replay of the same transaction from the TS package.
The [contract boundary gate](scripts/check-contract-boundaries.py) rejects native
execution dependencies in the public contract crates and client dependency tree.

Install [proto](https://moonrepo.dev/proto) if you haven't already:

**Linux, macOS, WSL:**

```sh
bash <(curl -fsSL https://moonrepo.dev/install/proto.sh)
```

**Windows (PowerShell):**

```powershell
irm https://moonrepo.dev/install/proto.ps1 | iex
```

If shims fail to run, allow local scripts for your user:

```powershell
Set-ExecutionPolicy -Scope CurrentUser RemoteSigned
```

From the repo root:

```sh
proto use    # installs the tools pinned in .prototools
```

```sh
moon run web:dev       # localhost:5173
moon run api:dev       # localhost:8787
moon run desktop:dev   # see apps/desktop/README.md
```

## Local development delivery

The Rust CLI/HTTP/MCP host and standalone SDKs can be packaged with `python3 scripts/package-local.py` and verified outside the repository with `python3 scripts/verify-local-package.py <archive>`. See [installation and upgrade instructions](design/architecture/local-distribution.md) and the [R2 completion audit](design/architecture/r2-completion-audit.md). These are local development bundles, not a complete R0–R7 product release.

CLI and stdio MCP accept project-scoped credentials through `--credential-file`; REST and Streamable HTTP accept the same credential as a bearer. Local owner administration remains under `access`. MCP sessions bind a credential and policy revision; streams and artifact downloads recheck authorization. See the [permission contract](design/architecture/access-control-contract.md) and [network acceptance](design/qa/r2-network-access-2026-09-21.md). Scoped media import, preflight/plugin access remain unfinished. Owner administration history is available through `opencut_access_audit_query` or `access events --input FILE`.

## Contributing

We're not set up to take outside contributions yet while the architecture is being designed. If you want to follow along, ask questions, or just hang out, [join the Discord](https://discord.gg/zmR9N35cjK) or [open an issue](https://github.com/opencut-app/opencut/issues).

## Sponsors

OpenCut is supported by companies that believe in open source creator tools.

- [**fal.ai**](https://fal.ai?utm_source=github-opencut&utm_campaign=oss): Generative image, video, and audio models all in one place.

Want your logo here? Reach out at [sponsor@opencut.app](mailto:sponsor@opencut.app).

## License

[MIT](LICENSE)

Media upload and repair now journal native publication before changing managed files. `opencut_maintenance_query` and `opencut_maintenance_reconcile` expose owner-only provenance and locked reconciliation; an unknown outcome must be inspected before retrying. See the [native publication contract](design/architecture/native-publication-contract.md). Plugin installation is not yet connected to this journal.
