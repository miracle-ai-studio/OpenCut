# Classic visual renderer

This package executes one fixed `classic-v31-gpu-normal-frame` plan from the Rust
composition/application session. It imports no Web workspace. It implements
actual Image/Video decoding, ordered quads, contain projection supplied by Rust,
translation/rotation/flips, raw opacity and Classic normal blending with the
original MIT WGSL. The native adapter streams RGBA into a bounded private PNG
sink; the Worker owns authorization, budget leases and publication.

Rust supplies exact project/sequence/revision/time identity, decoder request
seconds and requested rational time, source-window decisions, decode admission,
and final bound geometry after receiving actual display dimensions and PTS.
JavaScript does not infer timeline clocks, retime, animation, track order or gain.
Unsupported graphics/text/fonts/effects/masks/transitions/blend modes fail in
preparation; no Canvas2D compositor replaces WebGPU.

The kernel uses `bgra8unorm`, the original non-GL format. Each layer first renders
into a cleared UNORM texture, then the original normal blend shader combines it
with the prior scene. Readback converts BGRA to RGBA. This preserves intermediate
8-bit quantization and encoded-component blending; opacity is not clamped before
the layer pass. The initial clear is opaque black. Solid background is generated
with Canvas fillRect and uploaded unpremultiplied, preserving the original
premultiplication/unpremultiplication conversion. Canvas2D is only a source image
or color preparation step, as in Classic.

Static images require actual PNG encoding without APNG control chunks or JPEG
without MPF/multiple images. GIF/WebP/other encodings are currently rejected.
Transparent video is explicitly rejected before decoding; pinned mediabunny can
fallback to color-only output if its separate-alpha merger fails. RGBA PNG
alpha remains supported.
Encoded header dimensions and actual browser display dimensions both obey decode
admission; stored intrinsicSize is not authority. Video requires exactly one
video track, actual primary codec support, and a single initial
`CanvasSink.canvases(requestedTimeSeconds).next()` from pinned mediabunny. The
iterator is returned and input disposed. A decoded frame is valid only for its
observed half-open PTS interval. EOF, timestamp gap and prepared outside-window
are explicit no-frame facts; decoder failures are errors. This single-frame
profile does not claim equivalence with stateful Classic preview caching.

## Host setup

Production dependencies are exactly `mediabunny@1.41.0` (MPL-2.0) and
`playwright-core@1.63.0` (Apache-2.0), with a formal local file dependency on the
Classic audio package solely for its **internal host runtime**. Its audio
composition/DSP and public protocol are not consumed. The lock retains exact
package integrities. No browser is downloaded by this package. Configure trusted
absolute Node, package and Chromium locations in host assembly; the private Node
runner is `host/runner.mjs`. Browser execution is Unix supervised and uses an
explicit actual Chromium executable. Runtime metadata and entry files are
checked before availability is advertised.

For this checkout, the three ignored node_modules links reuse the already
installed exact audio dependency packages and the local shared runtime package;
no duplicate dependency tree or browser cache is created. Packaged deployments
must include the two exact installed dependencies and the sibling internal
runtime package using the declared dependency topology.

The visual-v1 private NDJSON protocol uses protocolVersion=1 and executionId on
all packets. Begin supplies fixed plan, opaque host resource handles, admission
and trusted runtime configuration. Correlated requests are limited to
resolveVerifiedBytes/readVerifiedBytes/releaseVerifiedBytes/admitDecode/bindFrame.
Verified bytes transfer in <=65536-byte chunks, with SHA-256 rechecked in the
realm. Bound frame and complete ordered decoded facts return through Rust; the
browser does not manufacture a second project model. Output rgba packets contain
canonical decimal offsetBytes and <=65536-byte, four-byte-aligned bytesHex.

The shared runtime bounds NDJSON queues, module reads, callback reassembly,
process stderr, startup time and termination. Fixed modules are routed in-process
without a listening server. External network/WebSockets, service workers and
unregistered modules are denied; only authorized blob media/images are decoded.
The execution module digest covers visual src/host, exact mediabunny bundle,
original WGSL and all actually imported shared Node control/validation files.
Shared Node files are hashed but never served. Names are sorted and separated
from captured bytes with NUL, so execution identity also changes for lifecycle
code changes. Shader source identity and MIT provenance are in
`vendor/classic-v31/source.json` and LICENSE.

GPU texture/buffer allocation and CPU encoded/decoded/output staging have
separate explicit budget counters. Actual source/header dimensions are admitted
before decoder allocation; driver/codec internal allocations remain subject to
process supervision. Actual browser hash/version, module/shader hashes, GPU API,
observed adapter information and texture format are recorded. Adapter information
can be browser-redacted; this is not a complete driver reproducibility identity.

`monitored-terminate` samples Node/browser process RSS and terminates on breach;
it is **not** a kernel hard limit, and the accounting is not a hard VRAM quota.
`strict-hard` requires an already effective Linux cgroup v2 memory quota and is
rejected where unavailable, including macOS. Launch/connect/navigation use the
remaining monotonic execution deadline. Rust allows five seconds for successful teardown and a bounded fifteen seconds
for failed/cancelled startup, continuously consuming scoped late browser PID
packets before reaping or emergency termination.
Complete/cancelled follows actual browser exit; native success additionally
requires Node exit and browser-group termination acknowledgement. RGBA is
private and tentative until Worker publication succeeds.

## Verification

`npm test` runs five finite Node gates for static encoding, actual dimensions,
private plan/resource handles, module/shader registry boundaries and semantic
JSON facts comparison independent of object key ordering. These do
not establish GPU rendering. Native integration fixtures in
`crates/native-services/tests/classic_visual.rs` use tiny generated CAS PNGs and
independent fixed RGB coordinates for contain/layering/opacity/flips/rotation,
track/source alpha over a colored background, plus a
lossless two-frame video source-time oracle. The root agent runs the actual
Chromium/Cargo acceptance and records its results in the canonical ledger.
