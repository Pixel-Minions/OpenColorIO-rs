# P41: bounded SDE partitions and complete profile joining

Independent component based on frozen P31/db540. GPU P40/be369 remains unchanged and
must land only after its own all17 normal checks. Re-integrate on GPU264 main before
any eventual SDE landing. No existing draft head is rewritten.

Own scripts/sde_cpu_tests.py, scripts/sde_join_shards.py,
scripts/diagnostics/p41_shard_controls.py and this card. No Rust arithmetic, oracle,
fixtures, dependencies, waivers, new library API or original test-body changes.

All10 prior profiles exhausted240min. Their13,362 completed invocations had no observed
failed/emulator results, but no complete profile proof exists. Preserve the original
quick tier, pinned SDE/CPU flags and two LUT filters. Split the complete native selected
target/name inventory into deterministic disjoint partitions; every case still runs in
a fresh emulator. Every job performs the actual existing Cargo runner preflight audit.

The joiner must require all partitions, matching complete native plans/commands/CPU
prefixes, exact union with no duplicates, actual one-test pass footers and raw hashes.
It emits the original full-profile report format with provenance, never a reduced
workload claim. One shard alone is explicitly marked as requiring aggregation.

Phase1: helper source and actual four-shard one-audit protocol smoke on Windows/Rocky,
with rejection controls for missing/duplicate shards/results, changed raw hashes,
emulator errors, mismatched CPU prefix and missing live preflight. This is not full
kernel coverage. Do not activate production matrix or merge on this limited smoke.

Phase2 after protocol proof: bounded production CI shards/aggregators preserving all
original work, then all10 complete full-profile proofs and exact-head normal CI.
Native input-name snapshots, original comparisons and global coverage bounds remain
enforced. The slowest individual cases must still fit; hash partitioning alone does
not establish the runtime budget. No new exclusions or tier/threshold relaxation.
