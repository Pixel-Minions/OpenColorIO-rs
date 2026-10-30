# P42: bounded complete SDE profiles on the integrated GPU/legacy stack

Prepares the frozen native-validated legacy/SDE stack and P41 helpers on P40's
immutable GPU264 candidate. P40 must finish its own all17 normal checks and land
first. This branch does not change PR88 or any older draft head.

Own .github/workflows/sde.yml and this card; integration README retains both GPU264
and the initial legacy matrix scope. Existing source components remain unchanged.
No new waiver, class rule, threshold, tier, oracle, fixture, dependency or library API.

P41 protocol proof: run41138515532/source06497, eight actual partition jobs and two
joins/controls passed independently on Windows-HSW and Rocky-SNB. Complete native
input graphs and8 rejection controls/platform passed. That limited audit is not a
full CPU profile proof and does not close the SDE task.

Production layout: five original CPUs on both platforms, four kernel shards/profile,
ten strict whole-profile joins, ten unchanged LUT sweeps and two unchanged depth jobs.
Keep original quick tier, pinned SDE/flags, the two existing LUT filters,240-minute
kernel and360-minute LUT limits. Kernel concurrency6/platform and LUT2/platform bound
runner use. Each canonical profile requires all its original cases exactly once,
raw pass footers/hashes, complete native Cargo inventory and matching plan/CPU prefix.
Incomplete/missing/duplicate/error/timeout results fail closed. Joined artifacts retain
the original full-profile schema for independent verification.

Integrated CPU alias has41 targets and the original plus legacy name inventory
(Windows2115/Linux2118 after the two original filters). Verify all ten full profiles
against those frozen input sets, then all17 normal checks on the final exact landing
head. Do not reuse old component-only normal passes as a combined-head pass. Preserve
all264 GPU cases and five approved pins; no diagnostic ci.yml may enter main.

Do not dispatch the complete production matrix before P40 lands. If any individual
case still exceeds its original budget, preserve that failure and investigate; do
not reduce the selected work, comparisons or proof requirements.
