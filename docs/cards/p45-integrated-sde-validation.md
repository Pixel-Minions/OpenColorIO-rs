# P45: complete bounded SDE validation with balanced normal CI

Owner override2026-11-01: the three exact-head normal checks govern merging.
SDE is advisory for merges; complete SDE qualification remains release work.
The prior all-ten-profile merge hold and final-attempt/no-reruns policy below
are historical and superseded by the one targeted run authorized in [P63](p63-sde-followups.md).

Own the integration, .github/workflows/sde.yml concurrency and this card. Combine
P44 bad41047 (full normal CI balancing) with frozen P42 5a3f1474 (P29-P41 native-
validated SDE/legacy components). No older draft or diagnostic branch is rewritten.
Never merge P43 diagnostic ci.yml. No upstream test/input/threshold/tier, oracle,
fixture, waiver, dependency, arithmetic or public library API changes are introduced.

Prerequisites: P43 four long-case budget probes must pass with exact original five
selected cases, native41-target inventories, actual preflight, raw pass logs/hashes,
pinned SDE/flags and unchanged original quick tier. This is a runtime feasibility
check, not a complete profile. P44's own exact-head normal21 must pass before it lands.
The P45 landing requires a separate complete normal21 gate on its combined head.

Production retains all five CPUs on Windows/Rocky, four disjoint kernel shards each,
ten strict complete-profile joins, and the ten unchanged LUT jobs. Kernel concurrency
is eight per platform (was six), plus the original two LUT jobs/platform: at most20
SDE test jobs at once. This cuts the20-shards/platform schedule from four waves to
three when capacity exists; queue time is still measured rather than assumed away.
The original 240/360-minute budgets, pinned SDE, CPU flags, original two LUT filters,
full native test plans and fresh emulator per selected case remain unchanged.
Depth jobs retain their original nightly/manual-only condition and opt-level0 checks.

Required proof before landing: all ten complete canonical kernel artifacts exactly
match frozen p39 native selections (41 targets, Windows2115/Linux2118 filtered cases),
with exact-once original case union, raw hashes/one-test footers, actual preflight and
no errors/timeouts. Every one of the ten unchanged LUT jobs must pass. Full normal21
also must pass on the exact combined candidate. Partial shards and protocol smoke
cannot satisfy this gate. Preserve the original GPU264 cases and exact pins.

Keep main and all public README completion claims honest while gates are pending.
P42's older normal17 wording is superseded by21 here, after the additional grading
shards. No all-SDE, all-legacy or M5 completion claim until the corresponding proof.
