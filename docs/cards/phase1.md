# Phase 1 cards: op engine, analytic transforms and GPU infrastructure

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

An op family lands whole: op data, CPU renderers, GPU writer, transform and glue.

Each card below is a list of **chunks**. A chunk is one commit and is mergeable on its own
(`CLAUDE.md` → "Chunks"). Line counts are upstream's non-blank, non-comment lines at v2.5.2.
Paths are relative to `src/OpenColorIO/` unless they start with `tests/`.

**Testing.**
- **Until 1.8 lands:** `ocio-ops` tests build ops directly from op data, and the oracle builds the equivalent transform (`cpu_apply` / `gpu_shader` with a transform spec). The test helper that maps transform parameters to op data cites upstream's op data constructors and `BuildXxxOp`.
- **After 1.8:** every check also runs through the real API, `Transform → Config::CreateRaw() → Processor`, with the same spec on both sides.

**Spike code.** S2/S5 (fast math, Log, Gamma), S4 (CPUInfo, Lut3D, half) and WP 0.5
(cfmt, NumberUtils, StringUtils) land first as their own chunks. The cards below refactor that
code into the architecture of `docs/architecture.md` rather than redo it. The Lut3D renderers
from S4 are kept for Phase 2.

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

- `OpData` variants are added family by family (WP 1.3). A match has no wildcard arm, so adding a family touches every match. That is intended.
- 1.2d needs at least one op to test with: port the NoOps (1.3n1) and Matrix (1.3m1–m2) first, or together.

## WP 1.3: analytic op families

One family at a time, each landing whole. Each family's chunks are, in order:
1. op data: validate, identity, no-op, cache ID, equality, inverse;
2. CPU renderers;
3. the op: combine, inverse, identity replacement, `getInfo`;
4. the GPU writer (needs WP 1.7).

Every chunk comes with its ported upstream tests and exact oracle checks.

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

## WP 1.6: optimizer (`ocio-ops`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 1.6a | `OpOptimizers.cpp`: `RemoveNoOpTypes`, `RemoveNoOps`, `ReplaceOps`, `ReplaceIdentityOps`, `RemoveInverseOps`, `CombineOps`, the 80-pass loop | `op_optimizers.rs` | `tests/cpu/OpOptimizers_tests.cpp` (the tests whose ops exist) |

`ReplaceInverseLuts` and the separable-prefix bit-depth bake need Lut1D, so they belong to Phase 2 (WP 2.5).

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
| O1.2 | `cpu_apply` with planar images and every channel order (`PlanarImageDesc`, `CHANNEL_ORDERING_*`), for 1.1c–1.1d |
| O1.3 | `gpu_shader`: the shader text, uniforms (names, types, values) and textures (names, sizes, channels, interpolation, values as a blob) from `extractGpuShaderInfo`. It takes any `GpuShaderDesc` settings: language, function name, resource prefix, pixel name, descriptor sets, 1D textures, texture limits. It works for the default and optimized GPU processors |
| O1.4 | `transform_text`: `str(transform)` (upstream's `operator<<`), validation errors, and equality results |

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
- **Verifier C:** reviews every chunk as it lands.

**Phase 1 exit (M0):** through OCIO's API, every analytic transform is byte-exact with the wheel:
- on Windows and Rocky Linux 9;
- on the CPU, at every bit depth, layout and optimization level, for every profile this machine can run;
- on the GPU, in all 10 languages.

Their upstream tests are ported and counted in `docs/parity.md`.
