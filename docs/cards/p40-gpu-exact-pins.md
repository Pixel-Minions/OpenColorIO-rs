# P40: five approved exact failure pins and 59 fixed-function cases

Owner decision 2026-10-30 supersedes the earlier hold for these five finite failures.
Read-only debugger report: local commit370cd592, branchclaude/aces2-gpu-debug. Do not modify
or push that worktree/branch. Existing protected2026-10-29 evidence remains unchanged.

Own the three preserved fixed-function test files, private GPU exact-pin enforcement and
its tests, existing testkit waiver parser and xtask schema reader, waiver/improvement docs/register, upstream map
and this card. No new public API, arithmetic, oracle, fixture or dependency change.

Run all59 original upstream cases with unchanged inputs/settings/thresholds. Keep existing
whole-image port/wheel GPU bit identity. Add W0014-W0018, one per approved test, pinning
original maximum-error bits and component, original comparison settings, exact renderer
(Mesa25.2.7/LLVM21.1.8) and nan_bug's GPU/CPU RGB bits from the report. Pins additionally
retain identical debug/release historical input/CPU/GPU hashes so any result change rejects.
Reject a newly passing test, other renderer/settings, missing proof, mismatch or changed pin.
No new class rule, threshold changes, skipped tests or retired Matrix-rule reactivation.

Evidence copied before implementation to kept/gpu-exact-pins/2026-10-30. Preserve report,
knockout/injection scripts/results, GamutComp13 pow proof and original real captures with
hash manifest. Cite the knockout experiments. Record upstream nan_bug ill-conditioning
as I-544. Keep raw debugger/CI payload locally; publish approved code/register/docs only.

Require parser/enforcement rejection controls and focused debug/release GPU validation,
including all264 cases with original thresholds and explicit exact-pin records. Then assemble
a normal two-parent landing on current main, regenerate counts and pass all17 full normal CI
checks on that exact head before fast-forward landing. No partial run is a full pass.
