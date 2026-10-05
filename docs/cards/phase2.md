# Phase 2 cards: LUTs and fixed functions

**Status: planned, 2026-10-04.** Phase 3 (configs, `docs/cards/phase3.md`) runs in parallel
(owner decision, 2026-10-05). Phase 3's built-in transforms and built-in configs need Phase 2's
ops: "Phase 3 needs from Phase 2" below lists which chunks unblock them, and the card order puts
those first.

**Goal: every op family complete on CPU and GPU (PLAN.md §10, Phase 2).** With Phase 3 this
makes milestone M1. The phase covers:
- **Lut1D:** the float and half-domain renderers, hue adjust, the exact and fast inverses,
  composition, every SIMD profile, and the GPU writer;
- **Lut3D:** the op, the CPU renderers beyond S4's forward kernels, the exact and fast
  inverses, composition, `Lut3DTransform`, and the GPU writer;
- **fixed functions:** 2.5.2's public styles except ACES 2.0, CPU and GPU, with
  `FixedFunctionTransform`, and W0001's bound for PQ on Windows;
- **ACES 2.0:** the four ACES 2.0 styles, their tables, CPU and GPU;
- **GradingRGBCurve:** the op that the ACES 1.x output built-ins use for their tone scale
  (the "B-spline evaluation" of PLAN.md WP 2.4; owner decision P2-1);
- **the optimizer's LUT passes:** `ReplaceInverseLuts`, the LUT pair identities and
  combinations, and the rest of the separable-prefix bake.

"Byte-exact" means what it meant in Phase 1:
- **CPU:** every bit depth, pixel layout, optimization level, and every numeric profile;
- **GPU:** shader text, uniforms and textures in all 10 shading languages.

Each work package below is a list of **chunks**. A chunk is one commit and is mergeable on its
own (`CLAUDE.md` → "Chunks"). Line counts are upstream's non-blank, non-comment lines at v2.5.2.
Paths are relative to `src/OpenColorIO/` unless they start with `tests/`.

## How this phase works

Everything in `docs/cards/phase1.md` → "How this phase works" still applies: cards and PRs,
the chunk gate, the battery, CPU-dependent tests, `tools/wheel-inspect`, owner items. What
changes in Phase 2:

- **Through the API from the start.** Phase 1's transforms and processors exist.
  - Every check runs through `Transform → Config::CreateRaw() → Processor` once the family's
    transform is in: `Lut1DTransform` now, `Lut3DTransform` after 2.2c, `FixedFunctionTransform`
    after 2.3e.
  - Until then, and for the GradingRGBCurve op (its transform is Phase 3's), `ocio-ops` tests
    build the op data directly, and the oracle builds the equivalent transform. The helper that
    maps parameters to op data cites upstream's op data constructors and `BuildXxxOp`.
  - Each family's closing chunk adds its class to the API sweep (`crates/ocio/tests/api_*`,
    `common/api_cases.rs`) and removes the class's deferral there.
- **LUTs in oracle specs.** A transform spec passes a LUT's values as a blob (O2.1), not as one
  `setValue` call per entry. A 65,536-entry half domain or a 129³ cube doesn't fit in JSON.
- **CPU-dependent values beyond pixels.** Composing LUTs and making a fast inverse LUT run the
  CPU renderers (`EvalTransform`, `ops/OpTools.cpp`), which dispatch by CPU: the Lut1D and Lut3D
  AVX2/AVX-512 kernels use FMA. So composed LUT values, the optimizer's combined LUTs, and the
  GPU textures made from them depend on the CPU. Their tests go in the `cpu-tests` alias and
  run under SDE, like the pixels.
- **Platform-dependent shader text.** ACES 2.0's GPU writer, and fixed-function writers that
  compute constants with libm, write those values into the shader as literals. Their text can
  differ between Windows and Linux (D12). Check them live; never commit fixtures for them.
- **SIMD kernels** are exact scalar profiles, as in S4 (`lut3d_op_cpu_avx2.rs`): no
  `core::arch` and no `unsafe` until Phase 8. A chunk that adds a kernel runs
  `scripts/sde.sh <cpu> cargo cpu-tests --release` on that kernel's CPUs, on both platforms.
- **Upstream tests that read files** (`.ctf`, `.clf`, `.spi1d`, `.spi3d`) wait for Phase 4's
  readers. Tests that need `Config::Create()` wait for Phase 3. The GPU suite (`tests/gpu`) is
  Phase 7's. Each chunk below lists only the tests it can port; the rest are counted at the end.
- **Shared files with Phase 3:** `crates/ocio/src/transform.rs` (the `BuildOps` and
  `CreateTransform` arms), `crates/ocio/src/lib.rs` (re-exports), `processor.rs`
  (`createGroupTransform`) and `upstream-map.toml`. Touch only your family's arms and entries.
  `OpData` matches have no wildcard arm, so the Lut3D, FixedFunction and GradingRGBCurve op
  chunks each touch every match: whichever lands second takes a mechanical rebase.

**Code already on `main`.**
- **Lut1D** (`p1-optimizer`, `p1-transforms-fam4`):
  - the forward op data (`Lut3by1DArray`, the constructors, the queries, `MakeLookupDomain`,
    `GetLutIdealSize`, `validate`, `equals`, `inverse`, the cache ID, `hasExtendedRange`,
    `scale`, forward `finalize`) and `ComposeVec`;
  - `Lut1DOp` except `combineWith`, the float `getCPUOp` and `extractGpuShaderInfo`;
  - the lookups from 8-, 10-, 12-, 16-bit and half codes to F32, and the CPU engine's Lut1D ends;
  - the separable-prefix bake (`OptimizeSeparablePrefix`);
  - `Lut1DTransform`, `BuildLut1DOp`, `CreateLut1DTransform`.
  - Tests: `crates/ocio-ops/tests/lut1d_{op_data,op,op_cpu,bake}_oracle.rs`,
    `crates/ocio/tests/lut1d_transform_oracle.rs`; 16 upstream tests.
  - Errors until Phase 2: float input, hue adjust, the inverse, composition, lookups to integer
    or half output, the GPU writer. The GPU processor of a processor with a baked LUT returns
    "not ported yet".
- **Lut3D** (spike S4): the array (identity fill, size limit), `Interpolation` (in
  `lut3d_op_data.rs`, to move to `open_color_types.rs`), and the forward renderers in every
  profile: generic, SSE2, AVX, AVX2 and AVX-512 tetrahedral, SSE2 and generic trilinear. They
  work in place and never write alpha. Oracle: `numerics_lut`'s `lut3d_apply`; tests:
  `crates/ocio-ops/tests/lut3d_oracle.rs` (`cpu-tests`), 2 upstream tests. Not yet an `OpData`
  variant.
- **Helpers:** fast math (`sse.rs`: `ssePower`, `sseAtan2`, `sseSinCos`, ...), the RGBA packs
  and F16C lanes (`sse2.rs`, `avx*.rs`), half conversions (`imath_half.rs`), `math_utils.rs`
  (`lerpf`, `sanitize_float`, `halfs_differ`, the C++ and SSE min/max), CPU dispatch
  (`cpu_info.rs`), `OpTools::EvalTransform`.
- **GPU infrastructure:** 1D, 2D and 3D textures, uniforms (with dynamic-property callbacks),
  texture limits, `setAllowTexture1D`, the class wrappers, all 10 languages
  (`declare_float_array_const`, `declare_int_array_const`, `atan2`, ...).
- **Test kit:** the battery, `image_apply`, `gpu_shader`, `processor_ops` (it dumps a
  LUT's values as blobs), and the API sweep (`crates/ocio/tests/api_battery_oracle.rs`,
  `api_formats_oracle.rs`, `api_gpu_oracle.rs`).

---

## Numerics risks

Each is a pitfall for the chunk that meets it. None needs a new waiver up front. If one can't
be matched, stop and report it (`CLAUDE.md`, rule 4).

- **FMA.** `Lut1DOpCPU_AVX2/AVX512` use `_mm256_fmadd_ps`/`_mm512_fmadd_ps`: use `f32::mul_add`
  there and nowhere else (S4 did the same for Lut3D). `Lut1DOpCPU_SSE2` and `_AVX` emulate a
  multiply-add with a multiply and an add.
- **Lut1D dispatch.** The two `BaseLut1DRenderer` constructors dispatch differently
  (`Lut1DOpCPU.cpp:273-340`): the first takes AVX-512 and ignores `AVXSlow()`, the second skips
  AVX-512 and checks `AVXSlow()`. Port both. If it changes outputs, it is an `I-` entry.
- **Lut1D SIMD only for F32 input with more than one pixel per call**
  (`Lut1DRenderer::apply`, 622-720). Single-pixel rows take the scalar path. Probe both, and
  row lengths that leave every remainder of the 4-, 8- and 16-wide kernels.
- **Rounding.** Scalar paths cast with add-0.5-and-truncate (`Converter<outBD>::CastValue`).
  The SIMD stores to integer output round to nearest even. A LUT that ends a CPU engine with
  integer output goes through one or the other.
- **Hue adjust is SSE2 at compile time.** `Lut1DRendererHueAdjust` has an `#if OCIO_USE_SSE2`
  branch inside the scalar renderer (909-973). Every x86-64 wheel takes it: truncation instead
  of `floor`, and a different high index. Port that branch, not the `#else`.
- **PQ.**
  - The renderer is chosen at compile time: the SSE renderer with fast math on, on both
    platforms.
  - With fast math off, Windows takes the SSE renderer with SVML's `_mm_pow_ps`; the port calls
    `powf` there (W0001). Linux takes the scalar `Renderer_PQ_TO_LIN<float>`, whose constants
    are doubles. So the two platforms run different code paths, not only different `pow`s
    (`cfg(windows)`, D12).
  - SVML may pick its own code path by CPU. S5 measured one CPU, so 2.3d measures W0001's bound
    under SDE on all five CPUs.
- **Float or double.** Fixed functions take `double` parameters and mix `std::pow(float,
  double)`, `std::log` and `powf`. ACES 2.0 calls `cos`, `sin`, `sqrt`, `log10` and `log`
  unqualified on floats (`ACES2/Transform.cpp`). With MSVC those resolve to the float
  overloads; with libstdc++ an unqualified call may be the C `double` function. Check each call
  site in both wheels (`wheel-inspect find`, `disasm`) before porting it.
- **Constant folding.** ACES 2.0 initializes its parameters from constants (`init_JMhParams`:
  `powf(5.f * L_A, 1.f/3.f)`, `constexpr` chains). GCC can fold these at compile time with a
  different implementation than the runtime `powf`; MSVC usually doesn't. Verify every such
  constant against the oracle (its GPU uniforms and literals show them).
- **ACES 2.0 tables** (cusp, reach, hue, gamma fit) are computed per op from libm results. They
  can differ between the platforms (D12) and are checked live. The GPU gets them as textures,
  so `gpu_shader` exposes every table entry: test the tables against the wheel's textures
  before the renderers exist (2.4c, 2.4d).
- **Composition and fast inverses depend on the CPU** (see "How this phase works").
  `MakeFastLut3DFromInverse` composes a 48³ grid through the exact inverse renderer;
  `MakeFastLut1DFromInverse` composes a lookup domain through the inverse 1D renderer.
- **Pass-through channels.** The Lut1D SIMD kernels move alpha through the RGBA packs when the
  bit depths are equal, and scale it when they differ. The PQ SSE renderers restore alpha with
  bit masks. The ACES 1.x renderers copy it. Never copy it in Rust code that computes the other
  channels (`CLAUDE.md` → "Channels that pass through").
- **NaN LUT entries.** Every renderer sanitizes or clamps them, so the outputs should be
  exact. W0002 covers NaN *parameters*, and this plan doesn't extend it to LUT entries (owner
  item P2-6).

---

## WP 2.1: Lut1D (`ocio-ops`, `ocio-gpu`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 2.1a | `ops/lut1d/Lut1DOpCPU.cpp`: the float paths of `BaseLut1DRenderer::update`/`updateData`/`reset`, `Lut1DRenderer`, `Lut1DRendererHalfCode`, `IndexPair::GetEdgeFloatValues`, every output bit depth, `GetLut1DRenderer` for forward LUTs (~390 of 29-155, 273-720, 1626-1756; the lookups are done) | `lut1d_op_cpu.rs` | `Lut1DOpCPU_tests.cpp`: `basic`, `half`, `nan`, `nan_test`, `nan_half_test`, `bit_depth_support`, `lut_1d_red`/`green`/`blue`, `lut_1d_identity_half`, `_half_to_int`, `_int_to_half`, `_half_code`; `Lut1DOp_tests.cpp`: `finite_value`, `extrapolation_errors`; oracle: the Lut1D battery (scalar profile, single-pixel rows) and the API sweep for `Lut1DTransform` with float input |
| 2.1b | `Lut1DOpCPU.cpp`: `GamutMapUtils::Order3`, `Lut1DRendererHueAdjust`, `Lut1DRendererHalfCodeHueAdjust` (228, 155-208 and 723-998) | `lut1d_op_cpu.rs` | `GamutMapUtil/order3_test`, `Lut1DRenderer/finite_value_hue_adjust`; oracle: hue adjust in the battery |
| 2.1c | `Lut1DOpCPU_SSE2.cpp` (155), `Lut1DOpCPU_AVX.cpp` (150), and the constructors' dispatch | `lut1d_op_cpu_sse2.rs`, `lut1d_op_cpu_avx.rs` | none upstream; oracle: the battery with these profiles as `other_profiles`, under SDE on `nhm` and `snb` |
| 2.1d | `Lut1DOpCPU_AVX2.cpp` (130, FMA, gather), `Lut1DOpCPU_AVX512.cpp` (108, FMA) | `lut1d_op_cpu_avx2.rs`, `lut1d_op_cpu_avx512.rs` | as 2.1c, under SDE on `hsw`, `skl` and `skx` |
| 2.1e | `ops/lut1d/Lut1DOpData.cpp`: the inverse set-up (`initializeFromForward`, `ComponentProperties`, inverse `finalize`), `isInverse`, `getPairIdentityReplacement` (205, 282-361, 570-603, 912-1119; and `Lut1DOpData.h`) | `lut1d_op_data.rs` | `Lut1DOpData_tests.cpp`: `inverse_hueadjust`, `is_inverse`, `inverse_increasing_effective_domain`, `inverse_flatten_test`, `inverse_half_domain` |
| 2.1f | `Lut1DOpCPU.cpp`: `FindLutInv`, `FindLutInvHalf`, `InvLut1DRenderer` and its half-code and hue-adjust variants, the inverse arms of `GetLut1DRenderer` (483, 209-272 and 999-1625) | `lut1d_op_cpu.rs` | `Lut1DRenderer/lut_1d_inv_identity`, `_inv_increasing`, `_inv_decreasing_reversals`, `_inv_clamp_to_range`, `_inv_flat_start_or_end`, `_inv_half_input`, `_inv_half_identity`; `Lut1DOp/inverse`, `Lut1D/inverse_twice`, `Lut1DTransform/non_monotonic`; oracle: the inverse in the battery, with `OPTIMIZATION_LUT_INV_FAST` off |
| 2.1g | `Lut1DOpData::Compose`, `MakeFastLut1DFromInverse` (106, 715-868); `Lut1DOp::combineWith` | `lut1d_op_data.rs`, `lut1d_op.rs` | `Lut1DOpData/lut_1d_compose`, `lut_1d_compose_sc`, `compose_inverse_luts`, `compose_only_forward`, `Lut1D/compose_big_domain`; oracle: composed and fast-inverse LUTs entry for entry (`processor_ops`), in `cpu-tests` |
| 2.1h | `ops/lut1d/Lut1DOpGPU.cpp` (207: `CreatePaddedLutChannels`, `CreatePaddedRedChannel`, `GetLut1DGPUShaderProgram`), `Lut1DOp::extractGpuShaderInfo` (20) | `ocio-gpu/src/ops/lut1d/lut1d_op_gpu.rs` | `Lut1DOpGPU_tests.cpp`: `pad_lut_one_dimension`, `pad_lut_two_dimension_1`, `_2`; oracle `gpu_shader`, all languages, 1D and 2D textures, half domain, hue adjust, inverse; the GPU sweep's Lut1D deferral and the baked-LUT "not ported yet" go |

- Order: 2.1a → 2.1b, then 2.1c → 2.1d and 2.1e → 2.1f → 2.1g in parallel, then 2.1h.
- `GenerateLinearScaleLut1D` (`Lut1DOp.cpp:209-228`) has callers only in file formats: Phase 4.
- File-based tests wait for Phase 4: 8 in `Lut1DOpCPU_tests.cpp`, 3 `make_fast_from_inverse_*`
  in `Lut1DOpData_tests.cpp`, and `Lut1D/lut_1d_compose_with_bit_depth`.

## WP 2.2: Lut3D (`ocio-ops`, `ocio`, `ocio-gpu`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 2.2a | `ops/lut3d/Lut3DOpData.cpp/.h` except `Compose` and `MakeFastLut3DFromInverse` (~340, 174-500): S4's data moves onto `ops/op_array.rs` and `open_color_types::Interpolation`; `validate`, `isNoOp`, `isIdentity`, equality, `clone`, `isInverse`, `inverse`, the cache ID, `scale`, `setArrayFromRedFastestOrder`, `getRGB`/`setRGB` | `lut3d_op_data.rs` | `Lut3DOpData_tests.cpp`: `accessors`, `clone`, `empty`, `equality`, `interpolation`, `is_inverse`, `not_supported_length` |
| 2.2b | `ops/lut3d/Lut3DOp.cpp/.h` (239): the op, `CreateLut3DOp`, `GenerateIdentityLut3D`, `Get3DLutEdgeLenFromNumPixels`; S4's renderers become the op's `CpuOp` (`math_utils::sanitize_float`); the `OpData::Lut3D` arms | `lut3d_op.rs`, `op.rs`, `op_data.rs` | `Lut3DOp_tests.cpp`: `cache_id`, `cpu_renderer_lut3d`, `edge_len_from_num_pixels`, `Lut3DOpData/create_op`, `lut_order`, `Lut3DOpStruct/lut3d_order`, `GenerateIdentityLut3D/throw_lut`; `lut3d_oracle.rs` moves onto the battery |
| 2.2c | `transforms/Lut3DTransform.cpp` (185) + `CreateLut3DTransform`, `BuildLut3DOp` | `crates/ocio/src/transforms/lut3d_transform.rs` | `Lut3DTransform_tests.cpp`: `basic`, `create_with_parameters`; `Lut3D/create_transform`; oracle `transform_text`; the CPU API sweep for `Lut3DTransform` |
| 2.2d1 | `ops/lut3d/Lut3DOpCPU.cpp`: `InvLut3DRenderer::RangeTree` (277, 78-196 and 1080-1444) | `lut3d_op_cpu.rs` (or `inv_lut3d.rs`) | none upstream; unit checks of the tree against upstream's own construction |
| 2.2d2 | `Lut3DOpCPU.cpp`: `extrapolate`, `extrapolate3DArray`, `InvLut3DRenderer::updateData` and `apply`, the inverse arm of `GetLut3DRenderer` (271, 1424-1765) | `lut3d_op_cpu.rs` | `Lut3DOp/inverse_comparison_check`; oracle: the exact inverse in the battery (`OPTIMIZATION_LUT_INV_FAST` off) |
| 2.2e | `Lut3DOpData::Compose`, `MakeFastLut3DFromInverse` (79, 25-173); `Lut3DOp::combineWith`; the optimizer's Lut3D arms (WP 2.5b) | `lut3d_op_data.rs`, `lut3d_op.rs`, `op_optimizers.rs` | `Lut3DOpData/compose_inverse_luts`, `lut_combine`; oracle: composed and fast-inverse cubes entry for entry, in `cpu-tests` |
| 2.2f | `ops/lut3d/Lut3DOpGPU.cpp` (177), `Lut3DOp::extractGpuShaderInfo` | `ocio-gpu/src/ops/lut3d/lut3d_op_gpu.rs` | oracle `gpu_shader`, all languages, tetrahedral and trilinear, inverse (its fast LUT); the GPU sweep for `Lut3DTransform` |

- Wait for Phase 4 (files): `Lut3DOp/cpu_renderer_cloned`, `cpu_renderer_inverse`,
  `cpu_renderer_lut3d_with_nan`, `Lut3DOpData/compose`, `compose_2`, `inv_lut3d_lut_size`.
  For Phase 3 (`Config::Create()`): `Lut3DTransform/build_op`.
- The exact inverse is slow (a range tree per cube). The battery's quick tier samples it.

## WP 2.3: fixed functions except ACES 2.0 (`ocio-ops`, `ocio`, `ocio-gpu`)

The 23 public styles less the four ACES 2.0 ones leave 19. Two of those,
`ACES_GAMUTMAP_02` and `_07`, are refused with upstream's message.

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 2.3a1 | `ops/fixedfunction/FixedFunctionOpData.cpp`: the styles, defaults, names, `GetStyle`, both `ConvertStyle`s (537, 1-590); `FixedFunctionStyle` in `open_color_types.rs` | `fixed_function_op_data.rs`, `open_color_types.rs` | (with 2.3a2) |
| 2.3a2 | `FixedFunctionOpData.cpp`: the constructors, `validate` for every style (ACES 2.0's included), `isInverse`, `invert`, `inverse`, the direction, equality, the cache ID (524, 591-1195) | `fixed_function_op_data.rs` | `FixedFunctionOpData_tests.cpp` (7) |
| 2.3b | `ops/fixedfunction/FixedFunctionOpCPU.cpp`: ACES red modifier 0.3 and 1.0, glow 0.3 and 1.0, dark to dim 1.0, gamut compression 1.3, forward and inverse (480, 1-132 and 558-1088); the dispatch for them (`GetFixedFunctionCPURenderer`, 2429-2645, its other arms "not ported yet") | `fixed_function_op_cpu.rs` | `FixedFunctionOpCPU/aces_red_mod_03`, `_10`, `aces_glow_03`, `_10`, `aces_dark_to_dim_10`, `aces_gamut_map_13`; oracle: the fixed-function battery |
| 2.3e | `ops/fixedfunction/FixedFunctionOp.cpp/.h` (157), `transforms/FixedFunctionTransform.cpp` (143) + `CreateFixedFunctionTransform`, `BuildFixedFunctionOp`; the `OpData::FixedFunction` arms | `fixed_function_op.rs`, `crates/ocio/src/transforms/fixed_function_transform.rs` | `FixedFunctionOp/basic`, `glow03_cpu_engine`, `darktodim10_cpu_engine`, `aces_red_mod_inv`, `aces_glow_inv`, `aces_darktodim10_inv`, `aces_gamutmap13_inv`, `create_transform`; `FixedFunctionTransform/basic`, `createEditableCopy`; `OpOptimizers/remove_inverse_ops`; oracle `transform_text`; the CPU API sweep |
| 2.3c | `FixedFunctionOpCPU.cpp`: Rec.2100 surround, RGB↔HSV, the three RGB↔HSY, XYZ↔xyY, XYZ↔u'v'Y, XYZ↔LUV (573, 210-349 and 1419-2027) | `fixed_function_op_cpu.rs` | `FixedFunctionOpCPU/rec2100_surround`, `RGB_TO_HSV`, `RGB_TO_HSY_LIN`/`_LOG`/`_VID`, `XYZ_TO_xyY`/`_uvY`/`_LUV`; `FixedFunctionOps/` the same seven; `FixedFunctionOp/rec2100_surround_inv` |
| 2.3d | `FixedFunctionOpCPU.cpp`: PQ (scalar, SSE with `ssePower`, and the Windows exact path as `powf`), gamma-log, double-log (430, 350-557 and 2028-2428). **Owner chunk (`waiver`):** it writes W0001's bound into `waivers.toml` and its comparison into `ocio-testkit` | `fixed_function_op_cpu.rs`, `ocio-testkit` | `FixedFunctionOpCPU/LIN_TO_PQ`, `LIN_TO_GAMMA_LOG`, `LIN_TO_DOUBLE_LOG`; `FixedFunctionOps/` the same three; oracle: the battery on both platforms, the bound measured under SDE on all five CPUs (Windows) |
| 2.3f | `ops/fixedfunction/FixedFunctionOpGPU.cpp`: the ACES 1.x shaders (254, 1-351); the dispatch (`GetFixedFunctionGPUShaderProgram`, `GetFixedFunctionGPUProcessingText`, 259, 2225-2493, its other arms "not ported yet") | `ocio-gpu/src/ops/fixedfunction/fixed_function_op_gpu.rs` | oracle `gpu_shader`, all languages |
| 2.3g1 | `FixedFunctionOpGPU.cpp`: surround, HSV, HSY, xyY, u'v'Y, LUV (288, 1636-1989) | same | as 2.3f |
| 2.3g2 | `FixedFunctionOpGPU.cpp`: PQ, gamma-log, double-log (141, 1990-2224) | same | as 2.3f; the GPU API sweep for `FixedFunctionTransform` |

- The ACES 2.0 styles' data (names, parameters, validation, inverse) is in 2.3a; their
  renderers and shaders are WP 2.4. Their dispatch arms return "not ported yet" until then.
- 2.3e lands before 2.3c/2.3d so that Phase 3 gets the ACES 1.x built-ins early; 2.3c and
  2.3d add their `FixedFunctionOps` tests and their styles to the API sweep.

## WP 2.4: ACES 2.0 (`ocio-ops`, `ocio-gpu`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 2.4a | `ops/fixedfunction/ACES2/Common.h` (169), `ColorLib.h` (33), `MatrixLib.h` (90), `Transform.h` (32) | `ops/fixedfunction/aces2/{common,color_lib,matrix_lib}.rs` | unit checks against upstream's constants |
| 2.4b | `ACES2/Transform.cpp`: hue intervals, the cone response, RGB↔Aab↔JMh, chroma-compression norm, toe, tone scale, chroma compression (325, 1-469) | `aces2/transform.rs` | (with 2.4e) |
| 2.4c | `Transform.cpp`: `init_JMhParams`, the cusp corners, the reach and hue tables, the display cusp, the gamut boundary (452, 470-1060) | `aces2/transform.rs` | the tables against the wheel's GPU textures (`gpu_shader` of an `ACES_OUTPUT_TRANSFORM_20`), entry for entry, both platforms |
| 2.4d | `Transform.cpp`: gamut compression forward and inverse, the upper-hull gamma fit, the parameter set-ups (279, 1061-1406) | `aces2/transform.rs` | as 2.4c, for the remaining tables and uniforms |
| 2.4e | `FixedFunctionOpCPU.cpp`: the four ACES 2.0 renderers, forward and inverse, and their dispatch arms (333, 133-209 and 1089-1418) | `fixed_function_op_cpu.rs` | `FixedFunctionOpCPU/aces_output_transform_20`, `aces_rgb_to_jmh_20`, `aces_tonescale_compress_20`, `aces_gamut_map_20`, `aces_ot_20_edge_cases`, `aces_ot_20_rec709_100n_rt`, `_p3d65_100n_rt`, `_p3d65_1000n_rt` (the round trips use `GenerateIdentityLut3D`, 2.2b); oracle: the battery per style, over peak luminances and primaries |
| 2.4f | `FixedFunctionOpGPU.cpp`: hue wrap, sin/cos, RGB↔JMh, the reach table texture, toe and tone scale, chroma compression (341, 352-806) | `fixed_function_op_gpu.rs` | oracle `gpu_shader`, all languages, live only |
| 2.4g | `FixedFunctionOpGPU.cpp`: the cusp table texture, focus gain, J intersection, gamut boundary, compression (324, 807-1226) | same | as 2.4f |
| 2.4h | `FixedFunctionOpGPU.cpp`: gamut compression shaders, the output transform, and the four styles' entry points (333, 1227-1635) | same | as 2.4f; the GPU API sweep for the ACES 2.0 styles |

- The code commented out in `Transform.cpp:255-334` (SSE/AVX horizontal sums) isn't compiled:
  port the scalar sum.

## WP 2.5: the optimizer's LUT passes (`ocio-ops`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 2.5a | `OpOptimizers.cpp`: the Lut1D arms of `ReplaceInverseLuts` (369-408) and of `RemoveInverseOps` (`getPairIdentityReplacement`); separable prefixes that hold LUT ops (their float renderers in `EvalTransform`) | `op_optimizers.rs` | `OpOptimizers/lut1d_half_domain_keep_prior_range`, `multi_op_prefix`; oracle: `processor_ops` for inverse LUT pairs and LUT prefixes at every optimization level and integer/half input |
| 2.5b | the Lut3D arms of `ReplaceInverseLuts`, `RemoveInverseOps` and `CombineOps` | `op_optimizers.rs` | (in 2.2e) |

- 2.5a lands in `p2-lut1d-inv` after 2.1g; 2.5b is part of chunk 2.2e.
- Wait for Phase 4 (files): `OpOptimizers/optimization`, `optimization2`, `lut1d_identities`,
  `lut1d_identity_replacement_order`, `invlut_pair_identities`, `mntr_identities`,
  `gamma_comp`, `gamma_comp_test2`, `log_identities`, `range_lut`,
  `prefer_pair_inverse_over_combine`, `opt_prefix_test1`. For Phase 5: `dynamic_ops`,
  `dyn_properties_prefix`.

## WP 2.6: GradingRGBCurve for the ACES 1.x tone scale (`ocio-ops`, `ocio-gpu`)

The ACES 1.x output built-ins apply their RRT and ODT shapers with a GradingRGBCurve op of
fixed B-spline curves (`transforms/builtins/ACES.cpp:174-385`). PLAN.md puts the grading ops in
Phase 5, but M1 needs this one. Owner decision P2-1: the whole op family here, or only its
non-dynamic path. The table assumes the whole family (dynamic property and GPU uniforms
included). `GradingRGBCurveTransform` is Phase 3's (3.1), and can come once this card lands.

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 2.6a | `ops/gradingrgbcurve/GradingBSplineCurve.cpp`: the curve, its control points and slopes, `validate`, `isIdentity` (254, 494-792); `GradingRGBCurve.cpp` (165) | `ops/gradingrgbcurve/grading_b_spline_curve.rs`, `grading_rgb_curve.rs` | `GradingBSplineCurve/basic`, `equals`, `validate`; `GradingRGBCurve/basic`, `curves`, `max_ctrl_pnts` |
| 2.6b | `GradingBSplineCurve.cpp`: RGB spline fitting (`EstimateRGBSlopes`, `FitRGBSpline`, `AdjustRGBSlopes`), knots and coefficients, `KnotsCoefs::evalCurve`/`evalCurveRev` (327, 335-493, 793-863, 1009-1022, 1247-1384) | `grading_b_spline_curve.rs` | `DynamicPropertyImpl/grading_rgb_curve_knots_coefs` (with 2.6c) |
| 2.6c | `DynamicProperty.cpp`: `DynamicPropertyGradingRGBCurveImpl` (~90, 195-300); `GradingRGBCurveOpData.cpp/.h` (247) | `dynamic_property.rs`, `grading_rgb_curve_op_data.rs` | `GradingRGBCurveOpData/accessors`, `validate`; `DynamicPropertyImpl/equal_grading_rgb_curve` |
| 2.6d | `GradingRGBCurveOpCPU.cpp` (321), `GradingRGBCurveOp.cpp` (194); the `OpData::GradingRGBCurve` arms | `grading_rgb_curve_op_cpu.rs`, `grading_rgb_curve_op.rs` | `GradingRGBCurveOpCPU/` (7), `GradingRGBCurveOp/create`; oracle: the battery with the ACES 1.x curves (op built directly; the oracle builds a `GradingRGBCurveTransform`, O2.2) |
| 2.6e | `GradingRGBCurveOpGPU.cpp` (297), `GradingBSplineCurveImpl::AddShaderEvalFwd`/`Rev` (114, 1023-1167) | `ocio-gpu/src/ops/gradingrgbcurve/grading_rgb_curve_op_gpu.rs` | oracle `gpu_shader`, all languages, dynamic and not |

- Phase 5 keeps the hue-curve parts of `GradingBSplineCurve.cpp` (~530 lines: `PrepHueCurveData`,
  `FitHueSpline`, `EstimateHueSlopes`, the hue knots, `AddShaderEvalRevHue`, `evalCurveRevHue`)
  with GradingHueCurve.
- `GradingRGBCurveOp/create_transform` and `build_ops` need the transform: Phase 3.

## Oracle support (owner-reviewed chunks)

| Chunk | What |
|---|---|
| O2.1 | **LUT values as blobs in transform specs** (`spec.py`): a value `{"blob": i, "dtype": "float32"}` becomes a NumPy array made from the call's blob `i`, so `Lut1DTransform.setData` and `Lut3DTransform.setData` take a whole LUT. Every command that builds a transform from a spec (`commands.py`, `debug_log.py`, `gpu.py`, `image.py`, `processor_cache.py`, `processor_ops.py`, `transform_text.py`) passes the blobs after its own. `checks.dump` already writes `getData()` back as a blob. For WP 2.1–2.2 |
| O2.2 | **Value objects in transform specs** (`spec.py`): `{"object": {"class", "args" or "factory", "calls"}}` builds a non-transform PyOpenColorIO object the same way: `GradingControlPoint`, `GradingBSplineCurve`, `GradingRGBCurve` (and Phase 5's `GradingPrimary` and `GradingTone`). `checks.dump` writes such objects back with their getters. For WP 2.6 and Phase 3's `GradingRGBCurveTransform` |

- No new command is needed. `cpu_apply`, `image_apply` (any row width, so single-pixel rows
  and SIMD remainders), `gpu_shader` (textures as blobs, so ACES 2.0's tables) and
  `processor_ops` (the optimizer's composed and fast-inverse LUTs) cover the phase.
- `numerics_lut`'s `lut3d_apply` stays for S4's tests until 2.2b moves them onto the battery.

## Test kit (tooling chunk)

| Chunk | What |
|---|---|
| T2.1 | **LUT families in the battery** (`ocio-testkit`): `Spec::Transform` carries blobs (O2.1); array parameters, whose generated cases put extreme finite, NaN and ±Inf values at chosen entries (first, last, middle, a node a probe lands on) rather than in every entry; probes at LUT nodes, midpoints and their ±N-ulp neighbourhoods, and outside the domain; row lengths 1 to 33 so every SIMD remainder runs |

---

## Phase 3 needs from Phase 2

Phase 3 can port the BuiltinTransform class, the registry's names and descriptions, the YAML
reader and writer, and load and serialize the built-in configs without Phase 2. Building a
*processor* from a built-in needs its ops:

| Built-ins | Ops they create | Phase 2 chunks |
|---|---|---|
| ARRI, RED, Sony and Panasonic cameras; `ACEScct`, `ACEScg`, the AP0/AP1 utilities; the gamma displays (REC.1886, G2.2, sRGB, G2.6, DCDM, DisplayP3) | Log, Matrix, Range, Gamma | none: Phase 1 |
| `ACEScc_to_ACES2065-1`, Canon CLog2 and CLog3 (`CreateLut`, 4,096 entries) | Lut1D, float input, standard domain: on x86-64 that is the SIMD kernel whenever a call has more than one pixel | 2.1a, 2.1c, 2.1d |
| ADX10/16, Apple Log, the ST-2084 and HLG curves, the PQ displays (REC.2100-PQ, ST2084-P3-D65, ST2084-DCDM-D65, DisplayP3-HDR), the D60/D65 white-roll sims (`CreateHalfLut`) | Lut1D, half domain | 2.1a |
| `ACES-LMT - ACES 1.3 Reference Gamut Compression` | FixedFunction `ACES_GAMUT_COMP_13` | 2.3a1, 2.3a2, 2.3b, 2.3e |
| `DISPLAY - CIE-XYZ-D65_to_REC.2100-HLG-1000nit` | FixedFunction `REC2100_SURROUND` | 2.3c (after 2.3e) |
| ACES 1.x outputs (`ACES-OUTPUT - ..._1.0`/`_1.1`, SDR and HDR) | FixedFunction red modifier, glow and dark to dim 1.0; GradingRGBCurve; half-domain Lut1D | 2.3b, 2.3e, WP 2.6 (2.6a–2.6d), 2.1a |
| ACES 2.0 outputs (the v4.0.0 configs' views) | FixedFunction `ACES_OUTPUT_TRANSFORM_20` | WP 2.4 (2.4a–2.4e) |
| Any of the above in the inverse direction | inverse Lut1D (exact; the fast LUT at the default optimization level), inverse fixed functions | 2.1e, 2.1f, 2.1g, 2.5a |
| GPU processors of the above | the GPU writers | 2.1h, 2.3f–2.3g2, 2.4f–2.4h, 2.6e |

- `OpHelpers.cpp` (`CreateLut`, `CreateHalfLut`, `Interpolate1D`) is Phase 3's; it needs only
  the Lut1D constructors already on `main`.
- **Priority for Phase 3:** 2.1a → 2.3a1, 2.3a2, 2.3b, 2.3e → 2.1c, 2.1d → WP 2.6 →
  2.3c → 2.4a–2.4e → the inverses → the GPU writers. The cards and their order below follow it.

---

## Order and parallelism

```
O2.1 → T2.1 ─┬─ 2.1a → 2.1b ─┬─ 2.1c → 2.1d ───────────────────┐
             │               └─ 2.1e → 2.1f → 2.1g → 2.5a ───────┴─ 2.1h
             └─ 2.2a → 2.2b → 2.2c → 2.2d1 → 2.2d2 → 2.2e → 2.2f
2.3a1 → 2.3a2 → 2.3b → 2.3e ─┬─ 2.3c → 2.3d (W0001) ─┬─ 2.3f → 2.3g1 → 2.3g2 ─┐
                             └─ 2.4a → 2.4b → 2.4c → 2.4d → 2.4e ──────────────┴─ 2.4f → 2.4g → 2.4h
O2.2 → 2.6a → 2.6b → 2.6c → 2.6d → 2.6e        (after owner decision P2-1)
all of the above ── p2-api-parity ── Phase 2 exit
```

- **Implementer A (LUTs):** `p2-lut1d-fwd` → `p2-lut1d-inv` → `p2-lut3d` → `p2-lut3d-inv`.
- **Implementer B (fixed functions):** `p2-ff-cpu` → `p2-ff-cpu-2` → `p2-aces2-cpu`.
- **Implementer C (tooling, SIMD, GPU):** `p2-oracle` → `p2-battery` → `p2-lut1d-simd` →
  `p2-rgbcurve` → `p2-ff-gpu` → `p2-lut1d-gpu` → `p2-aces2-gpu`.
- **Verifier:** reviews every card before it lands, with mutation testing in scratch clones, as
  in Phase 1.
- With two implementers, C's cards go to whoever frees up first, `p2-oracle` and `p2-battery`
  before anything else.

## Cards

Each card is one branch and one PR. Cards in different rows can run in parallel once their
dependencies have landed. "Phase 3" marks the cards that unblock the CPU processors of Phase 3's built-ins (the GPU writers follow).

| Card | Chunks | Who | Needs | Owner items | Phase 3 |
|---|---|---|---|---|---|
| `p2-oracle` | O2.1, O2.2, each its own chunk | C | — | `oracle` (both) | — |
| `p2-battery` | T2.1 | C | O2.1 | — | — |
| `p2-lut1d-fwd` | 2.1a, 2.1b | A | `p2-battery` (A ports meanwhile) | — | yes |
| `p2-ff-cpu` | 2.3a1, 2.3a2, 2.3b, 2.3e | B | — | `api`: `FixedFunctionTransform`, `FixedFunctionStyle` (P2-3) | yes |
| `p2-lut1d-simd` | 2.1c, 2.1d | C | `p2-lut1d-fwd` | — | yes |
| `p2-rgbcurve` | 2.6a–2.6e | C | O2.2; decision P2-1 | `api`: the grading value types (P2-4) | yes |
| `p2-ff-cpu-2` | 2.3c, 2.3d | B | `p2-ff-cpu` | `waiver`: W0001's bound (2.3d, P2-5) | yes |
| `p2-aces2-cpu` | 2.4a–2.4e | B | `p2-ff-cpu`; 2.2b for 2.4e's round-trip tests | — | yes |
| `p2-lut1d-inv` | 2.1e, 2.1f, 2.1g, 2.5a | A | `p2-lut1d-fwd` | — | yes |
| `p2-lut3d` | 2.2a, 2.2b, 2.2c | A | `p2-battery` | `api`: `Lut3DTransform` (P2-2) | — |
| `p2-lut3d-inv` | 2.2d1, 2.2d2, 2.2e, 2.2f | A | `p2-lut3d` | — | — |
| `p2-ff-gpu` | 2.3f, 2.3g1, 2.3g2 | C | `p2-ff-cpu-2` | — | — |
| `p2-lut1d-gpu` | 2.1h | C | `p2-lut1d-inv`, `p2-lut1d-simd` | — | — |
| `p2-aces2-gpu` | 2.4f, 2.4g, 2.4h | C or B | `p2-aces2-cpu`, `p2-ff-gpu` | — | — |
| `p2-api-parity` | 2 chunks: every Phase 2 transform through the port's API against the wheel, as `p1-api-parity-2` did: CPU at every bit depth in and out, packed and planar layouts, every optimization level; GPU in all 10 languages at every level; then the upstream tests the landed cards unblocked | A or B | every card above | — | — |

15 cards, 43 chunks. Small cards land sooner and are easier to verify. When a card grows past
about 6 chunks, split it at a dependency boundary.

**Upstream tests.** The chunks above port 134 C++ tests: Lut1D 40, Lut3D 20, fixed
functions 53, GradingRGBCurve 18, the optimizer 3. Left for later:
- Phase 4 (files): 12 Lut1D, 6 Lut3D and 12 optimizer tests, and the `CTFTransform`,
  `FileFormatCTF` and `FileFormatD1DL` LUT tests;
- Phase 3 (`Config::Create()`, transforms): `Lut3DTransform/build_op`,
  `GradingRGBCurveOp/create_transform`, `build_ops`, the `GradingRGBCurveTransform` tests;
- Phase 5: the optimizer's dynamic tests, the grading hue curves;
- Phase 7 (the GPU suite): `tests/gpu/Lut1DOp_test.cpp` (29), `Lut3DOp_test.cpp` (15),
  `FixedFunctionOp_test.cpp` (59), `GradingRGBCurveOp_test.cpp` (14).

## Owner items

**Decided by the owner on 2026-10-05:** P2-1, the whole GradingRGBCurve family in Phase 2 (WP 2.6); P2-2, P2-3, P2-4 and P2-6 as proposed. Still open: P2-5 (W0001's bound, measured in chunk 2.3d), P2-7, and P2-8 (each `oracle` chunk is reviewed when it is written).

| # | Item | Label | Proposal |
|---|---|---|---|
| P2-1 | Pull the GradingRGBCurve op family into Phase 2 (M1's ACES 1.x built-ins need it) | decision | The whole family, dynamic property and GPU uniforms included, since families land whole. Option B: only the non-dynamic path now, the rest in Phase 5. Either way the hue-curve parts stay in Phase 5 |
| P2-2 | `Lut3DTransform`'s Rust API | `api` | Mirror the landed `Lut1DTransform`: `new`, `with_grid_size`, `grid_size`/`set_grid_size`, `value(i, j, k)`/`set_value`, `interpolation`, `file_output_bit_depth`, `format_metadata(_mut)`, `direction`, `validate`, `equals`; `Interpolation` moves to `open_color_types` (re-exported, no change for callers) |
| P2-3 | `FixedFunctionTransform`'s Rust API | `api` | `FixedFunctionStyle` (the 23 C++ values); `new(style, params: &[f64])`, `style`/`set_style`, `params() -> Vec<f64>`/`set_params(&[f64])`, `direction`, `validate`, `equals`, `format_metadata` |
| P2-4 | The grading value types the op needs: `GradingControlPoint`, `GradingBSplineCurve`, `GradingRGBCurve`, `GradingStyle`, `RGBCurveType`, `BSplineType` | `api` | Public in `ocio` with upstream's getters and setters, as Phase 5 would make them; `GradingBSplineCurve` without the hue-curve types until Phase 5 |
| P2-5 | W0001's bound (chunk 2.3d) | `waiver` | Per function (`LIN_TO_PQ`, `PQ_TO_LIN`): NaN positions exact; finite values within the largest ulp distance measured on Windows under SDE on all five CPUs. S5 measured up to 311 ulp (`LIN_TO_PQ`) and 568,267 ulp (`PQ_TO_LIN`, only at black) on one CPU. The chunk proposes the numbers with their measurements, and the owner approves them |
| P2-6 | W0002 and NaN LUT entries | decision | No extension. The renderers sanitize or clamp NaN entries, so the outputs should compare exactly. If one doesn't, the chunk stops and reports it |
| P2-7 | PLAN.md's M3 row still lists "the exact Lut3D inverse", which v0.7 moved to Phase 2 | plan | Drop it from M3 in the next PLAN.md revision |
| P2-8 | `oracle` chunks O2.1 and O2.2 | `oracle` | As above |

**Phase 2 exit:** every op family is complete on CPU and GPU, byte-exact with the wheel through
OCIO's API (the GradingRGBCurve op through its op data until Phase 3 ports its transform):
- on Windows and Rocky Linux 9 (W0001 within its bound);
- on the CPU at every bit depth, layout and optimization level, for every profile (SDE for the
  CPUs this machine lacks);
- on the GPU in all 10 languages.

Their upstream tests that need no files, configs or GPU execution are ported and counted in
`docs/parity.md`. Together with Phase 3's exit, this is M1.
