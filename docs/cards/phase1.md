# Phase 1 cards: op engine, analytic transforms and GPU infrastructure

**Status: complete (M0), 2026-10-04.** Landed as the cards below. Deferrals to later phases are listed
in "Cards" and in `upstream-map.toml`.

**Goal: milestone M0.** The analytic section of OpenColorIO 2.5.2 is complete and
byte-exact with the wheel, through OCIO's own API (PLAN.md §1, §10). The section covers:
- the Matrix, Range, Exponent, ExponentWithLinear, Log, LogAffine, LogCamera, CDL and Group transforms;
- their ops;
- the op engine and optimizer;
- `Config::CreateRaw()` processors;
- CPU and GPU processors.

"Byte-exact" means both of these:
- **CPU:** every bit depth, pixel layout, optimization level, and every numeric profile the machine can run;
- **GPU:** shader text, uniforms and textures in all 10 shading languages.

An op family is complete when all of it is in: op data, CPU renderers, GPU writer, transform and
glue. Its parts land as separate cards as their dependencies allow (see "Cards" at the end).

Each work package below is a list of **chunks**. A chunk is one commit and is mergeable on its
own (`CLAUDE.md` → "Chunks"). Line counts are upstream's non-blank, non-comment lines at v2.5.2.
Paths are relative to `src/OpenColorIO/` unless they start with `tests/`.

## How this phase works

The rules are in `CLAUDE.md`; this is where each one applies in Phase 1.

- **Cards and PRs.**
  - Each card in "Cards" at the end is one branch, `card/<id>`, and lands as one PR (PLAN.md §7).
  - Agents commit chunks and never push. The orchestrator lands a card with `cargo xtask land`, which replays and gates each chunk, regenerates the generated files and runs the full gate. The orchestrator then pushes the result and opens the PR.
- **The chunk gate.** `cargo xtask gate` before every commit, or `--staged` to check exactly what is staged. Numeric and platform-sensitive chunks (every renderer, every text writer) use `gate --release --rocky`. Never commit `docs/parity.md` or `docs/ratchet.toml`.
- **Oracle tests go through the battery.** Every op family's renderers are tested with `ocio_testkit::battery` (`CLAUDE.md` → "Oracle tests").
  - Families already exist for Log, LogAffine, LogCamera, Exponent and ExponentWithLinear (`crates/ocio-ops/tests/log_oracle.rs`, `gamma_oracle.rs`). Extend them rather than replace them.
  - Declare pass-through channels and the renderers of every numeric profile (SSE2/AVX/AVX2/AVX-512 where the family has them).
  - Ported upstream tests stay separate: `*_tests.rs` files with their markers.
- **CPU-dependent tests.**
  - Put every test target for SIMD kernels, `CPUInfo` or libm-dependent math in the `cpu-tests` alias (`.cargo/config.toml`).
  - A chunk that adds or changes a kernel also runs `scripts/sde.sh <cpu> cargo cpu-tests --release` on the emulated CPUs whose kernel it touches: `nhm` SSE2, `snb` AVX, `hsw`/`skl` AVX2, `skx` AVX-512.
  - CI runs all five nightly and on kernel PRs.
- **Machine-code questions** go to `tools/wheel-inspect` (`docs/wheel-inspect.md`): operand order, float or double, libm, SVML or FMA. Don't write new disassembly scripts.
- **Owner items** stop at the owner and carry a label: oracle changes (their own chunk, `oracle`), waivers, deviations, public API shape (`api`) and new dependencies (`dependency`).

**Testing through the API.**
- **Until 1.8 lands:** `ocio-ops` tests build ops directly from op data, and the oracle builds the equivalent transform (`cpu_apply` / `gpu_shader` with a transform spec). The test helper that maps transform parameters to op data cites upstream's op data constructors and `BuildXxxOp`.
- **After 1.8:** every check also runs through the real API, `Transform → Config::CreateRaw() → Processor`, with the same spec on both sides.

**Code already on `main`.** Phase 0 landed S2/S5 (fast math, the Log and Gamma renderers), S4
(CPUInfo, the Lut3D forward renderers, half conversions) and WP 0.5 (cfmt, NumberUtils,
StringUtils, the yaml-cpp emitter). The cards below refactor that code into the architecture of
`docs/architecture.md` rather than redo it. The Lut3D renderers are kept for Phase 2. They already
work in place and never write alpha (`CLAUDE.md` → "Channels that pass through").

---

## WP 1.1: bit depths, image descriptions, packing (`ocio-ops`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 1.1a | `BitDepthUtils.h/.cpp` (232) | `bit_depth_utils.rs` | `tests/cpu/BitDepthUtils_tests.cpp` |
| 1.1b | `ImageDesc.cpp`: `PackedImageDesc` + `GenericImageDesc` (~350) | `image_desc.rs` | none dedicated upstream: the packed-image parts of `CPUProcessor_tests.cpp`, plus oracle probes (O1.2) |
| 1.1c | `ImageDesc.cpp`: `PlanarImageDesc` (~270) | `image_desc.rs` | as 1.1b, for planar images |
| 1.1d | `ImagePacking.h/.cpp` (271) | `image_packing.rs` | packing tests in `CPUProcessor_tests.cpp` |
| 1.1e | `ScanlineHelper.h/.cpp` (201) | `scanline_helper.rs` | (exercised in 1.2d) |

- `ImageDesc` is public API. It lives in `ocio-ops` because the engine needs it, and `ocio` re-exports it.
- Error texts for bad strides, sizes and channel orders are part of the output: copy them verbatim.

## WP 1.2: op model and CPU engine (`ocio-ops`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 1.2a | `fileformats/FormatMetadata.h/.cpp` (410) | `format_metadata.rs` | `tests/cpu/fileformats/FormatMetadata_tests.cpp` |
| 1.2b | `DynamicProperty.h/.cpp`: the double property and its base (~200) | `dynamic_property.rs` | the double parts of `tests/cpu/DynamicProperty_tests.cpp` |
| 1.2c | `Op.h/.cpp` (748): `OpData` base, `Op`, `OpRcPtrVec` → `OpVec`, `SerializeOpVec`, `CreateOpVecFromOpData`, `HasFlag` | `op.rs`, `op_data.rs` | `tests/cpu/Op_tests.cpp`, the tests that don't need unported families |
| 1.2d | `CPUProcessor.h/.cpp` (477): `CreateCPUEngine`, generic bit-depth helpers, `apply` | `cpu_processor.rs` | `tests/cpu/CPUProcessor_tests.cpp`, the tests whose ops exist |
| 1.2e | `Logging.h/.cpp` (from WP 0.6): levels, `OCIO_LOGGING_LEVEL` read once, the `[OpenColorIO Warning]: ` prefixes, line splitting (`StringUtils`), the callback called outside the lock (a no-output-change deviation of upstream's lock-held callback; `docs/architecture.md`) | `logging.rs` | `tests/cpu/Logging_tests.cpp`, and log capture through the oracle (`captured_log`) |

- `OpData` variants are added family by family (WP 1.3). A match has no wildcard arm, so adding a family touches every match. That is intended.
- 1.2d needs at least one op to test with: port the NoOps (1.3n1) and Matrix (1.3m1–m2) first, or together.

## WP 1.3: analytic op families

One family at a time, each landing whole. Each family's chunks are, in order:
1. op data: validate, identity, no-op, cache ID, equality, inverse;
2. CPU renderers;
3. the op: combine, inverse, identity replacement, `getInfo`;
4. the GPU writer (needs WP 1.7).

Every chunk comes with its ported upstream tests and exact oracle checks. The oracle checks are the
family's `battery::Family`: cases from upstream's tests and the card, plus generated extreme,
NaN and ±Inf parameters. Every numeric profile the family has is covered through
`other_profiles`, and the SIMD ones also under SDE.

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 1.3n1 | `ops/noop/NoOps.*` (352, including its GPU no-op), `ops/reference/ReferenceOpData.*` (121), `ops/allocation/AllocationOp.*` (117) | `ocio-ops/src/ops/{noop,reference,allocation}/` | `NoOps_tests.cpp`, `ReferenceOpData_tests.cpp`, `AllocationOp_tests.cpp` (the parts whose ops exist) |
| 1.3m1 | `ops/matrix/MatrixOpData.*` (783) + the matrix math in `MathUtils` | `ocio-ops/src/ops/matrix/matrix_op_data.rs` | `MatrixOpData_tests.cpp` |
| 1.3m2 | `ops/matrix/MatrixOpCPU.*` (337): scalar + SSE2 renderers (`(r+g)+(b+a)` summation) | `.../matrix/matrix_op_cpu.rs` | `MatrixOpCPU_tests.cpp`; oracle: every bit depth, both profiles |
| 1.3m3 | `ops/matrix/MatrixOp.*` (355) | `.../matrix/matrix_op.rs` | `MatrixOp_tests.cpp` |
| 1.3m4 | `ops/matrix/MatrixOpGPU.*` (62) | `ocio-gpu/src/ops/matrix/matrix_op_gpu.rs` | oracle `gpu_shader`, all 10 languages |
| 1.3r1 | `ops/range/RangeOpData.*` (571) | `.../range/range_op_data.rs` | `RangeOpData_tests.cpp` |
| 1.3r2 | `ops/range/RangeOpCPU.*` + `RangeOp.*` (389) | `.../range/range_op_cpu.rs`, `range_op.rs` | `RangeOpCPU_tests.cpp`, `RangeOp_tests.cpp` |
| 1.3r3 | `ops/range/RangeOpGPU.*` (74) | `ocio-gpu/src/ops/range/range_op_gpu.rs` | oracle `gpu_shader`, all languages |
| 1.3e1 | `ops/exponent/ExponentOp.*` (311, CPU and GPU in one file) | `.../exponent/` + `ocio-gpu/src/ops/exponent/` | `ExponentOp_tests.cpp`; oracle, CPU and all languages |
| 1.3g1 | `ops/gamma/GammaOpData.*` (812) + `GammaOpUtils.*` (114) | `.../gamma/gamma_op_data.rs`, `gamma_op_utils.rs` | `GammaOpData_tests.cpp`, `GammaOpUtils_tests.cpp` |
| 1.3g2 | `ops/gamma/GammaOpCPU.*` (665), from the S2 spike | `.../gamma/gamma_op_cpu.rs` | `GammaOpCPU_tests.cpp`; oracle, fast math on and off |
| 1.3g3 | `ops/gamma/GammaOp.*` (182) | `.../gamma/gamma_op.rs` | `GammaOp_tests.cpp` |
| 1.3g4 | `ops/gamma/GammaOpGPU.*` (300) | `ocio-gpu/src/ops/gamma/gamma_op_gpu.rs` | oracle `gpu_shader`, all languages |
| 1.3l1 | `ops/log/LogUtils.*` (330) + `LogOpData.*` (509) | `.../log/log_utils.rs`, `log_op_data.rs` | `LogUtils_tests.cpp`, `LogOpData_tests.cpp` |
| 1.3l2 | `ops/log/LogOpCPU.*` (760), from the S2 spike | `.../log/log_op_cpu.rs` | `LogOpCPU_tests.cpp`; oracle, fast math on and off |
| 1.3l3 | `ops/log/LogOp.*` (199) | `.../log/log_op.rs` | `LogOp_tests.cpp` |
| 1.3l4 | `ops/log/LogOpGPU.*` (279) | `ocio-gpu/src/ops/log/log_op_gpu.rs` | oracle `gpu_shader`, all languages |
| 1.3c1 | `ops/cdl/CDLOpData.*` (532) | `.../cdl/cdl_op_data.rs` | `CDLOpData_tests.cpp` |
| 1.3c2 | `ops/cdl/CDLOpCPU.*` (399): scalar + SSE2 renderers, luma order | `.../cdl/cdl_op_cpu.rs` | oracle, every style, fast math on and off |
| 1.3c3 | `ops/cdl/CDLOp.*` (209) | `.../cdl/cdl_op.rs` | `CDLOp_tests.cpp` |
| 1.3c4 | `ops/cdl/CDLOpGPU.*` (90) | `ocio-gpu/src/ops/cdl/cdl_op_gpu.rs` | oracle `gpu_shader`, all languages |

## WP 1.4: C++ numeric helpers (`ocio-ops`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 1.4a | `MathUtils.h/.cpp`: scalar helpers (clamp, NaN/Inf tests, `EqualWithAbsError`, `ConvertHalfBitsToFloat`, ...) | `math_utils.rs` (extends the S2 helpers) | `tests/cpu/MathUtils_tests.cpp` (scalar parts) |
| 1.4b | `MathUtils.h/.cpp`: matrix and vector math (`GetM44Inverse`, `GetM44Product`, ...) in f64 | `math_utils.rs` | `MathUtils_tests.cpp` (matrix parts) |
| 1.4c | `SSE.h` (fast math), from the S2 spike | `sse.rs` | `tests/cpu/SSE_tests.cpp` |

## WP 1.5: numeric profiles and dispatch (`ocio-ops`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 1.5a | `CPUInfo.*`, from the S4 spike | `cpu_info.rs` | the S4 dispatch tests; the test-only flag override (`docs/architecture.md`) |
| 1.5b | `SSE2.h` helpers used by the ported renderers (pack/unpack, software half) | `sse2.rs` | `tests/cpu/SSE2_tests.cpp` |
| 1.5c | `AVX.h`, `AVX2.h`, `AVX512.h` helpers used by the ported renderers | `avx.rs`, `avx2.rs`, `avx512.rs` | `AVX_tests.cpp`, `AVX2_tests.cpp`, `AVX512_tests.cpp` |

- `cpu_info_oracle` already runs on all five emulated CPUs in CI. It compares the port's `CPUInfo` with the wheel's `ociocpuinfo`, including the slow-gather rules for Haswell and AMD Zen 3 and older. Keep every dispatch-dependent test in `cargo cpu-tests`.

## WP 1.6: optimizer (`ocio-ops`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 1.6a | `OpOptimizers.cpp`: `RemoveNoOpTypes`, `RemoveNoOps`, `ReplaceOps`, `ReplaceIdentityOps`, `RemoveInverseOps`, `CombineOps`, the 80-pass loop | `op_optimizers.rs` | `tests/cpu/OpOptimizers_tests.cpp` (the tests whose ops exist) |

`ReplaceInverseLuts` and the separable-prefix bit-depth bake need Lut1D. The owner pulled the bake into Phase 1 (2026-10-01): `p1-optimizer` ported the forward Lut1D's data, its lookup renderers and the bake (part of WP 2.5), because `OPTIMIZATION_DEFAULT` bakes integer and half-float inputs, so M0's "every bit depth" needs it. `ReplaceInverseLuts`, the inverse LUT and the float renderers stay in Phase 2 (WP 2.1, 2.5).

## WP 1.7: GPU infrastructure (`ocio-gpu`)

All 10 languages from the start: Cg, GLSL 1.2 / 1.3 / 4.0, GLSL for Vulkan 4.6, HLSL SM 5.0, OSL 1, GLSL ES 1.0 / 3.0 and MSL 2.0.

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 1.7a | `GpuShaderUtils.h/.cpp` (1,417), part 1: `GpuShaderText` declarations, types, operators and float literals (`getFloatString`, through `cfmt`) | `gpu_shader_utils.rs` | `tests/cpu/GpuShaderUtils_tests.cpp` (its parts) |
| 1.7b | `GpuShaderUtils.cpp`, part 2: texture sampling and the remaining helpers, per language | `gpu_shader_utils.rs` | the rest of `GpuShaderUtils_tests.cpp` |
| 1.7c | `GpuShader.h/.cpp` (621) + `GpuShaderDesc.cpp` (343): the shader creator and description, uniforms (with dynamic-property callbacks), 1D/2D/3D textures, resource naming and prefix, descriptor sets, `setAllowTexture1D`, texture limits | `gpu_shader.rs`, `gpu_shader_desc.rs` | `tests/cpu/GpuShader_tests.cpp` |
| 1.7d | `GpuShaderClassWrapper.h/.cpp` (442): the OSL and MSL class wrappers | `gpu_shader_class_wrapper.rs` | via the oracle for OSL and MSL |
| 1.7e | `GPUProcessor.h/.cpp` (163): header, per-op extraction (`extract_gpu_shader_info` dispatch), footer, the resource key (`CacheIDHash`, `k_` prefix) | `gpu_processor.rs` | none dedicated upstream: oracle `gpu_shader`, all languages |

## WP 1.8: transforms and processors (`ocio`)

| Chunk | Upstream | Rust (`ocio/src/...`) | Port tests |
|---|---|---|---|
| 1.8a | `Transform.cpp` (361): the base, direction, `BuildOps` / `CreateTransform` dispatch (the families that exist); `transforms/GroupTransform.cpp` (183) | `transform.rs`, `transforms/group_transform.rs` | `tests/cpu/transforms/GroupTransform_tests.cpp` (the parts whose transforms exist) |
| 1.8b | `transforms/MatrixTransform.cpp` (309) + the Matrix glue (`BuildMatrixOp`, `CreateMatrixTransform`) | `transforms/matrix_transform.rs` | `MatrixTransform_tests.cpp` |
| 1.8c | `transforms/RangeTransform.cpp` (168) + glue | `transforms/range_transform.rs` | `RangeTransform_tests.cpp` |
| 1.8d | `transforms/ExponentTransform.cpp` (94), `ExponentWithLinearTransform.cpp` (137) + glue | `transforms/exponent_*.rs` | their `_tests.cpp` |
| 1.8e | `transforms/LogTransform.cpp` (77), `LogAffineTransform.cpp` (119), `LogCameraTransform.cpp` (152) + glue | `transforms/log_*.rs` | their `_tests.cpp` |
| 1.8f | `transforms/CDLTransform.cpp` (321) + glue (reading CDL files is Phase 4) | `transforms/cdl_transform.rs` | `CDLTransform_tests.cpp` (the parts without files) |
| 1.8g | `Config::CreateRaw()` and `Config::getProcessor(transform, direction)`, with the minimal `Config` and `Context` state they need | `config.rs`, `context.rs` | `Config_tests.cpp` raw-config tests |
| 1.8h | `Processor.h/.cpp` (552): `setTransform` (validate → build → finalize → metadata), cache ID, `getOptimizedProcessor`, the CPU and GPU processor getters, `createGroupTransform`, the dynamic-property accessors; the public `CPUProcessor` and `GPUProcessor` | `processor.rs`, `cpu_processor.rs`, `gpu_processor.rs` | `tests/cpu/Processor_tests.cpp` (its parts) |

- Each transform chunk ports the class whole: getters, setters, validation messages, equality, `operator<<` text (the basis of Python `repr`) and `FormatMetadata`.
- `CreateRaw()` parses an internal YAML profile upstream. Until the YAML reader lands (Phase 3.3), 1.8g builds the same config state directly, and says so in a code comment. Phase 3 switches it to parsing the profile.

## Oracle support (owner-reviewed chunks)

| Chunk | What |
|---|---|
| O1.1 | `processor_ops`: the optimized processor's `createGroupTransform()` as JSON (each child's class and getters) plus its cache ID, for any flags and bit depths. It shows what the wheel's optimizer produced |
| O1.2 | `image_apply` and `image_apply_rgb` (`oracle/ocio_oracle/image.py`; `cpu_apply` is unchanged): a CPU processor applied to images described every way PyOpenColorIO allows (`PackedImageDesc` with every channel order, `PlanarImageDesc`, any strides and bit depths), in place or not, with every buffer returned in full, padding included; and Python's `applyRGB` and `applyRGBA`. For 1.1b–1.1e and 1.2d |
| O1.3 | `gpu_shader`: the shader text, uniforms (names, types, values) and textures (names, sizes, channels, interpolation, values as a blob) from `extractGpuShaderInfo`. It takes any `GpuShaderDesc` settings: language, function name, resource prefix, pixel name, descriptor sets, 1D textures, texture limits. It works for the default and optimized GPU processors |
| O1.4 | `transform_text`: `str(transform)` (upstream's `operator<<`), validation errors, and equality results |
| O1.5 | `gpu_shader_desc` (`oracle/ocio_oracle/gpu_desc.py`, owner-approved 2026-10-01): a `GpuShaderDesc` from `CreateShaderDesc()` driven through the calls Python has, in order (setters, code sections, `createShaderText`, `finalize`, the cache ID, resource indices, textures, dynamic properties, `clone`), optionally at the debug log level: each call's return or exception, then everything the description holds, as `gpu_shader` reports it. It refuses a finalize in MSL that would make the Metal class wrapper read past a line, and a 1D texture whose float count would wrap. For 1.7c |

## Order and parallelism

```
spike chunks ─┬─ 1.1a → 1.1b → 1.1c → 1.1d → 1.1e ─┐
              ├─ 1.2a → 1.2b → 1.2c ───────────────┴─ 1.2d ─┬─ 1.3 families: data, CPU, op (in parallel) ─ 1.6a
              ├─ 1.4a → 1.4b → (1.3m1)                       │
              ├─ 1.5a → 1.5b → 1.5c ─────────────────────────┘
              └─ O1.3 → 1.7a → 1.7b → 1.7c → 1.7d → 1.7e ─── 1.3 GPU chunks (x4) ─┐
                                                          1.8a → 1.8b … 1.8f → 1.8g → 1.8h ── M0
```

- **Implementer A:** 1.1 → 1.2 → the families' data, CPU and op chunks (the critical path).
- **Implementer B:** 1.4 and 1.5, then the GPU infrastructure (1.7), then the families' GPU chunks.
- **Implementer A or B:** WP 1.8 as the families land.
- **Verifier C:** reviews every card before it lands, adversarially and in scratch clones only.
  - It mutates the port and the tests to prove the checks catch changes.
  - Its findings go back to the card's implementer as new commits, and it re-checks them.

## Cards

Each card is one branch and one PR. The order follows the graph above, and cards in different
rows can run in parallel.

| Card | Chunks | Who | Needs | Status |
|---|---|---|---|---|
| `p1-oracle-image` | O1.2, its own chunk; the owner reviews it, labelled `oracle` | B | — | done |
| `p1-oracle` | O1.1, O1.3, O1.4, each its own chunk; the owner reviews them, labelled `oracle` | B | — | done (`p1-oracle-2`) |
| `p1-bitdepth` | 1.1a–1.1e | A | `p1-oracle-image` | done (`p1-bitdepth-2`) |
| `p1-math` | 1.4a–1.4c | B | — | done |
| `p1-dispatch` | 1.5a–1.5c, finished in spike S4: `CPUInfo` with `with_flags`/`with_build`, `SSE2.h`, the `AVX*.h` headers and their tests | B | — | done |
| `p1-foundations` | 1.2a, 1.2b, 1.2e | A | — | done |
| `p1-foundations-fix` | review follow-ups of `p1-foundations` | B | `p1-foundations` | done |
| `p1-engine` | 1.2c, 1.2d (with the optimizer's generic core, owner decision 2026-10-01: Option A), 1.3n1, 1.3m1–m2 | A | `p1-bitdepth`, `p1-math`, `p1-foundations` | done (`p1-engine-3`) |
| `p1-matrix` | 1.3m3 | A | `p1-engine` | done (`p1-matrix-4`, with `p1-range`) |
| `p1-range` | 1.3r1–r2 | A or C | `p1-engine` | done (landed in `p1-matrix-4`) |
| `p1-exponent` | 1.3e1, its CPU part | A or C | `p1-engine` | done (`p1-exponent-4`, GPU writer included) |
| `p1-gamma` | 1.3g1–g3 | A or C | `p1-engine` | done (`p1-gamma-2`) |
| `p1-log` | 1.3l1–l3 | A or C | `p1-engine` | done (`p1-log-2`) |
| `p1-cdl` | 1.3c1–c3 | A or C | `p1-engine` | done (`p1-cdl-2`) |
| `p1-optimizer` | 1.6a: only the LUT steps remain (the generic core moved to 1.2d, in `p1-engine`) | A | the families above | done (`p1-optimizer-2`; `multi_op_prefix` → Phase 2, `opt_prefix_test1` → the CTF reader) |
| `p1-gpu-infra` | 1.7a–1.7e | B | O1.3 | done (`p1-gpu-infra-3`) |
| `p1-gpu-ops` | 1.3m4, r3, e1 (its GPU part), g4, l4, c4 | B | `p1-gpu-infra`, each family's op card | done (`p1-gpu-ops`, `p1-gpu-gamma-2`, `p1-gpu-ops4`) |
| `p1-transforms` | 1.8a–1.8f | A or B | the families' op cards | done (`p1-transforms-fam4`: fam1, fam2 and fam4, with AllocationTransform and Lut1DTransform; Lut1D E–J → Phase 2, WP 2.1 and 2.5) |
| `p1-processor` | 1.8g–1.8h | A or B | `p1-transforms`, `p1-optimizer`, `p1-gpu-infra` | done (`p1-processor-4`; tests that need `Config::Create()` → Phase 3) |

The op-family cards from `p1-range` to `p1-cdl` can go to a third implementer, C (the owner
approved one on 2026-10-01).

**Closing cards**, opened after the table was written:
- `tooling-1`, `tooling-2`: harness fixes during the phase. Done.
- `p1-tests-catchup-3`: 17 upstream tests that the landed cards unblocked, the optimizer's pair
  and combination checks, and a parity counter that counts only "Port of" markers. Done.
- `p1-api-parity-2`: every analytic transform through the port's public API, against the wheel.
  CPU at every bit depth (U8, U10, U12, U16, F16, F32) in and out, packed RGBA, RGB and BGRA and
  planar RGBA and RGB, every optimization level. GPU in all 10 languages at every level.
  AllocationTransform too. It found no parity bugs. Done.

**Deferred by owner decisions:**
- Lut1D E–J (float interpolation, hue adjust, SIMD, inverse): Phase 2, WP 2.1 and 2.5.
- The GPU processor of a processor with a baked U8 or LUT op returns "not ported yet" until
  Phase 2.
- `multi_op_prefix` (Phase 2) and `opt_prefix_test1` (needs the CTF reader).
- Upstream `Processor` and `CPUProcessor` tests that need `Config::Create()`: Phase 3 (owner
  decision 2026-10-04).
- F5: the 10- and 12-bit in-place wheel test is skipped.

**Waiver change:** W0002 was extended on 2026-10-04 (owner decision) to the NaN entries of the 1D
LUT that the optimizer bakes from NaN parameters (integer and half-float input), and to the CPU
cache ID that hashes that LUT. Everything else stays exact.

Small cards land sooner and are easier to verify. When a card grows past about 6 chunks, split it
at a dependency boundary.

**Phase 1 exit (M0):** through OCIO's API, every analytic transform is byte-exact with the wheel:
- on Windows and Rocky Linux 9;
- on the CPU, at every bit depth, layout and optimization level, for every profile this machine can run;
- on the GPU, in all 10 languages.

Their upstream tests are ported and counted in `docs/parity.md`.
