# Ordinary multitrack admission and legacy linear audio

The Hainan walkthrough reproduced an ordinary two-source 1080p scene with one
locked font failing at its 24-second cut. The native Scene wrapper reserved four
full CPU canvases even with no active mask, effect or blurred backdrop, on top
of the child output, text, font and decode allocations. The decoder consequently
received a cap smaller than one known 1080p source. Inert Scene wrappers now add
no CPU scratch reservation; active Scene work retains its previous conservative
reserve. GPU, encoded, decoded, output and process limits remain unchanged, and
the adapter continues to charge its actual allocations and check observations.

The same project exposed an explicit rejection of animated legacy linear gain
in preview and waveform display. Preview now samples those physical amplitudes
on the existing exact Classic export clock, validating immutable keys once per
block. Authored dB animation keeps its 60 Hz physical-amplitude ramps and source
preparation. Linear envelopes keep their existing source processing mode and
seek-independent element/sequence clocks. Static temporary dB edits still cannot
replace animated or legacy gains. Waveform display uses the original 200-point
midpoint grid with the matching physical gain unit; raw source peaks and cache
identity stay unchanged.

These changes stay in pure composition and the existing application admission
and source-art use cases. Native assembly, public declarations, domain ownership
and dependencies are unchanged. Analytic amplitude and budget tests precede
short cut/preview checks and one integrated export. This scope does not close
the retained long-audio, large-media or full migration acceptance gaps.
