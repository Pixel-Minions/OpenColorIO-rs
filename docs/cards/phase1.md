# Phase 1 cards: op engine and analytic ops

**Goal:** every op of the analytic families (Matrix, Range, Exponent, Gamma, Log, CDL, and the
no-op/marker ops) is bit-exact against the wheel through a real CPU processor engine. That
means every bit depth, pixel layout, optimization level and numeric profile available on the
machine (PLAN.md §10, Phase 1).

Each card below is a list of **chunks**. A chunk is one commit and is mergeable on its own
(`CLAUDE.md` → "Chunks"). The line counts are upstream's non-blank, non-comment lines at
v2.5.2. Paths are relative to `src/OpenColorIO/` unless they start with `tests/`.

**Testing in Phase 1.** The ops are built directly from op data in `ocio-ops` tests. The
oracle builds the equivalent transform (`cpu_apply` with a transform spec). The mapping from
transform parameters to op data is what upstream's op data constructors and `BuildXxxOp`
functions do, so the test helper that does it cites them. Phase 3 adds the real transforms
and builders, and repeats every check through `Transform → Processor`.

**Spike code.** S2/S5 (fast math, Log, Gamma), S4 (CPUInfo, Lut3D, half) and WP 0.5
(cfmt, NumberUtils, StringUtils) land first as their own chunks. The cards below refactor that
code into the architecture of `docs/architecture.md` rather than redo it.

---

## WP 1.1: bit depths, image descriptions, packing (`ocio-ops`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 1.1a | `BitDepthUtils.h/.cpp` (232) | `bit_depth_utils.rs` | `tests/cpu/BitDepthUtils_tests.cpp` |
| 1.1b | `ImageDesc.cpp`: `PackedImageDesc` + `GenericImageDesc` (~350) | `image_desc.rs` | none dedicated upstream: the packed-image parts of `CPUProcessor_tests.cpp`, plus oracle probes (O1.2) |
| 1.1c | `ImageDesc.cpp`: `PlanarImageDesc` (~270) | `image_desc.rs` | as 1.1b, for planar images |
| 1.1d | `ImagePacking.h/.cpp` (271) | `image_packing.rs` | packing tests in `CPUProcessor_tests.cpp` |
| 1.1e | `ScanlineHelper.h/.cpp` (201) | `scanline_helper.rs` | (exercised in 1.2d) |

- `ImageDesc` is public API. It lives in `ocio-ops` because the engine needs it, and
  `ocio` re-exports it.
- Error texts for bad strides, sizes and channel orders are part of the output: copy them
  verbatim.

## WP 1.2: op model and CPU engine (`ocio-ops`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 1.2a | `fileformats/FormatMetadata.h/.cpp` (410) | `format_metadata.rs` | `tests/cpu/fileformats/FormatMetadata_tests.cpp` |
| 1.2b | `DynamicProperty.h/.cpp`: the double property and its base (~200) | `dynamic_property.rs` | the double parts of `tests/cpu/DynamicProperty_tests.cpp` |
| 1.2c | `Op.h/.cpp` (748): `OpData` base, `Op`, `OpRcPtrVec` → `OpVec`, `SerializeOpVec`, `CreateOpVecFromOpData`, `HasFlag` | `op.rs`, `op_data.rs` | `tests/cpu/Op_tests.cpp`, the tests that don't need unported families |
| 1.2d | `CPUProcessor.h/.cpp` (477): `CreateCPUEngine`, generic bit-depth helpers, `apply` | `cpu_processor.rs` | `tests/cpu/CPUProcessor_tests.cpp`, the tests whose ops exist |
| 1.2e | `Processor.cpp`: op-list handling (`setTransform`'s validate → finalize → metadata; cache ID `"<NOOP>"` or `CacheIDHash`) | `processor_impl.rs` | `tests/cpu/Processor_tests.cpp` cache-ID tests whose ops exist |

- `OpData` variants are added family by family (WP 1.3). A match has no wildcard arm, so
  adding a family touches every match. That is intended.
- 1.2d needs at least one op to test with: port the NoOps (1.3n1) and Matrix (1.3m1–m2)
  first, or together.

## WP 1.3: analytic op families

One family at a time. Each family's chunks are, in order: op data (validate, identity,
no-op, cache ID, equality, inverse), CPU renderers, and the op (combine, inverse, identity
replacement, `getInfo`), each with its ported tests and exact oracle checks.

| Chunk | Upstream | Rust (`ocio-ops/src/ops/...`) | Port tests |
|---|---|---|---|
| 1.3n1 | `ops/noop/NoOps.*` (352), `ops/reference/ReferenceOpData.*` (121), `ops/allocation/AllocationOp.*` (117) | `noop/`, `reference/`, `allocation/` | `NoOps_tests.cpp`, `ReferenceOpData_tests.cpp`, `AllocationOp_tests.cpp` (the parts whose ops exist) |
| 1.3m1 | `ops/matrix/MatrixOpData.*` (783) + the matrix math in `MathUtils` | `matrix/matrix_op_data.rs` | `MatrixOpData_tests.cpp` |
| 1.3m2 | `ops/matrix/MatrixOpCPU.*` (337): scalar + SSE2 renderers (`(r+g)+(b+a)` summation) | `matrix/matrix_op_cpu.rs` | `MatrixOpCPU_tests.cpp`, oracle: every bit depth, both profiles |
| 1.3m3 | `ops/matrix/MatrixOp.*` (355) | `matrix/matrix_op.rs` | `MatrixOp_tests.cpp` |
| 1.3r1 | `ops/range/RangeOpData.*` (571) | `range/range_op_data.rs` | `RangeOpData_tests.cpp` |
| 1.3r2 | `ops/range/RangeOpCPU.*` + `RangeOp.*` (389) | `range/range_op_cpu.rs`, `range/range_op.rs` | `RangeOpCPU_tests.cpp`, `RangeOp_tests.cpp` |
| 1.3e1 | `ops/exponent/ExponentOp.*` (311) | `exponent/` | `ExponentOp_tests.cpp` |
| 1.3g1 | `ops/gamma/GammaOpData.*` (812) + `GammaOpUtils.*` (114) | `gamma/gamma_op_data.rs`, `gamma/gamma_op_utils.rs` | `GammaOpData_tests.cpp`, `GammaOpUtils_tests.cpp` |
| 1.3g2 | `ops/gamma/GammaOpCPU.*` (665), from the S2 spike | `gamma/gamma_op_cpu.rs` | `GammaOpCPU_tests.cpp`; oracle, fast math on and off |
| 1.3g3 | `ops/gamma/GammaOp.*` (182) | `gamma/gamma_op.rs` | `GammaOp_tests.cpp` |
| 1.3l1 | `ops/log/LogUtils.*` (330) + `LogOpData.*` (509) | `log/log_utils.rs`, `log/log_op_data.rs` | `LogUtils_tests.cpp`, `LogOpData_tests.cpp` |
| 1.3l2 | `ops/log/LogOpCPU.*` (760), from the S2 spike | `log/log_op_cpu.rs` | `LogOpCPU_tests.cpp`; oracle, fast math on and off |
| 1.3l3 | `ops/log/LogOp.*` (199) | `log/log_op.rs` | `LogOp_tests.cpp` |
| 1.3c1 | `ops/cdl/CDLOpData.*` (532) | `cdl/cdl_op_data.rs` | `CDLOpData_tests.cpp` |
| 1.3c2 | `ops/cdl/CDLOpCPU.*` (399): scalar + SSE2 renderers, luma order | `cdl/cdl_op_cpu.rs` | oracle, every style, fast math on and off |
| 1.3c3 | `ops/cdl/CDLOp.*` (209) | `cdl/cdl_op.rs` | `CDLOp_tests.cpp` |

The `*GPU.cpp` files of these families are Phase 3.13 and Phase 7.

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
| 1.6b | `OpOptimizers.cpp`: `ReplaceInverseLuts`, `optimizeForBitdepth` (separable-prefix bake) | `op_optimizers.rs` | needs Lut1D (Phase 2); scheduled there |

## Oracle support (owner-reviewed chunks)

| Chunk | What |
|---|---|
| O1.1 | `processor_ops`: the optimized processor's `createGroupTransform()` as JSON (each child's class and getters) plus its cache ID, for any flags and bit depths. It shows what the wheel's optimizer produced |
| O1.2 | `cpu_apply` with planar images and every channel order (`PlanarImageDesc`, `CHANNEL_ORDERING_*`), for 1.1c–1.1d |

## Order and parallelism

```
spike chunks ─┬─ 1.1a → 1.1b → 1.1c → 1.1d → 1.1e ─┐
              ├─ 1.2a → 1.2b → 1.2c ───────────────┴─ 1.2d → 1.2e ─┬─ 1.3 families (in parallel) ─ 1.6a
              ├─ 1.4a → 1.4b → (1.3m1)                              │
              └─ 1.5a → 1.5b → 1.5c ────────────────────────────────┘
```

- **Implementer A:** 1.1 → 1.2 (the critical path).
- **Implementer B:** 1.4 and 1.5 in parallel, then the families.
- **Verifier C:** reviews every chunk as it lands.

**Phase 1 exit:** every analytic family is bit-exact against the wheel on Windows and Rocky
Linux 9, at every bit depth, layout and optimization level, for every profile this machine
can run. The chunks' ported upstream tests are counted in `docs/parity.md`.
