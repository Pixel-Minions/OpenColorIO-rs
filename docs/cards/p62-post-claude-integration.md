# P62: rebase the legacy stack after tools, ICC and SIMD

Rebase PR85, PR86, PR87 and combined PR90 onto main
4bc8b1f6313ff2292cbfc61f307454a524acc4a9, preserving their merge ancestry.
This replaces no renderer, tools, ICC, oracle, fixture, waiver or arithmetic code.
The production source and SDE protocol match the prior PR90 head012d52d8.

Keep all41 current-main cpu-tests targets, including ocio-tools/tools_cpu_oracle,
matrix_oracle, grading_rgb_curve_oracle and cpu_processor_apply_oracle. PR86 and
later add ocio/legacy_gpu_oracle, making42 named integration-test targets plus
ocio-ops lib. The main normal-CI workflow and xtask/src/shards.rs remain byte-identical,
including the ICC heavy-group assignment and sample packages appended last.
All six submodules are retained; Little-CMS is initialized from the primary
checkout's .git/modules/upstream/Little-CMS, without a network fetch.

P45 and P61 are retained as historical evidence and gate definitions; their old
41-target totals are not new-head evidence. A new native inventory must derive
from the combined42-target alias. Preserve each original comparison and original
request/input union. P61's ten full profiles and ten LUT passes remain owed.
The older PR90 normal21 result does not validate this rebased head.

Owner reopened main on2026-11-01 after PR95, PR96 and PR97 merged. The earlier
intentional SDE budget was2/2 spent. No new workflow is started by this local
rebase. Remote synchronization itself starts SDE and requires a renewed budget.
PR95's ten kernel jobs ended at the240-minute job budget; the GitHub annotations
explicitly identify timeouts, not comparison failures. Changing only that budget
would not establish parity or resolve the older Windows legacy oracle crash.
Keep the prior flags, tiers, filters, thresholds and per-test limits unchanged.

Preferred landing: combined PR90 once its exact-head normal checks and retained
complete-profile gate pass; PR85-87 need no independent main merges. Close none
without first telling the owner. The post-ICC60-file map audit continues separately,
with seven prior Phase8-dependent entries now available for source reconciliation.

Local checks: alias/source/ancestry preservation, format, existing shard tests,
source/main-mode guards and SDE scheduling controls. Hosted validation is pending.
