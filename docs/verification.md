# How the port is verified

The port is accepted only on exact parity with the official OpenColorIO 2.6.0 package. The
definition of done is [`PLAN.md`](../PLAN.md) section 3; this page explains how it is checked.

## The rules

- **Text is byte-identical on every platform.** That covers config files written by
  `serialize()`, CLF/CTF and other LUT files, baked LUTs, shader code, cache IDs, error and
  warning messages, and Python `repr()` strings.
- **Pixels are bit-identical to the official library on the same machine.** The reference is
  the official `opencolorio==2.6.0` Python package, on the same OS and CPU. That holds at every
  bit depth, pixel layout and optimization level, and for every CPU SIMD variant (SSE2, AVX,
  AVX2, AVX-512) the official build can run.
- **GPU shaders are byte-identical**, with the same uniforms and textures, in all 10 shading
  languages OCIO supports.
- **Where OCIO itself differs between platforms, the port does too.** The checks found
  several such differences inside the official library, and the port reproduces each per
  platform:
  - camera-log math in float on Windows and in double on Linux;
  - the operand order of multiplications, which decides which NaN comes out;
  - NaN signs and text;
  - the Windows build's CRLF built-in configs;
  - how each platform's C++ library reads floats (Windows accepts hex floats, Linux reads tiny
    values as 0).

## How it is checked

- **The oracle.** The official package runs live next to the port, on the same machine, in
  every test run (`oracle/`, `ocio_testkit::Oracle`). Expected values come only from it, from
  OCIO's own tests, or from the platform's C runtime. They never come from the port itself.
- **Exact comparisons only.** Floats compare bit for bit, NaN bits and signed zeros included.
  There are no tolerances, except where a ported upstream test has its own.
- **A shared test battery.** Every op family is tested the same way (`ocio_testkit::battery`):
  probe sets, generated extreme and non-finite parameters, both directions, fast math on and
  off, and every numeric profile. An independent reviewer mutation-tested every card's tests to
  prove they catch deliberate breakage.
- **Both platforms, both build modes.** CI runs everything on Windows x86-64 and in a Rocky
  Linux 9 container (glibc 2.34, the VFX Reference Platform baseline), in debug and release
  builds. Compiler optimizations must not change a single bit.
- **Guardrails nobody can skip** (`cargo xtask ci`). Reference files are locked by hashes.
  Tests can't be disabled or loosened. The count of ported upstream tests can only go up. Every
  upstream source file is tracked in [`upstream-map.toml`](../upstream-map.toml).
- **GPU execution.** The 270 upstream GPU tests run headlessly on llvmpipe. The port's rendered
  images and shader text are compared bit for bit with the official shader's output on the same
  renderer. Legacy shader and LUT data are also compared with the official library's native API
  in all 10 languages ([coverage](cards/p88-legacy-live-qualification.md)).
- **Real-world configs.** All 46 public real-world configs match the official library on both
  platforms.

## Emulated CPUs

A machine only runs the SIMD kernels its own CPU selects. So the CPU-dependent tests
(`cargo cpu-tests`) also run under Intel's Software Development Emulator (SDE), as Nehalem
(`nhm`, SSE2), Sandy Bridge (`snb`, AVX), Haswell (`hsw`, AVX2 with slow gather), Skylake (`skl`,
AVX2) and Skylake server (`skx`, AVX-512). The official library and the port then both pick that
CPU's kernels, so every kernel is compared, whatever CPU the tests run on.

- `scripts/sde.sh <cpu> <command>` runs a command on an emulated CPU.
- The workflow (`.github/workflows/sde.yml`) runs when started by hand. It splits each CPU's
  tests into 16 kernel jobs and two LUT jobs, on both platforms, and runs every test in a fresh
  emulator.
- Under emulation on Windows, the official library's process, or the emulator in it, sometimes
  fails, and a concurrent run has hung on both platforms. Each case is in the qualification
  records below and in the [changelog's known issues](../CHANGELOG.md).

### The 1.1.0 qualification

Run on 2026-11-21 under SDE on one workstation, on Windows and on Rocky Linux 9, on `643f44b7`:
release builds, the quick tier, the official 2.6.0 wheel, no oracle cache. It covers part of
`cargo cpu-tests`: targets whose code 2.6.0 changed directly. GitHub Actions wasn't available,
so **the full workflow waits for CI**: every target on all five CPUs, the GPU sweeps
(`legacy_gpu_live_oracle`, `api_gpu_oracle`) included.

Every test in the first table passed at least once on each CPU and platform named. Not every
attempt passed: 27 didn't, listed in the second table, and an earlier run hung.

**What ran.** Each oracle test ran in a process of its own, and the ops unit tests in one. A
test can depend on the CPU where the dispatched kernels, CPUInfo or the math library compute
what it compares. The others compare names, limits, shader text, or results that aren't
expected to vary: that split comes from reading the tests, not from comparing the profiles with
each other.

| Group | Targets (tests) | CPUs, on both platforms |
|---|---|---|
| Can depend on the CPU | ops unit tests (604 on Windows, 607 on Rocky) | nhm, snb, hsw, skl, skx |
| Can depend on the CPU | `cpu_info_oracle` (1), `cpu_processor_apply_oracle` (4), `lut1d_op_oracle` (4), `lut1d_bake_oracle` (7), `api_formats_oracle` (24), `fixed_function_oracle` (9), `grading_hue_curve_oracle` (5) | nhm, snb, hsw, skl, skx |
| Can depend on the CPU | `legacy_gpu_oracle`, the value parts (10) | nhm, skx |
| Not expected to | `cpu_processor_tests` (15), `config_utils_oracle` (8), `config_from_configs_oracle` (5) | nhm, skx |
| Not expected to | `grading_hue_curve_op_gpu_oracle` (3); `legacy_gpu_oracle`, the plan, allocation and boundary parts (15) | nhm |

For `config_utils_oracle` and `config_from_configs_oracle` this shows that the port matches the
wheel on nhm and skx. It doesn't show that their bytes are the same on every profile.

**Attempts that didn't pass.** A test was stopped at 15 minutes (the cap) and run again with a
90-minute cap. The check is a last run of the tests that failed on Windows, strictly one at a
time, with nothing else of the qualification running.

| Platform, CPU | Test | Attempt | Later passes |
|---|---|---|---|
| Windows, nhm | legacy dynamic parts 0, 1, 2 | cap | 1,596, 1,613, 1,593 s |
| Windows, nhm | legacy mixed LUT, range and alpha | cap | 2,740 s |
| Windows, nhm | ordinary after legacy | cap | 1,157 s |
| Windows, nhm | small legacy lattices parts 0, 1, 2, 3 | cap | 1,153, 1,158, 1,179, 1,163 s |
| Rocky, nhm | legacy mixed LUT, range and alpha | cap | 1,446 s |
| Rocky, skx | legacy mixed LUT, range and alpha | cap | 1,336 s |
| Windows, skx | legacy dynamic parts 0, 2 | cap | 1,671, 1,688 s |
| Windows, skx | legacy mixed LUT, range and alpha | cap | 2,854 s |
| Windows, skx | ordinary after legacy | cap | 1,226 s |
| Windows, skx | small legacy lattices parts 0, 1 | cap | 1,172, 1,188 s |
| Windows, skx | `builtin_configs_match_the_wheel` (`config_utils_oracle`) | cap | 1,007 s |
| Windows, skx | small legacy lattices parts 0, 1, 2, 3 | **mismatch** inside the oracle: the wheel against its native helper, which had crashed | 1,172, 1,188, 1,189, 1,199 s; check: native 19, 21, 20, 22 s, skx 1,075, 1,092, 1,135, 1,262 s |
| Windows, skx | legacy dynamic part 1 | failure: the native helper crashed | 1,667 s; check: native 28 s, skx 1,468 s |
| Windows, skl | `alpha_offset_bakes_match_the_wheel` | failure: Pin assertion in the oracle's process | 21 s (not logged); check: native 1 s, skl 23 s |
| Windows, skx | `upstream_test_configs_match_the_wheel` (`config_utils_oracle`) | failure: Pin assertion in the oracle's process | 1,089 s; check: native 48 s |
| Rocky, snb and skl | `apply_in_place_and_to_itself_match_the_wheel` | failure: the oracle's process got SIGKILL | 20 s each |

That is 18 caps and 9 failures. A duplicate attempt of small legacy lattices part 2 (Windows,
skx) was also stopped by hand, after that test had passed.

**What ran concurrently.** Up to 13 emulated tests ran at once over the two platforms. Every
later pass above, except the check's, ran while others did: the Windows skx legacy parts with up
to 12 others, the rest with up to 7. A driver that was thought to be stopped also kept running
for about an hour and a half, repeating Windows skx legacy parts beside their reruns. The
check's 13 runs all passed, one at a time, and left no emulator log.

**What the logs show.**

- The helper's five crashes returned `0xC0000142` (DLL initialization failed). In the four
  mismatches the oracle had the wheel's normal result on one side and that crash on the other,
  so the port's output was never compared in those calls. Pin logged
  `WIN_FOLLOW_CHILD: ***Failed to inject` for child processes in the seconds those four failed,
  and within about 10 seconds of the fifth's failure.
- The two Pin assertions are `VM_THREAD: 106: assertion failed: !IS_PINOS_SYSCALL_ERROR(res)`.
  The oracle's process then exited with `0xffffffff`.
- Five Pin `lockAcquired` logs were written while several tests ran. None could be tied to a
  test.
- The kernel under the container killed nothing for memory from its boot, before the runs, to
  their end (`oom_kill` 0). By the runs' own timings, the runner's cleanup of a timed-out test
  (`pkill -9 -f ocio_oracle`, in the same container) came within 2 seconds of both SIGKILLs.
- **A hang, open for CI:** on `b648dc8c`, `api_formats_oracle` with its tests running
  concurrently (4 threads) was idle on nhm on both platforms, and was stopped after about 70
  minutes on Windows and 80 on Rocky. On Rocky, threads of three of its tests and their oracle
  processes were blocked reading pipes. It wasn't run concurrently again. One test at a time,
  all 24 passed natively and on the five emulated CPUs, on both platforms.
- For a failed test the runner kept the last 40 lines of output, for a capped one the last 15.
  The check kept all of it.

**What is inferred, or not known.**

- That the cleanup's `pkill` sent the two SIGKILLs is inferred from the times: no log names the
  sender.
- Why Pin couldn't inject, and whether that is what crashed the helper, isn't known. Both
  episodes came at about the times the operator tried to stop other runs with `taskkill`.
- The causes of the Pin assertions, of the `lockAcquired` logs and of the hang aren't known.
- The caps look like slowness, since each of those tests passed with more time. Nothing more was
  measured.

### The 1.0.0 qualification

Every CPU-dependent test passed on all five emulated CPUs, on Windows and on Rocky Linux 9,
with no difference from the official library:

- **CI:** SDE run 41658641028 on the release commit `ec1f768d`, run twice. Every Rocky
  job passed the first time. Every Windows job that failed did so for one of two reasons. Either
  the job ran out of its four-hour budget, or the official library's process crashed under the
  emulator (above), on a different test each time. A test counts as passed if it passed in
  either run.
- **Locally, under SDE on Windows,** on the release commit plus PR 111's changes (`2f31255f`),
  against the official wheel with no oracle cache: the tests that neither CI run reached or
  completed. That is the split parts of the P33 allocation sweep (skl) and the P34 dynamic
  sweep (nhm, skl, skx); the P34 sweep was split because it ran past the two-hour limit
  unsplit. It is also 26 tests the budget cut off or the crash interrupted: 7 on hsw, 5 on skl,
  3 on nhm and 11 on skx.

PR 111 adds no computation: it adds re-exports and splits two GPU sweeps into parts.

## Upstream tests

| Suite | Ported | Reviewed not applicable | Total |
|---|---:|---:|---:|
| C++ | 1,224 | 2 | 1,226 |
| GPU | 270 | 0 | 270 |
| Python | 392 | 0 | 392 |

The two C++ exclusions are the approved [P9-6 decisions](waivers.md). The Python tests run
unmodified against the port's module, and against the installed wheel, on both platforms. The
live numbers are in [`parity.md`](parity.md).

## Exceptions

Nothing is waived silently: CI fails on any difference that isn't listed.

- **Waivers** ([`waivers.toml`](../waivers.toml), explained in [`waivers.md`](waivers.md)).
  The two broad ones:
  - **W0001:** on Windows, the port uses standard `powf` in one exact-math PQ path, where the
    official build uses MSVC's vector math library. About 1.7% of values differ. For inputs up
    to 1 in magnitude the difference is bounded and invisible in practice; above 1, `PQ_TO_LIN`
    approaches its pole near 1.992, where the two libraries drift apart without bound, so those
    inputs are waived. Linux, and fast math, are bit-exact.
  - **W0002:** with a NaN parameter, the payload bits of the resulting NaN values may differ,
    including in the 1D LUT the optimizer bakes from it and the CPU cache ID that hashes that
    LUT. They are still NaN.

  The others are narrow: one GPU non-finite class ([W0013](gpu-nonfinite.md)), five pinned
  finite GPU results ([W0014-W0018](gpu-finite-exact-pins.md)), and nine single cases,
  W0019-W0027, most of them in the tools or in Windows file names.
- **Deviations** ([`deviations.md`](deviations.md)): where the port intentionally behaves
  differently.
- **Reproduced upstream bugs** ([`improvements.md`](improvements.md)): every upstream bug or
  limitation the port copies, and every case where upstream reads or writes memory it doesn't
  own and the port returns an error instead.
