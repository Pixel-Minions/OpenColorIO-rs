# P64: bounded execution and one targeted SDE follow-up

Own scripts/sde_cpu_tests.py, scripts/sde_targeted.py, their diagnostic controls,
.github/workflows/sde.yml and the execution split in legacy_gpu_oracle.rs.
No oracle, fixture, renderer, waiver, deviation or comparison changes.

Owner authorization2026-11-01 is in [P63](p63-sde-followups.md): one targeted
workflow execution, with no full-matrix rerun. SDE remains advisory for merging;
complete CPU qualification remains release work.

## Execution repair

The P35 boundary generator remains the original four chains, seven optimization
levels, five edge values and ten shader languages:1400 original requests, before=0.
Replace one long execution with seven200-request tests. Each preserves its original
relative order, batch size50 and all outcome/log/uniform/texture comparisons.
A native input audit independently enumerates the original domains and reconstructs
the original order from all seven parts. Each targeted profile additionally checks
the raw selected indices and200 compared/refused outcomes per part, covering all1400
exactly once. Fresh emulators per selected test reset the cumulative thread budget.
The original two-hour case limit remains for these smaller executions.

The full-profile kernel jobs get360 minutes, matching the existing LUT-job budget;
their old240-minute limit cancelled unfinished work. No full matrix is run for this
change, so complete-profile feasibility remains unproven. The focused full ICC cases
have a bounded five-hour case limit within a six-hour job, and retain every comparison.
Their existing creation-time check uses each real run's bounds, with every other byte
compared. No one-hour date assumption or exception at byte31 is added.

On Windows, if a restricted launcher denies taskkill for its own child, the watchdog
also terminates its owned Popen handle. A still-open descendant pipe cannot pass.
Native controls require thread-limit and timeout cases to fail promptly, and reject
unbounded budgets. They launch no SDE or OCIO processes.

## One workflow, five focused test jobs

Dispatch the existing SDE workflow with targeted=true on the verified repair SHA.
All full-matrix and depth jobs are excluded from that invocation. Draft PRs defer
SDE until ready_for_review, so the targeted label can be applied before any full
matrix is considered; ready PRs without the label retain automatic SDE coverage.

| Job | Exact selection | Coverage claim |
|---|---|---|
| Windows nhm boundary | input audit and all seven boundary parts | all1400 original boundary requests once, if every raw result passes |
| Windows snb boundary | same original1400-request union | reproduce the recorded oracle crash without dropping the final50 requests |
| Windows snb Python313 | api_formats_oracle::grading_rgb_curve_dynamic_sets_match_the_wheel_at_every_format_and_level | that full original test at the original quick tier |
| Rocky snb ICC generated | ociobakelut_icc_oracle::display_profiles_generated | the unchanged generated-profile sweep |
| Rocky snb ICC corrupt | ociobakelut_icc_oracle::corrupted_profiles, full tier | all3000 original generated corrupt-profile cases |

Each job records its native inventory, exact selected names, source SHA, pinned SDE
prefix, tier, raw logs/hashes and strict libtest footers. A failed or cancelled case
remains failed/incomplete; no retry is authorized by this card. These focused jobs
do not qualify any complete CPU profile. All ten profiles in P63 stay open.

Validation before dispatch: local format/source guards and13 native watchdog,
scheduling and ordering controls; actual Rust compilation and the input-union audit
on both platforms in normal CI. Local heavy compilation is avoided while Windows
has about3GB free RAM. No SDE run is consumed by native validation or skipped jobs.

Results: pending. Record the workflow ID, source SHA and every outcome here when known.
