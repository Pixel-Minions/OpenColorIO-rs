# P96: bound per-job SDE execution and retain complete selections

The old four-way Windows snb bucket1 completed 380 tests, including two legacy
tests taking 5,588s and 5,301s, then hit GitHub's six-hour limit with 169 cases
unproven. Equal request counts did not mean equal runtimes. The cancelled driver
could not write its final summary. No recorded case failed before cancellation.

Owned files: scripts/sde_cpu_tests.py, scripts/sde_complete_profile.py,
scripts/diagnostics/p64_targeted_controls.py, p50_schedule_controls.py and
p77_profile_controls.py, .github/workflows/sde.yml, .github/workflows/ci.yml,
the P95 card and this card. CI.yml returns to its original p64 invocation because
p64 already imports every profile control; do not run the same controls twice.

Use sixteen kernel partitions and two LUT partitions on every CPU/platform.
Each original LUT sweep has its own partition. The Cargo target/filter/tier plans
are unchanged, and joins still require every original case exactly once. The
profile CLI explicitly requires sixteen joined kernel partitions and two joined
LUT partitions. Its original four-way/unpartitioned defaults remain available for
validating run41386558773's historical receipts; wrong partition counts fail.

The parent passes a shared monotonic deadline to its Cargo runners. Its execution
budget is at most four hours, covering the preflight and selected test phase;
native setup remains separate. Each case retains its existing watchdog ceiling,
bounded by the remaining job time. On expiry the selected plan is unchanged,
unexecuted names remain unproven, the case/target fails, and the driver persists an
incomplete summary. No timeout or partial selection can qualify a profile. The
budget leaves time for setup and artifact export before GitHub's six-hour limit.

Native controls exercise a real sleeping child, kill it at the job deadline,
reject disabled/non-finite budgets, retain the unexecuted names, prove the
partition union for every supported count, and reject wrong profile partition
metadata. Existing raw-log, source, CPU, native-inventory and duplicate controls
remain unchanged. There are no numeric/tolerance/waiver or SDK changes.

Recorded snb case timings projected onto sixteen buckets peak at 114.67 minutes
on Rocky and 142.27 on Windows. Windows' projection excludes the 169 unproven
cases; this is planning evidence, not execution or qualification of a new plan.
All original passed snb receipts remain available. Only its missing execution
will run next; neither those passes nor either ICC sweep needs repeating.

The full matrix and its opt-level0 depth tests still require future owner
confirmation on the exact release/tagged commit. This chunk dispatches no run.
