# P39: land the follow-on initial legacy data matrix

Land the P33-P37 test chunks and P38's two-line SDE trigger correction as one coherent
follow-on to P32/PR86. No library, oracle, fixture, API, dependency, arithmetic, waiver or
held fixed-function GPU change. All 59 fixed-function cases remain unmerged.

Final Rust source `49b0435f` passed all four Windows/Rocky debug/release profiles in
41111548180. Independently checked artifacts verify exact source/validation identities,
7 tests, 7,120 full-tier requests, zero ignored, 1,080 exact uniform/property rows, guards,
target clippy, main-mode generation and identical generated files. The final matrix includes
5,739 shader/texture outcomes and 1,381 matching native refusals (invalid-size and unsupported
shader cases are compared exactly). Proof: `docs/legacy-gpu-matrix-validation.json`.

P33/P34/P35 also have independently verified standalone four-profile runs 41105867177,
41107619588 and 41109054528. P36's reverse-order implementation is unchanged in final source
and its 280 requests pass all four final profiles; do not conflate that with its separate
standalone workflow's completion state.

Coverage includes allocation/file cuts, mixed 1D/3D LUTs, ranges, zero-alpha baking, initial
exposure/contrast/gamma snapshots before/inside/after a lattice, boundary/large sizes and
both ordinary/legacy cache orders. Quick CPU tier checks 7,040 requests, with edge32 in
the large sweep; full tier checks 32/64/129. Post-extraction mutation is outside this scope.

Runtime tests equal final source49b. P38 adds only core/GPU package path triggers and keeps
all existing SDE jobs, rows, tiers, filters and timeouts. Normal diagnostic ci.yml is excluded.
First parent is frozen PR86 head51e6; second parent is the metadata component. Generated
files must match all four final focused artifacts. Counts stay CPP1188/GPU205/Python384.

Require all17 normal final-head checks and all10 complete41-target kernel manifests before
landing. Retarget to main only after the prerequisite merges with the identical base SHA.
Keep the external debugger/protected evidence and all owner GPU holds unchanged.
