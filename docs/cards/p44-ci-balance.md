# P44: balance complete normal CI by measured target time

Own xtask/src/shards.rs, .github/workflows/ci.yml and this card. Base is merged P40
be369c75000cefebfc8faf992b553add930ff6ae; no SDE stack or diagnostic workflow mixed in.

In run 41126692114, Windows release rest took 156.7 minutes. Tone, hue and primary
binaries contributed 36.7, 31.8 and 20.7 minutes. Move primary into existing heavy,
and tone/hue into one new grading group. Rest remains the fallback for every other
library, binary, integration target and doctest. All target filters still select over
the whole workspace; feature unification, full tier and live oracle checks are unchanged.
All eight test jobs/platform remain eligible to run concurrently; serializing profiles
would erase the intended critical-path improvement. Runner queue time must be reported
separately from test execution time.

Existing coverage tests must prove every workspace unit exactly once on each platform
and profile, including new fallback targets and duplicate-name rejection; matrices must
match all 16 shards. Exercise generated commands against actual Cargo metadata in CI.
Run the original complete Windows/Rocky debug/release checks and compare selected test
inventory/counts with the old grouping. Missing, duplicated, skipped or failed work
rejects the change. Required platform aggregators retain their all-success conditions.

Expected critical path from replayed timings is approximately 80-90 minutes, limited
by formats, but the actual new run must measure this. No speedup claim until measured.
No upstream test, oracle, fixture, input, comparison, waiver, API or dependency changes.
