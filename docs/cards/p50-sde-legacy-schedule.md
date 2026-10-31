# P50: balance complete legacy tests across the existing four SDE shards

Own scripts/sde_cpu_tests.py, scripts/diagnostics/p50_schedule_controls.py,
the SDE join control/trigger and this card. No Rust test bodies or inputs change.

The original four-way name hash put 0/980/2000/4060 quick-tier legacy requests in
its four buckets. Assign the seven unchanged complete tests to 1680/2000/1680/1680
requests instead. Put the largest legacy input group in the previously lightest
observed native bucket1; bucket2 retains the boundary/refusal-heavy pair. These are
input counts, not runtime predictions. Full production timing is still required.

All non-legacy targets, other partition counts and unsharded budget probes keep
their original behavior. Unknown future legacy names retain the hash fallback.
Controls read actual Rust test names and original validated input counts, prove
every name exactly once for all1..16 counts, enforce the load bound, verify other
targets' original hash and preserve unsharded probes. Exercise the complete frozen
41-target Windows/Linux native inventories and actual saved protocol controls too.

Keep the original four shards/profile, CPU flags, tier, time budgets, tests, inputs,
comparisons, raw proof requirements and ten LUT jobs. P43/P47 unsharded budget
evidence remains applicable; require all ten complete canonical full profiles and
combined normal21 before landing. No new oracle, fixture, waiver, API or arithmetic.
