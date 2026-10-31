# P48: compile the existing wheel client before entering SDE

Own scripts/sde_native_warmup.py, the kernel setup/trigger in sde.yml and this card.
The legacy command first calls native_build.executable(), which builds its C++ helper
if its source-keyed executable is absent. In an SDE test this can instrument compiler
children unnecessarily; the helper is setup code, not an expected numeric result.

After native oracle setup, locate its exact environment through Cargo's target directory
and run the unchanged existing builder with the oracle's allowlisted environment. Record
the pinned wheel/source/library provenance and compiled binary digest. The existing build
key, C++ source, compiler flags and loaded library are unchanged. Do not cache oracle
answers or move any actual comparison out of emulation. Every legacy request retains the
ordinary/native identity check and is executed normally under its selected CPU profile.

No oracle source, expected result, fixture, test input, tier, threshold, waiver or public
library API changes. P43/P47 cold probes remain conservative runtime evidence; full ten
profiles, ten LUT jobs and exact combined normal21 are still required. Validate actual
warmup/setup on both platforms before dispatching the complete production matrix.
