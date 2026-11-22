# Changelog

Each release names the OpenColorIO version it matches. A version's build metadata says it too:
`1.0.0+ocio.2.5.2`.

## 1.1.0

Matches **OpenColorIO 2.6.0**.

The port moves from OpenColorIO 2.5.2 to 2.6.0 (released 2026-10-01), checked byte by byte
against the official `opencolorio==2.6.0` wheel on the same OS and CPU, as 1.0.0 was against
2.5.2. As in 1.0.0: the four packages; Windows x86-64, and Linux x86-64 with glibc 2.34 or newer;
Python 3.13 only; the four tools; fuzzing still to come; and the speed misses accepted for
1.0.0, below, which haven't been measured again against 2.6.0. The wheel's version is `1.1.0`,
with `PyOpenColorIO.__version__` `2.6.0`.

- **Config features.**
  - Display and view aliases (PR #2341): a view's `aliases` and the config's
    `use_display_aliases`, read and written at config version 2.6; display and view names
    resolve through them, in `DisplayViewTransform` too. New calls:
    `Config::canonical_display_name`, `canonical_view_name`,
    `resolved_display_view_color_space_name`, `display_view_alias` and its siblings, and
    `add_display_view_with_aliases` and `add_shared_view_with_aliases`.
  - `Config::display_description`.
  - Color Interop IDs (PR #2345): `Config::find_color_space_for_id`,
    `generate_local_id_for_color_space` and `Config::locate_builtin_color_space`.
  - The HDR display compatibility check (PR #2344):
    `apphelpers::config_compatibility::check_compatibility` with
    `ConfigCompatibility::HdrDisplaySupport26`.
  - The Python bindings of all of these.
- **New ops and data.**
  - `FIXED_FUNCTION_ACES_RGB_TO_HMJ_20` (PR #2336), on the CPU, in shaders and in CTF files.
  - The built-in transform `APPLE_LOG-APPLEWG_to_ACES2065-1` (PR #2343).
  - Both need config version 2.6.
  - The ACES 2.1 built-in configs, `cg-config-v5.0.0_aces-v2.1_ocio-v2.6` and
    `studio-config-v5.0.0_aces-v2.1_ocio-v2.6`. `ocio://default`, `ocio://cg-config-latest` and
    `ocio://studio-config-latest` now name them.
- **SMPTE ST 2136-1 CLF** (PRs #2265, #2349, #2350): reading and writing, with the SMPTE
  namespace, the `Id` element (`METADATA_ID_ELEMENT`), descriptions with their language and
  `urn:uuid:` ids; Range ops written with `noClamp`; and XML readers that stop the parser at
  the first error.
- **Outputs that 2.6.0 changed**, and the port with it:
  - processor cache IDs are UUID-shaped hashes;
  - an image without alpha gets the bit depth's maximum as its alpha (1.0 in float), where
    2.5.2 gave 0 (PR #2197);
  - transforms nest at most 33 calls deep, and deeper ones, cycles among them, are an error
    (PR #2318);
  - the optimizer's 2.6.0 steps, and its debug log (PR #2244);
  - the video hue curve grades luma by a gain;
  - the shader edits of PRs #2319 and #2342;
  - CPUInfo drops AVX-512 on an AMD EPYC 9V45 (PR #2324);
  - `Config::create_from_file` checks that the file exists before it opens it (PR #2310);
  - the version is 2.6.0 and the namespace `OpenColorIO_v2_6`, where the output names them.
- **Bundled libraries.** The port follows the libraries the 2.6.0 wheels are built with:
  yaml-cpp 0.9.0 (numbers written through dragonbox, its shortest form), expat 2.8.4,
  minizip-ng 4.2.2, Imath 3.2.3 and zlib 1.3.2. Imath's half conversions and zlib's deflate
  output are the same as before.
- **Tools.** `ociocheck` takes `-i` (for `--iconfig`) and `-v`, and reports the config's
  compatibility. `ociochecklut` changed in 2.6.0 only in GPU code the wheels don't build. The
  four tools are still 1.0.0's.
- **Exceptions.**
  - Waivers: W0028 is new (a config path that isn't ASCII, on Windows; U-750). W0013, W0014,
    W0016 and W0017 hold for 2.6.0 too. W0015 and W0018 are 2.5.2's only: 2.6.0 raised
    their tests' thresholds, and the tests pass without them.
  - Deviations: D-22 is new (how extracting an `.ocioz` archive reads a link's target on
    Windows). D-6 names the 2.6.0 libraries. D-21: 2.6.0 bounds the nesting of transforms
    itself, and the port matches it, so that part no longer deviates.
  - The register (`docs/improvements.md`): I-950, I-951 (yaml-cpp 0.9.0), I-952 (minizip-ng
    4.2.2 on Windows), I-980 (merging configs drops view aliases), I-1030, I-1031, I-1032
    (CLF), U-730, U-750 and U-790 are new. I-171 changes with expat 2.8.4, 2.6.0 fixes I-180,
    and U-58 records upstream's new nesting limit.

### Known issues

- **Emulated CPUs.** The full emulated-CPU workflow hasn't run on 1.1.0, because GitHub Actions
  wasn't available: it waits for CI. A part of it ran locally. What ran, and each attempt that
  didn't pass, is in
  [`docs/verification.md`](https://github.com/Pixel-Minions/OpenColorIO-rs/blob/main/docs/verification.md).
- **Crashes under emulation, on Windows, with the 2.6.0 wheel.** The wheel's native helper
  crashed five times with `0xC0000142`, and Pin logged that it failed to inject into child
  processes. Four of those crashes showed as a mismatch inside the oracle, between the wheel and
  that helper, before any comparison with the port. The oracle's process also died twice on a
  Pin assertion. Each of those tests passed later, natively too. Five Pin lock logs couldn't be
  tied to a test. The causes aren't known.
- **A hang under emulation, open for CI.** `api_formats_oracle`, with its tests running
  concurrently, hung on `nhm` on Windows and on Rocky Linux 9. One test at a time it passed
  natively and on the five emulated CPUs. The cause isn't known.

## 1.0.0 (2026-11-05)

Matches **OpenColorIO 2.5.2**.

The first release: the complete port of OpenColorIO 2.5.2, checked byte by byte against the
official `opencolorio==2.5.2` wheel on the same OS and CPU ([PLAN.md](https://github.com/Pixel-Minions/OpenColorIO-rs/blob/main/PLAN.md) section 3).

- **What it is.** The library, with its file formats and GPU shader generation; the
  `PyOpenColorIO` Python module; and four tools.
- **Packages.** Four crates on crates.io: `opencolorio-rs` (the library `ocio`),
  `opencolorio-rs-ops`, `opencolorio-rs-formats` and `opencolorio-rs-gpu`. `ocio` names every
  type of the API, `CpuProcessor` and the image descriptions included, so a user depends on
  `opencolorio-rs` alone; the other three are its parts. One wheel on PyPI,
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
