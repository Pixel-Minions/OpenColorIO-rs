# P80: finish the private log-test support

Own ocio-ops/src/unit_test_log_utils.rs and test-only first-match wrappers in
std_regex, plus the two source mappings. Constructors, capture, level restoration,
clear/empty and the reentrant test lock already existed. Add the missing literal
line removal, one-match-at-a-time regex removal, raw diagnostic print and muting
guard. Role/message-specific helpers remain with their existing ocio callers.

All new matcher entry points are crate-private and cfg(test); no public Rust helper
or SDK matcher behavior changes. Keep each platform matcher's existing stack and
refusal limits. C++ removes one match, then starts over: replacing all original
matches together is not equivalent for arbitrary patterns. A platform C++ probe
of input ax-space-y and pattern x-space OR word-boundary-y produces first span1..3,
then ay; all-at-once replacement produces a. Both actual Rust matcher modules pass
the isolated probe against that observed result. Evidence stays outside target in
kept/continuation/2026-11-01/p80-{cpp,rust}-probe*. Native probes use only installed
tools with a0.75GB cap; they do not compile or run the SDK.

Four helper controls cover literal removal, ordered regex deletion, raw bytes and
muting without changing the level. Integrated ocio-ops compilation/runtime remains
required in both normal CI profiles/platforms before landing. No oracle/fixture,
renderer, dependency, unsafe, numerical comparison, waiver or deviation changes.
The upstream empty-match infinite-loop limitation is preserved and recorded as I-800.
