# Classic browser audio execution

This internal package executes a fixed Rust V4 audio composition in a browser.
It accepts immutable CAS resources and compiled sample facts, not project DTOs,
source URLs, local paths, timeline edits or keyframes. Host assembly must bind
every compilation callback to the same fixed project revision and policy.

`renderClassicExport(plan, host, { signal, maxPcmBytes, maxEncodedBytes })`
returns whole-scene mastered PCM, or a range sliced **after** whole-scene
mastering. Rust chooses placement, authored duration, source position, retiming
profile, channel participation and physical gain. `src/index.d.mts` defines the
typed in-process ports. This is not a new frozen public wire declaration.

The host connects `compilePitchPreparation` to
`classic_audio_preparation_facts_v4`; its runtime `decodedFrameCount` maps to the
helper's `decodedFrames`. The host supplies the original authored duration and
source span and applies the helper's four explicit budgets. It connects
`compileSourcePositions` to the original exact source clock and `compileBlock`
to `compile_audio_block_v4`. Prepared buffers use source origin zero and unit
rate; gain remains on the original element-local clock. Decimal strings retain
u64/u128 facts across a JS boundary. JS never derives authored floors, ceils,
curves, trims or placement from floating-point seconds.

The typed Rust session in `crates/editor-application/src/classic_audio` and the
native process adapter in `crates/native-services/src/classic_audio` implement
these callbacks. `SourceFormat` reports each actual buffer's sample rate, frame
count, channel count and prepared mode. Rust checks those facts against decode
admission and pitch preparation before compiling executable sample blocks.

For video resources, the authorized bytes create one Mediabunny `Input`. Its
primary track supplies `{trackId, codec, sampleRate, channels}` to Rust's
`admitDecode` before a PCM sink or buffer is allocated. Decoding uses that same
track, and the receipt repeats the admitted selection. Rust budgets the actual
selected native channel count; imported first-track metadata cannot substitute
for these facts. The resampled output retains at most the first two channels.
Audio resources keep the WebAudio path and report `primary: null`.

Audio assets use `AudioContext.decodeAudioData` at the requested sample rate.
Video assets use Classic's pinned `mediabunny` 1.41.0 primary-audio sink,
concatenate returned buffers in order and use `OfflineAudioContext` resampling.
This retains Classic's timestamp-gap/decoder behavior rather than substituting
FFmpeg PCM. The production profile requires an unspecified stream: an explicit
stream index cannot be rebound to the primary/default track. A bounded ISO
source-byte capability gate rejects multi-edit or non-unit edit rates that the
pinned demuxer would warn about and discard. This gate covers every track in the
ISO file; it does not claim unsupported non-primary edits can be ignored.
Export normally requests 44100 Hz. Codec priming and resampling are
part of the actual browser engine identity, not universal decoder guarantees.

The vendored, replaceable SoundTouchJS 0.3.0 source is unmodified; its complete
LGPL license and package/source identity are in `vendor/soundtouchjs-0.3.0`.
Pitch execution preserves its 4096-frame ScriptProcessor node, internal default
sample rate and filter history. A maintain-pitch flag at unit or epsilon-close
rates still prepares an intermediate Float32 buffer. No Rust, FFmpeg or newer
SoundTouch implementation stands in for that algorithm.

Mastering scans the complete scene. At peak <=0.98 it returns the mix unchanged.
Otherwise a real WebAudio DynamicsCompressor runs threshold -1 dB, knee 0,
ratio 20, attack 0.001 s, release 0.12 s, followed by output gain 0.98 and final
sample clamping. Preview always-compress/system-volume scheduling is a separate
profile; this export entry does not implement preview scheduling.

Rust's fixed plan retains the complete scene in `frameCount` and the selected
length in `outputFrameCount`. An optional `range` contains canonical decimal
`startSample` and `frameCount`; Rust validates its bounds. The browser compiles
and mixes every complete-scene block, measures the complete-scene peak, runs
mastering when required, and then copies the selected PCM samples. Range jobs
therefore still admit the full mix/mastering allocation plus the range copy;
native output coverage and the WAV length use only the selected frame count.
They never use the selected range's peak to decide whether to master.

The adapter admits at most 32 voices and resources, 4096-frame blocks and 65536
voice samples per block. It reserves decoder upper bounds before decoding,
checks actual format/finite PCM, accounts encoded copies, full-scene mix,
preparation buffers, mastering output and range copies. Decoder/SoundTouch
internal allocations additionally require the host's explicit process quota
profile. Array accounting alone cannot enforce process RSS. `abortRendering`
must actually terminate the realm/process
and acknowledge it: an OfflineAudioContext cannot be cancelled with `close()`
or a promise rejection. Contexts and nodes are closed/disconnected on exit.

`host/runner.mjs` is the isolated production Node entry. Its private NDJSON v1
contract is declared in `host/protocol.d.mts`; stdout contains only protocol
packets. Begin fixes the execution ID, Rust plan, private resource handles,
admission and explicit Chromium executable. Correlated callbacks transfer
authorized bytes in at most 65536-byte chunks and compiled facts in bounded
ordered replies. PCM is stereo planar f32le in at most 4096-frame chunks; chunks
remain tentative until a terminal complete packet follows browser termination.
The runner reports the browser PID for host supervision, hashes the executable
and fixed execution sources, and binds its receipt to that engine identity.
The module digest includes renderer code, vendored SoundTouch, the pinned
mediabunny bundle and all `host/*.mjs` control/page code within the same byte and
file budgets. Entries are sorted by their fixed relative name and hashed as
name, NUL, source bytes, NUL. Host code participates only in the digest and is
never exposed through browser module routes.

Dependencies are package-local, pinned and locked: Playwright Core 1.63.0 and
mediabunny 1.41.0. Installation does not download Chromium; the host supplies
the admitted executable. A fixed registry serves the renderer and the official
mediabunny browser bundle. External URLs, filesystem paths, Web workspace
imports, page WebSockets and service workers cannot supply execution modules.

For a Unix host, set `OPENCUT_CLASSIC_AUDIO_PACKAGE` to this installed package's
absolute directory and `OPENCUT_CLASSIC_AUDIO_CHROMIUM` to an absolute Chromium
executable. `OPENCUT_CLASSIC_AUDIO_NODE` may specify an absolute Node executable;
otherwise the host searches absolute directories in its trusted `PATH` without
invoking a shell. When the package variable is absent, an existing
`libexec/classic-audio-renderer` beside the host executable is the only fallback.
Install the exact locked dependencies beforehand; discovery never downloads,
launches Node or probes Chromium. `opencut_project_v4_audio_capability` reports
missing configuration or unavailable files/pins with path-free reason codes.
The new schema 1 submission tool is `opencut_job_v4_audio_wav`, supporting
44100 and 48000 Hz; `opencut_job_v4_audio_range_wav` submits a fixed sample range
through the same execution and publication path. Each job's captured policy
supplies its execution budgets;
local assembly selects `monitored-terminate` explicitly.

`monitored-terminate` measures the runner/browser process tree and terminates
on a detected RSS or deadline violation. It reports the observed peak and
sampling interval; it is not a kernel hard memory limit. `strict-hard` requires
an existing effective Linux cgroup v2 memory quota. It rejects with
`UNSUPPORTED_AUDIO_QUOTA` on macOS or without that enforced quota; it never
creates cgroups or elevates privileges. Cancellation closes or kills the actual
browser and waits for process exit before acknowledging cancellation. The
native supervisor also owns emergency Node/browser process termination.

The native host retains verified CAS leases and checks cancellation/policy
through execution; descriptor release does not release the execution lease.
This package does not publish artifacts. The native queued Worker writes an
admitted stereo Float32 WAV through an inode-held attempt directory, validates
its exact length and receipt, then publishes through the existing lease and
SQL transaction fence. Publication rechecks the original submitter, current
policy and cancellation; the directory lease is released immediately afterward.
The Web audio dialog and shared Tasks download use these fixed-revision jobs.
This batch's five native Rust-to-browser decoder fixtures passed: independent
Float32 pulses with trim/placement/EOF and dB boundaries; container PTS gaps
concatenated as Classic export does; default stereo selection after a mono
first track; reverse/repeated sample ranges; and one AAC-LC priming fixture.
The AAC fixture's independently inspected edit/packet facts produce 24576
decoded frames, with pulse peaks at samples 4096/16384 and, after a 1024-sample
trim plus 17-sample placement, 3089/15377. These checks do not establish arbitrary
codec priming, complex edit lists, non-integral packet edit offsets, or realtime
seek scheduling. Complex unsupported edits remain rejected. The centralized
Web acceptance also passed the historical-revision range dialog, durable job,
idempotent replay and authenticated Tasks download: 24000 stereo frames at
48 kHz, exactly 192058 bytes, independent 660 Hz / -6 dB PCM, and equality to
the corresponding complete-scene crop. This low-peak range fixture and the
separate high-peak whole-mastering fixture have distinct verification scopes.

Run limited pure execution checks with `npm test`. The browser acceptance
module is a fixture entry for the centralized isolated-browser runner; it uses
small in-memory hand-authored WAVs and independent waveform invariants. The tone
is PCM16 with a frequency oracle. Constant-amplitude/mastering checks use
WAVE_FORMAT_IEEE_FLOAT (format 3, 32-bit) to store 0.5 exactly and keep strict
0.25 mixed-sample and whole-scene peak 2 assertions. Chromium 153 was observed
decoding PCM16 +16384 as 16384/32767; this codec observation changes the fixture,
not product gain or a general claim about every browser's PCM16 decoder.

## Realtime source producer

`host/preview-runner.mjs` uses a separate private protocol declared in
`host/preview-protocol.d.mts`. It prepares each authorized source and pitch
buffer once, then returns at most one 4096-frame unmastered stereo block per
command. It allocates no whole-scene mix and never calls export mastering.
The local player consumes these blocks through its realtime WebAudio master
chain. The Web player imports the same pure `createClassicMasteringChain` from
`src/mastering.mjs`: the actual WebAudio compressor and 0.98 output gain run
continuously, with no export peak gate or whole-scene output allocation. The
producer's PCM is before this graph; downloading a block does not count as
device consumption. Fixed revision, generation and overlay epoch accompany
every block; host assembly owns seek/overlay state and discards stale
completions. Seek and temporary-gain replacement reuse prepared sources in the
same realm, while flushing the old output generation. Stop releases the realm.

Ordinary uploaded audio and video both use the admitted Mediabunny primary
track, preserving observed PTS segments at the native sample rate. Rust maps
the exact source clock into half-open segments: a timestamp gap or actual EOF
produces silence, rather than extending the previous segment. Animated element
gain or maintain-pitch selects the prepared concatenation branch, which joins
decoder buffers, resamples and applies the pinned pitch algorithm when required;
the decoder PTS gaps are absent from that prepared buffer. Rust binds these
observations and compiles segment-local interpolation taps and 60 Hz amplitude
ramps. Ramp anchors belong to the playback generation, not individual blocks;
JS does not derive these clocks or curves. Temporary gain is an explicit Rust
override before mixing and does not write a project revision. Ordinary primary
layouts above
two channels are explicitly unsupported until the real WebAudio downmix is
implemented. The prepared branch retains Classic's first-two-channel behavior.
Every decoded resource is reported to Rust before preparing or decoding another
resource, so retained PCM reduces subsequent admission.

The long-lived producer has one command credit, bounded callback framing and
4096-frame blocks. Host and Worklet credit limit queued output to two blocks
(8192 frames); a read timeout retains its pending command rather than sending
it again. Preparation has a deadline of at most 120 seconds and the session has
a one-hour TTL plus a renewable owner lease. Verified CAS resources and decode,
encoded-byte and process budgets remain held through execution. RSS monitoring
continues while idle; native assembly calls the idle checkpoint to recheck the
original caller's authorization and observe process failure. The quota profiles
have the same explicit limitations described above: monitoring is not a kernel
hard limit. Stop/cancel waits for actual browser exit and the independent
process-group acknowledgement.
All preview browser and Node modules participate in the existing execution
digest. A separate source-binding digest covers the accepted primary, segment
and preparation facts; block completions repeat this digest without copying the
entire segment inventory. Its canonical representation sorts object keys,
preserves array order, encodes numbers as big-endian IEEE-f64 bits and normalizes
negative zero.

The limited source tests cover independent two-voice gain levels, PTS adjacency
and gaps, 60 Hz ramps, generation FIFO and protocol bounds. Centralized actual
Rust-to-browser HTTP checks passed independent 660 Hz / -6 dB ordinary PCM,
temporary -12 dB gain without edits, and exact repeated-seek PCM. The prepared
branch preserved an independent 440 Hz tone under 1.5x retiming and reused the
same PCM after seek. In the final frozen CLI's scoped acceptance, actual product
AudioWorklet consumption reported 4096
frames; pause confirmed Rust's scene time of 8896/48000 seconds. Resume kept
the same session, advanced its generation and produced another real consumption
report. The historical fixed revision and current project head stayed unchanged.

A separate actual product gesture acceptance used two real-source voices at
-6 and -18 dB. Its measured pre-master RMS was 0.054961 versus the independent
0.055427 expectation. Changing only the first voice to -12 dB matched the
independent sum ratio 0.601329 with maximum sample error 6.47e-9. During real
Worklet playback, Escape removed the draft without an edit; pointer release
submitted one unchanged shared transaction. The old session stopped after the
head changed, and a subsequent explicit Play bound the new revision. This
verifies control, mixing and revision transitions, not physical speaker output.

These are separate pre-master PCM and playback-clock observations, not a claim
of complete Classic realtime equivalence or audible-device acceptance. The full
actual realtime PTS-gap/AAC priming matrix, authorization-revocation teardown,
long-session quota pressure and post-master signal matrix remain unverified.
The export/range and export AAC results above do not establish those realtime
cases; ordinary multichannel layouts above two channels remain explicitly
rejected.
