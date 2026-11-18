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
- **Phase 3 API (2026-11-18):** the 2.6 config API follows the existing conventions:
  `Config::add_display_view_with_aliases(display, view, view_transform, color_space, looks,
  rule, description, aliases: &[impl AsRef<[u8]>])` and `Config::add_shared_view_with_aliases(..)`
  for the overloads with aliases (the existing functions forward an empty list);
  `canonical_display_name`, `canonical_view_name`, `resolved_display_view_color_space_name`,
  `display_description` and `display_view_alias` returning `&[u8]` (`""` for none);
  `num_display_view_aliases -> i32`, `has_display_view_alias -> bool`,
  `use_display_aliases()` / `set_use_display_aliases(bool)`;
  `find_color_space_for_id -> Option<&ColorSpace>`,
  `generate_local_id_for_color_space -> Result<Vec<u8>>`,
  `Config::locate_builtin_color_space<'b>(src, name, builtin: &'b Config) -> Result<&'b [u8]>`;
  `apphelpers::config_compatibility::check_compatibility(&Config, ConfigCompatibility) -> bool`
  with `enum ConfigCompatibility { HdrDisplaySupport26 = 0 }`. `canonical_view_name` borrows
  its result from the config or from its `view_name` argument, as upstream returns the
  caller's pointer (orchestrator, within the owner's approval).
- **CreateFromFile with a non-ASCII path on Windows (2026-11-18):** 2.6.0 checks the path with
  `std::filesystem::exists`, which MSVC reads in the ANSI code page. The port is exact for
  ASCII paths on Windows and for every path on Linux; a non-ASCII path on Windows is the port's
  error (U-750, waiver W0028), following U-113/W0027.

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

## Upstream tests changed or added in 2.6.0

Every `OCIO_ADD_TEST` / `OCIO_ADD_GPU_TEST` of `tests/cpu` and `tests/gpu` whose body differs
between v2.5.2 and v2.6.0 (trailing spaces ignored; 153), and every new one (41; none was
removed), with the card that re-ports it. A port marked `@ v2.5.2` of a changed test is stale
until that card re-ports it at v2.6.0 (`@ v2.6.0`). The CTF/CLF ones belong to `ocio26-clf`;
config, display/view alias, interop ID, merge and compatibility ones (and `Config/version`,
`Config/display_view`) to `ocio26-config`; built-in configs and transforms and the fixed
functions to `ocio26-ops`; the rest to `ocio26-base` (phase 1).

The list is also data, `xtask/data/upstream-tests-changed-2.6.0.txt` (`cargo xtask
upstream-changed` writes it, `--check` checks it): `cargo xtask parity` warns about the stale
markers and lists them in `docs/parity.md` ("Stale ports"), and `cargo xtask ci --no-stale`,
a step of the 1.1.0 release (`docs/release-checklist.md`), fails while any remain. The ratchet
still counts them (orchestrator, 2026-11-18, option (b)).

| Test | Upstream file:line @ v2.6.0 | | Card |
|---|---|---|---|
| `OCIO_ADD_TEST(BuiltinConfigs, basic)` | `tests/cpu/builtinconfigs/BuiltinConfig_tests.cpp` | changed | `ocio26-ops` |
| `OCIO_ADD_TEST(BuiltinConfigs, create_builtin_config)` | `tests/cpu/builtinconfigs/BuiltinConfig_tests.cpp` | changed | `ocio26-ops` |
| `OCIO_ADD_TEST(Builtins, aces2_displayview_roundtrip)` | `tests/cpu/transforms/BuiltinTransform_tests.cpp` | changed | `ocio26-ops` |
| `OCIO_ADD_TEST(Builtins, interpolate)` | `tests/cpu/transforms/BuiltinTransform_tests.cpp` | changed | `ocio26-ops` |
| `OCIO_ADD_TEST(Builtins, read_write)` | `tests/cpu/transforms/builtins/BuiltinTransformRegistry_tests.cpp` | changed | `ocio26-ops` |
| `OCIO_ADD_TEST(Builtins, validate)` | `tests/cpu/transforms/BuiltinTransform_tests.cpp` | changed | `ocio26-ops` |
| `OCIO_ADD_TEST(CPUProcessor, with_one_1d_lut)` | `tests/cpu/CPUProcessor_tests.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_TEST(CPUProcessor, with_one_matrix)` | `tests/cpu/CPUProcessor_tests.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_TEST(CTFReaderTransform, accessors)` | `tests/cpu/fileformats/ctf/CTFTransform_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, cdl_clf)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, gamma1_ctf)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, gamma1_mirror_ctf)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, gamma1_pass_thru_ctf)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, gamma3_ctf)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, load_edit_save_matrix)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, load_edit_save_matrix_clf)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, load_grading_tone)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, load_save_matrix)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, lut1d_clf)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, matrix3x3_clf)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, range1_clf)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, range2_clf)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, range3_clf)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, range4_clf)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, save_cdl)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, save_group)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, save_log_affine)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, save_log_camera)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, save_lut1d_f16_raw)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, save_lut1d_f32)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, save_lut1d_halfdomain)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, save_matrix)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, save_range)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFVersion, read_version)` | `tests/cpu/fileformats/ctf/CTFTransform_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFVersion, version_write)` | `tests/cpu/fileformats/ctf/CTFTransform_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(Config, display_view)` | `tests/cpu/Config_tests.cpp` | changed | `ocio26-config` |
| `OCIO_ADD_TEST(Config, fixed_function_serialization)` | `tests/cpu/Config_tests.cpp` | changed | `ocio26-ops` |
| `OCIO_ADD_TEST(Config, get_processor_from_two_configs)` | `tests/cpu/Config_tests.cpp` | changed | `ocio26-config` |
| `OCIO_ADD_TEST(Config, inactive_color_space_read_write)` | `tests/cpu/Config_tests.cpp` | changed | `ocio26-config` |
| `OCIO_ADD_TEST(Config, version)` | `tests/cpu/Config_tests.cpp` | changed | `ocio26-config` |
| `OCIO_ADD_TEST(ConfigUtils, processor_to_known_colorspace)` | `tests/cpu/ColorSpace_tests.cpp` | changed | `ocio26-config` |
| `OCIO_ADD_TEST(FileFormatCTF, 3by1d_lut)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, attribute_float_parse_leading_spaces)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, bake_1d)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, bake_1d_3d)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, bake_1d_shaper)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, bake_3d)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, cdl_all_styles)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, cdl_clamp_fwd)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, cdl_missing_sat)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, cdl_missing_sop)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, cdl_missing_style)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, cdl_various_in_ctf)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, check_utf8)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, clf_examples)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, clf_process_list_bad_version)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, clf_process_list_higher_version)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, difficult_syntax)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, difficult_xml_unknown_elements)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, exponent_all_styles)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, exposure_contrast_linear)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, exposure_contrast_log)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, exposure_contrast_no_gamma)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, exposure_contrast_video)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, ff_load_save_ctf)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, gamma_alpha_test1)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, gamma_alpha_test2)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, gamma_alpha_test3)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, gamma_alpha_test4)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, gamma_alpha_test5)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, gamma_test1)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, gamma_test2)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, gamma_test3)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, gamma_test4)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, gamma_test6)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, indexMap_test1_clfv2)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, indexMap_test2_clfv2)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, info_example)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, inverse_of_id_test)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, load_deprecated_ops_file)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, load_fixed_function_file)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, load_grading_huecurves_log)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, load_grading_primary_lin)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, load_grading_primary_log)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, load_grading_primary_video)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, load_grading_rgbcurves_lin)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, load_grading_rgbcurves_log)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, log_all_styles)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, log_bad_version)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, log_default_params)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, log_lintolog_3chan)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, log_logtolin)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, log_logtolinv2)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, log_ocio_params_channels)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, lut1d_half_domain_raw_half_set)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, lut1d_inv)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, lut1d_inv_scaling)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, lut1d_long_lut)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, lut3d)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, lut3d_inv)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, lut_1d)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, lut_3by1d_with_nan_infinity)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, matrix4x4)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, matrix_1_3_3x3)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, matrix_1_3_4x4)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, matrix_1_3_alpha_offsets)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, matrix_1_3_offsets)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, matrix_no_newlines)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, matrix_windows_eol)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, matrix_with_offset)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, multiple_ops)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, process_list_no_version)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, process_list_valid_version)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, process_list_version_revision)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, range_default)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, range_test1_clamp)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, range_test1_noclamp)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, range_test2)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, reference_load_alias)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, reference_load_multiple)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, reference_load_path)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, reference_load_path_utf8)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, tabluation_support)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, transform_id_empty)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, transform_missing_id)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, unknown_elements)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FileTransform, load_file_ok)` | `tests/cpu/transforms/FileTransform_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_TEST(FixedFunctionOpCPU, aces_rgb_to_jmh_20)` | `tests/cpu/ops/fixedfunction/FixedFunctionOpCPU_tests.cpp` | changed | `ocio26-ops` |
| `OCIO_ADD_TEST(FixedFunctionOpData, is_inverse)` | `tests/cpu/ops/fixedfunction/FixedFunctionOpData_tests.cpp` | changed | `ocio26-ops` |
| `OCIO_ADD_TEST(FixedFunctionTransform, basic)` | `tests/cpu/transforms/FixedFunctionTransform_tests.cpp` | changed | `ocio26-ops` |
| `OCIO_ADD_TEST(GpuShader, MetalSupport9)` | `tests/cpu/GpuShader_tests.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_TEST(Lut3DRenderer, nan_tetra_test)` | `tests/cpu/ops/lut3d/Lut3DOpCPU_tests.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_TEST(MergeConfigs, config_utils_find_equivalent_colorspace)` | `tests/cpu/ConfigUtils_tests.cpp` | changed | `ocio26-config` |
| `OCIO_ADD_TEST(MergeConfigs, view_transforms_section)` | `tests/cpu/apphelpers/MergeConfigsHelpers_tests.cpp` | changed | `ocio26-config` |
| `OCIO_ADD_TEST(Op, non_dynamic_ops)` | `tests/cpu/Op_tests.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_TEST(OpOptimizers, combine_ops)` | `tests/cpu/OpOptimizers_tests.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_TEST(OpOptimizers, dynamic_ops)` | `tests/cpu/OpOptimizers_tests.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_TEST(OpOptimizers, gamma_comp)` | `tests/cpu/OpOptimizers_tests.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_TEST(OpOptimizers, gamma_comp_test2)` | `tests/cpu/OpOptimizers_tests.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_TEST(OpOptimizers, lut1d_identity_replacement_order)` | `tests/cpu/OpOptimizers_tests.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_TEST(OpOptimizers, opt_prefix_test1)` | `tests/cpu/OpOptimizers_tests.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_TEST(OpRcPtrVec, dynamic_property)` | `tests/cpu/Op_tests.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_TEST(Processor, basic_cache)` | `tests/cpu/Processor_tests.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_TEST(Processor, basic_cache_lut)` | `tests/cpu/Processor_tests.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_TEST(Reference, load_nested_resolve_internal)` | `tests/cpu/ops/reference/ReferenceOpData_tests.cpp` | changed | `ocio26-clf` |
| `OCIO_ADD_GPU_TEST(FixedFunction, style_aces2_1000nit_p3_rndtrip)` | `tests/gpu/FixedFunctionOp_test.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_GPU_TEST(FixedFunction, style_aces2_4000nit_p3_rndtrip)` | `tests/gpu/FixedFunctionOp_test.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_GPU_TEST(FixedFunction, style_aces2_4000nit_rec2020_rndtrip)` | `tests/gpu/FixedFunctionOp_test.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_GPU_TEST(FixedFunction, style_aces2_nan_bug)` | `tests/gpu/FixedFunctionOp_test.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_GPU_TEST(FixedFunction, style_aces2_rgb_to_jmh_fwd)` | `tests/gpu/FixedFunctionOp_test.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_GPU_TEST(FixedFunction, style_aces2_rgb_to_jmh_inv)` | `tests/gpu/FixedFunctionOp_test.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_GPU_TEST(FixedFunction, style_aces_gamutcomp13_inv)` | `tests/gpu/FixedFunctionOp_test.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_GPU_TEST(GradingHueCurve, style_log_rev_dynamic)` | `tests/gpu/GradingHueCurveOp_test.cpp` | changed | `ocio26-base` |
| `OCIO_ADD_TEST(CTFTransform, fixed_function_aces_hmj_to_rgb_20_ctf)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | new | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, fixed_function_aces_rgb_to_hmj_20_ctf)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | new | `ocio26-clf` |
| `OCIO_ADD_TEST(CTFTransform, get_equivalent_noclamp_range)` | `tests/cpu/fileformats/ctf/CTFTransform_tests.cpp` | new | `ocio26-clf` |
| `OCIO_ADD_TEST(Config, active_display_alias)` | `tests/cpu/Config_tests.cpp` | new | `ocio26-config` |
| `OCIO_ADD_TEST(Config, aliased_display_name)` | `tests/cpu/Config_tests.cpp` | new | `ocio26-config` |
| `OCIO_ADD_TEST(Config, aliased_view_name)` | `tests/cpu/Config_tests.cpp` | new | `ocio26-config` |
| `OCIO_ADD_TEST(Config, cyclic_color_space_linearity_check)` | `tests/cpu/Config_tests.cpp` | new | `ocio26-base` |
| `OCIO_ADD_TEST(Config, cyclic_color_space_transform)` | `tests/cpu/Config_tests.cpp` | new | `ocio26-base` |
| `OCIO_ADD_TEST(Config, cyclic_color_space_two_step)` | `tests/cpu/Config_tests.cpp` | new | `ocio26-base` |
| `OCIO_ADD_TEST(Config, cyclic_display_view_transform)` | `tests/cpu/Config_tests.cpp` | new | `ocio26-base` |
| `OCIO_ADD_TEST(Config, cyclic_look_transform)` | `tests/cpu/Config_tests.cpp` | new | `ocio26-base` |
| `OCIO_ADD_TEST(Config, display_description)` | `tests/cpu/Config_tests.cpp` | new | `ocio26-config` |
| `OCIO_ADD_TEST(Config, is_colorspace_used_aliases)` | `tests/cpu/Config_tests.cpp` | new | `ocio26-config` |
| `OCIO_ADD_TEST(Config, resolved_display_view_color_space_name)` | `tests/cpu/Config_tests.cpp` | new | `ocio26-config` |
| `OCIO_ADD_TEST(Config, view_aliases)` | `tests/cpu/Config_tests.cpp` | new | `ocio26-config` |
| `OCIO_ADD_TEST(ConfigCompatibility, hdr_display_support_26)` | `tests/cpu/apphelpers/ConfigCompatibility_tests.cpp` | new | `ocio26-config` |
| `OCIO_ADD_TEST(ConfigUtils, find_color_space_for_id)` | `tests/cpu/ConfigUtils_tests.cpp` | new | `ocio26-config` |
| `OCIO_ADD_TEST(ConfigUtils, generate_local_id_for_color_space)` | `tests/cpu/ConfigUtils_tests.cpp` | new | `ocio26-config` |
| `OCIO_ADD_TEST(ConfigUtils, generate_local_id_for_color_space_alias_preference)` | `tests/cpu/ConfigUtils_tests.cpp` | new | `ocio26-config` |
| `OCIO_ADD_TEST(ConfigUtils, sanitize_id_token)` | `tests/cpu/ConfigUtils_tests.cpp` | new | `ocio26-config` |
| `OCIO_ADD_TEST(DisplayViewTransform, context_variables_with_resolved_display_view)` | `tests/cpu/transforms/DisplayViewTransform_tests.cpp` | new | `ocio26-config` |
| `OCIO_ADD_TEST(DisplayViewTransform, display_alias_fallback)` | `tests/cpu/transforms/DisplayViewTransform_tests.cpp` | new | `ocio26-config` |
| `OCIO_ADD_TEST(DisplayViewTransform, use_display_name_alias)` | `tests/cpu/transforms/DisplayViewTransform_tests.cpp` | new | `ocio26-config` |
| `OCIO_ADD_TEST(DisplayViewTransform, view_alias_fallback)` | `tests/cpu/transforms/DisplayViewTransform_tests.cpp` | new | `ocio26-config` |
| `OCIO_ADD_TEST(FileFormatCTF, non_smpte_xmlns)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | new | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, smpte_all_metadata)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | new | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, smpte_broadcast_profile_noclamp_range)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | new | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, smpte_conflicting_version)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | new | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, smpte_higher_ns_version)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | new | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, smpte_id_bad_value)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | new | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, smpte_id_element)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | new | `ocio26-clf` |
| `OCIO_ADD_TEST(FileFormatCTF, smpte_namespaces)` | `tests/cpu/fileformats/FileFormatCTF_tests.cpp` | new | `ocio26-clf` |
| `OCIO_ADD_TEST(LegacyViewingPipeline, display_view_alias_fallback)` | `tests/cpu/apphelpers/LegacyViewingPipeline_tests.cpp` | new | `ocio26-config` |
| `OCIO_ADD_TEST(Lut3DRenderer, avx512_tetrahedral_bounds)` | `tests/cpu/ops/lut3d/Lut3DOpCPU_tests.cpp` | new | `ocio26-base` |
| `OCIO_ADD_TEST(OpRcPtrVec, move_semantics)` | `tests/cpu/Op_tests.cpp` | new | `ocio26-base` |
| `OCIO_ADD_GPU_TEST(FixedFunction, style_aces2_rgb_to_hmj_fwd)` | `tests/gpu/FixedFunctionOp_test.cpp` | new | `ocio26-ops` |
| `OCIO_ADD_GPU_TEST(FixedFunction, style_aces2_rgb_to_hmj_inv)` | `tests/gpu/FixedFunctionOp_test.cpp` | new | `ocio26-ops` |
| `OCIO_ADD_GPU_TEST(GradingHueCurve, style_vid_fwd)` | `tests/gpu/GradingHueCurveOp_test.cpp` | new | `ocio26-base` |
| `OCIO_ADD_GPU_TEST(GradingHueCurve, style_vid_fwd_dynamic)` | `tests/gpu/GradingHueCurveOp_test.cpp` | new | `ocio26-base` |
| `OCIO_ADD_GPU_TEST(GradingHueCurve, style_vid_rev)` | `tests/gpu/GradingHueCurveOp_test.cpp` | new | `ocio26-base` |
| `OCIO_ADD_GPU_TEST(GradingHueCurve, style_vid_rev_dynamic)` | `tests/gpu/GradingHueCurveOp_test.cpp` | new | `ocio26-base` |

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
