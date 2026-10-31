# P55: bound original P32 executions and retain every request

The unchanged Windows P32 case timed out at7200.2seconds in P47. With native
helper setup it instead failed after2521.56seconds in P52: the oracle process
exited0xc0000005 during a50-call batch. That is a failed run, not evidence of a
completed comparison. The precise crash cause remains unresolved.

Keep the complete original1680-request generator and its global bound unchanged.
Execute four contiguous420-request parts, each in a fresh emulator through the
existing runner. Preserve request order, per-request cache-order settings, all
exact outcomes/log comparisons, and P32's no-dynamic checks. A separate input
plan audit proves the full original Cartesian product and ordered once-only union.
Print input-batch boundaries before oracle calls so a future failure identifies
its request range. No expected shader, texture, CPU value or oracle response is
manufactured or cached.

Keep all four parts and the zero-request plan audit in P32's original scheduling
bucket. Original quick request loads remain1680/2000/1680/1680. The other six
legacy cases and every non-legacy selection are unchanged. Native alias still
selects41 executables; four added test names produce2119 Windows/2122 Linux
selected cases, compared with2115/2118 before this execution split.

Owner limit2026-10-31: at most two further intentional SDE workflow executions,
then document whatever remains. Source preparation alone consumes no attempt.
No timeout, tier, CPU flag, LUT filter, threshold, waiver or upstream test changes.
