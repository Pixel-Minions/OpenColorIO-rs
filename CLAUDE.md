# OpenColorIO-rs: rules for agents

An exact Rust port of OpenColorIO **2.5.2**, built by agents and accepted only on
byte-by-byte parity with the official `opencolorio==2.5.2` wheel. `PLAN.md` is the source of
truth: read §3 (definition of done), §7 (how agents work) and your card before writing code.

## The rules

1. **Port, don't reinvent.**
   - Read the upstream code in `upstream/OpenColorIO` (submodule pinned at `v2.5.2`) and translate it.
   - Every ported function's doc comment cites its source: `` Port of `Foo::bar` (src/OpenColorIO/Foo.cpp:120-188 @ v2.5.2). ``
   - Don't guess behavior, simplify it, or "fix" it. Upstream bugs that change outputs are ported too.
2. **Expected values come only from the oracle or from upstream's tests.**
   - The oracle is the real OCIO 2.5.2 (`ocio_testkit::Oracle`, `fixtures/`).
   - Upstream tests are copied verbatim, with a citation.
   - Never use the port's own output as an expected value. Never type a number into a test that didn't come from one of those two places.
3. **Comparisons are exact.**
   - Floats compare bitwise (`assert_f32_bits_eq`, `assert_pixels_bits_eq`); text compares byte for byte (`assert_text_eq`).
   - The only tolerances allowed are upstream's own, in ported upstream tests, through `ocio_testkit::upstream` helpers.
4. **Never weaken a check.**
   - No `#[ignore]`, `todo!()` or `unimplemented!()`, and no loosened comparison.
   - Never edit `fixtures/`, `oracle/uv.lock`, `waivers.toml` or `docs/ratchet.toml` to get green.
   - If you can't explain a mismatch, stop. Write a minimal reproduction (an oracle probe plus a failing test), leave it failing, and report it.
5. **Match upstream's arithmetic exactly.** See "Bit-exact porting" below.
6. **Stay in scope.**
   - Change only the files your card names. Other agents are working in parallel.
   - Don't commit, push, or change git config. The orchestrator merges.

## Bit-exact porting

- **Precision and order.**
  - Keep every f32/f64 boundary and the operation order exactly as in C++, including overload promotions: `std::pow(float, float)` is `powf`, but `std::pow(float, double)` computes in double.
  - Keep casts where C++ has them: `static_cast<float>(double_expr)` becomes `as f32` at the same place.
- **FMA.** Rust never fuses multiply-adds. Use `f32::mul_add` only where upstream calls an FMA intrinsic (`Lut1DOpCPU_AVX2/AVX512`, `Lut3DOpCPU_AVX2/AVX512`), or where a spike proved that a compiler fused the operation in the wheel.
- **min, max and clamp.**
  - C++ `std::min(a, b)` is `(b < a) ? b : a`, and `std::max(a, b)` is `(a < b) ? b : a`.
  - SSE `_mm_min_ps(a, b)` is `a < b ? a : b`, and `_mm_max_ps(a, b)` is `a > b ? a : b`.
  - They treat NaN differently from each other and from Rust's `f32::min`/`f32::max`. Use the helpers that match the C++ you are porting.
- **Rounding.**
  - Scalar integer conversions add 0.5 and truncate (`BitDepthUtils.h`).
  - SIMD stores round to nearest-even.
  - Port whichever the upstream path uses.
- **Math library.**
  - Use the same function the C++ calls: `logf`/`log2f`/`expf`/`powf` on f32, or `log`/`pow` on f64, through Rust `std`. Rust `std` calls the same CRT or glibc routine as C++ on that platform.
  - Compilers can fold constant math calls at compile time with a different implementation. When upstream computes a constant with a math function, verify it against the oracle.
- **Fast math.** OCIO's default optimization uses its own approximations (`SSE.h`: `sseLog2`, `sseExp2`, `ssePower`, ...), not libm. Port them operation for operation.
- **SIMD.**
  - Scalar code comes first. Each profile (SSE2, AVX, AVX2, AVX-512) is its own renderer and replicates the C++ kernel's arithmetic.
  - `unsafe` is allowed only in SIMD modules and `ocio-py`; `cargo xtask guards` enforces this.
- **Platform differences.** Where OCIO gives different bytes on Windows and on Linux, the port must too (PLAN.md D12). Check both platforms.

## The oracle

- `ocio_testkit::Oracle::get().call(cmd, json_args, &[blobs])` runs a command of `oracle/ocio_oracle/` in the pinned wheel on this machine.
- Responses are cached under `<target>/oracle-cache`. The cache key covers the oracle sources, the lock file and this machine's CPU.
- Pixel checks always run live, because the kernel choice and the math library belong to the machine. Never commit pixel data.
- Text that is identical on every platform is committed with `cargo xtask oracle regen <group>` (groups live in `oracle/ocio_oracle/regen.py`). Tests read it with `ocio_testkit::fixtures::read_text`.
- **New oracle commands.**
  - Add them in your own module under `oracle/ocio_oracle/`, registered in `commands.py`.
  - A command reports what the library does and never computes expected values.
  - The owner reviews every oracle change.
- **Both reference platforms:**
  - Windows: `cargo test ...`
  - Rocky Linux 9: `scripts/rocky9.sh cargo test ...`. It uses the `docker/rocky9` image, with the checkout mounted at `/work`.

## Layout and conventions

- **Where code goes.**
  - Files mirror upstream: `src/OpenColorIO/ops/lut1d/Lut1DOpCPU.cpp` becomes `crates/ocio-ops/src/ops/lut1d/lut1d_op_cpu.rs`.
  - `upstream-map.toml` records each upstream file's target and status (`todo | partial | done | n/a`). Update it as you port.
- **Ported upstream tests.**
  - A `tests/cpu/.../Foo_tests.cpp` file becomes `foo_tests.rs` next to `foo.rs`, included from `foo.rs` with `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;`.
  - Each ported test carries a marker directly above `#[test]`: ``/// Port of `OCIO_ADD_TEST(Group, name)` @ v2.5.2.``
  - `cargo xtask parity` and the ratchet count these markers.
- **Headers.** Every `.rs` file starts with `// SPDX-License-Identifier: BSD-3-Clause` and `// Copyright Contributors to the OpenColorIO Project.`
- **Dependencies.** Pin them exactly in the root `Cargo.toml` `[workspace.dependencies]` (`=x.y.z`); crates use `workspace = true`. New dependencies need a reason in your report.
- **Errors.** Upstream exception text is part of the output, so copy it verbatim.
- **Before reporting done:** `cargo fmt --all`, `cargo clippy --workspace --all-targets` (no warnings), `cargo xtask ci`, and `cargo test --workspace`, on Windows and in Rocky Linux 9.

## Commands

| Command | Purpose |
|---|---|
| `cargo xtask ci` | Guards, fixture hashes, ratchet, parity dashboard freshness |
| `cargo xtask guards` | Forbidden patterns, the `unsafe` allowlist, headers, dependency pins, the upstream map |
| `cargo xtask parity` | Regenerate `docs/parity.md` after porting tests |
| `cargo xtask ratchet --update` | Record the new number of ported upstream tests |
| `cargo xtask upstream-tests` | Every upstream test and whether it is ported |
| `cargo xtask oracle info` | The oracle's versions and this machine's CPU features |
| `cargo xtask oracle regen <group>` | Regenerate committed fixtures (owner-reviewed) |
| `cargo xtask oracle check-all` | Prove committed fixtures are identical on this platform |
| `scripts/rocky9.sh <cmd>` | Run a command on the Linux reference platform |
