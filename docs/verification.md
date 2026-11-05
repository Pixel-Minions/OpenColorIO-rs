# How the port is verified

The port is accepted only on exact parity with the official OpenColorIO 2.5.2 package. The
definition of done is [`PLAN.md`](../PLAN.md) section 3; this page explains how it is checked.

## The rules

- **Text is byte-identical on every platform.** That covers config files written by
  `serialize()`, CLF/CTF and other LUT files, baked LUTs, shader code, cache IDs, error and
  warning messages, and Python `repr()` strings.
- **Pixels are bit-identical to the official library on the same machine.** The reference is
  the official `opencolorio==2.5.2` Python package, on the same OS and CPU. That holds at every
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
- **GPU execution.** The 264 upstream GPU tests run headlessly on llvmpipe. The port's rendered
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
- Under emulation on Windows, the official library's process sometimes crashes (`0xc0000005`)
  in the legacy GPU tests, each time on a different test. It is in the
  [changelog's known issues](../CHANGELOG.md); it is not a difference in the port.

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
| C++ | 1,189 | 2 | 1,191 |
| GPU | 264 | 0 | 264 |
| Python | 384 | 0 | 384 |

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
