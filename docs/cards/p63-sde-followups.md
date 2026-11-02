# P63: advisory SDE qualification and execution repairs

Owner decision,2026-11-01: PR90 lands after its three required normal checks pass
on the exact regenerated land commit. SDE is advisory for merging. Complete SDE
qualification remains required before declaring the port finished. This supersedes
P45/P61's all-ten-profile merge hold and final-attempt/no-reruns wording.

One new **targeted** SDE workflow execution is authorized. Do not dispatch or rerun
the whole matrix. Record its exact source SHA, selection, input union and results.
PR90 carries the `sde-targeted` label: its automatic SDE workflow selects no full
matrix jobs. Other PRs and the scheduled matrix retain their existing selection.
The separate CI repair chunk adds the targeted execution and fixes the remaining
thread-limit and time-budget problems without weakening any comparison.

## Open evidence

| Item | Recorded evidence | Status / required next step |
|---|---|---|
| Windows nhm boundary-case timeout | [PR90 run41215068146](https://github.com/Pixel-Minions/OpenColorIO-rs/actions/runs/41215068146), artifact `sde-nhm-Windows-kernel-shard-2`: `legacy_boundary_edges_match_native_wheel` hit7200.19s after batch1300..1350 of1400 | Open. Partition the original execution while proving all1400 requests and comparisons remain, then run the complete union under nhm. |
| Windows snb oracle crash | Same run, artifact `sde-snb-Windows-kernel-shard-2`: oracle exit`0xc0000005` during batch1350..1400, about6104.72s | OPEN, unexplained. It did not reproduce in run41351906906: all seven original boundary parts passed on snb. A separate post-run parser error failed that job; P76 recovers the strict input-union proof from its unchanged raw logs. Non-reproduction does not establish the crash's cause or close it. |
| Python313.dll emulator error | [Earlier combined run41165677912](https://github.com/Pixel-Minions/OpenColorIO-rs/actions/runs/41165677912), Windows snb: illegal-instruction emulator error in hosted `Python313.dll` during `grading_rgb_curve_dynamic_sets_match_the_wheel_at_every_format_and_level` | Open. Include that exact test in the targeted snb execution. Partial earlier reports are not a pass. |
| Four-hour job cancellations | [PR95 run41265170638](https://github.com/Pixel-Minions/OpenColorIO-rs/actions/runs/41265170638): ten kernel job timeouts; [combined run41215068146](https://github.com/Pixel-Minions/OpenColorIO-rs/actions/runs/41215068146):21 shard timeouts; [PR97 run41317603548](https://github.com/Pixel-Minions/OpenColorIO-rs/actions/runs/41317603548): ten kernel timeouts | Open execution problem, not a recorded pixel comparison failure. PR97's ten logs all report `SDE ERROR: Run exceeds the maxmimal number of threads` in the first ocio library target before the time limit. Fresh emulators per selected test address the cumulative budget; per-case timeouts still need bounded execution. |
| Rocky snb ICC sweeps | [PR96 known gaps](https://github.com/Pixel-Minions/OpenColorIO-rs/pull/96): `ociobakelut_icc_oracle::display_profiles_generated` and `corrupted_profiles` did not compare past byte31 before the date-bound repair; the full ICC target is absent from `cpu-tests` | Open. Explicitly select both unchanged tests on Rocky snb, retaining their creation-time validation and every other output byte. Sampled `tools_cpu_oracle` is not a substitute. |

Raw evidence is retained outside target in
`kept/continuation/2026-10-31/ci-41215068146/` and
`kept/continuation/2026-11-01/pr97-sde-logs/`. The older Python313.dll evidence is
retained with the earlier run; expected outputs still come only from the wheel.

## CPU qualification ledger

All ten complete profiles on the combined current-main/legacy head remain unproven:

| Platform | nhm | snb | hsw | skl | skx |
|---|---|---|---|---|---|
| Windows | open | open | open | open | open |
| Rocky Linux9 | open | open | open | open | open |

All ten LUT jobs passed on PR97; these are narrower than complete CPU profiles.
P55's complete1680-request small-legacy plan passed on Windows/Rocky nhm in
[run41170634900](https://github.com/Pixel-Minions/OpenColorIO-rs/actions/runs/41170634900),
but it is also narrower than a complete profile. The old scheduled ICC-main Rocky
skx success predates Phase8 and does not qualify the merged SIMD head.

The rebased alias retains all41 main named test targets plus legacy_gpu_oracle.
Cargo metadata predicts46 executables:42 integration targets and four libraries.
Collect and validate actual native inventories before claiming complete profiles.
CDL, Gamma, Log, FixedFunction and GradingPrimary's complete SDE coverage remains open.

Post-extraction legacy dynamic updates also remain separate library/test work.
No threshold, waiver, CPU flag, SDE pin, original filter or expected output is changed.

Latest owner decision: land PR100's exact6fe76c84 after its three normal required
checks pass; it contains90/98/99, so do not merge those separately. After P76's small
boundary-audit correction is on main, ONE further SDE workflow is authorized for
the two Rocky snb ICC sweeps, strict boundary audit and one COMPLETE snb CPU profile
on Windows and Rocky Linux9. No other CPU profiles or full matrix are authorized
now. After all library work lands, run the full matrix once as final qualification.
