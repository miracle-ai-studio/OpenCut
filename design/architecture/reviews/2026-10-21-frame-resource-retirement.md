# Frame resource retirement

The Hainan walkthrough exposed a persistent decoder closing the previous clip
after native frame admission had moved to the next clip. Rejecting that cleanup
aborted an otherwise valid fixed-revision export at its first cut.

The existing native resource adapter now distinguishes an advertised descriptor
handle from a fresh opaque transfer identity for each open. Continuous frames
reuse their verified descriptor and live transfer. Removing a descriptor revokes
its reads immediately and gives the next frame one release-only acknowledgement
for that formerly active transfer. The set is bounded by the previous frame's
32-resource admission and is dropped with that frame; it contains no file bytes
or grants. Unknown, repeated and out-of-frame reads remain refused. Fresh open
identities prevent an old cleanup from closing a newly opened transfer.

Live policy/cancellation checks, fixed resource versions, descriptor integrity,
byte limits, frame/request correlation and publication fences stay in place.
Domain and application ports, public DTOs, dependencies and the reviewed
dependency baseline remain unchanged. The focused checks cover continued reads,
retirement, cleanup under revoked authority, fresh opens and stale-close refusal,
followed by an actual short export across the original six-second clip boundary.
