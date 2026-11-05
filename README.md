# OpenColorIO-rs

**A pure-Rust port of [OpenColorIO](https://github.com/AcademySoftwareFoundation/OpenColorIO)
that gives exactly the same output as the original, down to the last bit.**

OpenColorIO (OCIO) is the color management library of the VFX and animation industry. This
project ports it to Rust one upstream release at a time, and checks every result against the
official library. The current release, **1.0.0** (`1.0.0+ocio.2.5.2`), is the complete port of
**OpenColorIO 2.5.2**: the same pixels, shaders, text, errors and accepted configs, from Rust and
from Python.

## Installing

```sh
cargo add opencolorio-rs      # in code, the library is `ocio`
pip install opencolorio-rs    # Python 3.13; `import PyOpenColorIO`
```

```rust
let config = ocio::Config::create_raw()?;
let processor = config.processor_between("raw", "raw")?;
let cpu = processor.default_cpu_processor()?;

let mut pixels = vec![0.18_f32; 4 * 2 * 2];
let mut image = ocio::PackedImageDesc::new(&mut pixels, 2, 2, 4)?;
cpu.apply(&mut image)?;
```

- The crate documentation (`cargo doc -p opencolorio-rs --open`) starts with this example.
- The wheel installs `PyOpenColorIO`, so it can't share an environment with the official
  `opencolorio` package. Moving over: [`docs/python-migration.md`](docs/python-migration.md).
- Platforms: Windows x86-64, and Linux x86-64 with glibc 2.34 or newer. ARM and macOS come
  later.

## What's included

| Part | Status |
|---|---|
| The library: configs, every transform and op, processors, the optimizer, CPU renderers for every SIMD profile (SSE2, AVX, AVX2, AVX-512) | Complete |
| All 24 file formats, `.ocioz` archives and the baker | Complete |
| GPU shader generation, in all 10 shading languages | Complete |
| Dynamic properties, grading and the app helpers | Complete |
| The `PyOpenColorIO`-compatible Python module | Complete: all 384 upstream Python tests pass unmodified |
| Tools: `ociocheck`, `ociochecklut`, `ociowrite`, `ociobakelut` | Included; the official wheel's other tools aren't |

All 913 upstream source files are accounted for: 685 ported and 228 reviewed as not
applicable ([`upstream-map.toml`](upstream-map.toml)). What changed, the accepted speed
exceptions and the known issues are in the [changelog](CHANGELOG.md).

## How exact

A release ships only when all of these hold:

- **Text is byte-identical on every platform:** written configs, LUT files, baked LUTs, shader
  code, cache IDs, error and warning messages, and Python `repr()` strings.
- **Pixels are bit-identical** to the official `opencolorio==2.5.2` package on the same OS and
  CPU, at every bit depth, pixel layout, optimization level and SIMD profile.
- **GPU shaders are byte-identical**, with the same uniforms and textures.
- **Where OCIO differs between Windows and Linux, the port does too.**
- **OCIO's own tests pass:** 1,189 of 1,191 C++ tests ported (two reviewed not applicable),
  all 264 GPU tests and all 384 Python tests.

The few exceptions are written down and approved: [waivers](docs/waivers.md) (for example,
NaN payload bits when a parameter is NaN), [intended deviations](docs/deviations.md) and the
[upstream bugs the port reproduces](docs/improvements.md). How all this is checked, the oracle,
the reference platforms and the emulated CPUs, is in [`docs/verification.md`](docs/verification.md).

## Why a Rust port

- **Interchangeable with other OCIO applications.** A config gives the same pixels, shaders,
  baked LUTs and cache IDs as any application using OCIO 2.5.2 on the same machine.
- **No C++ in your stack.** The whole build is `cargo build`: no CMake, no C++ dependencies,
  no ABI mismatches.
- **Safe with untrusted files.** The config and LUT parsers have no `unsafe` code. Where
  upstream reads memory it doesn't own, the port returns an error instead.
- **One engine for Rust and Python.** Scripts and applications give identical results.
- **Room to be faster without changing results.** `CpuProcessor::apply_threaded`, which
  upstream doesn't have, renders rows on several threads, and the tests check it against the
  official library.
- **Tracks new OCIO releases.** Each upstream release is ported as a diff and checked again
  against that release. The version says what it matches: `1.0.0+ocio.2.5.2`.

## Building and testing

You need Rust 1.98.1 (pinned in `rust-toolchain.toml`), [uv](https://docs.astral.sh/uv/) for
the oracle's Python environment, and Docker for the Linux reference container.

```sh
git clone --recurse-submodules https://github.com/Pixel-Minions/OpenColorIO-rs
cd OpenColorIO-rs
cargo xtask gate                   # fmt, clippy, guardrails and the tests against the official package
cargo xtask gate --release --rocky # also in release, then all of it again on Rocky Linux 9
```

Contributing works through plan cards and chunks: see [`PLAN.md`](PLAN.md) and
[`CLAUDE.md`](CLAUDE.md), which list every command.

| Where | What |
|---|---|
| `crates/` | The port: `ocio-ops`, `ocio-formats`, `ocio-gpu`, `ocio` (the library), `ocio-py` (the Python module), `ocio-tools`; and the test kit, `ocio-testkit` and `ocio-gpu-exec` |
| `oracle/` | The pinned official package, used as the reference |
| `fixtures/` | Committed reference outputs, locked by hashes |
| `upstream/` | The upstream sources at the matched tags |
| `docs/` | [Verification](docs/verification.md), [history](docs/history.md), [parity dashboard](docs/parity.md), plan cards |

## License

BSD-3-Clause, the same license as OpenColorIO, for this project's own code. The code it ports
keeps its own license, and each package's license field names them. SampleICC's ICC Software
License 0.2 has no SPDX identifier, so the `opencolorio-rs` crate's README names it.

This project is a derivative work of OpenColorIO (Copyright Contributors to the OpenColorIO
Project); [`NOTICE`](NOTICE) is its notice, and
[`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md) has every third-party notice in full.
OpenColorIO-rs is not affiliated with the Academy Software Foundation.
