# Changelog

Each release names the OpenColorIO version it matches. A version's build metadata says it too:
`1.0.0+ocio.2.5.2`.

## 1.0.0

Matches **OpenColorIO 2.5.2**.

The first release: the complete port of OpenColorIO 2.5.2, checked byte by byte against the
official `opencolorio==2.5.2` wheel on the same OS and CPU ([PLAN.md](https://github.com/Pixel-Minions/OpenColorIO-rs/blob/main/PLAN.md) section 3).

- **What it is.** The library, with its file formats and GPU shader generation; the
  `PyOpenColorIO` Python module; and four tools.
- **Packages.** Four crates on crates.io: `opencolorio-rs` (the library `ocio`),
  `opencolorio-rs-ops`, `opencolorio-rs-formats` and `opencolorio-rs-gpu`. One wheel on PyPI,
  `opencolorio-rs`, version `1.0.0`, with `PyOpenColorIO.__version__` `2.5.2`.
- **Platforms.** Windows x86-64, and Linux x86-64 with glibc 2.34 or newer. Rocky Linux 9 is the
  Linux reference. Where the official library gives different bytes on Windows and on Linux, the
  port does too. No macOS or ARM yet.
- **Python 3.13 only** (`cp313`; [PLAN.md](https://github.com/Pixel-Minions/OpenColorIO-rs/blob/main/PLAN.md) D14). The wheel can't share an environment with the
  official `opencolorio` wheel, because both install `PyOpenColorIO`; see
  [`docs/python-migration.md`](https://github.com/Pixel-Minions/OpenColorIO-rs/blob/main/docs/python-migration.md).
- **Four tools:** `ociocheck`, `ociochecklut`, `ociowrite` and `ociobakelut` ([PLAN.md](https://github.com/Pixel-Minions/OpenColorIO-rs/blob/main/PLAN.md) section 4).
  The official wheel's other tools aren't included.
- **Speed.** Most paths run within 10% of the official library on the same machine. A ratio
  below is the port's time over the official wheel's, Linux ones against the Linux wheel: 1.13
  means the port takes 13% longer. These exceptions are accepted for 1.0.0 (owner
  decision 2026-11-01; measured in [`docs/cards/phase8.md`](https://github.com/Pixel-Minions/OpenColorIO-rs/blob/main/docs/cards/phase8.md)):
  - `apply_rgba` (one pixel per call) for Log and GradingPrimary, on Windows and Linux: the
    kernels spend their set-up per call. Whole images and rows are within 10%;
  - `ff_red_mod_10`, on both: scalar C++ on both sides, with no SIMD to port;
  - `lut1d` from 10-bit integers to f32, 1.13 on both: a scan of each row that upstream doesn't
    have;
  - Linux only, where GCC's scalar code is much faster than MSVC's and the port matches the
    Windows wheel: GradingTone 1.23-1.35, `range` 1.92, `ff_glow_10` 1.78, `ff_rgb_to_hsv` 1.29,
    `ff_rgb_to_hsy_log` 1.20, and the first op's row copy in `lut3d_tetra`, `lut1d_inv` and
    `lut1d_half`, 1.38-1.47 (ocioperf's in place loop is 1.01 there).

  `CpuProcessor::apply_threaded`, which upstream doesn't have, renders rows on several threads.
- **Fuzzing** of the parsers against the official library comes after 1.0.0 ([PLAN.md](https://github.com/Pixel-Minions/OpenColorIO-rs/blob/main/PLAN.md) section 8).
- **Exceptions to byte-exact output.** The approved waivers are in [`waivers.toml`](https://github.com/Pixel-Minions/OpenColorIO-rs/blob/main/waivers.toml), explained in
  [`docs/waivers.md`](https://github.com/Pixel-Minions/OpenColorIO-rs/blob/main/docs/waivers.md). Intended differences from OpenColorIO are in [`docs/deviations.md`](https://github.com/Pixel-Minions/OpenColorIO-rs/blob/main/docs/deviations.md), and the
  upstream bugs and limitations the port reproduces, for a later decision, in
  [`docs/improvements.md`](https://github.com/Pixel-Minions/OpenColorIO-rs/blob/main/docs/improvements.md).

### Known issues

- **An oracle crash under emulation, on Windows.** Twice, under Intel SDE on Windows (once on
  `snb`, once on `nhm`), the oracle process running the official 2.5.2 wheel exited with
  `0xc0000005` partway through a 50-call batch of `legacy_gpu_shader` requests. Each of those
  requests starts a native helper process. The `snb` crash didn't reproduce in later CI runs, in
  a native replay of the same 1400 requests, or in a local SDE `snb` replay of the crashing
  batch. In the `snb` crash's batch, native memory stays flat, and the boundary requests are
  refused before they allocate anything. The cause of either crash is unconfirmed. The leading
  suspect is SDE/Pin's injection into child processes on Windows, the same mechanism behind an
  earlier, now-fixed startup crash. Both crashes are in the reference process under emulation,
  not in the port.
