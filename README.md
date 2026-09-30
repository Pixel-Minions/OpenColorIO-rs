# OpenColorIO-rs

**A pure-Rust port of [OpenColorIO](https://github.com/AcademySoftwareFoundation/OpenColorIO)
that produces exactly the same output as the original, down to the last bit.**

OpenColorIO (OCIO) is the color management library of the VFX and animation industry. This
project ports it to Rust one upstream release at a time. The current line matches **OpenColorIO
2.5.2**: the same results, the same text output, the same errors and the same accepted configs,
from Rust and, later, from Python.

> **Status: early development.** Phase 0 (test harness and feasibility proofs) is complete.
> Nothing is usable end to end yet. See [Progress](#progress).

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
  there are two, and neither is visible:
  - **W0001:** on Windows, the port uses standard `powf` in one exact-math PQ path. The official
    build uses MSVC's vector math library there. About 1.7% of values differ, far below one
    8-, 10- or 12-bit code value.
  - **W0002:** in a config with a NaN parameter, the internal bits of the resulting NaN values
    may differ. They are still NaN.
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

Status as of 2026-09-30: **Phase 0 complete** (test harness and feasibility proofs), about 5%
of the planned work. Phase 1 (the op engine and analytic transforms) is next.

**Done:** everything below is bit-exact against the official library on Windows and Linux, in
debug and release builds, and was reviewed independently before merging.
- **Test harness:**
  - the live oracle;
  - hash-locked reference fixtures;
  - guardrails (`cargo xtask ci`);
  - the parity dashboard;
  - a Rocky Linux 9 reference container;
  - CI on Windows and Linux.
- **Cache-ID hashing (XXH3-128).** It reproduces the official cache IDs of all 8 built-in
  configs.
- **OCIO's fast-math functions** (the approximations of `log2`, `exp2` and `pow` used by
  default), ported operation for operation.
- **The Log and Gamma CPU renderers**, with fast math on and off.
- **CPU feature detection**, identical to OCIO's own (`ociocpuinfo`).
- **The 3D LUT forward kernels in every SIMD variant** (SSE2, AVX, AVX2, AVX-512, including
  the FMA kernels), identical to the official library's own kernels.
- **All three half-float conversions OCIO uses**, compared on every possible input.
- **C and C++ number formatting and parsing** as OCIO uses them, identical to each platform's
  runtime.
- **A port of the yaml-cpp emitter.** It writes the `serialize()` output of all 8 built-in
  configs byte for byte, with OCIO's byte-string semantics.

Along the way, the checks found platform differences inside the official library itself. The
port reproduces each of them per platform:
- camera-log math in float on Windows and double on Linux;
- NaN signs and text;
- the Windows build's CRLF built-in configs.

**Upstream tests ported:**

| Suite | Ported | Total |
|---|---:|---:|
| C++ | 68 | 1,191 |
| GPU | 0 | 264 |
| Python | 0 | 384 |

The live numbers are in [`docs/parity.md`](docs/parity.md).

### Milestones

Each milestone is a complete section of OCIO, byte-exact on CPU and GPU:

| Milestone | Section | Version |
|---|---|---|
| **M0** | Analytic transforms (matrix, range, exponent, log, CDL), the op engine and the processor API | `0.1.0` |
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
- Docker, for the Linux reference container.

```sh
git clone --recurse-submodules https://github.com/Pixel-Minions/OpenColorIO-rs
cd OpenColorIO-rs
cargo xtask ci                     # guardrails, fixture hashes, ratchet, parity dashboard
cargo test --workspace             # includes live checks against the official package
scripts/rocky9.sh cargo test --workspace   # the same on Rocky Linux 9
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
