# Porting OpenColorIO 2.6.0

The 1.0.0 release matches OpenColorIO 2.5.2. This plan moves the port to **2.6.0** (released
2026-10-01, the base of VFX Reference Platform CY2027) with the same definition of done
(`PLAN.md` §3): byte-for-byte parity with the official `opencolorio==2.6.0` wheel on Windows
and Rocky Linux 9.

## Owner decisions (2026-11-17)

- **Downloads approved:** the `v2.6.0` tag into `upstream/OpenColorIO` and the
  `opencolorio==2.6.0` wheels into the oracle environment. Sources of the bumped third-party
  libraries (yaml-cpp 0.9.0, expat 2.8.4, Imath 3.2.3, zlib 1.3.2, minizip-ng 4.2.2) are a
  separate ask, made when phase 2 starts.
- **Integration branch:** the work lands on `ocio-2.6`, and one PR merges it into `main` at
  the end. `main` stays on 2.5.2 and releasable until then; the tag `v1.0.0` is the base for
  any 2.5.x maintenance branch. Locally, `phase0` is the integration branch as before
  (`cargo xtask land` is unchanged); landing pushes it to `origin/ocio-2.6` instead of `main`.
- **Red window:** once the oracle answers as 2.6.0, tests whose expected values changed go red
  until the behaviour changes are ported. This is allowed only inside the phase 0/1 card
  branch: its intermediate commits may be red, each commit message lists the failing tests,
  and each later commit must shrink that list. `cargo xtask land` gates the card's final
  state, so nothing red ever reaches `ocio-2.6` or `main`.
- **Version:** `1.1.0+ocio.2.6.0`.

## The upstream change, measured

`git diff v2.5.2 v2.6.0` in the submodule: 53 commits, 383 files, +52,384 / -22,112. Most of
the volume is data or out of scope:

| Area | Files | +/- | Scope |
|---|---:|---:|---|
| `tests/data/files` (CLF/SMPTE and config test files) | 86 | +36,207 / -110 | test inputs |
| `src/apps/ocioview` (moved to its own repository) | 98 | deletions | n/a |
| `src/libutils/oglapphelpers` (DirectX 12 and Vulkan GPU test backends) | 15 | +4,017 / -137 | see "Not ported" |
| `.github`, `share/cmake`, `share/ci`, `docs` | ~25 | | n/a (docs read for behaviour) |
| `src/OpenColorIO/builtinconfigs` (ACES 2.1 configs) | 8 | +2,232 / -4 | port |
| `src/OpenColorIO` core files | 27 | +1,921 / -750 | port |
| `src/OpenColorIO/fileformats` (CTF/CLF, CDL, Iridas look, xmlutils) | 13 | +1,220 / -478 | port |
| `src/OpenColorIO/ops` (fixed function, small edits in 12 op families) | ~20 | +250 / -75 | port |
| `src/OpenColorIO/apphelpers`, `transforms` | 7 | +370 / -32 | port |
| `include/OpenColorIO` (public API) | 3 | +260 / -8 | port |
| `src/bindings/python` | 6 | +118 / -15 | port |
| `src/apps` (ociocheck, ocioconvert, ociomakeclf, ociodisplay, ociochecklut, ...) | 12 | +75 / -32 | port |
| upstream tests: C++ `OCIO_ADD_TEST` / `OCIO_ADD_GPU_TEST` | | 1,433 → 1,474 (+41, none removed) | port |
| upstream tests: Python `def test` | | 388 → 396 | port |

Bundled third-party versions (`share/cmake/modules/FindExtPackages.cmake`, recommended versions,
to be confirmed in the wheel itself in phase 0): yaml-cpp 0.8.0 → **0.9.0**, expat 2.7.2 →
**2.8.4**, Imath 3.2.1 → **3.2.3**, zlib 1.3.1 → **1.3.2**, minizip-ng 4.0.10 → **4.2.2**;
pystring 1.1.4, lcms2 2.17 and pybind11 unchanged. The PyPI wheels for 2.6.0 have the same
platform tags as 2.5.2 (`win_amd64`, `manylinux_2_28_x86_64`, cp313).

## Wheel facts (phase 0, 2026-11-18)

Read from the pinned wheels with `tools/wheel-inspect` (`--wheel 2.5.2` for the previous ones):
the MSVC Rich header, the ELF `.comment` and symbol versions, the libraries' own version strings
and symbols, and a per-function comparison of both releases' machine code.

| | 2.5.2 | 2.6.0 |
|---|---|---|
| Windows library | `OpenColorIO_2_5.dll`, sha256 `f75646b4...` | `OpenColorIO_2_6.dll`, sha256 `fdab9f21...` |
| Windows compiler (C and C++ objects) | cl 19.44.**35226** | cl 19.44.**35229** |
| Windows linker; static CRT objects (SVML) | 14.44; 14.44.35207 | 14.44; 14.44.35207 |
| Linux library | `libOpenColorIO.so`, sha256 `18c4d7ad...` | `libOpenColorIO.so`, sha256 `b9289e30...` |
| Linux compiler | GCC 14.2.1 20250110 (Red Hat 14.2.1-**7**) | GCC 14.2.1 20250110 (Red Hat 14.2.1-**11**) |
| zlib | 1.3.1 (both, version strings) | **1.3.2** (both) |
| expat | 2.7.2 (`expat_2.7.2`, Linux) | **2.8.4** (`expat_2.8.4`, Linux; the Windows DLL's expat debug strings changed the same way) |
| yaml-cpp | 0.8.0 | **0.9.0**: new `YAML::FpToString` with dragonbox (`YAML::jkj::dragonbox`), `Utils` writers taking `(const char *, size_t)` |
| minizip-ng | 4.0.10 | newer: adds `mz_path_is_symlink_target_safe`, `mz_dir_has_unsafe_symlink`, `mz_os_get_temp_path`, `mz_zip_writer_get_follow_links` (upstream's install script fetches 4.2.2) |
| Imath | 3.2.x (namespace `Imath_3_2`) | 3.2.x (same namespace; the patch level is not visible in the binary; upstream's install script fetches 3.2.3) |
| pystring, lcms2 (apps only), pybind11 | no version marker in the binaries | unchanged markers (pybind11 internals `v11`) |
| OpenEXR (apps only) | `Imf_3_4` | `Imf_3_4` |

What this means for the port:

- **Compiler change: patch builds only.** MSVC moved from build 35226 to 35229 of the same
  14.44 toolset (Visual Studio 17.14) and GCC from the Red Hat 14.2.1-7 to the 14.2.1-11
  rebuild of the same 14.2.1 snapshot. Of the functions named in both builds, 8,980 of 10,387
  (Linux) and 2,188 of 2,330 (Windows, RTTI and exports) compiled to identical instructions,
  addresses aside. Every function whose floating-point instruction sequence changed is one
  whose upstream source (or a bundled library) changed: the hue curve renderers
  (`GradingHueCurveOpCPU.cpp`), the op data `equals`/`validate`/`isInverse`/`compose`
  functions (`Op.h`), `MatrixOpData::getAsForward`, `combineWith`, the YAML, CTF and CDL
  writers, and expat. (On Windows a few more matches were jump-table data decoded as
  instructions.) No renderer with unchanged source moved, so the operand
  orders the 2.5.2 port pinned still hold where the source did not change; changed code is
  re-read with `wheel-inspect` when it is ported.
- **FMA, F16C, libm and SVML unchanged.** Each library still has 108 FMA and 24 F16C
  instructions; the math library imports are the same functions with the same glibc symbol
  versions (only call-site counts grew, from new code: `fmodf` +2 on Linux, +1 on Windows).
  The Windows PQ kernel still calls MSVC's SVML `__vdecl_powf4` (now at `0x180358e60`), whose
  `__sse2_powf4` kernel has the same 594 instructions as in 2.5.2: waiver W0001 is unchanged.
- **yaml-cpp 0.9.0 formats floats differently** (dragonbox): the regenerated fixtures show
  `1.401298e-45` written as `1e-45`, `3.402823e+38` as `3.402824e+38`, `1.13` as
  `1.13000000000000`. This is card `ocio26-deps` (phase 2); until then every test comparing
  config YAML with such numbers is on the red list.

## Phases

Each item is a card that lands as reviewable chunks under the usual rules (`CLAUDE.md`).
The `upstream-map.toml` entries are re-based on the 2.6.0 diff in phase 0, and every later
card updates them as it ports.

### Phase 0: re-baseline (oracle change, verifier-reviewed, first chunks of card `ocio26-base`)

1. Move the submodule pin to `v2.6.0`.
2. Pin the oracle to `opencolorio==2.6.0` (`oracle/pyproject.toml`, `oracle/uv.lock`), confirm
   with `tools/wheel-inspect` which third-party versions and compiler each wheel was built
   with, and record any toolchain change (it can move operand order and FMA contraction;
   `docs/wheel-inspect.md`).
3. Regenerate the committed fixture groups (`cargo xtask oracle regen`), with owner review of
   the changed text as for any regen.
4. `upstream-map.toml`: mark every file the 2.6.0 diff touches as `partial` with its new line
   ranges; add the new files (`apphelpers/ConfigCompatibility.*`, the two ACES 2.1 configs).
5. Run the gate on both platforms and record the failing tests: this is the red list the
   phase 1 chunks must empty.

### Phase 1: behaviour changes to existing outputs (rest of card `ocio26-base`)

Ported in dependency order, each chunk shrinking the red list:

- `Op.h` / `Op.cpp` / `OpOptimizers.cpp`: the optimizer refactor (PR #2244; about 380 lines
  rewritten, plus debugging output). It decides which ops are combined and removed, so it
  changes processor cache IDs and pixels of optimized pipelines. Highest risk; the battery
  runs with every optimization flag set.
- Alpha defaults to 1 instead of 0 when the input image has no alpha channel (PR #2197;
  `ImagePacking.cpp`/`.h`, `ScanlineHelper.cpp`).
- Cycle prevention in transforms (PR #2318; `Transform.cpp`, `ContextVariableUtils.cpp`).
- `HashUtils` (cache IDs), `CPUInfo`, `Processor`, `ParseUtils`, `ViewingRules`,
  `OCIOZArchive`, the one-to-three-line edits in 12 op families (CDL, exponent,
  exposure/contrast, grading primary/tone/RGB curve/hue curve, log, LUT1D/3D, matrix, noop,
  range).
- GPU shader changes from PR #2319 (`GpuShaderClassWrapper`, `GpuShaderUtils`,
  `GradingRGBCurveOpGPU`, fixed function GPU) and the RGB/hue curve change for Metal
  (PR #2342), checked against the wheel's shader text for all ten shading languages.

### Phase 2: bundled libraries (card `ocio26-deps`; needs the third-party source download)

For each library the wheel bundles at a new version, diff the old and new upstream sources,
port the differences that reach translated code, and port that library's own tests at the
new version. Order: yaml-cpp 0.9.0 (it can change the bytes of written configs), expat 2.8.4,
minizip-ng 4.2.2, Imath 3.2.3 (half conversion), zlib 1.3.2. `THIRD-PARTY-NOTICES.md` updated.

### Phase 3: config features (card `ocio26-config`)

- Display and view aliases (PR #2341): `use_display_aliases`, view `aliases`,
  `getCanonicalDisplayName`/`getCanonicalViewName`, `getResolvedDisplayViewColorSpaceName`,
  the alias accessors and the new `addDisplayView`/`addSharedView` overloads,
  `DisplayViewTransform` resolution, YAML read/write with the 2.6 version gate.
- `getDisplayDescription`.
- Color Interop ID (PR #2345): `findColorSpaceForID`, `LocateBuiltinColorSpace`,
  `generateLocalIDForColorSpace`.
- HDR display compatibility (PR #2344): `ConfigCompatibilityHelpers::CheckCompatibility`
  with `CONFIG_HDR_DISPLAY_SUPPORT_26`.
- The matching Python bindings and Python tests.

New Rust API shapes follow the existing conventions; anything without a precedent goes to the
owner as a short question with a recommendation.

### Phase 4: new ops and data (card `ocio26-ops`)

- `FIXED_FUNCTION_ACES_RGB_TO_HMJ_20` (PR #2336): op data, CPU renderer, GPU shader, the
  2.6 version gate; checked through the battery with its pass-through channels declared.
- Built-in transform `APPLE_LOG-APPLEWG_to_ACES2065-1` (PR #2343).
- The ACES 2.1 built-in configs (`cg-config-v5.0.0_aces-v2.1_ocio-v2.6`,
  `studio-config-v5.0.0_aces-v2.1_ocio-v2.6`), the registry, and the new targets of
  `ocio://default`, `ocio://cg-config-latest` and `ocio://studio-config-latest`.

### Phase 5: SMPTE ST 2136-1 CLF (card `ocio26-clf`)

Reading and writing (PRs #2265, #2349): `CTFTransform`, `CTFReaderHelper`, `CTFReaderUtils`,
`FileFormatCTF`, `FormatMetadata` (`METADATA_ID_ELEMENT`), xmlutils, and the CDL parser and
Iridas look changes that came with it. The largest test port (`FileFormatCTF_tests` +790,
`CTFTransform_tests` +265) and most of the new test data.

### Phase 6: tools and Python parity (card `ocio26-tools`)

`ociocheck` (checks the library version first, PR #2310; reports compatibility),
`ocioconvert` (bit-depth handling, PR #2323), `ociomakeclf`, `ociodisplay`, `ociochecklut`, and
the one-line edits in the other tools. Then `cargo xtask wheel-check` against upstream's 2.6.0
Python tests.

### Phase 7: release 1.1.0+ocio.2.6.0

Full tier on both platforms, the SDE workflow on all five CPUs, `docs/improvements.md` and
`docs/deviations.md` reviewed for entries that 2.6.0 fixed or changed, README and CHANGELOG,
then the PR from `ocio-2.6` into `main` and the release checklist.

Phases 2 to 5 are independent and run in parallel once phase 1 has landed; phase 6 follows
them.

## Not ported

- `src/apps/ocioview` (removed upstream).
- The DirectX 12 and Vulkan GPU unit-test backends (PRs #2271, #2243): upstream test
  infrastructure that executes shaders, not library behaviour. The port keeps checking shader
  text in every language against the wheel, and executing GLSL as today. Revisit only if the
  owner wants HLSL or Vulkan execution.
- CI, CMake and website changes.
