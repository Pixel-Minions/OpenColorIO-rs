# P61: final combined legacy candidate after the bounded P32 source split

Combine the full source ancestry of PR85/86/87/90 with P55, the completed Python
source-file status audit, the final C++ accounting and the six-entry Phase9 library
audit. No Claude-owned Phase8, tools or apputils source is included. The normal ci.yml,
upstream-map.toml, improvements register and output-waiver ledger remain identical
to the PR94 parent; the diagnostic P55 ci.yml is never merged.

P55 replaces one execution of the unchanged original 1680-request generator with
four ordered 420-request executions and an input-union audit. P61 supersedes only
P45's old frozen-name counts: the alias still has 41 executables, now selecting
2119 Windows and 2122 Linux kernel cases, with four extra names and the same original
request/comparison union. P45's strict full normal21, ten complete canonical CPU
profiles and ten unchanged LUT jobs remain required. Every original runtime unit,
GPU264 case, approved exact pin and Python384 function is retained.

The earlier P90 SDE run was cancelled after preserving its ten passing LUT jobs and
16 partial kernel artifacts. Those contain 6113 passing invocations and one recorded
Windows SNB emulator illegal-instruction error inside hosted Python313.dll during
grading_rgb_curve_dynamic_sets_match_the_wheel_at_every_format_and_level. No complete
CPU profile was proven. This is not explained as a port defect or suppressed. The
earlier Windows P32 oracle crash (0xc0000005) is also retained.

Owner cap, 2026-10-31: P55 run41170634900 is the first of at most two additional
intentional SDE workflow executions. Preparing/backing up this candidate starts none.
Reserve the second attempt in the durable ledger before synchronizing PR90, because
that synchronization itself starts SDE. After the second execution, start no more
SDE jobs and record every remaining failure or missing profile. A partial run or a
new pass under altered flags, timeouts, filters, tiers, thresholds or waivers cannot
replace the required proof. Keep the combined PR unmerged while that proof is absent.
