# Changelog

Each release names the OpenColorIO version it matches. A version's build metadata says it too:
`1.0.0+ocio.2.5.2`.

## 1.0.0

Matches **OpenColorIO 2.5.2**.

The first release: the complete port of OpenColorIO 2.5.2, checked byte by byte against the
official `opencolorio==2.5.2` wheel on the same OS and CPU (PLAN.md section 3).

- **What it is.** The library, with its file formats and GPU shader generation; the
  `PyOpenColorIO` Python module; and four tools.
- **Packages.** Four crates on crates.io: `opencolorio-rs` (the library `ocio`),
  `opencolorio-rs-ops`, `opencolorio-rs-formats` and `opencolorio-rs-gpu`. One wheel on PyPI,
  `opencolorio-rs`, version `1.0.0`, with `PyOpenColorIO.__version__` `2.5.2`.
- **Platforms.** Windows x86-64, and Linux x86-64 with glibc 2.34 or newer. Rocky Linux 9 is the
  Linux reference. Where the official library gives different bytes on Windows and on Linux, the
  port does too. No macOS or ARM yet.
- **Python 3.13 only** (`cp313`; PLAN.md D14). The wheel can't share an environment with the
  official `opencolorio` wheel, because both install `PyOpenColorIO`; see
  `docs/python-migration.md`.
- **Four tools:** `ociocheck`, `ociochecklut`, `ociowrite` and `ociobakelut` (PLAN.md section 4).
  The official wheel's other tools aren't included.
- **Speed.** Most paths run within 10% of the official library on the same machine. The listed
  exceptions are accepted for 1.0.0 (`docs/cards/phase8.md`, owner decision 2026-11-01).
  `CpuProcessor::apply_threaded`, which upstream doesn't have, renders rows on several threads.
- **Fuzzing** of the parsers against the official library comes after 1.0.0 (PLAN.md section 8).
- **Exceptions to byte-exact output.** The approved waivers are in `waivers.toml`, explained in
  `docs/waivers.md`. Intended differences from OpenColorIO are in `docs/deviations.md`, and the
  upstream bugs and limitations the port reproduces, for a later decision, in
  `docs/improvements.md`.

### Known issues

- **An oracle crash under emulation, on Windows.** Twice, under Intel SDE on Windows (once on
  `snb`, once on `nhm`), the oracle process running the official 2.5.2 wheel exited with
  `0xc0000005` partway through a 50-call batch of `legacy_gpu_shader` requests. Each of those
  requests starts a native helper process. It didn't reproduce in later CI runs, in a native
  replay of the same 1400 requests, or in a local SDE `snb` replay of the crashing batch. Native
  memory stays flat, and the boundary requests are refused before they allocate anything. The
  cause is unconfirmed. The leading suspect is SDE/Pin's injection into child processes on
  Windows, the same mechanism behind an earlier, now-fixed startup crash. The crash is in the
  reference process under emulation, not in the port.
