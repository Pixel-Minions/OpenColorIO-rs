# Phase 0 cards

The harness work packages (WP 0.1–0.4, 0.6) are done by the orchestrator. These cards are
the spikes and WP 0.5. Each ends with a written report (`docs/spikes/<id>.md`) that the owner
reads: what was proven, what was not, and anything that needs a decision.

Shared facts:
- **Upstream:** OpenColorIO `v2.5.2` in `upstream/OpenColorIO`.
- **Oracle:** `opencolorio==2.5.2` and `numpy==2.3.5` on Python 3.13, locked in `oracle/uv.lock`.
- **Reference machine:**
  - AMD Ryzen 9 9950X3D (Zen 5: SSE2, AVX, AVX2, FMA, F16C and AVX-512F/DQ/BW/VL).
  - Windows 11, plus Rocky Linux 9.8 (glibc 2.34) under Docker on the same CPU.
- **Dependency versions the wheel builds with** (`share/cmake/modules/FindExtPackages.cmake`): yaml-cpp 0.8.0, expat 2.7.2, pystring 1.1.4, Imath 3.2.1, minizip-ng 4.0.10, pybind11 3.0.1.
- **Found so far:** the Windows wheel embeds the built-in configs' YAML with CRLF line endings, while upstream's files and the Linux wheel use LF. So `BuiltinConfigRegistry()[name]` differs between platforms, and the port must do the same (D12). `serialize()` output and cache IDs are identical on both.

---

### S2 + S5: fast math and exact math

| | |
|---|---|
| **Agent** | Implementer A |
| **Upstream** | `src/OpenColorIO/SSE.h`, `SSE2.h` (helpers used by these ops), `ops/log/LogOpCPU.cpp`, `ops/log/LogOpData.cpp`, `ops/log/LogUtils.cpp`, `ops/gamma/GammaOpCPU.cpp`, `ops/gamma/GammaOpData.cpp`, `ops/gamma/GammaOpUtils.cpp`, `MathUtils.cpp/.h`, `BitDepthUtils.h` @ v2.5.2 |
| **Rust target** | `crates/ocio-ops/src/sse.rs` (the fast-math approximations, as exact scalar code), `crates/ocio-ops/src/math_utils.rs` (C++ `std::min`/`std::max`/`Clamp` semantics), and `crates/ocio-ops/src/ops/{log,gamma}/` (op data and F32 CPU renderers, enough for the probes) |

**Prove:**
1. **Fast math is bit-exact.** With the default optimization (FAST_LOG_EXP_POW on), the Log and Gamma renderers are bit-identical to the wheel for every style they have. For Log that is log2, log10, ln and antilog; the affine and camera variants of LogAffineTransform and LogCameraTransform; forward and inverse. For Gamma that is basic, moncurve, mirror and pass-thru; forward and inverse.
   - Probe with all 65,536 half values, the specials, and ≥10⁶ seeded random values, in RGBA.
   - Run on Windows and in Rocky Linux 9.
2. **S5: exact math.** With FAST_LOG_EXP_POW off, the same ops call libm (`logf`, `powf`, ...). Check that the port, calling Rust `std` (the same CRT/glibc routines), is bit-identical on both platforms.
   - Where Windows differs, find out why: MSVC auto-vectorizing into SVML (`__vdecl_*`), a `/fp:` mode, or constant folding. Report it with a minimal reproduction.
3. **Also report:**
   - which renderer each transform uses;
   - any place where the C++ build must have fused multiply-adds or reordered arithmetic;
   - anything that differs between Windows and Linux.

**Oracle:** use the `cpu_apply` command with transform specs (`oracle/ocio_oracle/spec.py`). Add commands in `oracle/ocio_oracle/numerics.py` only if needed.

**Port tests:** ported tests are welcome where they come for free, e.g. `tests/cpu/SSE_tests.cpp`, `ops/log/LogOpCPU_tests.cpp` and `ops/gamma/GammaOpCPU_tests.cpp`, with markers. Full porting is Phase 1.

**Done when:** exact oracle checks pass on both platforms, or each mismatch has a root cause and a minimal reproduction; the report is in `docs/spikes/s2-s5.md`; `cargo xtask ci` passes.

---

### S4: CPU dispatch, Lut3D profiles and half conversion

| | |
|---|---|
| **Agent** | Implementer B |
| **Upstream** | `src/OpenColorIO/CPUInfo.cpp/.h`, `CPUInfoConfig.h.in`, `ops/lut3d/Lut3DOpCPU.cpp`, `Lut3DOpCPU_SSE2.cpp`, `Lut3DOpCPU_AVX.cpp`, `Lut3DOpCPU_AVX2.cpp`, `Lut3DOpCPU_AVX512.cpp`, `ops/lut3d/Lut3DOpData.cpp` (as needed), `SSE2.h`, `AVX.h`, `AVX2.h`, `AVX512.h` (the half conversions and pack/unpack they use) @ v2.5.2 |
| **Rust target** | `crates/ocio-ops/src/cpu_info.rs`; `crates/ocio-ops/src/ops/lut3d/` (forward renderers: tetrahedral and trilinear, one numeric profile per C++ kernel, written as exact scalar code first); half-conversion helpers in the SIMD modules (e.g. `sse2.rs`, `avx.rs`) |

**Prove:**
1. **CPU dispatch.** `CPUInfo` detects the same flags and quirks as upstream 2.5.2: SSE2_SLOW, SSE3_SLOW, SSSE3_SLOW, AVX_SLOW and AVX2_SLOWGATHER. (The EPYC 9V45 AVX-512 exception is 2.6 and later: upstream `c2bd98f7`, not in v2.5.2.) Dispatch picks the kernel the C++ would pick on this machine.
   - Find out which kernel the wheel actually runs here, e.g. by comparing every Rust profile's output with the wheel's.
2. **Lut3D is bit-exact.** Lut3D forward (tetrahedral and trilinear, F32 in and out, RGBA) is bit-identical to the wheel for the profile the wheel uses on this machine, on Windows and in Rocky Linux 9.
   - Sizes: 2, 3, 17, 33, 65 and 129.
   - Seeded random LUT values.
   - Probe inputs: half values, specials (NaN, ±Inf, negatives, >1), and random values.
   - The FMA kernels must use `f32::mul_add` exactly where the C++ uses FMA intrinsics.
3. **Half conversion.** OCIO has three f32→half and half→f32 conversions:
   - the scalar one (Imath);
   - the SSE2 software one;
   - the F16C one.

   Compare them exhaustively (all 2³² f32 inputs; all 65,536 half inputs) and report where they differ.
4. **Other profiles.** Say which profiles (SSE2, AVX, AVX2, AVX-512) could not be verified against the wheel on this machine, and propose how to verify them. Intel SDE is an option that needs the owner's approval.

**Oracle:** `cpu_apply` with a `Lut3DTransform` spec (`setGridSize`, then `setValue` calls, or a better bulk path you add in `oracle/ocio_oracle/numerics_lut.py`).

**Port tests:** `tests/cpu/CPUInfo`-related, `ops/lut3d/Lut3DOpCPU_tests.cpp`, and `AVX*_tests.cpp`/`SSE2_tests.cpp` where they come for free, with markers.

**Done when:** as S2, with the report in `docs/spikes/s4.md`.

---

### WP 0.5 + S1: C/C++ text formatting and the YAML emitter

| | |
|---|---|
| **Agent** | Implementer C |
| **Upstream** | every float→text and text→float site in `src/OpenColorIO`: `ParseUtils.cpp`, `utils/NumberUtils.h`, `utils/StringUtils.h`, `GpuShaderUtils.cpp` (`getFloatString`), `fileformats/ctf/CTFTransform.cpp` (writer widths and precision), `OCIOYaml.cpp` (emitter settings), the transforms' `operator<<`; and yaml-cpp **0.8.0**'s emitter (`emitter.cpp`, `emitterstate.cpp`, `emitterutils.cpp`, `ostream_wrapper.cpp`). Clone yaml-cpp 0.8.0 for reading into `target/ref/` (git-ignored) |
| **Rust target** | `crates/ocio-ops/src/cfmt.rs` (C `printf` and C++ iostream float and integer formatting, as OCIO uses them); `crates/ocio-ops/src/utils/number_utils.rs` and `string_utils.rs`; `crates/ocio/src/yaml_cpp/` (a port of yaml-cpp 0.8.0's emitter: the event API, styles, quoting rules, indentation, literal blocks, verbatim tags, float precision) |

**Prove:**
1. **cfmt.**
   - List every formatting call site OCIO uses, and which C/iostream behavior each needs.
   - `cfmt` reproduces them byte for byte. Verify against the platform C library itself: call `snprintf` through FFI in `crates/ocio-testkit/src/crt.rs`, the one place outside SIMD and `ocio-py` where `unsafe` is allowed.
   - Cover ≥10⁷ values (random bit patterns, specials, halfway cases) on Windows (UCRT) and on Rocky Linux 9 (glibc).
2. **Number parsing.** `NumberUtils` parsing (`from_chars` / `strtod` with the C locale) matches the platform: accepted syntax, partial parses, and hex, inf and nan spellings. Verify against `strtod`/`strtof` via FFI.
3. **S1, the YAML emitter.** The yaml-cpp emitter port writes the eight built-in configs' `serialize()` output byte-identically. Read each fixture (`fixtures/builtin_configs/*/serialize.ocio`) with your own event reader (e.g. `saphyr-parser`), keeping each node's flow or block style and its tags. Re-emit it through the emitter with the calls OCIO's writer makes (`OCIOYaml.cpp` save functions), including float precision settings. The output must be byte-identical.
   - This proves the emitter. The config model and `OCIOYaml` writer are Phase 3.
   - Also emit hand-built event streams for the cases the configs don't cover: empty containers, special characters needing quotes, multi-line descriptions, long lines, non-ASCII, `.inf`/`.nan`. Their expected text must come from the oracle: add a `yaml_emit` probe that builds the same content through a real OCIO config and serializes it.
4. **Report** anything in yaml-cpp's behavior that OCIO depends on and that is surprising.

**Done when:** as S2, with the report in `docs/spikes/s1-wp05.md`. New dependencies (e.g. `saphyr-parser`) are pinned exactly in the root `Cargo.toml`. (`saphyr-parser` was removed in Phase 3, 3.7d, when the S1 tests moved to the port's reader and writer: owner item D7.)
