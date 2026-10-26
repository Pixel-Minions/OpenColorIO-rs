# Phase 7 cards: the legacy GPU path and GPU execution

**Status: planned, 2026-10-25. Owner decisions P7-1 to P7-8 taken 2026-10-25: every recommendation, (a) where there are options.** Phases 1–5 already
give every op family its GPU writer in all 10 languages, checked against the wheel byte for
byte (shader text, uniforms, textures, cache IDs). Phase 7 adds what is left of OCIO's GPU
side and runs the shaders.

**Goal: Phase 7's part of milestone M5** (PLAN.md §10, Phase 7, ~3 pw):
- **the legacy GPU path** (`Processor::getOptimizedLegacyGPUProcessor`, which bakes the ops
  the analytic shader can't write into one 3D LUT): `Op::supportedByLegacyShader`,
  `PartitionGPUOps`, `Create3DLut` and their helpers (`ops/noop/NoOps.cpp`), byte-exact with
  the wheel, with upstream's 3 C++ tests of it (P9-7);
- **GPU execution**: a port of upstream's GPU test harness (`tests/gpu/GPUUnitTest.cpp/.h`
  and the OpenGL helpers it uses, `src/libutils/oglapphelpers/oglapp.cpp` and `glsl.cpp`),
  replaying upstream's 264 GPU tests (`tests/gpu/*_test.cpp`) on a software renderer;
- **compile checks** of the shaders, in every language that has a compiler on the reference
  platforms.

"Done" means, on Windows and Rocky Linux 9 unless P7-3 says otherwise:
- **legacy path:** the shader text and cache IDs byte for byte, and the baked 3D LUT's texture
  bit for bit, as the wheel's legacy GPU processor gives them on the same OS and CPU (the
  bake runs the CPU renderers, so its bits follow the CPU kernels and libm, D12);
- **GPU suite:** each of the 264 tests passes as upstream's harness defines passing (the GPU
  output within the test's own tolerance of the CPU's, NaN and infinity in the same places),
  with the CPU side computed by the wheel; or it is on the reviewed not-applicable list
  (PLAN.md §3) or under a renderer waiver the owner signed (P7-6). P7-7's four
  inverse 3D LUT tests run on Linux with upstream's settings and thresholds;
- **compile checks:** every shader of the suite compiles in each language P7-5 covers.

GPU pixels are not in PLAN.md §3's byte-exact table: the GPU's arithmetic belongs to the
driver. The port's shaders, uniforms and textures are the wheel's byte for byte, so on one
renderer the port's GPU output is the wheel's. The suite proves what text parity can't: that
the port's description drives a real pipeline (a uniform bound to the wrong slot, a texture
uploaded with the wrong layout), and that the legacy path's bake is right.

Each work package below is a list of **chunks**. A chunk is one commit and is mergeable on its
own (`CLAUDE.md` → "Chunks"). Line counts are upstream's non-blank lines at v2.5.2 without `//`
comments. Paths are relative to `src/OpenColorIO/` unless they start with `tests/`, `src/`,
`include/` or `crates/`.

## How this phase works

Everything in `docs/cards/phase3.md` → "How this phase works" still applies: cards and PRs, the
chunk gate, oracle-first, owner items, verifier rounds with mutation testing. What changes in
Phase 7:

- **The wheel can't render, and can't reach the legacy path from Python.**
  - The binding has no legacy GPU processor: `PyProcessor.cpp:75-78` binds only
    `getDefaultGPUProcessor` and `getOptimizedGPUProcessor`. The wheel's library exports it
    (`OpenColorIO_2_5.dll` exports
    `?getOptimizedLegacyGPUProcessor@Processor@OpenColorIO_v2_5@@QEBA...`, which the wheel's
    `ociochecklut.exe` imports; checked 2026-10-25). P7-1 decides how the oracle reaches it.
  - The wheel's tools are built without OpenGL (`ociochecklut.exe` and `ocioconvert.exe`
    print "Compiled without OpenGL support, GPU options are not available."), and the wheel
    ships no GPU test runner. GPU pixels come from a renderer on the test machine, driven by
    the port's harness (P7-2).
- **Expected values** (below): the wheel's CPU output through the oracle, compared with
  upstream's own tolerances, as upstream's harness compares its GPU output with its CPU
  output (`GPUUnitTest.cpp:388-526`). The port's CPU processor is never the reference, though
  it is bit-identical to the wheel's.
- **A software renderer, in Rocky Linux 9** (P7-2 to P7-4). GitHub's runners have no GPU. The
  suite runs where a GL 2.0 context exists without one: Mesa's llvmpipe in the Rocky image.
  Chunks that touch the harness or the suite run `cargo xtask gate --release --rocky`.
- **Not CPU tests.** llvmpipe compiles shaders for the host CPU, and the CPU side is the
  wheel's on the same machine; the tolerances absorb both. The GPU targets stay out of the
  `cpu-tests` alias and of SDE.
- **Owner items** stop at the owner, labelled `api`, `dependency`, `deviation`, `waiver` or
  `n/a` (P7-1 to P7-8). Oracle chunks are reviewed by a verifier agent (the owner's decision
  of 2026-10-05).

**What Phase 7 needs from Phases 1–6.** All landed:
- Phases 1–2: the op model, `OpVec::finalize`, the CPU renderers (`getCPUOp(false)`, which
  `Create3DLut` applies), the Lut3D op and `GenerateIdentityLut3D`, the allocation ops
  (`CreateAllocationOps`, `CreateGpuAllocationNoOp`), every GPU writer, `GpuShaderDesc`,
  `GpuProcessor` and the processor's GPU cache (1.8h3).
- Phase 3: configs from streams, `FileTransform`, `Config::getProcessor`.
- Phase 4: the LUT files the suite reads (`spi1d`, `spi3d`, `csp`, CTF).
- Phase 5: dynamic properties as GPU uniforms; the ExposureContrast and grading writers.
- Phase 6: none (the binding has no legacy GPU processor).

---

## Expected values (CLAUDE.md rule 2)

| Kind of test | Tests | GPU side (the port) | Expected values | Comparison |
|---|---:|---|---|---|
| The default GPU processor | 185 | The port's `getDefaultGPUProcessor` shader, GLSL 1.2, on the renderer | The wheel's `getDefaultCPUProcessor()` applied to the same input image (`cpu_apply`, F32 RGBA), on the same machine | Upstream's (`DiffComponent`, `ValidateInf`, `ValidateNan`, `ComputeDiff`, `GPUUnitTest.cpp:26-124, 355-383`): the test's `setErrorThreshold`, absolute or relative (`setRelativeComparison`, `setExpectedMinimalValue`), NaN and ±Inf (beyond `FLT_MAX`) in the same places |
| Dynamic properties | 45 (7 with retests) | The same, with the port's dynamic properties set before each run (`addRetest`, `GPUUnitTest.cpp:702-718`) | The wheel's CPU processor after the same sets in the same order (`dynamic_apply`: set, then apply, per retest) | Upstream's, per run |
| The legacy GPU processor | 30 | The port's `getOptimizedLegacyGPUProcessor(OPTIMIZATION_DEFAULT, edge)` shader | The wheel's default CPU processor (upstream's harness compares the legacy GPU output with the default CPU output too, `GPUUnitTest.cpp:391`) | Upstream's. The shader and its 3D texture are also compared byte for byte with the wheel's legacy processor (P7-1) |
| Config tests (counted above: 4 default, 1 legacy) | 5 | The processor of a config read from a stream, its search path at upstream's test files | The wheel's CPU processor of the same config (`cpu_apply` with `{"yaml": ...}`) | Upstream's |
| Disabled only in upstream Windows debug | 4 (included above) | Run normally on Linux (`Lut3DOp_test.cpp`: `inv3dlut_*`; disable guard is `!NDEBUG && WIN32`) | The wheel's CPU processor | P7-7: owner approved all four Linux tests, 2026-10-26 |
| C++ tests of the legacy path | 3 | — | Upstream's own assertions, verbatim (`NoOps_tests.cpp`; `GpuShader_tests.cpp` `MetalLutTest`'s expected text) | Exact |
| Compile checks | — | The port's shader text (first asserted equal to the wheel's) | The compiler's verdict (the platform's tool, P7-5) | Pass or fail |

- **Inputs** are generated by the ported harness (`UpdateImageTexture`, `GPUUnitTest.cpp:217-329`:
  NaN rows, ±Inf rows, then a ramp `min + step * float(i)` in f32) or copied from the test
  (`setCustomValues`); each test file pins a digest of its inputs (rule 2 allows it).
- **The harness's processors.** `setProcessor(transform)` builds its processor with
  `Config::Create()` and the processor cache off (`GPUUnitTest.cpp:137-142`). The oracle's
  `cpu_apply` uses `CreateRaw()` when given no config; O7.2 adds `Config.Create()` to the
  config spec, so both sides build the same processor.
- **Tolerances** are upstream's own, in ported upstream tests (rule 3): each test's
  `setErrorThreshold` and comparison settings are copied verbatim, and the comparison helpers
  are ported into `ocio_testkit::upstream::gpu`, citing `GPUUnitTest.cpp`.

**The wheel's own GPU output.** No check needs it to pass, but two things use it: the
baseline of what passes on the renderer (P7-6), and an identity check of the harness.
- **How (recommended): the wheel's shader through the port's harness.** `gpu_shader` already
  gives the wheel's shader text, uniforms with their values and textures with every value's
  bits. 7.2c builds a description from them and renders it like the port's. On one renderer,
  the port's GPU output must then equal the wheel shader's bit for bit (the expected values
  are the wheel's shader, executed by the platform), and a test that fails with the port's
  shader fails with the wheel's too. Cost: ~150 lines in 7.2c; no dependency. Legacy cases need
  P7-1's oracle for the wheel's shader.
- **Alternative: build upstream's `test_gpu_exec`** (`tests/gpu` with `oglapphelpers`, GLEW and
  EGL) against the wheel's library. It needs GLEW and the EGL/GL development headers (downloads),
  upstream's CMake build of its test utilities and a link against the wheel's DLL/.so: about
  a week, two toolchains, and a second harness to maintain. Not recommended unless the
  baseline is disputed.
- **Alternative: a real GPU, as upstream's CI** (an NVIDIA GPU in AWS CodeBuild,
  `.github/workflows/gpu_workflow.yml`): a paid GPU runner. Not needed for §3.

---

## WP 7.1: the legacy GPU path (`ocio-ops`, `ocio`)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 7.1a | `Op.h:247-248` (`supportedByLegacyShader`), its overrides `ops/lut1d/Lut1DOp.cpp:54` and `ops/lut3d/Lut3DOp.cpp:100`; `ops/noop/NoOps.cpp:94-101` (`DefinesGpuAllocation`), `110-173` (`GetGpuUnsupportedIndexRange`, `GetGpuAllocation`), `175-206` (`Create3DLut`), `208-287` (`PartitionGPUOps`, with its message "Invalid GpuUnsupportedIndexRange: ..."); `NoOps.h:17-31` (~140) | `ocio-ops/src/op.rs`, `ops/noop/no_ops.rs` | `tests/cpu/ops/noop/NoOps_tests.cpp` `partition_gpu_ops`, `throw` (with the file's helpers, `17-95`) |
| 7.1b | `Processor.cpp:198-202, 447-489` (`getOptimizedLegacyGPUProcessor`: the partition, the lattice's `finalize`, `Create3DLut`, the second `finalize`, then `getGPUProcessor` and its cache, `491-525`); `include/OpenColorIO/OpenColorIO.h:2736-2742` (P7-8) (~40) | `ocio/src/processor.rs` | `tests/cpu/GpuShader_tests.cpp` `MetalLutTest` |
| 7.1c | — (the oracle sweep, after O7.1) | — | Against the wheel's legacy processor (O7.1), in the 10 languages: chains with no LUT (the analytic shader alone); a LUT alone; analytic ops, LUTs, analytic ops; several LUTs apart; allocations (`uniform`, `lg2`, with and without vars) at the cut; `FileTransform`s (the `FileNoOp` before the LUT); a dynamic op inside and outside the lattice; edge lengths 2, 3, 32, 64, 129 and the edges O7.1 probes (0, 1, large); every optimization level; the default and the legacy processor requested in both orders (the shared cache, below). Each: the shader's text, cache ID, uniforms, the 3D texture's bits, or the exception. Then the 30 legacy tests of the suite get their identity check against the wheel's shader (7.2c's runner) |

7.1a and 7.1b are implemented together as one coherent legacy-processor chunk: about 150
new non-test lines for the API and its partition/bake helpers, with all three upstream tests.
That combined unit runs the full staged release/Rocky gate before its commit. The independent
oracle sweep remains a separate `p7-legacy-parity` test chunk after O7.1.

- **The shared GPU cache.** The legacy processor goes through `getGPUProcessor`, whose cache is
  keyed by the optimization flags only (`Processor.cpp:505-516`). With the cache on, a default
  GPU processor requested first is returned for a legacy request with the same flags, and the
  first legacy edge length wins over later ones. Ported as is; an `I-` entry.
- **The lattice's alpha.** `Create3DLut` applies the ops to an RGBA lattice whose alpha is 0:
  the vector is zero-initialised and `GenerateIdentityLut3D` writes RGB only (`NoOps.cpp:185-186`).
  A matrix that mixes alpha into RGB bakes with alpha 0. Ported as is; an `I-` entry.
- **Edge lengths.** `edgelen` 1 gives `1 / (edgeLen - 1)` = inf in the identity
  (`Lut3DOp.cpp:33`). `edgelen^3` is computed in 32-bit `unsigned` (`NoOps.cpp:180`), but
  `Lut3DOpData` construction calls `Lut3DArray::resize` before allocating the RGBA vector and
  rejects every edge greater than 129 (`Lut3DOpData.cpp:194-204`), so the wrapped pixel count
  is never used. At edge zero, `&lut3D[0]` is formed on an empty vector, but count-zero loops
  access no pixel; both wheels raise "Lut3D content array issue: Array content is empty."
  during GPU finalization (I-503). O7.1 records the boundary cases on both wheels.
- The CPU renderers the bake uses are `getCPUOp(false)` (`Op.h:240-244`): fast math off,
  whatever the flags. The texture's bits follow the platform's libm (D12).

## WP 7.2: the GPU execution harness (new crate `ocio-gpu-exec`, P7-2)

| Chunk | Upstream | Rust | Port tests |
|---|---|---|---|
| 7.2a | — (infrastructure, P7-4) | `docker/rocky9/Dockerfile` (Mesa's llvmpipe with EGL and GL, and `glslang`, pinned), `scripts/rocky9.sh` (`EGL_PLATFORM=surfaceless`), `.github/workflows/ci.yml` (the Rocky job rebuilds the image) | A probe: `eglinfo`-like output from the container (vendor, renderer `llvmpipe`, GL version, compatibility profile, GLSL 1.20) |
| 7.2b | `src/libutils/oglapphelpers/oglapp.cpp:1-272, 309-390` and `oglapp.h` (`OglApp`: `initImage`, `updateImage`, `createGLBuffers`, `reshape`, `redisplay`, `readImage`, `setShader` with its GLSL 1.2 `main`; `HeadlessApp` through EGL); `glsl.cpp` and `glsl.h` (`OpenGLBuilder`: 1D, 2D and 3D textures, uniforms of every type, shader compile and link, `GetTextureMaxWidth`) (~720; one unit, past the usual ~600, as 5.7b) | `ocio-gpu-exec/src/oglapp.rs`, `glsl.rs` (the `unsafe` allowlist, P7-2) | The harness renders an identity description (no op): the read-back image equals the input image bit for bit (the renderer is the reference); every uniform type and texture kind round-trips through a shader that writes it out |
| 7.2c | `tests/gpu/GPUUnitTest.h` (127), `GPUUnitTest.cpp:26-526, 661-753` (the test object and its defaults, the input image, `UpdateOCIOGLState`, the comparison, one test's run with its retests; `main`'s registry and argument parsing are not ported) (~520); the wheel-shader runner (above, ~150) | `ocio-gpu-exec/src/gpu_unit_test.rs`; `ocio-testkit/src/upstream/gpu.rs` (the comparison helpers) | `tests/gpu/MatrixOp_test.cpp` (10, 8 of them legacy: needs 7.1b), each also checked bit for bit against the wheel's shader on the same renderer |

- `ScreenApp` (`oglapp.cpp:274-307`, a GLUT window) and the Metal helpers (`metalapp`, `msl`,
  `mtltexture`) stay `n/a`: upstream's harness uses them only with a display or on macOS.
- `gluOrtho2D` (`oglapp.cpp:160`) is GLU's: ported inline as `glOrtho(0, w, 0, h, -1, 1)`, its
  definition, rather than linking libGLU.
- `HeadlessApp` asks for `EGL_DEFAULT_DISPLAY` (`oglapp.cpp:332`). In a container with no
  X, Wayland or GPU device, Mesa needs `EGL_PLATFORM=surfaceless` to give one; 7.2a sets it
  in the environment, and the harness keeps upstream's call.
- One GL context per test, created and destroyed by the test (upstream runs every test in one
  context, `GPUUnitTest.cpp:590-623`; the state each test sets is all its own). A test file's
  CPU expectations go to the oracle in one batch (`Oracle::batch`).
- The upstream-map targets of `tests/gpu/*` move from `crates/ocio-gpu/tests/` to
  `crates/ocio-gpu-exec/tests/`: `ocio-gpu` can't use `ocio` (configs, files, processors),
  which depends on it.

## WP 7.3: upstream's GPU suite (`ocio-gpu-exec/tests`)

| File | Tests | Legacy | Dynamic | Custom values | Upstream lines | Chunk |
|---|---:|---:|---:|---:|---:|---|
| `MatrixOp_test.cpp` | 10 | 8 | — | — | 99 | 7.2c |
| `RangeOp_test.cpp` | 8 | — | — | — | 89 | 7.3a |
| `GammaOp_test.cpp` | 14 | 2 | — | — | 193 | 7.3a |
| `CDLOp_test.cpp` | 15 | 2 | — | — | 265 | 7.3b |
| `LogOp_test.cpp` | 22 | 4 | — | — | 249 | 7.3b |
| `Lut1DOp_test.cpp` | 29 | 8 | — | 1 | 277 | 7.3c |
| `Lut3DOp_test.cpp` | 15 (all run on Linux) | 5 | — | — | 148 | 7.3c |
| `Config_test.cpp` | 5 | 1 | — | — | 163 | 7.3c |
| `FixedFunctionOp_test.cpp`, ACES 2.0 (`style_aces2_*`) | 23 | — | — | 22 | ~690 | 7.3d |
| `FixedFunctionOp_test.cpp`, the others (ACES 1.x, Rec.2100 surround, HSV, HSY, xyY, uvY, LUV, PQ, gamma log, double log) | 36 | — | — | 18 | ~600 | 7.3e |
| `ECOp_test.cpp` | 9 | — | 3 (1 retest) | — | 266 | 7.3f |
| `GradingPrimaryOp_test.cpp` | 14 | — | 8 (2 retests) | — | 383 | 7.3f |
| `GradingRGBCurveOp_test.cpp` | 14 | — | 8 (2 retests) | — | 330 | 7.3f |
| `GradingToneOp_test.cpp` | 34 | — | 18 (2 retests) | — | 466 | 7.3g |
| `GradingHueCurveOp_test.cpp` | 16 | — | 8 | 2 | 215 | 7.3g |
| **Total** | **264** | **30** | **45** | **43** | 6,889 (with the harness) | |

- Each chunk ports its files verbatim, one `#[test]` per `OCIO_ADD_GPU_TEST` with its marker
  (`/// Port of `OCIO_ADD_GPU_TEST(Group, name)` @ v2.5.2.`), and updates their map entries.
  Test-only chunks: their size is the test files'.
- The matrix, range, gamma, CDL and log tests already exist as data (`ocio-testkit/src/gpu_cases.rs`,
  69 cases with their thresholds, built for this). Their files replay those tables; the
  other families' tests are ported as upstream writes them, a setup closure per test.
- **Legacy tests** need 7.1b; every file but Range, FixedFunction, EC and grading has some.
- **Dynamic tests** set the properties the way upstream does: the GPU description's and the
  CPU processor's, separately, through each retest's callback (`ECOp_test.cpp:217-249`).
- **Files** come from `tests/data/files` through absolute paths, as upstream's
  `OCIO_UNIT_TEST_FILES_DIR` (`lut1d_1.spi1d` to `lut1d_5.spi1d`, `lut1d_halfdom.ctf`,
  `lut1d_green.ctf`, `lut1d_1024_hue_adjust_test.ctf`, `lut3d_*.csp`, `lut3d_*.spi3d`, ...).
  `lut1d_3.spi1d` (131,072 entries) and `3dlut_biggest_supported` (129³) are the largest.

## WP 7.4: compile checks (P7-5)

| Chunk | What | Where | Rust |
|---|---|---|---|
| 7.4a | Every shader of the 264 tests (the port's, asserted equal to the wheel's `gpu_shader` first), in GLSL 1.2, 1.3, 4.0, ES 1.0, ES 3.0 and Vulkan GLSL 4.6, each wrapped in a minimal fragment shader of its language (`main`, the input, the output; GLSL 1.2's is `oglapp.cpp:206-215`), compiled by `glslangValidator` (`-V` for Vulkan GLSL); plus every shader the family oracle tests extract (`crates/ocio-gpu/tests/*_oracle.rs`), in the same languages | Rocky Linux 9 | `ocio-gpu-exec/tests/compile_glsl.rs` (~250) |
| 7.4b | The same shaders in HLSL (`GPU_LANGUAGE_HLSL_DX11`, Shader Model 5.0), wrapped in a pixel shader entry point, compiled by the Windows SDK's `fxc.exe /T ps_5_0` and by `dxc.exe /T ps_6_0` | Windows | `ocio-gpu-exec/tests/compile_hlsl.rs` (~150) |

- A shader of the wheel's that fails to compile is upstream's bug: it is listed in
  `docs/improvements.md`, and the test's expectation for it comes to the owner (P7-6), never a
  skip.
- MSL 2.0, OSL 1 and Cg have no compiler on the reference platforms (P7-5): text parity only.

---

## Upstream tests

| File | Tests | Chunks | Waits |
|---|---:|---|---|
| `tests/gpu/*_test.cpp` (14 files) | 264 | 7.2c, 7.3a–7.3g | All four conditional inverse tests run on Linux (P7-7); any renderer failure: P7-6 |
| `tests/cpu/ops/noop/NoOps_tests.cpp` | 2 of its tests | `partition_gpu_ops`, `throw` (7.1a) | — |
| `tests/cpu/GpuShader_tests.cpp` | 1 of its tests | `MetalLutTest` (7.1b) | — |

267 upstream tests land in Phase 7: all 264 GPU, including the four inverse 3D LUT
tests disabled only in upstream Windows debug, and 3 C++. The ratchet counts the GPU ones as `gpu` (`xtask/src/parity.rs`).

## Oracle support (verifier-reviewed chunks)

The existing commands give most of what the phase needs: `cpu_apply` (the CPU side),
`dynamic_apply` (O5.1, the dynamic tests and their retests), `gpu_shader` (O1.3, the wheel's
shaders for the identity check and the compile checks), `with_files` (O4.2).

| Chunk | Command (module) | What |
|---|---|---|
| O7.1 | `legacy_gpu_shader` (`legacy_gpu.py`, with a native helper, P7-1) | The wheel's `getOptimizedLegacyGPUProcessor(flags, edgelen)` for a processor given as `gpu_shader` takes it, then the description's contents as `gpu_shader` reports them (text, uniforms, textures with their bits, cache IDs, the exception); with an optional default GPU processor requested before, to show the shared cache. The helper is a small C++ program built against the wheel's library and upstream's v2.5.2 headers (P7-1 (a)). Probes the edge lengths 0, 1 and large, on both wheels, before any case asks for them |
| O7.2 | `spec.config` (`spec.py`) | `{"create": {"processor_cache": "off"}}`: `Config.Create()` with the processor cache off, the harness's config (`GPUUnitTest.cpp:137-142`), with contract tests |

- O7.1 and O7.2 land first, in card `p7-oracle`, and a verifier agent reviews each with
  mutation testing.
- No fixture group: GPU pixels and CPU pixels run live; the legacy shader's text is checked
  live too (its texture is platform-dependent).

## New dependencies

Every row is a `dependency` owner item. None of the crates needs a download on this machine:
they are in its cargo cache already (`glow-0.17.0`, `khronos-egl-6.0.0`, `libloading-0.8.9`,
`wgpu-30.0.1`, `naga-30.0.1` and their trees, from another project; the first chunk confirms
the trees with `cargo tree --offline`). The Mesa and glslang packages are downloaded once,
when the Rocky image is rebuilt. Licences are all in `deny.toml`'s list.

| Dependency | Version | Licence | Why | Alternatives |
|---|---|---|---|---|
| `glow` | =0.17.0 | MIT OR Apache-2.0 OR Zlib | The GL calls of the ported `oglapp.cpp` and `glsl.cpp` | `gl` (`gl_generator` 0.14, also cached), or hand-written GL declarations |
| `khronos-egl` | =6.0.0, feature `dynamic` | MIT OR Apache-2.0 | The headless context of upstream's `HeadlessApp` (`eglGetDisplay`, `eglChooseConfig`, a pbuffer, `eglCreateContext`, `eglGetProcAddress`) | `glutin` (larger, windowing), or hand-written EGL declarations |
| `libloading` (through `khronos-egl`'s `dynamic`) | =0.8.9 | ISC | Loads `libEGL.so.1` at run time, so no `-devel` package is needed to build, and Windows builds the crate without EGL | `khronos-egl`'s `static` feature (links `-lEGL`; needs `mesa-libEGL-devel` in the image) |
| Mesa llvmpipe: `mesa-dri-drivers`, `mesa-libEGL`, `mesa-libGL` and their `libglvnd` dependencies (Rocky 9 AppStream) | Exact NVRs pinned in the Dockerfile | MIT (Mesa), MIT-style (libglvnd) | A GL 4.5 compatibility-profile renderer on the CPU, with EGL's surfaceless platform: upstream's GLSL 1.2 harness runs as written, in the container, locally and on GitHub's runners | lavapipe (`mesa-vulkan-drivers`, Vulkan, for wgpu); a GPU runner (paid) |
| `glslang` (`glslangValidator`, Rocky 9 AppStream) | Exact NVR pinned | BSD-3-Clause and Khronos' permissive licences | GLSL compile checks (7.4a), every GLSL flavour including Vulkan GLSL | The `glslang` or `shaderc` crates (build glslang's C++ from source; not cached); naga (its GLSL front end reads only GLSL 4.40+ core) |
| `fxc.exe`, `dxc.exe` (Windows SDK 10.0.22000, present on this workstation; GitHub's Windows images ship an SDK) | The SDK's, located at run time | Microsoft's SDK licence (used, not redistributed) | HLSL compile checks (7.4b): fxc is the Shader Model 5.0 compiler OCIO's HLSL targets; dxc checks it for Shader Model 6 | `hassle-rs` (MIT; loads `dxcompiler.dll` in process, `unsafe`; not cached; dxc only) |
| `wgpu` + `naga` (PLAN.md §9's pick) | =30.0.1 | MIT OR Apache-2.0 | Only under P7-2 (b) or P7-3 (c) | — |

## Owner decisions

| # | Decision | Recommendation (the owner took each, 2026-10-25) |
|---|---|---|
| P7-1 | **The oracle for the legacy GPU processor** (`dependency`: a native helper in the oracle). The binding doesn't expose it, so the oracle can't call it from Python | **(a) A native helper:** `oracle/native/legacy_gpu.cpp` (~200 lines), built against the wheel's own library with upstream's v2.5.2 public headers: on Windows by MSVC (VS 2022 is on this workstation and GitHub's images; an import library generated from the DLL's exports with `dumpbin` and `lib /def`, the `/MD` release runtime), on Linux by the image's GCC (linking the wheel's `.so`, with the libstdc++ ABI the wheel's symbols show). `legacy_gpu.py` runs it and reports like `gpu_shader`. The oracle's cache key covers its source. Without it, the legacy path's text and texture parity (PLAN.md §3) has no oracle. **(b)** `ctypes` into the wheel's library, calling the mangled C++ member through pybind11's holder pointers: fragile, per-platform ABI code in Python. **(c)** Rebuild the result from public calls (the optimized GPU processor of the pre ops, a `Lut3DTransform` baked from a CPU processor at `OPTIMIZATION_NONE` on an identity lattice, the post ops): it encodes our reading of `PartitionGPUOps` into the expected value, so it proves nothing about it. **(d)** No parity check: only the GPU suite's 30 legacy tests and the 3 C++ tests |
| P7-2 | **The execution backend** (`dependency`, `unsafe`, layout). Upstream's harness is OpenGL with GLSL 1.2 and the fixed-function pipeline (`gl_TexCoord`, `gl_FragColor`, `glBegin(GL_QUADS)`, `oglapp.cpp:126-148, 206-215`) | **(a) OpenGL, as upstream: `glow` and `khronos-egl` on Mesa's llvmpipe.** A port of `oglapp.cpp` (its `HeadlessApp`) and `glsl.cpp` runs the suite in upstream's own language, GL state, texture formats and filtering. glow's calls are `unsafe`: the harness's two GL files join the `unsafe` allowlist (as P9-3's FFI file did). Everything lives in a new test-only crate, `crates/ocio-gpu-exec` (`publish = false`), so the library's dependencies don't change. PLAN.md §8 and §9 then say OpenGL and llvmpipe instead of wgpu and naga. **(b) wgpu 30 (PLAN.md's pick):** naga's GLSL front end reads only GLSL 4.40+ core, so the suite would run another language (Vulkan GLSL 4.6, or the HLSL output on DX12) through naga's translation, a different test from upstream's, with a translator between the wheel's text and the device. Its gain: WARP on Windows, built into Windows and GitHub's images (P7-3 (c)). **(c) Both:** (a) now, (b) later for Windows |
| P7-3 | **Where the suite runs** | **(a) Rocky Linux 9 only:** locally (`scripts/rocky9.sh`) and in CI's Rocky job, on llvmpipe. Windows is covered by the text, uniform and texture parity of Phases 1–5 and by 7.4b; the GPU targets compile to nothing on Windows (`#![cfg(target_os = "linux")]`, stated in each file, never a skip). This workstation's GPU (AMD Radeon RX 9070 XT, through WGL) can run the suite as a local diagnostic later (~150 more `unsafe` lines for a WGL context). **(b)** Windows too, through Mesa's llvmpipe for Windows (an `opengl32.dll` build: a download, run locally and on GitHub's runners). **(c)** Windows through wgpu on WARP (P7-2 (b)/(c)) |
| P7-4 | **The Rocky image** (`dependency`: a download at image build) | **Add Mesa's llvmpipe with EGL and GL, and `glslang`, from Rocky 9's repositories, exact NVRs pinned**, and rebuild the image once (7.2a). The Windows side needs nothing new |
| P7-5 | **Compile checks: tools and languages** | **glslang for the six GLSL languages (Rocky), the Windows SDK's `fxc` (ps_5_0) and `dxc` (ps_6_0) for HLSL (Windows), as subprocesses; no `hassle-rs`, no naga.** MSL 2.0 (needs Apple's compiler), OSL 1 (`oslc` isn't packaged for Rocky 9 or Windows) and Cg (NVIDIA's toolkit is discontinued) get no compile check: text parity only, the reason in PLAN.md §8, reopened with macOS (D11). PLAN.md §8's "naga for Vulkan GLSL" becomes glslang `-V` |
| P7-6 | **Tests that fail on the renderer with upstream's own shader** (`waiver`). Upstream tuned its thresholds on an NVIDIA GPU; llvmpipe's transcendentals, NaN rules and filtering differ | **One waiver per test and renderer (`waivers.toml` and `docs/waivers.md`), with its evidence:** the wheel's shader fails the same way (the port's GPU output equals the wheel shader's bit for bit, 7.2c) and the diff upstream's harness prints. The waived test still runs, and checks that identity and the rest of the image. Agents batch the list per card into one owner question; none is decided by an agent |
| P7-7 | **Four inverse 3D LUT tests**: `Lut3DOp/inv3dlut_file_spi3d_linear`, `_tetra`, `_bizarre_linear`, `_bizarre_tetra` disable only inside `#if !defined(NDEBUG) && defined(WIN32)` (`Lut3DOp_test.cpp:156-239`). Upstream runs them on Linux and Windows release | **Owner correction, 2026-10-26: run all four on Linux with upstream settings and thresholds.** This replaces the earlier exclusion approved from this card's incorrect never-runs rationale. No not-applicable entry or waiver for these four |
| P7-8 | **The legacy API** (`api`) | **`Processor::optimized_legacy_gpu_processor(&self, o_flags: OptimizationFlags, edgelen: u32) -> Result<Arc<GpuProcessor>>`**, beside `optimized_gpu_processor` (1.8h3), sharing its cache as upstream does. No Python binding: upstream has none |

## Platforms and runners

| Where | The GPU suite | GLSL compile | HLSL compile | O7.1's helper |
|---|---|---|---|---|
| This workstation, Windows (AMD Radeon RX 9070 XT; WARP) | Only under P7-3 (b) or (c), or as a local diagnostic | No (glslang on Windows needs the Vulkan SDK, a download) | Yes (Windows SDK 10.0.22000: `fxc.exe`, `dxc.exe`, `dxcompiler.dll`, checked 2026-10-25) | Yes (MSVC 14.44, VS 2022 Community) |
| Rocky Linux 9, local (`scripts/rocky9.sh`) | Yes: llvmpipe through EGL, after 7.2a | Yes, after 7.2a | No | Yes (the image's `gcc-c++`) |
| GitHub `windows-2025` | No: no GPU, and Windows' own OpenGL is GDI's 1.1, below GLSL; P7-3 (b) or (c) would change that | No | Yes, with the image's Windows SDK (7.4b's first CI run confirms its path) | Yes (VS 2022) |
| GitHub `ubuntu-24.04`, the Rocky container | Yes, as locally (CPU rendering of 264 images of 256×256; minutes at most, to measure in 7.2c) | Yes | No | Yes |
| SDE (`scripts/sde.sh`) | No (llvmpipe under emulation; the GPU suite is not CPU-dependent in what it checks) | No | No | — |

## Platform risks

- **llvmpipe against upstream's thresholds.** Upstream runs its suite on an NVIDIA GPU
  (`.github/workflows/gpu_workflow.yml`); its other CI workflows build with
  `OCIO_BUILD_GPU_TESTS=OFF`. llvmpipe has its own `exp2`/`log2`/`pow`, flushes denormals and
  handles NaN its own way. The first run of each family is the baseline: what fails with the
  wheel's shader too goes to P7-6.
- **The input image is sampled with `GL_LINEAR`** (`oglapp.cpp:65-66`). At texel centres the
  weights should be exact, but a filter that computes `a + w * (b - a)` spreads a NaN
  neighbour into finite pixels. The harness keeps upstream's state; the baseline shows it.
- **Results by CPU.** llvmpipe compiles shaders with the host's features (FMA on AVX2 hosts):
  GPU output can differ between runners. The tolerances absorb it; the identity check compares
  the port with the wheel's shader within one run, on one machine.
- **Mesa's version.** The image pins exact package versions; a Rocky point release that moves
  Mesa is a new image, rerun in full.
- **EGL in a container.** `EGL_DEFAULT_DISPLAY` needs `EGL_PLATFORM=surfaceless` (7.2a).
- **Shader text by platform.** A negative NaN prints `-nan(ind)` on Windows and `-nan` on Linux,
  and the ACES 2.0 tables follow libm (`oracle/ocio_oracle/gpu.py`, module doc). Neither changes
  the suite's shaders, which have no NaN parameter; the ACES 2.0 tables are compared per
  platform, as today.
- **The legacy bake by platform.** The lattice runs the CPU renderers with fast math off, so
  libm and the CPU's kernels decide its bits: 7.1c compares it with each platform's wheel.
- **The native helper's ABI** (P7-1 (a)). MSVC's STL is binary-compatible across 14.x with the
  release runtime; on Linux the helper's `_GLIBCXX_USE_CXX11_ABI` must match the wheel's (its
  exported symbols show `__cxx11` or not). O7.1 checks both before its first case.
- **Undefined behaviour in the wheel.** Large edge lengths wrap and allocate before any check
  (WP 7.1). O7.1 runs those probes in a child process with a memory limit; a crash is a `U-`
  entry and a deliberate error in the port.

## Order and parallelism

```
p7-oracle (O7.1, O7.2) ──────────────────────────── p7-legacy-parity (7.1c)
p7-legacy (7.1a, 7.1b) ──────────────────────────┐
p7-harness (7.2a ── 7.2b ── 7.2c) ───────────────┴─ p7-suite-1 (7.3a, 7.3b) ──────┐
                                                    p7-suite-2 (7.3c) ─────────────┤
                                                    p7-suite-3 (7.3d, 7.3e) ───────┼─ Phase 7 done
                                                    p7-suite-4 (7.3f, 7.3g) ───────┤
p7-compile (7.4b now; 7.4a after 7.2a) ────────────────────────────────────────────┘
```

**The fastest path to a large share of the 264.**
1. **Decide P7-2 to P7-4 first.** The harness waits for them; P7-1 only gates 7.1c.
2. **Wave 1 (3 implementers, in parallel):** A: `p7-harness`, the critical path
   (7.2a → 7.2b → 7.2c). B: `p7-legacy` (one small implementation chunk; done before 7.2c needs it), then
   `p7-compile`'s 7.4b. C: `p7-oracle`, then `p7-legacy-parity`.
3. **Wave 2 (up to 4 implementers, in parallel), once `p7-harness` lands:** the four
   `p7-suite-*` cards. Each is test-only and independent of the others. With 3 agents, A takes
   `p7-suite-3` (the largest), B `p7-suite-4`, C `p7-suite-1` then `p7-suite-2`.
4. **Owner items in batches:** each suite card collects its renderer failures (P7-6) into one
   question with the baseline evidence.

Counted as they land: 10 tests with `p7-harness` (MatrixOps); 69 with `p7-suite-1` (every
family `gpu_cases.rs` already holds); 156 with `p7-suite-4`; 215 with `p7-suite-3`; 264 with
`p7-suite-2`, including P7-7's four inverse tests. A P7-6 renderer waiver keeps
the test running and checking same-renderer identity. 234 of the 264 don't use the legacy
processor. No test's upstream check needs P7-1; the identity check of the 30 legacy tests
against the wheel's shader comes with `p7-legacy-parity`, once O7.1 exists. The critical path is five chunks deep: 7.2a,
7.2b, 7.2c, then a suite card's two.

- **Verifier:** reviews every card before it lands, with mutation testing (a threshold, the
  relative/absolute switch, `expectedMinimalValue`, NaN and Inf consistency, the ramp's step,
  the partition's walk back to an allocation, the lattice's order, the cache key, a uniform's
  type).

## Cards

Each card is one branch and one PR. "Now" means the card can start once its owner decisions are
made, without waiting for other Phase 7 cards.

| Card | Chunks | Size | Who | Needs | Owner items |
|---|---|---|---|---|---|
| `p7-oracle` | O7.1, O7.2 | 2 | C | now | P7-1 (O7.1 only) |
| `p7-legacy` | 7.1a + 7.1b | 1, ~180 and their tests | B | now | P7-8 |
| `p7-legacy-parity` | 7.1c | 1 (tests) | C | `p7-oracle`, `p7-legacy`; the identity check after `p7-harness` | — |
| `p7-harness` | 7.2a–7.2c | 3, ~1,400 | A | now (7.2c: `p7-legacy`) | P7-2, P7-3, P7-4 |
| `p7-suite-1` | 7.3a (Range, Gamma), 7.3b (CDL, Log) | 2 (tests, 59) | any | `p7-harness` | P7-6 (renderer failures, if any) |
| `p7-suite-2` | 7.3c (Lut1D, Lut3D, Config) | 1 (tests, 49) | any | `p7-harness` | P7-6, P7-7 |
| `p7-suite-3` | 7.3d (ACES 2.0), 7.3e (the other fixed functions) | 2 (tests, 59) | any | `p7-harness` | P7-6 |
| `p7-suite-4` | 7.3f (EC, GradingPrimary, GradingRGBCurve), 7.3g (GradingTone, GradingHueCurve) | 2 (tests, 87) | any | `p7-harness` | P7-6 |
| `p7-compile` | 7.4a, 7.4b | 2, ~400 | B | 7.4b now; 7.4a after 7.2a | P7-5 |

9 cards, 16 chunks: 6 with library or harness code (~1,800 lines), 8 test-only, 2 oracle.

**Can start now** (once their owner items are decided): `p7-oracle`, `p7-legacy`,
`p7-harness`, and `p7-compile`'s 7.4b.

**Waits for other phases:** nothing. `p9-dashboard` (Phase 9) waits for Phase 7.

## Phase 7 exit

On Windows and Rocky Linux 9 (the GPU suite where P7-3 says):
- the legacy GPU processor's shaders, uniforms, 3D textures and cache IDs match the wheel's
  (O7.1) for every chain, edge length, optimization level and language of 7.1c's sweep, with
  the same errors;
- upstream's 264 GPU tests are ported and pass on llvmpipe against the wheel's CPU output with
  upstream's tolerances, each also bit-identical to the wheel's shader on the same renderer,
  with all four P7-7 inverse tests included; a signed renderer waiver (P7-6) keeps
  its test running and checking same-renderer wheel identity;
- `NoOps/partition_gpu_ops`, `NoOps/throw` and `GpuShader/MetalLutTest` are ported and pass;
- every suite shader compiles with glslang in the six GLSL languages and with fxc and dxc in
  HLSL (P7-5);
- `upstream-map.toml` has `NoOps.cpp/.h`, `Processor.cpp`, `tests/gpu/*`, `GPUUnitTest.*`,
  `oglapp.*` and `glsl.*` done, and `docs/parity.md` counts the GPU tests.
