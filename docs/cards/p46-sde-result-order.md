# P46: preserve original selected-case order in joined SDE reports

Own scripts/sde_join_shards.py, scripts/diagnostics/p46_join_order_controls.py,
the SDE join control step/trigger and this card. Source is on the prepared P45 stack.

The original joiner appended results in shard order, while Selected retained native
order. A one-case protocol smoke could not reveal this. Reconstructed three-case
protocol inputs from saved real Windows-HSW and Rocky-SNB logs reproduced the
misordering on both platforms. No OCIO expected outputs were invented or changed.

After the existing complete/disjoint case-union check, order the retained tuples by
their original native selection. Keep every actual row, log path/hash, shard source
and copied raw log unchanged. Four controls cover cross-shard native order, a native
order different from alphabetical order, missing/duplicate/unknown results and empty
targets. Run them in every production join. Re-run the saved actual shard protocol
controls and the real three-case reconstruction locally; full native CPU coverage
still requires all ten complete production profiles, LUT jobs and normal21.

No runtime test, input, comparison, threshold, tier, oracle, fixture, waiver, CPU flag,
emulator, native inventory or public library API change. Canonical report ordering
does not change which tests run. Preserve the P43 budget probe and P44 normal heads.
