# Phase 10: the dev tools

Port the four in-scope tools of OCIO 2.5.2 into `crates/ocio-tools`: `ociocheck`,
`ociochecklut`, `ociowrite` and `ociobakelut`, as `crates/ocio-tools/src/bin/<tool>.rs`, plus
what they use from `src/apputils/`. PLAN.md §4 scopes the other apps out.

The expected result is what the real binary of the `opencolorio==2.5.2` wheel does on the same
platform: stdout, stderr, exit code and the files it writes, byte for byte, on Windows and
Rocky Linux 9.

## Facts that shape the port

- Both wheels' tools are built without OpenGL (`Compiled without OpenGL support` is in both
  `ociochecklut` binaries), so the `OCIO_GPU_ENABLED` code is not compiled in and is not
  ported: every GPU option ends in that message.
- `std::cout`, `std::cerr` and `std::ofstream` are text streams. On Windows the C runtime
  writes each `\n` as `\r\n`, in the tools' output and in the files they write. Rust does not,
  so `crates/ocio-tools/src/stdio.rs` does. Python sets its own stdio to binary, which is why
  the library's default logging function writes `\n`, correctly for the Python module: the
  tools replace it with one that goes through `stdio.rs`.
- `argv` numbers go through the C runtime (`atoi`, `atof`, `strtof`); `apputils/c_numbers.rs`
  reproduces them on each platform, checked against the platform C runtime (`crt.rs`).
- Text that isn't stable: none of the four tools prints a timing (`measure.h` is used only by
  `ocioconvert`, `ocioperf` and `ociolutimage`, which are out of scope, so it isn't ported),
  and the version banner is the library's `2.5.2` on both sides. Absolute paths appear in the
  output (`Working Dir:`, error messages); the sweeps run the wheel's tool and the port's in
  the same directory, with the same arguments and environment, so that they need no
  normalising at all. On Linux `ociocheck`'s `CacheID` covers the inode of each file a config
  refers to, so the sweeps read LUT files where they are, not from a copy made for each run, and
  never replay the wheel's output from the oracle's cache.

## Chunks

| # | Chunk | Upstream | Checks |
|---|---|---|---|
| 1 | This card | | |
| 2 | Oracle: `tool_run` (`oracle/ocio_oracle/tools_api.py`), on its own | | contract test against the real binaries |
| 3 | `stdio`, `c_numbers` | the C runtime | `crt.rs` for the numbers |
| 4 | `argparse`, `strutil`, with `ociowrite`, the first tool to exercise them | `src/apputils/argparse.*`, `strutil.*`, `src/apps/ociowrite/main.cpp` | sweep: configs and options, bad arguments |
| 5 | `log_guard`, with `ociocheck` | `src/apputils/logGuard.*`, `src/apps/ociocheck/main.cpp` | sweep: upstream configs, built-in configs, broken configs, `--oconfig` |
| 6 | `ociochecklut` | `src/apps/ociochecklut/main.cpp` | sweep: upstream's LUT files, pixels, `-t`, `-s`, `-v`, `--inv` |
| 7 | `ociobakelut` (not ICC) | `src/apps/ociobakelut/main.cpp` | sweep: configs and LUTs × formats × options |

## Stopped for the owner

ICC output in `ociobakelut` (`ocioicc.cpp`): needs lcms2 or a translation of it. The format
reports a clear error until the owner decides.
