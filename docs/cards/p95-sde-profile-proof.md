# P95: persist complete-profile evidence after its strict audit

Run41386558773's Rocky join validated all 46 targets, 2,219 tests and the original
1,400 boundary requests on ee5671bc, then failed because the output directory for
kept/sde-complete-profile/proof.json did not exist. Its failure is a receipt-writer
bug, not a comparison failure. The failed CI status is retained; offline recovery
rechecks the original raw logs, hashes, native inventories and exact-once union.

Owned files: scripts/sde_complete_profile.py,
scripts/diagnostics/p77_profile_controls.py, .github/workflows/ci.yml and this card.
Create the output parent only after complete() has validated the evidence. Keep
every source, CPU, inventory, raw-log, filter and exact-once rejection unchanged.

The new actual-CLI regression reproduces FileNotFoundError on the old code, then
requires a valid persisted proof in a missing nested parent. A second CLI control
requires invalid source evidence to create neither a proof nor its parent. The
existing p64 native controls import ProfileControls, so both required platform
checks already execute these regressions; a separate invocation is unnecessary.
These controls do not emulate a CPU or fabricate OCIO output.

The original Windows join failed before the strict external boundary audit, due
to its cancelled shard1. Its successful shard2 contains every original boundary
part and the plan audit; offline validation also proves the complete 1,400 union.
Windows shard1 has 380 validated passes and 169 unproven cases, no recorded case
failures. Preserve passed receipts; missing-only snb execution and bounded shard
scheduling are subsequent chunks. No full matrix, threshold or waiver changes.
