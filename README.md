# OpenColorIO-rs

**A pure-Rust port of [OpenColorIO](https://github.com/AcademySoftwareFoundation/OpenColorIO)
that produces exactly the same output as the original, down to the last bit.**

OpenColorIO (OCIO) is the color management library of the VFX and animation industry. This
project ports it to Rust one upstream release at a time. The current line matches **OpenColorIO
2.5.2**: the same results, the same text output, the same errors and the same accepted configs,
from Rust and, later, from Python.

> **Status: early development.** Phase 1 (the op engine and the analytic transforms) is
> complete: milestone M0. Through OCIO's API, every analytic transform matches the official
> library on the CPU and the GPU. Phases 2 (LUTs, fixed functions, ACES 2.0) and 3 (configs)
> are under way, in parallel. Python comes later. See [Progress](#progress).

## How precise: byte for byte

"Close enough" is not the goal. A release ships only when all of these hold:

- **Text output is byte-identical on every platform.** That covers config files written by
  `serialize()`, CLF/CTF and other LUT files, baked LUTs, shader code, cache IDs, error and
  warning messages, and Python `repr()` strings.
- **Pixels are bit-identical to the official library on the same machine.** The reference is
  the official `opencolorio==2.5.2` Python package, running on the same OS and CPU. That holds
  at every bit depth, pixel layout and optimization level, and for every CPU SIMD variant
  (SSE2, AVX, AVX2, AVX-512) the official build can run.
- **GPU shaders are byte-identical**, with the same uniforms and textures, in all 10 shading
  languages OCIO supports.
- **OCIO's own tests pass.** Upstream's 1,191 C++ unit tests and 264 GPU tests are ported, and
  its 384 Python tests run unmodified against this port's Python module.
- **Nothing is waived silently.** Every exception is a written, approved waiver in
  [`waivers.toml`](waivers.toml), and CI fails on any difference that isn't listed there. Today
  there are two:
  - **W0001:** on Windows, the port uses standard `powf` in one exact-math PQ path. The official
    build uses MSVC's vector math library there. About 1.7% of values differ. For inputs up to
    1 in magnitude the difference is bounded and invisible in practice. Above 1, `PQ_TO_LIN`
    approaches its pole near 1.992, where the two libraries drift apart without bound, so
    those inputs are waived. Linux, and fast math, are bit-exact.
  - **W0002:** with a NaN parameter, the internal bits of the resulting NaN values may differ,
    including in the 1D LUT the optimizer bakes from it and the CPU cache ID that hashes that LUT;
    they are still NaN.
- **Deviations are listed too.** [`docs/deviations.md`](docs/deviations.md) records where the
  port intentionally behaves differently.

**Reference platforms:** Windows x86-64, and Linux x86-64 on Rocky Linux 9 (glibc 2.34, the
VFX Reference Platform baseline). ARM and macOS come later.

**Where OCIO itself differs between platforms, the port does too.** Two examples: the Windows
and Linux builds of OCIO 2.5.2 compute some camera-log values with different precision, and
they print NaN differently. The port reproduces each platform's behavior rather than picking
one.

### How it is checked

- **The oracle.** The official `opencolorio==2.5.2` package runs live next to the port, on the
  same machine, in every test run. Expected values come only from it, from OCIO's own tests, or
  from the platform's C runtime. They never come from the port itself.
- **Exact comparisons only.** Floats compare bit for bit, NaN bits and signed zeros included.
  There are no tolerances, except where a ported upstream test has its own.
- **Both platforms, both build modes.** CI runs everything on Windows and in a Rocky Linux 9
  container, in debug and release builds. Compiler optimizations must not change a single bit.
- **Guardrails nobody can skip.** Reference files are locked by hashes. Tests can't be disabled
  or loosened. The count of ported upstream tests can only go up. Every upstream source file is
  tracked in [`upstream-map.toml`](upstream-map.toml).

## Why a Rust port

- **Exact OCIO results, interchangeable with other OCIO applications.** A config gives the same
  pixels, shaders, baked LUTs and cache IDs as any other application using OCIO 2.5.2 on the
  same machine.
- **No C++ in your stack.** The whole build is `cargo build`: no CMake, no C++ dependency
  builds, no DLL or ABI mismatches, and no C++ exceptions crossing into Rust.
- **Safe with untrusted files.** Config and LUT parsers are written without `unsafe` code, and
  they will be fuzzed against the official library. Known data races in upstream get fixed
  without changing any output.
- **Room to be faster without changing results.** Planned: multithreaded processing, caching
  of expensive setup (such as the ACES 2.0 tables), and more SIMD. Each is only allowed if it
  is bit-identical to the exact version, and the tests enforce that.
- **One engine for Rust and Python.** A module compatible with `PyOpenColorIO` (planned)
  runs on the same Rust code, so scripts and applications give identical results.
- **Tracks new OCIO releases.** Each upstream release is ported as a diff and proven again
  against that release's official package. Versions say what they match:
  `1.0.0+ocio.2.5.2` is the complete port of OCIO 2.5.2, and a later major version will match
  2.6.

## Progress

Status as of 2026-10-05: **Phase 1 (the op engine and the analytic transforms) is complete**:
milestone M0, about 22% of the planned work. Through OCIO's own API, every analytic transform
matches the official library on the CPU and the GPU. Phases 2 and 3 now run in parallel, toward
milestone M1:
- **Phase 2:** LUTs (1D and 3D, with their inverses), fixed functions, ACES 2.0 and the
  GradingRGBCurve op that the ACES 1.x built-ins use
  ([`docs/cards/phase2.md`](docs/cards/phase2.md));
- **Phase 3:** configs (a port of yaml-cpp's parser, the config model, context, file and viewing
  rules, the YAML writer, `validate()`, the built-in transforms and configs)
  ([`docs/cards/phase3.md`](docs/cards/phase3.md)).

**Merged so far in Phases 2 and 3:** the fixed functions' op data, the ACES 1.x fixed-function
renderers (red modifier, glow, dark-to-dim and gamut compression) and FixedFunctionTransform;
the rest of the fixed functions on the CPU (surround, HSV, HSY, xyY, u'v'Y, LUV, gamma-log and
double-log) and ACES 2.0 on the CPU (the output transform, tone scale, gamut compression and
RGB-to-JMh, with the color-matrix helpers they use) and on the GPU in all 10 shading languages,
as are the other fixed functions; the forward 1D LUT renderers with their SSE2, AVX, AVX2 and
AVX-512 kernels; the GradingRGBCurve op and transform that the ACES 1.x built-ins use; and
std::regex as each wheel's C++ library implements it (for file rules), with the Linux crash
limits replaced by errors;
the config transforms ColorSpaceTransform, DisplayViewTransform, LookTransform, FileTransform
and BuiltinTransform with its registry (their classes and text; their processors come with the
config). The live oracle can now also drive a config, its context and its file rules call by
call in the official library, which the Phase 3 cards are checked against. A config's context
is merged too: its environment (byte for byte as each platform's C runtime and system apply
it), its variables and their expansion, its search paths and how it finds files, with the
pystring and ParseUtils helpers it uses. The test battery also checks LUT families now: the oracle
takes a LUT's values as a blob, and grading values as objects. The port now reads YAML with
its own port of yaml-cpp 0.8.0, the parser the official library uses, numbers included: each
platform's C++ library reads floats its own way (Windows accepts hex floats, Linux reads
tiny values as 0), and the port does too, with yaml-cpp's own tests ported. The config's model
objects are merged too: ColorSpace, ColorSpaceSet, Look, ViewTransform and NamedTransform,
with their text and errors byte for byte (names that aren't UTF-8 included). The test harness
is sturdier under load: the oracle never sends a second, broken reply, and tests that set
environment variables keep them on their own thread. Landing merges the register of copied
upstream bugs (`docs/improvements.md`) entry by entry and stops if any change would be lost.
Each also went through an independent review before merging.

**Done:** everything below is bit-exact against the official library on Windows and Linux, in
debug and release builds, and was reviewed independently before merging.
- **The op engine:**
  - bit-depth conversion and every pixel layout OCIO accepts (packed, planar, strided, RGB and
    RGBA, every channel order);
  - format metadata, dynamic properties and logging;
  - the optimizer: its passes, every optimization level, and the separable-prefix bake into a
    1D LUT for integer and half-float input;
  - the CPU processor and its renderers, with fast math on and off, for every SIMD profile.
- **Every analytic op family:** Matrix, Range, Exponent, ExponentWithLinear (moncurve), Log,
  LogAffine, LogCamera and CDL, plus the forward 1D LUT that the optimizer bakes. Each one has:
  - its op data and validation;
  - its CPU renderers;
  - its GPU shader writer, in all 10 shading languages.
- **The transforms:** MatrixTransform, RangeTransform, ExponentTransform,
  ExponentWithLinearTransform, LogTransform, LogAffineTransform, LogCameraTransform,
  CDLTransform, AllocationTransform, Lut1DTransform and GroupTransform. Their text output,
  getters, equality and errors match OCIO byte for byte.
- **The processors:** `Config::CreateRaw()`, `getProcessor` with its caches, the optimized
  processors, `createGroupTransform`, and the CPU and GPU processors. Cache IDs, metadata and
  flags match OCIO.
- **The GPU infrastructure:** the shader description, the shader-text helpers for all 10
  languages, uniforms and dynamic properties.
- **A sweep through the public API:** every analytic transform runs through the port's own
  processors and is compared with the official library:
  - on the CPU, at every bit depth (8, 10, 12 and 16-bit integer, half and float) in and out,
    in packed RGBA, RGB and BGRA and planar RGBA and RGB, at every optimization level;
  - on the GPU, in all 10 shading languages at every optimization level.
  It found no differences.
- **The test harness:**
  - the live oracle;
  - hash-locked reference fixtures;
  - guardrails (`cargo xtask ci`);
  - the parity dashboard;
  - a Rocky Linux 9 reference container;
  - CI on Windows and Linux on every pull request, with `main` accepting only commits that
    passed it.
- **Checks on emulated CPUs.** The CPU-dependent tests also run under Intel's CPU emulator, as
  Nehalem, Sandy Bridge, Haswell, Skylake and Skylake server. So every SIMD kernel the official
  library has (SSE2, AVX, AVX2, AVX-512) is compared with the port's, whatever CPU the tests
  run on.
- **A shared test battery.** Every op family is tested the same way against the official
  library: probe sets, generated extreme and non-finite parameters, and every numeric profile.
  Every card's tests were mutation-tested by an independent reviewer to prove they catch
  deliberate breakage.
- **Earlier foundations:**
  - cache-ID hashing (XXH3-128), which reproduces the official cache IDs of all 8 built-in
    configs;
  - OCIO's fast-math functions;
  - CPU feature detection, identical to OCIO's own;
  - the 3D LUT forward kernels in every SIMD variant;
  - all three half-float conversions OCIO uses;
  - C and C++ number formatting and parsing, identical to each platform's runtime;
  - a port of the yaml-cpp emitter that writes the `serialize()` output of all 8 built-in
    configs byte for byte.

Along the way, the checks found platform differences inside the official library itself. The
port reproduces each of them per platform:
- camera-log math in float on Windows and double on Linux;
- the operand order of multiplications, which decides which NaN comes out on each platform;
- NaN signs and text;
- the Windows build's CRLF built-in configs.

Where the official library reads or writes memory it doesn't own, the port returns an error
instead. Every such case, and every upstream bug the port reproduces, is listed in
[`docs/improvements.md`](docs/improvements.md) for a decision at the end of the port.

**Upstream tests ported:**

| Suite | Ported | Total |
|---|---:|---:|
| C++ | 379 | 1,191 |
| GPU | 0 | 264 |
| Python | 0 | 384 |

The live numbers are in [`docs/parity.md`](docs/parity.md). The GPU tests need a GPU to run
on; they are ported with the pixel harness in a later phase. The shaders themselves are already
compared with the official library's, text for text.

### Milestones

Each milestone is a complete section of OCIO, byte-exact on CPU and GPU:

| Milestone | Section | Version |
|---|---|---|
| **M0** (done) | Analytic transforms (matrix, range, exponent, log, CDL), the op engine and the processor API | `0.1.0` |
| **M1** | LUTs, fixed functions (including ACES 2.0), built-in transforms and configs, config read and write | `0.2.0` |
| **M2** | All 24 file formats, file-based configs and `.ocioz`, the baker | `0.3.0` |
| **M3** | Dynamic properties, grading ops, app helpers | `0.4.0` |
| **M4** | The `PyOpenColorIO`-compatible Python module | `0.5.0` |
| **M5** | Full OCIO 2.5.2 parity | `1.0.0` |

The full plan is in [`PLAN.md`](PLAN.md).

## Building and testing

You need:
- Rust 1.98.1 (pinned in `rust-toolchain.toml`);
- [uv](https://docs.astral.sh/uv/), for the oracle's Python environment;
- Docker, for the Linux reference container;
- [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) 0.20.2, for `cargo xtask gate --main`
  and `cargo xtask land` (`cargo install cargo-deny --version 0.20.2 --locked`).

```sh
git clone --recurse-submodules https://github.com/Pixel-Minions/OpenColorIO-rs
cd OpenColorIO-rs
cargo xtask gate                   # fmt, clippy, guardrails and the tests (live checks against
                                   # the official package), stopping at the first failure
cargo xtask gate --release --rocky # also in release, and all of it again on Rocky Linux 9
cargo xtask ci                     # guardrails, fixture hashes, the ratchet (branch mode)
cargo xtask ci --main              # also: the parity dashboard and the ratchet are current
```

**Where things are:**
- `crates/`: the port.
- `oracle/`: the pinned official package, used as the reference.
- `fixtures/`: reference outputs.
- `upstream/OpenColorIO`: the upstream source at the matched tag.
- `docs/`: plan cards, spike reports, the parity dashboard, deviations.

## License

BSD-3-Clause, the same license as OpenColorIO. This project is a derivative work of
OpenColorIO (Copyright Contributors to the OpenColorIO Project). [`NOTICE`](NOTICE) lists the
third-party code it ports: Imath (BSD-3-Clause) and yaml-cpp (MIT). OpenColorIO-rs is not
affiliated with the Academy Software Foundation.
