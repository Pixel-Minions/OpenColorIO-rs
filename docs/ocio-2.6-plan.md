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
