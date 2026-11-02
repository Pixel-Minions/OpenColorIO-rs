# P92: qualify the upstream Python suite in release as well as debug

The release audit found that py-check and gate's upstream Python-suite step built
only the debug extension. Existing release Rust tests did not exercise upstream's
unmodified Python modules against the optimized extension.

Owned files: xtask/src/py.rs, xtask/src/gate.rs, .github/workflows/ci.yml and this
card. py-check keeps its existing clippy/binding checks and complete debug suite,
then builds the release extension and runs the same complete upstream suite.
Both required platform-check jobs already call py-check; their names now expose
the two-profile coverage. No new matrix job or separate full CI round is needed.
A release gate likewise adds py-suite --release after its release binding tests.

Both profiles retain every upstream module, all 384 required functions, inherited
runs, exact wheel outcomes, the original standalone driver and its CLI checks.
No passing-list update, filter, expected value or comparison changes. Keep debug
outcomes in py-suite.json and release outcomes in py-suite-release.json, with an
explicit build_profile field. A --no-build run records null rather than claiming
to have built a particular profile. The suite always rebuilds in these CI/gate
paths; --no-build is not used.

Run formatting, xtask clippy/unit tests and source guards locally under the
existing caps. The grouped exact-head normal CI must then build and run both
profiles on Windows and Rocky Linux 9. No local uncapped binding build is needed.
Release packaging/entrypoints, GPU compile checks and notices belong to Claude
and are unchanged by this chunk.

Local validation of the final source: all 102 xtask tests pass under the 1.5GiB
cap; xtask clippy with warnings denied and workspace formatting pass. The upstream
Python runtime pass in release is intentionally established by the two required
normal platform checks on the grouped land SHA, not inferred from these unit tests.
