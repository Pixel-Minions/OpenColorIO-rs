# P99: canonical missing-only input plans across hosts

The P97 planner runs on one Linux runner and its final auditor on another. Their
artifact directories may enumerate in different orders. All partition contents
were already sorted deterministically, but the flat missing-name list used by the
plan digest retained baseline enumeration order. Equivalent checked inventories
could therefore produce different plan hashes and fail the final audit.

Sort that flat list by logical target and original test name before hashing. This
does not change any selected name, partition contents, native inventory, raw-log
check, tier, comparison or expected value. Existing native selection order remains
enforced in the original shard reports and reconstructed profile.

The new control reverses equivalent baseline enumeration: it fails before this
fix and passes after it. All47 native controls pass. The real baseline still has
380 retained passes and exactly169 missing names in parts1/60/61/47. Evidence is
kept in kept/continuation/2026-11-02/p99-plan-order-red.log and
p99-all-native-controls.log. No SDE run was started before this repair.
