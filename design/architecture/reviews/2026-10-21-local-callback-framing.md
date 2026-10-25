# Local callback framing boundary review

The ordinary Hainan multitrack walkthrough exposed large, repeatedly escaped
callback metadata between the native adapter and its local browser supervisor.
This is transport work, not a new editing or rendering use case.

`opencut-native-services` may directly use the already locked `flate2 = 1.1.9`
for lossless RFC 1950 framing. The dependency is confined to the existing native
transport module. Domain, time, composition and application do not depend on it.
Only this normal dependency edge is added to the reviewed baseline; no migration
exception or generated allowlist is introduced.

Public tools and DTOs remain unchanged. The private receiver accepts the original
identity encoding and an explicit zlib encoding; decoding errors never retry an
alternative encoding. It validates ordered chunks, execution correlation, exact
decoded length and the existing decoded/shared reply budgets before publishing a
value. Compressed and reserved decoded storage are both charged. Packet limits,
caller checks, cancellation, processing policy, leases and publication fences
remain in place.

The application also owns one private, pure byte-encoding helper shared by its
audio and visual callback sessions. It replaces per-byte temporary allocations
without changing either callback shape or any native resource policy. This
internal module introduces no dependency edge and does not expose a public tool.

Export reuses the preview transport's typed-compilation pattern: one application
compiler supplies exact facts after the same live checkpoint, and the native
adapter serializes them directly. The local supervisor carries a bounded UTF-8
JSON string to the isolated page, where the original parser and mixer validate
it. No source clocks, sampling, gains, byte ceilings or publication fences move
into transport code. The ordinary callback view remains a serialization adapter
over that same compiler, without a second implementation or execution fallback.

The native export packet projector and existing browser packet module share
sample tables only after exact equality checks. Every original voice frame still
counts toward the existing work admission; sharing never raises the 32-voice or
65,536-sample ceiling. Export fractions remain u64/u128 decimal strings and gain
tables use bitwise float equality. The header uses the existing composition
serializer, and the isolated page expands tables before the unchanged mixer.
This new internal native module introduces no dependency edge or public DTO.

The unchanged interpolation formula can reuse parsed facts within one admitted
block only for frozen own string data properties. Mutable points and getters
retain validation on every read, and every source still checks its own decoded
bounds. The cache is discarded with the block; it neither retains scene data nor
authors new timing. Analytic stereo PCM, zero-tail and mutable/getter controls
cover this optimization before ordinary-media acceptance.

Transport tests cover escaped identities, Unicode, exact reconstructed bytes,
ordered bounded chunks, unknown encodings, expansion limits and shared-budget
release. The same immutable revision must pass actual multitrack media acceptance
before this framing change is recorded as runtime verified.
